//! AudioHub — top-level facade for the Audio module.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::routing;
use super::sinks::{ChannelInfo, SinkManager};
use super::streamer::{self, Streamer, StreamerState};
use crate::config::AudioConfig;

pub struct AudioHub {
    // Declared before `sink_mgr` so it drops first: Streamer's Drop restores
    // the classic graph while the channel sinks it rewires still exist.
    streamer: Arc<Streamer>,
    sink_mgr: Arc<SinkManager>,
    streams: Arc<RwLock<Vec<routing::StreamInfo>>>,
}

impl AudioHub {
    pub async fn new(config: &AudioConfig) -> Result<Self> {
        // SinkManager::create_all is blocking (subprocess calls) — run off-tokio
        let sink_mgr = tokio::task::spawn_blocking(SinkManager::create_all)
            .await
            .expect("sink creation panicked")?;
        let sink_mgr = Arc::new(sink_mgr);

        // NOTE: We deliberately do NOT re-apply `config.default_volumes` here. Doing so on
        // every daemon (re)start fired a `pactl set-sink-volume` per channel → an OS volume
        // notification on each start and clobbered whatever the user had set. Brand-new
        // null-sinks already default to 100%; the user's saved per-channel levels are
        // restored once at app startup by the Tauri host's `hydrate_audio_routing`, which
        // reads them from ui-settings.json (the UI's source of truth).
        let _ = &config.default_volumes;

        // Streamer Mode is seeded from the UI's saved `mixer.stream`, so it
        // survives a daemon restart (the UI is that file's only writer).
        let (stream_on, levels) = streamer::seed_from_settings(&load_ui_settings());
        let streamer = Arc::new(Streamer::new(levels));
        if stream_on {
            let s = Arc::clone(&streamer);
            match tokio::task::spawn_blocking(move || s.enable()).await {
                Ok(Err(e)) => tracing::warn!("Streamer Mode could not be restored: {e:#}"),
                Err(e) => tracing::error!("Streamer Mode restore panicked: {e}"),
                Ok(Ok(())) => {}
            }
        }

        let hub = Self {
            streamer,
            sink_mgr,
            streams: Arc::new(RwLock::new(Vec::new())),
        };
        hub.start_stream_watcher();
        Ok(hub)
    }

    pub async fn get_channels(&self) -> Vec<ChannelInfo> {
        // The UI polls this every 2s, which is also what catches Personal
        // fader moves made outside OpenGG (pavucontrol, Ear Blast) while
        // Streamer Mode has the Personal path on a loopback.
        if self.streamer.is_enabled() {
            let s = Arc::clone(&self.streamer);
            let _ = tokio::task::spawn_blocking(move || s.sync_personal()).await;
        }
        let mut channels = self.sink_mgr.get_channels();
        let streams = self.streams.read().await;
        for ch in &mut channels {
            ch.assigned_apps = routing::apps_for_channel(&streams, &ch.name);
        }
        channels
    }

    pub async fn get_streams(&self) -> Vec<routing::StreamInfo> {
        self.streams.read().await.clone()
    }

    pub fn set_volume(&self, channel: &str, volume: u32) -> Result<()> {
        self.sink_mgr.set_volume(channel, volume as f32 / 100.0)?;
        self.streamer.sync_personal();
        Ok(())
    }

    pub fn set_mute(&self, channel: &str, muted: bool) -> Result<()> {
        self.sink_mgr.set_mute(channel, muted)?;
        self.streamer.sync_personal();
        Ok(())
    }

    pub fn streamer_state(&self) -> StreamerState {
        self.streamer.state()
    }

    pub async fn set_streamer_mode(&self, enabled: bool) -> Result<()> {
        let s = Arc::clone(&self.streamer);
        tokio::task::spawn_blocking(move || if enabled { s.enable() } else { s.disable() })
            .await
            .expect("streamer toggle panicked")
    }

    pub async fn set_stream_level(&self, channel: String, volume: Option<u32>, muted: Option<bool>) -> Result<()> {
        let s = Arc::clone(&self.streamer);
        tokio::task::spawn_blocking(move || s.set_level(&channel, volume, muted))
            .await
            .expect("stream level panicked")
    }

    pub async fn set_personal_device(&self, channel: String, device: String) -> Result<()> {
        let s = Arc::clone(&self.streamer);
        tokio::task::spawn_blocking(move || s.set_personal_device(&channel, &device))
            .await
            .expect("personal device panicked")
    }

    pub fn route_app(&self, stream_id: u32, channel: &str) -> Result<()> {
        routing::route_stream(stream_id, channel)
    }

    pub async fn remove_virtual_audio(&self) -> Result<()> {
        let sink_mgr = Arc::clone(&self.sink_mgr);
        let streamer = Arc::clone(&self.streamer);
        // Streamer Mode first: its modules would otherwise be swept up by the
        // teardown while it still believed itself enabled.
        tokio::task::spawn_blocking(move || {
            streamer.disable()?;
            sink_mgr.teardown_all()
        })
            .await
            .expect("teardown panicked")
    }

    pub async fn create_virtual_audio(&self) -> Result<()> {
        let sink_mgr = Arc::clone(&self.sink_mgr);
        tokio::task::spawn_blocking(move || sink_mgr.ensure_created())
            .await
            .expect("sink creation panicked")
    }

    fn start_stream_watcher(&self) {
        let streams = Arc::clone(&self.streams);
        tokio::spawn(async move {
            loop {
                match tokio::task::spawn_blocking(routing::list_streams).await {
                    Ok(Ok(new)) => { *streams.write().await = new; }
                    Ok(Err(e)) => { tracing::warn!("Stream scan error: {e}"); }
                    Err(e) => { tracing::error!("Stream watcher panic: {e}"); }
                }
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        });
    }
}

/// `~/.config/opengg/ui-settings.json`, or `{}` when absent or unreadable.
fn load_ui_settings() -> serde_json::Value {
    dirs::config_dir()
        .map(|d| d.join("opengg/ui-settings.json"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

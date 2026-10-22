//! AudioHub — top-level facade for the Audio module.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::eq::{EqHost, EqState};
use super::micfx::{MicFx, MicFxHost};
use super::routing;
use super::sinks::{ChannelInfo, SinkManager};
use crate::config::AudioConfig;

pub struct AudioHub {
    sink_mgr: Arc<SinkManager>,
    streams: Arc<RwLock<Vec<routing::StreamInfo>>>,
    eq: Arc<EqHost>,
    micfx: Arc<MicFxHost>,
    /// Asks the reconciler for a relink pass.
    reconcile: Arc<tokio::sync::Notify>,
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

        let eq = Arc::new(EqHost::default());
        let micfx = Arc::new(MicFxHost::default());
        // Keeps sinks, links, EQ chains and app rules right as devices come
        // and go and the sound server restarts.
        let reconcile = super::reconcile::spawn(Arc::clone(&sink_mgr), Arc::clone(&eq), Arc::clone(&micfx));
        let hub = Self {
            sink_mgr,
            streams: Arc::new(RwLock::new(Vec::new())),
            eq,
            micfx,
            reconcile,
        };
        hub.start_stream_watcher();
        Ok(hub)
    }

    pub async fn get_channels(&self) -> Vec<ChannelInfo> {
        let sink_mgr = Arc::clone(&self.sink_mgr);
        let mut channels = blocking(move || Ok(sink_mgr.get_channels())).await.unwrap_or_default();
        let streams = self.streams.read().await;
        for ch in &mut channels {
            ch.assigned_apps = routing::apps_for_channel(&streams, &ch.name);
        }
        channels
    }

    pub async fn get_streams(&self) -> Vec<routing::StreamInfo> {
        self.streams.read().await.clone()
    }

    pub async fn set_volume(&self, channel: &str, volume: u32) -> Result<()> {
        let (sink_mgr, channel) = (Arc::clone(&self.sink_mgr), channel.to_string());
        blocking(move || sink_mgr.set_volume(&channel, volume as f32 / 100.0)).await
    }

    pub async fn set_mute(&self, channel: &str, muted: bool) -> Result<()> {
        let (sink_mgr, channel) = (Arc::clone(&self.sink_mgr), channel.to_string());
        blocking(move || sink_mgr.set_mute(&channel, muted)).await
    }

    pub async fn route_app(&self, stream_id: u32, channel: &str) -> Result<()> {
        let channel = channel.to_string();
        blocking(move || routing::route_stream(stream_id, &channel)).await
    }

    /// Set a channel's EQ (JSON `EqState`), then have the reconciler route
    /// the channel through or around it.
    pub async fn set_eq(&self, channel: &str, state_json: &str) -> Result<()> {
        let state: EqState = serde_json::from_str(state_json)?;
        let (eq, channel) = (Arc::clone(&self.eq), channel.to_string());
        blocking(move || eq.set(&channel, &state).map_err(anyhow::Error::msg)).await?;
        self.reconcile.notify_one();
        Ok(())
    }

    /// Set microphone processing (JSON `MicFx`), then relink the mic.
    pub async fn set_micfx(&self, json: &str) -> Result<()> {
        let fx: MicFx = serde_json::from_str(json)?;
        let host = Arc::clone(&self.micfx);
        blocking(move || host.set(&fx).map_err(anyhow::Error::msg)).await?;
        self.reconcile.notify_one();
        Ok(())
    }

    pub async fn micfx_status(&self) -> String {
        let host = Arc::clone(&self.micfx);
        blocking(move || Ok(host.status_json())).await.unwrap_or_else(|_| "{}".into())
    }

    /// Ask for a relink pass (after the UI saved a device choice).
    pub fn request_reconcile(&self) {
        self.reconcile.notify_one();
    }

    pub async fn eq_status(&self) -> String {
        let eq = Arc::clone(&self.eq);
        blocking(move || Ok(eq.status_json())).await.unwrap_or_else(|_| "{}".into())
    }

    pub async fn remove_virtual_audio(&self) -> Result<()> {
        let sink_mgr = Arc::clone(&self.sink_mgr);
        tokio::task::spawn_blocking(move || sink_mgr.teardown_all())
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

/// Run subprocess-bound work off the async runtime, so one slow `pactl`
/// does not stall every other D-Bus request on the same worker.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| anyhow::anyhow!("audio task failed: {e}"))?
}

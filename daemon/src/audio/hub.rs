//! AudioHub — top-level facade for the Audio module.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::routing;
use super::sinks::{ChannelInfo, SinkManager};
use crate::config::AudioConfig;

pub struct AudioHub {
    sink_mgr: Arc<SinkManager>,
    streams: Arc<RwLock<Vec<routing::StreamInfo>>>,
}

// SinkManager is Send+Sync, so AudioHub is too.
unsafe impl Send for AudioHub {}
unsafe impl Sync for AudioHub {}

impl AudioHub {
    pub async fn new(config: &AudioConfig) -> Result<Self> {
        // SinkManager::create_all is blocking (subprocess calls) — run off-tokio
        let sink_mgr = tokio::task::spawn_blocking(SinkManager::create_all)
            .await
            .expect("sink creation panicked")?;
        let sink_mgr = Arc::new(sink_mgr);

        // Apply configured default volumes
        for (name, &vol) in &config.default_volumes {
            sink_mgr.set_volume(name, vol as f32 / 100.0)?;
        }

        let hub = Self {
            sink_mgr,
            streams: Arc::new(RwLock::new(Vec::new())),
        };
        hub.start_stream_watcher();
        Ok(hub)
    }

    pub async fn get_channels(&self) -> Vec<ChannelInfo> {
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
        self.sink_mgr.set_volume(channel, volume as f32 / 100.0)
    }

    pub fn set_mute(&self, channel: &str, muted: bool) -> Result<()> {
        self.sink_mgr.set_mute(channel, muted)
    }

    pub fn route_app(&self, stream_id: u32, channel: &str) -> Result<()> {
        routing::route_stream(stream_id, channel)
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

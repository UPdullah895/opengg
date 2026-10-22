//! D-Bus interface: org.opengg.Daemon.Audio

use std::collections::HashMap;
use std::sync::Arc;
use zbus::interface;

use crate::audio::AudioHub;

pub struct AudioInterface {
    hub: Arc<AudioHub>,
}

impl AudioInterface {
    pub fn new(hub: AudioHub) -> Self {
        Self { hub: Arc::new(hub) }
    }
}

#[derive(Debug, serde::Serialize)]
struct ChannelData {
    name: String,
    volume: u32,
    muted: bool,
    apps: Vec<AppData>,
}

#[derive(Debug, serde::Serialize)]
struct AppData {
    id: u32,
    name: String,
    binary: String,
}

#[interface(name = "org.opengg.Daemon.Audio")]
impl AudioInterface {
    async fn get_channels(&self) -> String {
        let channels = self.hub.get_channels().await;
        let data: Vec<ChannelData> = channels.into_iter().map(|ch| ChannelData {
            name: ch.name,
            volume: (ch.volume * 100.0) as u32,
            muted: ch.muted,
            apps: ch.assigned_apps.into_iter().map(|a| AppData {
                id: a.id, name: a.name, binary: a.binary,
            }).collect(),
        }).collect();
        serde_json::to_string(&data).unwrap_or_else(|_| "[]".into())
    }

    async fn set_volume(&self, channel: &str, volume: u32) -> zbus::fdo::Result<()> {
        self.hub.set_volume(channel, volume).await.map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn set_mute(&self, channel: &str, muted: bool) -> zbus::fdo::Result<()> {
        self.hub.set_mute(channel, muted).await.map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn get_apps(&self) -> String {
        let streams = self.hub.get_streams().await;
        let data: Vec<HashMap<String, String>> = streams.into_iter().map(|s| {
            let mut m = HashMap::new();
            m.insert("id".into(), s.index.to_string());
            m.insert("name".into(), s.app_name);
            m.insert("binary".into(), s.binary);
            m.insert("channel".into(), s.channel);
            m.insert("icon".into(), s.icon);
            m.insert("volume".into(), s.volume.to_string());
            m
        }).collect();
        serde_json::to_string(&data).unwrap_or_else(|_| "[]".into())
    }

    async fn route_app(&self, app_id: u32, channel: &str) -> zbus::fdo::Result<()> {
        self.hub.route_app(app_id, channel).await.map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Set one channel's EQ: `{"enabled": bool, "bands": [dB x10], "preamp": dB}`.
    async fn set_eq(&self, channel: &str, state_json: &str) -> zbus::fdo::Result<()> {
        self.hub.set_eq(channel, state_json).await.map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// `{"available", "reason", "channels": {ch: {enabled, bands, preamp, running}}}`
    async fn get_eq(&self) -> String {
        self.hub.eq_status().await
    }

    /// Set microphone processing:
    /// `{"nr": {enabled, intensity}, "gate": {enabled, threshold}, "comp": {enabled, level}}`.
    async fn set_mic_fx(&self, json: &str) -> zbus::fdo::Result<()> {
        self.hub.set_micfx(json).await.map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// `{"caps": {nr, gate, comp}, "state": {...}, "running": bool}`
    async fn get_mic_fx(&self) -> String {
        self.hub.micfx_status().await
    }

    /// Re-link channels from the saved settings (`mixer.devices`, EQ) now.
    async fn reconcile(&self) {
        self.hub.request_reconcile();
    }

    async fn remove_virtual_audio(&self) -> zbus::fdo::Result<()> {
        self.hub.remove_virtual_audio().await.map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn create_virtual_audio(&self) -> zbus::fdo::Result<()> {
        self.hub.create_virtual_audio().await.map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
}

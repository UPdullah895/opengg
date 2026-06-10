//! Linux platform backend implementation — Thin delegations to existing facades.
//!
//! This module implements the cross-platform traits by delegating to AudioHub,
//! Recorder, and DeviceInterface. No behavior changes; pure structural adaptation.

#![cfg(target_os = "linux")]

use async_trait::async_trait;
use anyhow::Result;

use super::{AudioBackend, CaptureBackend, DeviceBackend, ChannelInfo, StreamInfo, RecordMode, DeviceInfo};
use crate::audio::AudioHub;
use crate::replay::Recorder;
use crate::device::DeviceInterface;

// ─────────────────────────────────────────────────────────────────────────
// Audio Backend for Linux
// ─────────────────────────────────────────────────────────────────────────

#[async_trait]
impl AudioBackend for AudioHub {
    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn get_channels(&self) -> Vec<ChannelInfo> {
        AudioHub::get_channels(self).await
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn get_streams(&self) -> Vec<StreamInfo> {
        AudioHub::get_streams(self).await
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn set_volume(&self, channel: &str, volume: u32) -> Result<()> {
        AudioHub::set_volume(self, channel, volume)
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn set_mute(&self, channel: &str, muted: bool) -> Result<()> {
        AudioHub::set_mute(self, channel, muted)
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn route_app(&self, stream_id: u32, channel: &str) -> Result<()> {
        AudioHub::route_app(self, stream_id, channel)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Capture Backend for Linux
// ─────────────────────────────────────────────────────────────────────────

#[async_trait]
impl CaptureBackend for Recorder {
    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn status(&self) -> RecordMode {
        Recorder::status(self).await
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn start_replay(&self, duration: u32) -> Result<()> {
        Recorder::start_replay(self, duration).await
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn start_recording(&self) -> Result<()> {
        Recorder::start_recording(self).await
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn stop(&self) -> Result<()> {
        Recorder::stop(self).await
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn save_replay(&self) -> Result<()> {
        Recorder::save_replay(self).await
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Device Backend for Linux
// ─────────────────────────────────────────────────────────────────────────

#[async_trait]
impl DeviceBackend for DeviceInterface {
    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        // DeviceInterface.get_devices_json() returns JSON string; parse it.
        let json_str = self.get_devices_json().await;
        let devices: Vec<DeviceInfo> = serde_json::from_str(&json_str)
            .unwrap_or_default();
        Ok(devices)
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn set_dpi(&self, device_id: &str, dpi: u32) -> Result<()> {
        self.set_dpi(device_id, dpi)
            .await
            .map_err(|e| anyhow::anyhow!("set_dpi failed: {e}"))
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn set_polling_rate(&self, device_id: &str, rate: u32) -> Result<()> {
        self.set_polling_rate(device_id, rate)
            .await
            .map_err(|e| anyhow::anyhow!("set_polling_rate failed: {e}"))
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn set_sidetone(&self, device_id: &str, level: u32) -> Result<()> {
        self.set_sidetone(device_id, level)
            .await
            .map_err(|e| anyhow::anyhow!("set_sidetone failed: {e}"))
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn set_chatmix(&self, device_id: &str, level: u32) -> Result<()> {
        self.set_chatmix(device_id, level)
            .await
            .map_err(|e| anyhow::anyhow!("set_chatmix failed: {e}"))
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn set_inactive_time(&self, device_id: &str, minutes: u32) -> Result<()> {
        self.set_inactive_time(device_id, minutes)
            .await
            .map_err(|e| anyhow::anyhow!("set_inactive_time failed: {e}"))
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn set_microphone_volume(&self, device_id: &str, level: u32) -> Result<()> {
        self.set_microphone_volume(device_id, level)
            .await
            .map_err(|e| anyhow::anyhow!("set_microphone_volume failed: {e}"))
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn set_mic_mute_led_brightness(&self, device_id: &str, brightness: u32) -> Result<()> {
        self.set_mic_mute_led_brightness(device_id, brightness)
            .await
            .map_err(|e| anyhow::anyhow!("set_mic_mute_led_brightness failed: {e}"))
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn set_volume_limiter(&self, device_id: &str, enabled: bool) -> Result<()> {
        self.set_volume_limiter(device_id, enabled)
            .await
            .map_err(|e| anyhow::anyhow!("set_volume_limiter failed: {e}"))
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn set_bt_when_powered_on(&self, device_id: &str, enabled: bool) -> Result<()> {
        self.set_bt_when_powered_on(device_id, enabled)
            .await
            .map_err(|e| anyhow::anyhow!("set_bt_when_powered_on failed: {e}"))
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn set_bt_call_volume(&self, device_id: &str, level: u32) -> Result<()> {
        self.set_bt_call_volume(device_id, level)
            .await
            .map_err(|e| anyhow::anyhow!("set_bt_call_volume failed: {e}"))
    }

    /// port scaffolding — implemented per-OS in Phase 4-5
    async fn set_eq_preset(&self, device_id: &str, preset_idx: u32) -> Result<()> {
        self.set_eq_preset(device_id, preset_idx)
            .await
            .map_err(|e| anyhow::anyhow!("set_eq_preset failed: {e}"))
    }
}

//! Non-Linux platform stub implementations.
//!
//! These stubs provide type-correct (but non-functional) implementations of the
//! platform traits for Windows and macOS targets. They are behind #[cfg(not(target_os = "linux"))]
//! and will not be compiled on Linux.
//!
//! These are placeholder implementations designed to allow the daemon to compile
//! on Windows/macOS targets. They return errors and are not intended to be used
//! in production until actual platform implementations are provided in Phase 4-5.

#![cfg(not(target_os = "linux"))]

use async_trait::async_trait;
use anyhow::{anyhow, Result};

use super::{AudioBackend, CaptureBackend, DeviceBackend, ChannelInfo, StreamInfo, RecordMode, DeviceInfo};

// ─────────────────────────────────────────────────────────────────────────
// Stub type for non-Linux audio backend (not instantiated on Linux)
// ─────────────────────────────────────────────────────────────────────────

/// Stub for audio operations on non-Linux platforms.
pub struct StubAudioBackend;

#[async_trait]
impl AudioBackend for StubAudioBackend {
    async fn get_channels(&self) -> Vec<ChannelInfo> {
        vec![]
    }

    async fn get_streams(&self) -> Vec<StreamInfo> {
        vec![]
    }

    async fn set_volume(&self, _channel: &str, _volume: u32) -> Result<()> {
        Err(anyhow!("Audio backend not available on this platform"))
    }

    async fn set_mute(&self, _channel: &str, _muted: bool) -> Result<()> {
        Err(anyhow!("Audio backend not available on this platform"))
    }

    async fn route_app(&self, _stream_id: u32, _channel: &str) -> Result<()> {
        Err(anyhow!("Audio backend not available on this platform"))
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Stub type for non-Linux capture backend (not instantiated on Linux)
// ─────────────────────────────────────────────────────────────────────────

/// Stub for capture/replay operations on non-Linux platforms.
pub struct StubCaptureBackend;

#[async_trait]
impl CaptureBackend for StubCaptureBackend {
    async fn status(&self) -> RecordMode {
        RecordMode::Idle
    }

    async fn start_replay(&self, _duration: u32) -> Result<()> {
        Err(anyhow!("Capture backend not available on this platform"))
    }

    async fn start_recording(&self) -> Result<()> {
        Err(anyhow!("Capture backend not available on this platform"))
    }

    async fn stop(&self) -> Result<()> {
        Err(anyhow!("Capture backend not available on this platform"))
    }

    async fn save_replay(&self) -> Result<()> {
        Err(anyhow!("Capture backend not available on this platform"))
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Stub type for non-Linux device backend (not instantiated on Linux)
// ─────────────────────────────────────────────────────────────────────────

/// Stub for device operations on non-Linux platforms.
pub struct StubDeviceBackend;

#[async_trait]
impl DeviceBackend for StubDeviceBackend {
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        Ok(vec![])
    }

    async fn set_dpi(&self, _device_id: &str, _dpi: u32) -> Result<()> {
        Err(anyhow!("Device backend not available on this platform"))
    }

    async fn set_polling_rate(&self, _device_id: &str, _rate: u32) -> Result<()> {
        Err(anyhow!("Device backend not available on this platform"))
    }

    async fn set_sidetone(&self, _device_id: &str, _level: u32) -> Result<()> {
        Err(anyhow!("Device backend not available on this platform"))
    }

    async fn set_chatmix(&self, _device_id: &str, _level: u32) -> Result<()> {
        Err(anyhow!("Device backend not available on this platform"))
    }

    async fn set_inactive_time(&self, _device_id: &str, _minutes: u32) -> Result<()> {
        Err(anyhow!("Device backend not available on this platform"))
    }

    async fn set_microphone_volume(&self, _device_id: &str, _level: u32) -> Result<()> {
        Err(anyhow!("Device backend not available on this platform"))
    }

    async fn set_mic_mute_led_brightness(&self, _device_id: &str, _brightness: u32) -> Result<()> {
        Err(anyhow!("Device backend not available on this platform"))
    }

    async fn set_volume_limiter(&self, _device_id: &str, _enabled: bool) -> Result<()> {
        Err(anyhow!("Device backend not available on this platform"))
    }

    async fn set_bt_when_powered_on(&self, _device_id: &str, _enabled: bool) -> Result<()> {
        Err(anyhow!("Device backend not available on this platform"))
    }

    async fn set_bt_call_volume(&self, _device_id: &str, _level: u32) -> Result<()> {
        Err(anyhow!("Device backend not available on this platform"))
    }

    async fn set_eq_preset(&self, _device_id: &str, _preset_idx: u32) -> Result<()> {
        Err(anyhow!("Device backend not available on this platform"))
    }
}

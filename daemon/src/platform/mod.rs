//! Cross-platform backend trait layer — Port scaffolding for Phase 4-5 Windows/macOS ports.
//!
//! This module defines the abstraction seam for audio, capture (replay), and device backends.
//! The Linux daemon currently uses the facades directly (AudioHub, Recorder, DeviceInterface),
//! and these traits are implemented by those facades as thin delegations (see linux.rs).
//!
//! Future Windows/macOS ports will implement these traits with platform-specific code.
//! Non-Linux targets compile stub implementations (see stub.rs) that return errors — these
//! are not functional but ensure type-correct cross-platform compilation.
//!
//! NOTE: The existing D-Bus handlers and call sites remain unchanged. This is purely
//! additive scaffolding; the facades continue to be used directly by the daemon.

use anyhow::Result;
use async_trait::async_trait;

// Re-export domain types used in trait methods
pub use crate::audio::sinks::ChannelInfo;
pub use crate::audio::routing::StreamInfo;
pub use crate::device::types::DeviceInfo;
pub use crate::replay::recorder::RecordMode;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(not(target_os = "linux"))]
pub mod stub;

/// Platform identifier — port scaffolding, to be used in Phase 4-5.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    Windows,
    MacOS,
    Unsupported,
}

impl Platform {
    /// Return the current platform based on compile-time cfg flags.
    /// Port scaffolding — to be used in Phase 4-5.
    #[allow(dead_code)]
    pub fn current() -> Self {
        #[cfg(target_os = "linux")]
        {
            Platform::Linux
        }
        #[cfg(target_os = "windows")]
        {
            Platform::Windows
        }
        #[cfg(target_os = "macos")]
        {
            Platform::MacOS
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            Platform::Unsupported
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::Linux => write!(f, "Linux"),
            Platform::Windows => write!(f, "Windows"),
            Platform::MacOS => write!(f, "macOS"),
            Platform::Unsupported => write!(f, "Unsupported"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Audio Backend Trait
// ─────────────────────────────────────────────────────────────────────────

/// Cross-platform audio backend trait.
///
/// Methods mirror the public API of AudioHub. Implementations delegate to
/// platform-specific audio routing and mixing APIs.
/// Port scaffolding — implemented per-OS in Phase 4-5.
#[allow(dead_code)]
#[async_trait]
pub trait AudioBackend: Send + Sync {
    /// Get all audio channels and their current state.
    async fn get_channels(&self) -> Vec<ChannelInfo>;

    /// Get all audio streams currently playing or routed.
    async fn get_streams(&self) -> Vec<StreamInfo>;

    /// Set the volume of a channel (0–100, represented as u32).
    async fn set_volume(&self, channel: &str, volume: u32) -> Result<()>;

    /// Mute or unmute a channel.
    async fn set_mute(&self, channel: &str, muted: bool) -> Result<()>;

    /// Route an audio stream (by index) to a channel.
    async fn route_app(&self, stream_id: u32, channel: &str) -> Result<()>;
}

// ─────────────────────────────────────────────────────────────────────────
// Capture Backend Trait
// ─────────────────────────────────────────────────────────────────────────

/// Cross-platform capture (replay & recording) backend trait.
///
/// Methods mirror the public API of Recorder. Implementations manage
/// screen recording, replay buffer, and clip saving.
/// Port scaffolding — implemented per-OS in Phase 4-5.
#[allow(dead_code)]
#[async_trait]
pub trait CaptureBackend: Send + Sync {
    /// Get current recorder state (Idle, Replay, or Recording).
    async fn status(&self) -> RecordMode;

    /// Start the replay buffer with a given duration (seconds).
    async fn start_replay(&self, duration: u32) -> Result<()>;

    /// Start continuous recording.
    async fn start_recording(&self) -> Result<()>;

    /// Stop the current recording or replay buffer.
    async fn stop(&self) -> Result<()>;

    /// Save the current replay buffer to a clip file.
    async fn save_replay(&self) -> Result<()>;
}

// ─────────────────────────────────────────────────────────────────────────
// Device Backend Trait
// ─────────────────────────────────────────────────────────────────────────

/// Cross-platform device backend trait.
///
/// Methods mirror the public API of DeviceInterface. Implementations manage
/// mice, headsets, and other input/output devices.
/// Port scaffolding — implemented per-OS in Phase 4-5.
#[allow(dead_code)]
#[async_trait]
pub trait DeviceBackend: Send + Sync {
    /// List all connected devices.
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>>;

    /// Set mouse DPI.
    async fn set_dpi(&self, device_id: &str, dpi: u32) -> Result<()>;

    /// Set mouse polling rate.
    async fn set_polling_rate(&self, device_id: &str, rate: u32) -> Result<()>;

    /// Set headset sidetone level.
    async fn set_sidetone(&self, device_id: &str, level: u32) -> Result<()>;

    /// Set headset game/chat mix level.
    async fn set_chatmix(&self, device_id: &str, level: u32) -> Result<()>;

    /// Set headset inactive auto-shutdown time (minutes).
    async fn set_inactive_time(&self, device_id: &str, minutes: u32) -> Result<()>;

    /// Set headset microphone volume level.
    async fn set_microphone_volume(&self, device_id: &str, level: u32) -> Result<()>;

    /// Set headset mic-mute LED brightness.
    async fn set_mic_mute_led_brightness(&self, device_id: &str, brightness: u32) -> Result<()>;

    /// Enable/disable headset volume limiter.
    async fn set_volume_limiter(&self, device_id: &str, enabled: bool) -> Result<()>;

    /// Enable/disable Bluetooth on headset power-on.
    async fn set_bt_when_powered_on(&self, device_id: &str, enabled: bool) -> Result<()>;

    /// Set headset Bluetooth call volume.
    async fn set_bt_call_volume(&self, device_id: &str, level: u32) -> Result<()>;

    /// Set headset EQ preset by index.
    async fn set_eq_preset(&self, device_id: &str, preset_idx: u32) -> Result<()>;
}


//! Configuration Management
//!
//! Loads/saves TOML config from `$XDG_CONFIG_HOME/opengg/daemon.toml`.
//! Each module has its own config section that can be enabled/disabled.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Top-level daemon configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub modules: ModuleConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub device: DeviceConfig,
    #[serde(default)]
    pub replay: ReplayConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleConfig {
    #[serde(default = "default_true")]
    pub audio_enabled: bool,
    #[serde(default = "default_true")]
    pub device_enabled: bool,
    #[serde(default = "default_true")]
    pub replay_enabled: bool,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self {
            audio_enabled: true,
            device_enabled: true,
            replay_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Default volumes per channel (0–100).
    #[serde(default = "default_volumes")]
    pub default_volumes: HashMap<String, u32>,
    /// Enable parametric EQ.
    #[serde(default)]
    pub eq_enabled: bool,
    /// Enable RNNoise mic denoising.
    #[serde(default)]
    pub noise_cancel_enabled: bool,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            default_volumes: default_volumes(),
            eq_enabled: false,
            noise_cancel_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    /// Path to device profiles directory.
    #[serde(default = "default_profiles_dir")]
    pub profiles_dir: PathBuf,
    /// OpenRGB SDK host.
    #[serde(default = "default_openrgb_host")]
    pub openrgb_host: String,
    /// OpenRGB SDK port.
    #[serde(default = "default_openrgb_port")]
    pub openrgb_port: u16,
    /// Process names that trigger profile switching.
    #[serde(default)]
    pub game_profiles: HashMap<String, String>,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            profiles_dir: default_profiles_dir(),
            openrgb_host: default_openrgb_host(),
            openrgb_port: default_openrgb_port(),
            game_profiles: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayConfig {
    /// Clips output directory.
    #[serde(default = "default_clips_dir")]
    pub clips_dir: PathBuf,
    /// Replay buffer duration in seconds.
    #[serde(default = "default_replay_duration")]
    pub replay_duration: u32,
    /// Recording FPS.
    #[serde(default = "default_fps")]
    pub fps: u32,
    /// Recording quality preset.
    #[serde(default = "default_quality")]
    pub quality: String,
    /// Keyboard shortcuts.
    #[serde(default)]
    pub shortcuts: ShortcutConfig,
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self {
            clips_dir: default_clips_dir(),
            replay_duration: default_replay_duration(),
            fps: default_fps(),
            quality: default_quality(),
            shortcuts: ShortcutConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutConfig {
    #[serde(default = "default_save_replay_key")]
    pub save_replay: String,
    #[serde(default = "default_toggle_record_key")]
    pub toggle_recording: String,
    #[serde(default = "default_screenshot_key")]
    pub screenshot: String,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self {
            save_replay: default_save_replay_key(),
            toggle_recording: default_toggle_record_key(),
            screenshot: default_screenshot_key(),
        }
    }
}

// ── Default value functions ─────────────────────────────────────

fn default_true() -> bool { true }

fn default_volumes() -> HashMap<String, u32> {
    [
        ("Game".into(), 100),
        ("Chat".into(), 100),
        ("Media".into(), 100),
        ("Aux".into(), 100),
        ("Mic".into(), 100),
    ]
    .into()
}

fn default_profiles_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("opengg/profiles")
}

fn default_openrgb_host() -> String { "127.0.0.1".into() }
fn default_openrgb_port() -> u16 { 6742 }

fn default_clips_dir() -> PathBuf {
    dirs::video_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap().join("Videos"))
        .join("OpenGG")
}

fn default_replay_duration() -> u32 { 30 }
fn default_fps() -> u32 { 60 }
fn default_quality() -> String { "High".into() }
fn default_save_replay_key() -> String { "Alt+F10".into() }
fn default_toggle_record_key() -> String { "Alt+F9".into() }
fn default_screenshot_key() -> String { "Alt+F12".into() }

// ── Load / Save ─────────────────────────────────────────────────

/// Get the config file path.
pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("opengg/daemon.toml")
}

/// Load config from disk, creating defaults if the file doesn't exist.
pub fn load() -> Result<Config> {
    let path = config_path();

    if path.exists() {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config at {}", path.display()))?;
        let config: Config =
            toml::from_str(&content).context("Failed to parse config TOML")?;
        Ok(config)
    } else {
        let config = Config {
            modules: ModuleConfig::default(),
            audio: AudioConfig::default(),
            device: DeviceConfig::default(),
            replay: ReplayConfig::default(),
        };
        save(&config)?;
        Ok(config)
    }
}

/// Save config to disk.
pub fn save(config: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config).context("Failed to serialize config")?;
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write config to {}", path.display()))?;
    tracing::info!("Config saved to {}", path.display());
    Ok(())
}

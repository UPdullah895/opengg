//! Configuration Management
//!
//! Loads/saves TOML config from `$XDG_CONFIG_HOME/opengg/daemon.toml`.
//! Each module has its own config section that can be enabled/disabled.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Top-level daemon configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
        // No HOME at all (minimal service environments): fall back to the temp
        // dir rather than panicking inside a serde default.
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("Videos")
        })
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
    // `dirs::config_dir()` is None only with neither XDG_CONFIG_HOME nor a
    // resolvable home. The old literal "~/.config" fallback created a directory
    // actually named "~" in the working directory; use the temp dir instead.
    dirs::config_dir()
        .unwrap_or_else(|| std::env::temp_dir().join("opengg-config"))
        .join("opengg/daemon.toml")
}

/// What `load` did, so the caller can log it (and diagnostics can report it).
pub struct Loaded {
    pub config: Config,
    /// Set when the on-disk file could not be used and defaults were taken
    /// instead. Human-readable, includes the path and the parse error.
    pub problem: Option<String>,
}

/// Parse a config file's text. Separate from I/O so the error text is testable.
fn parse(content: &str) -> std::result::Result<Config, toml::de::Error> {
    toml::from_str(content)
}

/// Load config from disk, creating defaults if the file doesn't exist.
///
/// Never fails the daemon: a hand-edited or truncated `daemon.toml` used to
/// abort startup, and with `Restart=on-failure` that became an endless crash
/// loop the app could only see as "daemon not installed". Now a file that does
/// not parse is moved aside as `daemon.toml.bad-<unix-ts>` (so the user's edits
/// are not lost), an ERROR naming the parse error is logged, and the daemon
/// continues on defaults.
pub fn load() -> Loaded {
    load_from(&config_path())
}

fn load_from(path: &Path) -> Loaded {
    if !path.exists() {
        let config = Config::default();
        if let Err(e) = save_to(path, &config) {
            tracing::warn!("Could not write default config: {e:#}");
        }
        return Loaded { config, problem: None };
    }

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            // Unreadable (permissions, I/O error): do not touch the file.
            let problem = format!(
                "cannot read {}: {e} - running on built-in defaults",
                path.display()
            );
            tracing::error!("{problem}");
            return Loaded { config: Config::default(), problem: Some(problem) };
        }
    };

    match parse(&content) {
        Ok(config) => Loaded { config, problem: None },
        Err(e) => {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let mut bad = path.as_os_str().to_owned();
            bad.push(format!(".bad-{ts}"));
            let bad = PathBuf::from(bad);
            let moved = match std::fs::rename(path, &bad) {
                Ok(()) => format!("kept as {}", bad.display()),
                Err(re) => format!("could not be moved aside ({re})"),
            };
            let problem = format!(
                "{} is not valid TOML: {} - using built-in defaults; the bad file was {moved}",
                path.display(),
                e.to_string().trim()
            );
            tracing::error!("{problem}");
            let config = Config::default();
            if let Err(we) = save_to(path, &config) {
                tracing::warn!("Could not rewrite default config: {we:#}");
            }
            Loaded { config, problem: Some(problem) }
        }
    }
}

/// Read the config without any side effect (no quarantine, no default file).
/// For `--diagnose`, which must be safe to run next to a live daemon.
pub fn peek() -> std::result::Result<Config, String> {
    let path = config_path();
    if !path.exists() {
        return Ok(Config::default());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    parse(&content).map_err(|e| format!("{}: {}", path.display(), e.to_string().trim()))
}

/// Save config to disk.
#[allow(dead_code)]
pub fn save(config: &Config) -> Result<()> {
    save_to(&config_path(), config)
}

/// Write atomically: serialize to a sibling temp file, fsync, rename over the
/// target. A crash or full disk mid-write can then never leave a truncated
/// `daemon.toml` behind (which is exactly what used to produce the corrupt file
/// that crash-looped the daemon).
fn save_to(path: &Path, config: &Config) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let content = toml::to_string_pretty(config).context("Failed to serialize config")?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)
            .with_context(|| format!("Failed to create {}", tmp.display()))?;
        f.write_all(content.as_bytes())
            .with_context(|| format!("Failed to write {}", tmp.display()))?;
        f.sync_all().ok();
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        anyhow::anyhow!("Failed to move {} into place at {}: {e}", tmp.display(), path.display())
    })?;
    tracing::info!("Config saved to {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("openggd-cfg-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_writes_defaults_atomically() {
        let dir = scratch("missing");
        let path = dir.join("daemon.toml");
        let loaded = load_from(&path);
        assert!(loaded.problem.is_none());
        assert!(path.exists());
        assert!(!dir.join("daemon.toml.tmp").exists(), "temp file must be renamed away");
        // And what was written round-trips.
        assert!(parse(&std::fs::read_to_string(&path).unwrap()).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_is_quarantined_and_defaults_used() {
        let dir = scratch("corrupt");
        let path = dir.join("daemon.toml");
        std::fs::write(&path, "[modules\naudio_enabled = tru").unwrap();
        let loaded = load_from(&path);
        let problem = loaded.problem.expect("a parse failure must be reported");
        assert!(problem.contains("daemon.toml") && problem.contains("not valid TOML"), "{problem}");
        assert!(loaded.config.modules.audio_enabled);
        let bad: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".bad-"))
            .collect();
        assert_eq!(bad.len(), 1, "the user's file must be kept");
        assert!(std::fs::read_to_string(bad[0].path()).unwrap().contains("tru"));
        // A fresh, valid default file replaced it.
        assert!(parse(&std::fs::read_to_string(&path).unwrap()).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn valid_partial_file_loads_without_problem() {
        let dir = scratch("partial");
        let path = dir.join("daemon.toml");
        std::fs::write(&path, "[modules]\naudio_enabled = false\n").unwrap();
        let loaded = load_from(&path);
        assert!(loaded.problem.is_none());
        assert!(!loaded.config.modules.audio_enabled);
        assert!(loaded.config.modules.device_enabled);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

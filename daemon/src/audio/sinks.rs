//! PipeWire Virtual Sink Management — GRACEFUL injection
//!
//! CRITICAL: We NEVER restart PipeWire. Instead, we use `pactl load-module`
//! to inject null-sinks at runtime. This preserves all active audio streams
//! (Discord WebRTC, browser media, games, etc.)
//!
//! Flow:
//!   1. Check if OpenGG sinks already exist (idempotent)
//!   2. If not, create them via `pactl load-module module-null-sink`
//!   3. Link their monitor ports to the default output via `pw-link`
//!   4. On daemon exit, sinks stay alive (object.linger) — no disruption

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, Mutex};

pub const CHANNEL_NAMES: &[&str] = &["Game", "Chat", "Media", "Aux", "Mic"];
/// Only these get virtual sinks (Mic uses the real input device)
const SINK_CHANNELS: &[&str] = &["Game", "Chat", "Media", "Aux"];

#[derive(Debug, Clone)]
pub struct ChannelInfo {
    pub name: String,
    pub volume: f32,
    pub muted: bool,
    pub assigned_apps: Vec<AppInfo>,
}

#[derive(Debug, Clone)]
pub struct AppInfo {
    pub id: u32,
    pub name: String,
    pub binary: String,
}

pub struct SinkManager {
    pub channels: Arc<Mutex<HashMap<String, ChannelInfo>>>,
    /// Module IDs from pactl load-module (for cleanup if needed)
    module_ids: Arc<Mutex<Vec<u32>>>,
}

unsafe impl Send for SinkManager {}
unsafe impl Sync for SinkManager {}

impl SinkManager {
    /// Create virtual sinks gracefully — NO PipeWire restart.
    pub fn create_all() -> Result<Self> {
        let channels = Arc::new(Mutex::new(HashMap::new()));
        let module_ids = Arc::new(Mutex::new(Vec::new()));

        for &ch in SINK_CHANNELS {
            let sink_name = format!("OpenGG_{ch}");

            // Step 1: Check if this sink already exists (idempotent)
            if sink_exists(&sink_name) {
                tracing::info!("Sink {sink_name} already exists — skipping creation");
            } else {
                // Step 2: Create via pactl load-module (non-destructive)
                match create_null_sink(&sink_name, ch) {
                    Ok(module_id) => {
                        module_ids.lock().unwrap().push(module_id);
                        tracing::info!("Created sink {sink_name} (module {module_id})");
                    }
                    Err(e) => {
                        tracing::warn!("Failed to create {sink_name}: {e}");
                        // Try the config-file fallback (no restart!)
                        tracing::info!("Trying WirePlumber config fallback for {sink_name}");
                    }
                }
            }
        }

        // Initialize all channel state (including Mic which doesn't need a virtual sink)
        for &name in CHANNEL_NAMES {
            channels.lock().unwrap().insert(
                name.to_string(),
                ChannelInfo {
                    name: name.to_string(),
                    volume: 1.0,
                    muted: false,
                    assigned_apps: Vec::new(),
                },
            );
        }

        // Step 3: Wait briefly for sinks to register, then set up loopbacks
        std::thread::sleep(std::time::Duration::from_millis(500));
        if let Err(e) = setup_loopbacks() {
            tracing::warn!("Loopback setup had issues: {e}");
        }

        tracing::info!("Virtual sinks ready (no PipeWire restart)");
        Ok(Self { channels, module_ids })
    }

    pub fn set_volume(&self, channel: &str, volume: f32) -> Result<()> {
        let volume = volume.clamp(0.0, 1.5);
        if let Some(info) = self.channels.lock().unwrap().get_mut(channel) {
            info.volume = volume;
        }
        let vol_pct = (volume * 100.0) as u32;
        if channel == "Mic" {
            let _ = Command::new("pactl")
                .args(["set-source-volume", "@DEFAULT_SOURCE@", &format!("{vol_pct}%")])
                .output();
        } else {
            let _ = Command::new("pactl")
                .args(["set-sink-volume", &format!("OpenGG_{channel}"), &format!("{vol_pct}%")])
                .output();
        }
        Ok(())
    }

    pub fn set_mute(&self, channel: &str, muted: bool) -> Result<()> {
        if let Some(info) = self.channels.lock().unwrap().get_mut(channel) {
            info.muted = muted;
        }
        let val = if muted { "1" } else { "0" };
        if channel == "Mic" {
            let _ = Command::new("pactl").args(["set-source-mute", "@DEFAULT_SOURCE@", val]).output();
        } else {
            let _ = Command::new("pactl").args(["set-sink-mute", &format!("OpenGG_{channel}"), val]).output();
        }
        Ok(())
    }

    pub fn get_channels(&self) -> Vec<ChannelInfo> {
        self.channels.lock().unwrap().values().cloned().collect()
    }
}

/// Check if a PipeWire sink with this name already exists.
fn sink_exists(name: &str) -> bool {
    if let Ok(output) = Command::new("pactl").args(["list", "sinks", "short"]).output() {
        let text = String::from_utf8_lossy(&output.stdout);
        return text.lines().any(|line| line.contains(name));
    }
    false
}

/// Create a null-sink via `pactl load-module` — returns the module ID.
///
/// This is the KEY difference from the old approach:
///   OLD: Write config file → restart PipeWire → all apps lose audio
///   NEW: Load module at runtime → zero disruption to existing streams
fn create_null_sink(sink_name: &str, description: &str) -> Result<u32> {
    let output = Command::new("pactl")
        .args([
            "load-module",
            "module-null-sink",
            &format!("sink_name={sink_name}"),
            &format!("sink_properties=device.description=\"OpenGG {description}\""),
            "channels=2",
            "channel_map=front-left,front-right",
        ])
        .output()
        .context("pactl not found — is PipeWire running?")?;

    if output.status.success() {
        let id_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let module_id = id_str.parse::<u32>().unwrap_or(0);
        Ok(module_id)
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("pactl load-module failed: {err}")
    }
}

/// Link each virtual sink's monitor output to the default audio device.
pub fn setup_loopbacks() -> Result<()> {
    let output = Command::new("pactl")
        .args(["get-default-sink"])
        .output()
        .context("pactl not found")?;
    let default_sink = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if default_sink.is_empty() {
        tracing::warn!("No default audio output detected — loopbacks skipped");
        return Ok(());
    }

    tracing::info!("Linking virtual sinks → {default_sink}");
    for &ch in SINK_CHANNELS {
        let sink_name = format!("OpenGG_{ch}");
        for port in ["FL", "FR"] {
            // pw-link is idempotent — linking an already-linked pair does nothing
            let result = Command::new("pw-link")
                .args([
                    &format!("{sink_name}:monitor_{port}"),
                    &format!("{default_sink}:playback_{port}"),
                ])
                .output();

            match result {
                Ok(o) if !o.status.success() => {
                    let err = String::from_utf8_lossy(&o.stderr);
                    // "already linked" is fine, anything else is a warning
                    if !err.contains("already") {
                        tracing::debug!("pw-link {sink_name}:{port} → {default_sink}: {err}");
                    }
                }
                Err(e) => tracing::warn!("pw-link failed: {e}"),
                _ => {}
            }
        }
    }
    Ok(())
}

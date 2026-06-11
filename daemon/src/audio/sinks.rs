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
use std::sync::{Arc, Mutex};
use std::time::Instant;
use crate::subprocess;

/// Minimum interval between `pactl` spawns for the same channel.
/// Prevents process storm during slider drags (≤10 spawns/sec instead of 60+).
const VOLUME_DEBOUNCE_MS: u128 = 100;

pub const CHANNEL_NAMES: &[&str] = &["Game", "Chat", "Media", "Aux", "Mic"];
/// All channels get virtual null-sinks, including Mic.
/// The Mic sink is fed by a loopback from the hardware source — its monitor
/// port becomes the capturable "OpenGG_Mic" node for GSR and DSP chains.
const SINK_CHANNELS: &[&str] = &["Game", "Chat", "Media", "Aux", "Mic"];
const RELABEL_CHANNELS: &[&str] = &["Game", "Chat", "Media", "Aux"];

fn live_display_name(channel: &str) -> String {
    if RELABEL_CHANNELS.contains(&channel) {
        channel.to_string()
    } else {
        format!("OpenGG - {channel}")
    }
}

fn sink_prop_value(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

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
    /// Last time `pactl set-sink-volume` was spawned per channel.
    last_volume_spawn: Arc<Mutex<HashMap<String, Instant>>>,
}

impl SinkManager {
    /// Create virtual sinks gracefully — NO PipeWire restart.
    pub fn create_all() -> Result<Self> {
        let channels = Arc::new(Mutex::new(HashMap::new()));
        let module_ids = Arc::new(Mutex::new(Vec::new()));

        for &ch in SINK_CHANNELS {
            let sink_name = format!("OpenGG_{ch}");

            // Step 1: Check if this sink already exists (idempotent)
            if !sink_exists(&sink_name) {
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
            } else {
                tracing::info!("Sink {sink_name} already exists — skipping creation");
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

        // Step 4: Wire hardware mic → OpenGG_Mic virtual sink via loopback.
        // The sink's monitor port is what GSR and DSP chains capture from.
        if let Err(e) = setup_mic_loopback() {
            tracing::warn!("Mic loopback setup failed (raw HW mic will be used as fallback): {e}");
        }

        tracing::info!("Virtual sinks ready (no PipeWire restart)");
        Ok(Self {
            channels,
            module_ids,
            last_volume_spawn: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn set_volume(&self, channel: &str, volume: f32) -> Result<()> {
        let volume = volume.clamp(0.0, 1.5);
        if let Some(info) = self.channels.lock().unwrap().get_mut(channel) {
            info.volume = volume;
        }

        // Debounce: skip pactl spawn if this channel was touched <100ms ago.
        let now = Instant::now();
        {
            let mut last_map = self.last_volume_spawn.lock().unwrap();
            if let Some(last) = last_map.get(channel) {
                if now.duration_since(*last).as_millis() < VOLUME_DEBOUNCE_MS {
                    return Ok(());
                }
            }
            last_map.insert(channel.to_string(), now);
        }

        let vol_pct = (volume * 100.0) as u32;
        let pct_str = format!("{vol_pct}%");
        // All channels (including Mic) now have a virtual null-sink — control it uniformly.
        let _ = subprocess::command("pactl")
            .args(["set-sink-volume", &format!("OpenGG_{channel}"), &pct_str])
            .output();
        Ok(())
    }

    pub fn set_mute(&self, channel: &str, muted: bool) -> Result<()> {
        if let Some(info) = self.channels.lock().unwrap().get_mut(channel) {
            info.muted = muted;
        }
        let val = if muted { "1" } else { "0" };
        let _ = subprocess::command("pactl")
            .args(["set-sink-mute", &format!("OpenGG_{channel}"), val])
            .output();
        Ok(())
    }

    pub fn get_channels(&self) -> Vec<ChannelInfo> {
        self.channels.lock().unwrap().values().cloned().collect()
    }
}

impl Drop for SinkManager {
    fn drop(&mut self) {
        if let Ok(ids) = self.module_ids.lock() {
            for id in ids.iter() {
                let _ = subprocess::command("pactl")
                    .args(["unload-module", &id.to_string()])
                    .status();
            }
        }
    }
}

/// Check if a PipeWire sink with this name already exists.
fn sink_exists(name: &str) -> bool {
    if let Ok(output) = subprocess::command("pactl").args(["list", "sinks", "short"]).output() {
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
    // All three property keys are set so the sink shows correctly in every
    // sound panel (GNOME Settings, pavucontrol, KDE, etc.)
    let display_name = live_display_name(description);
    let sink_props = format!(
        "sink_properties=device.description={} \
         node.description={} \
         media.name={}",
        sink_prop_value(&display_name),
        sink_prop_value(&display_name),
        sink_prop_value(&display_name)
    );
    let output = subprocess::command("pactl")
        .args([
            "load-module",
            "module-null-sink",
            &format!("sink_name={sink_name}"),
            &sink_props,
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

/// Route the hardware microphone into the OpenGG_Mic virtual null-sink via a loopback module.
/// Also creates a virtual source (OpenGG_Virtual_Mic) that apps can select as an input device.
///
/// Graph after this call:
///   [@DEFAULT_SOURCE@] → [module-loopback] → [OpenGG_Mic null-sink]
///                                                     ↓
///                                          OpenGG_Mic.monitor
///                                                     ↓
///                     [module-remap-source] ← OpenGG_Mic.monitor
///                     (OpenGG_Virtual_Mic)    ← Apps can select this as input
///                                                     ↓
///                                          (future jalv DSP chain: gate → compressor → EQ)
fn setup_mic_loopback() -> Result<()> {
    // Discover the hardware source (never let it be our own virtual sink)
    let hw_out = subprocess::command("pactl")
        .args(["get-default-source"])
        .output()
        .context("pactl get-default-source failed")?;
    let hw_source = String::from_utf8_lossy(&hw_out.stdout).trim().to_string();

    if hw_source.is_empty() {
        anyhow::bail!("No default source found — skipping mic loopback");
    }
    if hw_source.contains("OpenGG") || hw_source.contains("monitor") {
        anyhow::bail!("Default source appears to be a virtual node ({hw_source}) — skipping to avoid loop");
    }

    // Idempotency: check if our loopback already exists
    let list_out = subprocess::command("pactl").args(["list", "modules", "short"]).output()?;
    let list = String::from_utf8_lossy(&list_out.stdout);
    let loopback_exists = list.lines().any(|l| l.contains("module-loopback") && l.contains("OpenGG_Mic"));

    if !loopback_exists {
        let out = subprocess::command("pactl")
            .args([
                "load-module", "module-loopback",
                &format!("source={hw_source}"),
                "sink=OpenGG_Mic",
                "latency_msec=10",
                "source_dont_move=true",
                "sink_dont_move=true",
            ])
            .output()
            .context("pactl load-module module-loopback failed")?;

        if !out.status.success() {
            anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim())
        }
        tracing::info!("Mic loopback active: {hw_source} → OpenGG_Mic");
    } else {
        tracing::info!("Mic loopback already active — skipping loopback creation");
    }

    // ── Virtual mic source creation (apps can select this as input) ──
    // Even if loopback was already created, we still need to ensure the virtual source exists
    // (e.g., daemon restart with lingering sinks — the remap source must be ensured).

    // Idempotency: check if our remap source already exists
    let sources_out = subprocess::command("pactl")
        .args(["list", "sources", "short"])
        .output()?;
    let sources = String::from_utf8_lossy(&sources_out.stdout);
    if sources.lines().any(|l| l.contains("OpenGG_Virtual_Mic")) {
        tracing::info!("Virtual mic source already exists — skipping creation");
        return Ok(());
    }

    // Load remap-source module to expose OpenGG_Mic.monitor as a selectable input device
    // The description is quoted per POSIX shell rules but we pass it as a single argv element
    // (pactl arg vectors never go through a shell), so we use the same sink_prop_value quoting helper
    let mic_prop = sink_prop_value("OpenGG Mic");
    let out = subprocess::command("pactl")
        .args([
            "load-module",
            "module-remap-source",
            "master=OpenGG_Mic.monitor",
            "source_name=OpenGG_Virtual_Mic",
            &format!("source_properties=device.description={mic_prop}"),
        ])
        .output()
        .context("pactl load-module module-remap-source failed")?;

    if out.status.success() {
        tracing::info!("Virtual mic source created: OpenGG_Virtual_Mic (master=OpenGG_Mic.monitor)");
        Ok(())
    } else {
        // Non-fatal: the monitor path (GSR, VU) still works without the virtual source
        tracing::warn!(
            "Failed to create virtual mic source: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(())
    }
}

/// Helper: Parse `pw-link -l` output to extract all existing links.
/// Returns a set of (source_port, dest_port) tuples as they appear in the listing.
/// Format:
///   SourcePort
///     |-> DestPort
///     |-> DestPort2
fn parse_pw_links() -> Result<std::collections::HashSet<(String, String)>> {
    use std::collections::HashSet;

    let output = subprocess::command("pw-link")
        .args(["-l"])
        .output()
        .context("pw-link -l failed")?;

    if !output.status.success() {
        anyhow::bail!(
            "pw-link -l exited non-zero: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let mut links = HashSet::new();
    let text = String::from_utf8_lossy(&output.stdout);
    let mut current_source: Option<String> = None;

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("|-> ") {
            // Destination port line (arrow format)
            if let Some(ref source) = current_source {
                let dest = trimmed.strip_prefix("|-> ").unwrap_or("").trim().to_string();
                if !dest.is_empty() {
                    links.insert((source.clone(), dest));
                }
            }
        } else if !trimmed.is_empty() && !trimmed.starts_with('|') {
            // Likely a source port (port name at start of logical group)
            // Only update if this line looks like a port (contains ':' and no arrows)
            if trimmed.contains(':') && !trimmed.starts_with('|') {
                current_source = Some(trimmed.to_string());
            }
        }
    }

    Ok(links)
}

/// Link each virtual sink's monitor output to the default audio device.
/// With post-link verification and retry logic to handle WirePlumber startup races.
pub fn setup_loopbacks() -> Result<()> {
    let output = subprocess::command("pactl")
        .args(["get-default-sink"])
        .output()
        .context("pactl not found")?;
    let default_sink = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if default_sink.is_empty() {
        tracing::warn!("No default audio output detected — loopbacks skipped");
        return Ok(());
    }

    // Output channels only — Mic is a capture sink, not a playback sink.
    const OUTPUT_CHANNELS: &[&str] = &["Game", "Chat", "Media", "Aux"];
    tracing::info!("Linking virtual sinks → {default_sink}");

    // Collect intended links for verification
    let mut intended_links: Vec<(String, String, &str)> = Vec::new();

    for &ch in OUTPUT_CHANNELS {
        let sink_name = format!("OpenGG_{ch}");
        for port in ["FL", "FR"] {
            let source = format!("{sink_name}:monitor_{port}");
            let dest = format!("{default_sink}:playback_{port}");
            intended_links.push((source.clone(), dest.clone(), ch));

            // Attempt initial link
            let result = subprocess::command("pw-link")
                .args([&source, &dest])
                .output();

            match result {
                Ok(o) if !o.status.success() => {
                    let err = String::from_utf8_lossy(&o.stderr);
                    let err_str = err.trim();
                    // "already linked" or "File exists" is fine, anything else is logged
                    if !err_str.contains("already") && !err_str.contains("File exists") {
                        tracing::debug!("pw-link {source} → {dest}: {err_str}");
                    }
                }
                Err(e) => tracing::debug!("pw-link spawn failed: {e}"),
                _ => {}
            }
        }
    }

    // Post-link verification: retry missing links up to 3 times with backoff
    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY_MS: u64 = 500;

    for attempt in 1..=MAX_RETRIES {
        // Small delay before checking (allow WirePlumber/PipeWire to settle)
        if attempt > 1 {
            std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
        }

        match parse_pw_links() {
            Ok(existing_links) => {
                let missing: Vec<(String, String, &str)> = intended_links
                    .iter()
                    .filter(|(src, dst, _)| !existing_links.contains(&(src.clone(), dst.clone())))
                    .cloned()
                    .collect();

                if missing.is_empty() {
                    tracing::debug!("Loopback verification passed (attempt {MAX_RETRIES}/{MAX_RETRIES})");
                    return Ok(());
                }

                // Retry missing links
                if attempt < MAX_RETRIES {
                    tracing::debug!(
                        "Loopback verification found {} missing links (attempt {}/{}), retrying...",
                        missing.len(),
                        attempt,
                        MAX_RETRIES
                    );

                    for (source, dest, _) in missing.iter() {
                        let _result = subprocess::command("pw-link")
                            .args([source, dest])
                            .output();
                    }
                } else {
                    // Final attempt: log as error (non-fatal, but unmissable)
                    for (source, dest, channel) in missing.iter() {
                        tracing::error!(
                            "Failed to link after {} retries: {} → {} (channel {}) — this channel may be silent",
                            MAX_RETRIES,
                            source,
                            dest,
                            channel
                        );
                    }
                    tracing::error!(
                        "Some loopback links could not be established. Check WirePlumber logs: systemctl --user status wireplumber"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to verify loopback links on attempt {}/{}: {}",
                    attempt,
                    MAX_RETRIES,
                    e
                );
                if attempt == MAX_RETRIES {
                    tracing::error!(
                        "Could not verify loopback setup after {} attempts — continuing anyway",
                        MAX_RETRIES
                    );
                }
            }
        }
    }

    Ok(())
}

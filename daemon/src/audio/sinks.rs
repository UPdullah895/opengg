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
use std::env;
use std::path::PathBuf;
use crate::subprocess;

/// Minimum interval between `pactl` spawns for the same channel.
/// Prevents process storm during slider drags (≤10 spawns/sec instead of 60+).
const VOLUME_DEBOUNCE_MS: u128 = 100;

pub const CHANNEL_NAMES: &[&str] = &["Game", "Chat", "Media", "Aux", "Mic"];
/// All channels get virtual null-sinks, including Mic.
/// The Mic sink is fed by a loopback from the hardware source — its monitor
/// port becomes the capturable "OpenGG_Mic" node for GSR and DSP chains.
const SINK_CHANNELS: &[&str] = &["Game", "Chat", "Media", "Aux", "Mic"];
const RELABEL_CHANNELS: &[&str] = &["Game", "Chat", "Media", "Aux", "Mic"];

fn remove_legacy_sink_config() {
    let cfg_home = env::var("XDG_CONFIG_HOME")
        .ok()
        .or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".config").to_string_lossy().to_string())
        })
        .unwrap_or_default();

    let pw_dir = PathBuf::from(&cfg_home).join("pipewire/pipewire.conf.d");

    if let Ok(entries) = std::fs::read_dir(&pw_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name == "opengg-sinks.conf" || name.starts_with("opengg-") && name.ends_with(".conf") {
                    if let Err(e) = std::fs::remove_file(&path) {
                        tracing::debug!("Failed to delete {}: {}", path.display(), e);
                    } else {
                        tracing::info!("Deleted legacy config: {}", path.display());
                    }
                }
            }
        }
    }
}

fn list_opengg_node_ids() -> Vec<u32> {
    let mut ids = Vec::new();

    // Scan sinks
    if let Ok(out) = subprocess::command("pactl")
        .args(["list", "sinks", "short"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts[1];
                if name.starts_with("OpenGG_") && !name.contains(".monitor") {
                    if let Ok(id) = parts[0].parse::<u32>() {
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                    }
                }
            }
        }
    }

    // Scan sources
    if let Ok(out) = subprocess::command("pactl")
        .args(["list", "sources", "short"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts[1];
                if name.starts_with("OpenGG_") && !name.contains(".monitor") {
                    if let Ok(id) = parts[0].parse::<u32>() {
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                    }
                }
            }
        }
    }

    ids
}

fn destroy_node(id: u32) {
    let _ = subprocess::command("pw-cli")
        .args(["destroy", &id.to_string()])
        .status();
}

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
        remove_legacy_sink_config();

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
        match setup_mic_loopback() {
            Ok(mic_module_ids) => {
                for id in mic_module_ids {
                    module_ids.lock().unwrap().push(id);
                }
            }
            Err(e) => {
                tracing::warn!("Mic loopback setup failed (raw HW mic will be used as fallback): {e}");
            }
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
        // Master has no virtual sink — it controls the hardware default output directly.
        let sink_name = if channel == "Master" { "@DEFAULT_SINK@".to_string() } else { format!("OpenGG_{channel}") };
        let _ = subprocess::command("pactl")
            .args(["set-sink-volume", &sink_name, &pct_str])
            .output();
        Ok(())
    }

    pub fn set_mute(&self, channel: &str, muted: bool) -> Result<()> {
        if let Some(info) = self.channels.lock().unwrap().get_mut(channel) {
            info.muted = muted;
        }
        let val = if muted { "1" } else { "0" };
        let sink_name = if channel == "Master" { "@DEFAULT_SINK@".to_string() } else { format!("OpenGG_{channel}") };
        let _ = subprocess::command("pactl")
            .args(["set-sink-mute", &sink_name, val])
            .output();
        Ok(())
    }

    /// Return channel state with **live** volume/mute read from pactl, so the mixer
    /// always reflects reality (including external changes via pavucontrol) instead of a
    /// stale in-memory default. The cached in-memory value is the fallback when a sink
    /// isn't found (e.g. before creation or right after a restart).
    pub fn get_channels(&self) -> Vec<ChannelInfo> {
        let live = read_live_sink_state();
        let default_sink = get_default_sink_name();
        let mut channels: Vec<ChannelInfo> = self.channels.lock().unwrap().values().cloned().collect();
        for ch in &mut channels {
            // Master controls the hardware default output directly; everything else is OpenGG_{name}.
            let sink_name = if ch.name == "Master" {
                default_sink.clone()
            } else {
                Some(format!("OpenGG_{}", ch.name))
            };
            if let Some(name) = sink_name {
                if let Some(&(vol, muted)) = live.get(&name) {
                    ch.volume = vol;
                    ch.muted = muted;
                }
            }
        }
        channels
    }
}

/// Resolve the current default sink's node name via `pactl get-default-sink`.
fn get_default_sink_name() -> Option<String> {
    let out = subprocess::command("pactl")
        .args(["get-default-sink"])
        .output()
        .ok()?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if name.is_empty() { None } else { Some(name) }
}

/// Parse one `pactl list sinks` pass into name → (volume 0.0–1.5, muted).
/// Volume is taken from the first channel's percentage; mute from the `Mute:` line.
fn read_live_sink_state() -> HashMap<String, (f32, bool)> {
    let mut map = HashMap::new();
    let out = match subprocess::command("pactl").args(["list", "sinks"]).output() {
        Ok(o) => o,
        Err(_) => return map,
    };
    let text = String::from_utf8_lossy(&out.stdout);

    let mut cur_name: Option<String> = None;
    let mut cur_muted = false;
    let mut cur_vol: Option<f32> = None;
    let flush = |name: &mut Option<String>, vol: &mut Option<f32>, muted: &mut bool, map: &mut HashMap<String, (f32, bool)>| {
        if let (Some(n), Some(v)) = (name.take(), vol.take()) {
            map.insert(n, (v, *muted));
        }
        *muted = false;
    };

    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Name: ") {
            // New block starts at "Name:" — flush the previous one first.
            flush(&mut cur_name, &mut cur_vol, &mut cur_muted, &mut map);
            cur_name = Some(rest.trim().to_string());
        } else if let Some(rest) = t.strip_prefix("Mute: ") {
            cur_muted = rest.trim() == "yes";
        } else if t.starts_with("Volume:") && cur_vol.is_none() {
            // e.g. "Volume: front-left: 39322 /  60% / -13.40 dB,  front-right: ..."
            if let Some(pct) = t.split('%').next().and_then(|s| s.rsplit('/').next()).and_then(|s| s.trim().parse::<f32>().ok()) {
                cur_vol = Some((pct / 100.0).clamp(0.0, 1.5));
            }
        }
    }
    flush(&mut cur_name, &mut cur_vol, &mut cur_muted, &mut map);
    map
}

impl SinkManager {
    /// Teardown all virtual audio sinks and sources.
    ///
    /// This method uses ordered, idempotent steps:
    /// 1. Delete legacy config file (if present)
    /// 2. Unload tracked module IDs
    /// 3. Generalized straggler scan — unload any pactl module whose args contain "OpenGG_"
    /// 4. Enumerate live OpenGG nodes (by id) and destroy via pw-cli
    /// 5. Bounded retry (up to 4 passes): repeat steps 3–4 until clean
    /// 6. Restore OS defaults
    /// 7. Verify no OpenGG nodes remain
    pub fn teardown_all(&self) -> anyhow::Result<()> {
        // Step 1: Remove legacy config file
        remove_legacy_sink_config();

        // Step 2: Unload all tracked module IDs
        if let Ok(mut ids) = self.module_ids.lock() {
            for id in ids.drain(..) {
                let _ = subprocess::command("pactl")
                    .args(["unload-module", &id.to_string()])
                    .status();
            }
        }

        // Steps 3–5: Bounded retry loop for straggler modules + node destruction
        const MAX_PASSES: u32 = 4;
        const RETRY_DELAY_MS: u64 = 150;

        for pass in 1..=MAX_PASSES {
            // Step 3: Generalized straggler scan — unload any module with "OpenGG_" in args
            let modules_out = subprocess::command("pactl")
                .args(["list", "modules", "short"])
                .output()
                .context("pactl list modules short failed")?;

            let modules = String::from_utf8_lossy(&modules_out.stdout);
            for line in modules.lines() {
                if line.contains("OpenGG_") {
                    if let Some(id) = line.split_whitespace().next() {
                        let _ = subprocess::command("pactl")
                            .args(["unload-module", id])
                            .status();
                        tracing::debug!("Unloaded straggler module {id}");
                    }
                }
            }

            // Step 4: Enumerate live OpenGG node IDs and destroy them
            let node_ids = list_opengg_node_ids();
            for id in node_ids {
                destroy_node(id);
                tracing::debug!("Destroyed PipeWire node {id}");
            }

            // Check if clean (no more OpenGG nodes)
            let remaining = list_opengg_node_ids();
            let modules_out = subprocess::command("pactl")
                .args(["list", "modules", "short"])
                .output()
                .context("pactl list modules short failed")?;
            let modules = String::from_utf8_lossy(&modules_out.stdout);
            let has_opengg_modules = modules.lines().any(|l| l.contains("OpenGG_"));

            if remaining.is_empty() && !has_opengg_modules {
                tracing::debug!("Teardown: clean graph achieved on pass {pass}");
                break;
            }

            if pass < MAX_PASSES {
                std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
            }
        }

        // Step 6: Restore OS defaults
        let sinks_out = subprocess::command("pactl")
            .args(["list", "sinks", "short"])
            .output()
            .context("pactl list sinks short failed")?;

        let sinks = String::from_utf8_lossy(&sinks_out.stdout);
        if let Some(first_real_sink) = sinks
            .lines()
            .find(|line| !line.contains("OpenGG_"))
            .and_then(|line| line.split_whitespace().nth(1))
        {
            let _ = subprocess::command("pactl")
                .args(["set-default-sink", first_real_sink])
                .output();
            tracing::info!("Restored default sink: {first_real_sink}");
        } else {
            tracing::warn!("No non-OpenGG sinks found to restore as default");
        }

        let sources_out = subprocess::command("pactl")
            .args(["list", "sources", "short"])
            .output()
            .context("pactl list sources short failed")?;

        let sources = String::from_utf8_lossy(&sources_out.stdout);
        if let Some(first_real_source) = sources
            .lines()
            .find(|line| !line.contains("OpenGG_") && !line.contains(".monitor"))
            .and_then(|line| line.split_whitespace().nth(1))
        {
            let _ = subprocess::command("pactl")
                .args(["set-default-source", first_real_source])
                .output();
            tracing::info!("Restored default source: {first_real_source}");
        } else {
            tracing::warn!("No non-OpenGG non-monitor sources found to restore as default");
        }

        // Step 7: Verify no OpenGG sinks/sources remain
        let final_sinks_out = subprocess::command("pactl")
            .args(["list", "sinks", "short"])
            .output()
            .context("pactl list sinks short (verification) failed")?;

        let final_sinks = String::from_utf8_lossy(&final_sinks_out.stdout);
        let stragglers: Vec<&str> = final_sinks
            .lines()
            .filter(|line| line.contains("OpenGG_"))
            .collect();

        if !stragglers.is_empty() {
            let straggler_list = stragglers.join("; ");
            tracing::error!("Verification failed: OpenGG sinks still present after retries: {straggler_list}");
            return Err(anyhow::anyhow!(
                "Teardown incomplete — {} sink(s) still present (config deleted; restart may clear them)",
                stragglers.len()
            ));
        }

        tracing::info!("Virtual audio teardown complete — all OpenGG sinks removed");
        Ok(())
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
/// Returns the module IDs (loopback + remap-source) for lifecycle tracking.
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
fn setup_mic_loopback() -> Result<Vec<u32>> {
    let mut module_ids = Vec::new();

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
                // Name the two loopback streams instead of the default PID-based
                // "loopback-<pid>-<n>" so they are identifiable in the graph and
                // consistent with the other OpenGG nodes. (The module overrides
                // node.description, but node.name applies — verified via pw-dump.)
                "source_output_properties=node.name=OpenGG_Mic_Loopback_in media.name=OpenGG_Mic_Loopback",
                "sink_input_properties=node.name=OpenGG_Mic_Loopback_out media.name=OpenGG_Mic_Loopback",
            ])
            .output()
            .context("pactl load-module module-loopback failed")?;

        if !out.status.success() {
            anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim())
        }

        let id_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Ok(module_id) = id_str.parse::<u32>() {
            module_ids.push(module_id);
            tracing::info!("Mic loopback active: {hw_source} → OpenGG_Mic (module {module_id})");
        } else {
            tracing::warn!("Could not parse loopback module ID, assuming already created");
        }
    } else {
        tracing::info!("Mic loopback already active — skipping loopback creation");
    }

    // ── Virtual mic source creation (apps can select this as input) ──
    // Even if loopback was already created, we still need to ensure the virtual source exists
    // (e.g., daemon restart with lingering sinks — the remap source must be ensured).

    // The remap source is only meaningful when its master exists. Loading
    // module-remap-source with a missing master "succeeds" but silently
    // binds to the DEFAULT device while keeping the declared args — the
    // virtual mic then carries desktop audio. Hard requirement check:
    let sources_out = subprocess::command("pactl")
        .args(["list", "sources", "short"])
        .output()?;
    let sources = String::from_utf8_lossy(&sources_out.stdout);
    if !sources.lines().any(|l| l.contains("OpenGG_Mic.monitor")) {
        anyhow::bail!("OpenGG_Mic.monitor source missing — not creating virtual mic (would bind to default)");
    }

    // Idempotency + self-healing: if the remap source already exists, verify
    // it is actually FED by OpenGG_Mic:monitor_*. The declared module args
    // cannot be trusted (see above) — only the live link topology can. A
    // stale remap (e.g. it outlived its master across a daemon restart and
    // re-bound to the default sink monitor) is unloaded and recreated.
    if sources.lines().any(|l| l.contains("OpenGG_Virtual_Mic")) {
        if virtual_mic_wired_correctly() {
            tracing::info!("Virtual mic source already exists and is wired to OpenGG_Mic — skipping");
            return Ok(module_ids);
        }
        tracing::warn!("Virtual mic source exists but is wired to the wrong master — rebuilding");
        let modules_out = subprocess::command("pactl").args(["list", "modules", "short"]).output()?;
        let modules = String::from_utf8_lossy(&modules_out.stdout);
        for line in modules.lines() {
            if line.contains("module-remap-source") && line.contains("OpenGG_Virtual_Mic") {
                if let Some(id) = line.split_whitespace().next() {
                    let _ = subprocess::command("pactl").args(["unload-module", id]).status();
                    tracing::info!("Unloaded stale virtual mic remap module {id}");
                }
            }
        }
    }

    // Load remap-source module to expose OpenGG_Mic.monitor as a selectable input device.
    // pactl's remap-source proplist parser truncates the description at the first space and
    // ignores simple `"..."`/`'...'`/escaped-space quoting (it produced just "OpenGG"). Only a
    // doubly-quoted form survives — the whole proplist quoted AND the value quoted:
    //   source_properties="device.description=\"OpenGG Mic\""   (verified via pw-dump)
    let mic_desc_arg =
        format!("source_properties=\"device.description=\\\"{}\\\"\"", "OpenGG Mic");
    let out = subprocess::command("pactl")
        .args([
            "load-module",
            "module-remap-source",
            "master=OpenGG_Mic.monitor",
            "source_name=OpenGG_Virtual_Mic",
            &mic_desc_arg,
        ])
        .output()
        .context("pactl load-module module-remap-source failed")?;

    if out.status.success() {
        let id_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Ok(module_id) = id_str.parse::<u32>() {
            module_ids.push(module_id);
            tracing::info!("Virtual mic source created: OpenGG_Virtual_Mic (module {module_id})");
        } else {
            tracing::warn!("Could not parse remap-source module ID");
        }
        Ok(module_ids)
    } else {
        // Non-fatal: the monitor path (GSR, VU) still works without the virtual source
        tracing::warn!(
            "Failed to create virtual mic source: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(module_ids)
    }
}

/// True when the OpenGG_Virtual_Mic remap node is fed by OpenGG_Mic's
/// monitor ports in the live graph. The remap module's declared args can't
/// be trusted (a missing master at load time silently re-binds to the
/// default device), so only the link topology is authoritative.
fn virtual_mic_wired_correctly() -> bool {
    match parse_pw_links() {
        Ok(links) => links.iter().any(|(src, dst)| {
            dst.contains("OpenGG_Virtual_Mic") && src.starts_with("OpenGG_Mic:monitor")
        }),
        Err(e) => {
            tracing::warn!("Could not verify virtual mic wiring ({e}) — assuming correct");
            true
        }
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

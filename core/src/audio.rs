//! Audio business logic (Tauri-free, blocking) — moved verbatim from
//! `frontend/src-tauri/src/commands/audio.rs`. The Tauri host keeps thin
//! `#[command]` wrappers (async, via `spawn_blocking`) that delegate here; the
//! Qt/QML shell calls these functions directly. Per the core D-Bus decision,
//! daemon calls use the blocking `crate::daemon` client and pactl/pw calls use
//! the blocking `crate::subprocess::run_cmd_sync`.
//!
//! Deferred to the Tauri host (AppHandle/State/Emitter- and libpulse-coupled):
//! VU-meter streaming, per-app routing (`route_app` + `Router`), and the
//! ear-blast protection loop. The four helpers `get_sink_index_by_name`,
//! `live_display_name`, `sink_prop_value`, and `get_linked_device_for_monitor`
//! are `pub` because that Tauri-side code shares them (single definition).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use crate::daemon::{call_dbus, call_dbus_void, AU_IFACE, AU_PATH};
use crate::subprocess::run_cmd_sync;

const ROUTING_BLACKLIST: &[&str] = &[
    "plasmashell", "kwin_wayland", "kwin_x11", "swaync", "sway",
    "xdg-desktop-portal", "xdg-desktop-portal-gnome", "xdg-desktop-portal-kde",
];

const VIRTUAL_CHANNELS: &[&str] = &["Game", "Chat", "Media", "Aux"];

/// Returns true if any OpenGG virtual sink or source is present in PipeWire.

// ═══ Audio Devices ═══
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AudioDevice {
    pub name: String,
    pub description: String,
    pub device_type: String,
    pub is_default: bool,
}

/// A real, selectable capture source for the "Audio Capture Devices" dropdown.
/// `value` is the real PipeWire node.name (what GSR records from); `label` is friendly.
#[derive(Serialize)]
pub struct CaptureSource {
    pub value: String,
    pub label: String,
}

type StreamInfo = (String, String, u32, u32, &'static str);

pub fn get_channels() -> Result<String, String> {
    call_dbus("GetChannels", AU_PATH, AU_IFACE, ())
}
/// Volume control with pactl fallback — controls both sinks and sink-inputs

pub fn set_volume(channel: String, volume: u32) -> Result<(), String> {
    // Try D-Bus first
    if call_dbus_void("SetVolume", AU_PATH, AU_IFACE, (channel.as_str(), volume))
        
        .is_ok()
    {
        return Ok(());
    }
    // Direct pactl fallback
    let pct = format!("{volume}%");
    let sink = format!("OpenGG_{channel}");
    run_cmd_sync("pactl", &["set-sink-volume", &sink, &pct])?;
    Ok(())
}

pub fn set_mute(channel: String, muted: bool) -> Result<(), String> {
    if call_dbus_void("SetMute", AU_PATH, AU_IFACE, (channel.as_str(), muted))
        
        .is_ok()
    {
        return Ok(());
    }
    let val = if muted { "1" } else { "0" };
    run_cmd_sync("pactl", &["set-sink-mute", &format!("OpenGG_{channel}"), val])?;
    Ok(())
}

/// Unmute any WebKit video/webaudio sink-inputs belonging to this app.
/// Called whenever clip playback starts to counteract module-stream-restore
/// auto-muting the media.role="video" stream.

pub fn unmute_media_streams() -> Result<(), String> {
    let output = run_cmd_sync("pactl", &["list", "sink-inputs"])?;

    let mut current_id: Option<u32> = None;
    let mut is_opengg = false;
    let mut is_media = false;

    for line in output.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Sink Input #") {
            if let Some(id) = current_id {
                if is_opengg && is_media {
                    let _ = run_cmd_sync("pactl", &["set-sink-input-mute", &id.to_string(), "0"]);
                }
            }
            current_id = rest.parse().ok();
            is_opengg = false;
            is_media = false;
        } else if t.contains(r#"application.name = "opengg""#) {
            is_opengg = true;
        } else if t.contains(r#"media.role = "video""#) || t.contains(r#"media.role = "webaudio""#)
        {
            is_media = true;
        }
    }
    // handle last block
    if let Some(id) = current_id {
        if is_opengg && is_media {
            let _ = run_cmd_sync("pactl", &["set-sink-input-mute", &id.to_string(), "0"]);
        }
    }
    Ok(())
}

/// Set volume for an individual app (sink-input) by its pactl index

pub fn set_app_volume(app_index: u32, volume: u32) -> Result<(), String> {
    let pct = format!("{volume}%");
    run_cmd_sync(
        "pactl",
        &["set-sink-input-volume", &app_index.to_string(), &pct],
    )?;
    Ok(())
}
/// System-critical processes that must never be routed. They appear in the
/// app list as "locked" so the user knows they cannot be moved.

pub fn get_apps() -> Result<String, String> {
    match call_dbus::<String>("GetApps", AU_PATH, AU_IFACE, ()) {
        Ok(r) => {
            // Post-process D-Bus result: mark blacklisted system processes as locked
            let mut apps: Vec<serde_json::Value> = serde_json::from_str(&r).unwrap_or_default();
            for app in &mut apps {
                if let Some(binary) = app["binary"].as_str() {
                    if is_blacklisted_binary(binary) {
                        app["locked"] = serde_json::json!(true);
                    }
                }
            }
            Ok(serde_json::to_string(&apps).unwrap_or_else(|_| "[]".into()))
        }
        Err(_) => scan_sink_inputs(),
    }
}

// ══════════════════════════════════════════════════════════════════════
//  ★ AUDIO ROUTING — Ported from Python pulsectl (backend.py line 110)
//
//  Python:   pulse.sink_input_move(si.index, sink.index)
//  Rust:     pactl move-sink-input <si_index> <sink_index>
//
//  BOTH arguments must be PulseAudio INTEGER INDICES.
//  Using a PipeWire node ID or a sink NAME string will silently fail.
// ══════════════════════════════════════════════════════════════════════

/// Structured audio router with explicit fallback strategies.
///
/// Routing attempts, in order:
/// 1. D-Bus daemon (preferred — single source of truth)
/// 2. Direct `pactl move-sink-input` by integer index (with retry)
/// 3. PipeWire node ID → pactl sink-input index cross-reference
/// 4. `pw-metadata target.node` (WirePlumber-native move, with verification)

pub fn get_audio_devices() -> Result<Vec<AudioDevice>, String> {
    let mut d = Vec::new();
    let ds = run_cmd_sync("pactl", &["get-default-sink"]).unwrap_or_default();
    let dr = run_cmd_sync("pactl", &["get-default-source"]).unwrap_or_default();
    if let Ok(o) = run_cmd_sync("pactl", &["-f", "json", "list", "sinks"]) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&o) {
            for s in v.as_array().unwrap_or(&vec![]) {
                let n = s["name"].as_str().unwrap_or("").to_string();
                if n.starts_with("OpenGG_") {
                    continue;
                }
                d.push(AudioDevice {
                    is_default: n == ds,
                    description: s["description"].as_str().unwrap_or(&n).into(),
                    name: n,
                    device_type: "sink".into(),
                });
            }
        }
    }
    if let Ok(o) = run_cmd_sync("pactl", &["-f", "json", "list", "sources"]) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&o) {
            for s in v.as_array().unwrap_or(&vec![]) {
                let n = s["name"].as_str().unwrap_or("").to_string();
                if n.contains(".monitor") {
                    continue;
                }
                d.push(AudioDevice {
                    is_default: n == dr,
                    description: s["description"].as_str().unwrap_or(&n).into(),
                    name: n,
                    device_type: "source".into(),
                });
            }
        }
    }
    Ok(d)
}
// ══════════════════════════════════════════════════════════════
//  ★ EPIC 1 FIX: PipeWire routing leak — audio duplication bug
//
//  Previous bug: the old code only unlinked from the system-default
//  sink. If the channel had been routed to a DIFFERENT device earlier,
//  that link was never destroyed → audio played from two outputs.
//
//  Fix: query ALL current sinks and run `pw-link -d` for every one.
//  pw-link -d is a no-op when no link exists, so it's safe to spam.
// ══════════════════════════════════════════════════════════════

/// Query `pw-link -l` to find which physical sink device a virtual sink's
/// monitor port is currently linked to. Returns the device name (e.g.
/// "alsa_output.usb-...") or an empty string if not linked to any playback port.

pub fn set_channel_device(channel: String, device_name: String) -> Result<(), String> {
    if channel == "Mic" {
        let current = run_cmd_sync("pactl", &["get-default-source"])
            
            .unwrap_or_default();
        if current.trim() != device_name {
            run_cmd_sync("pactl", &["set-default-source", &device_name])?;
            eprintln!("set_channel_device: Mic source → {device_name}");
        }
        return Ok(());
    }
    if channel == "Master" {
        let current = run_cmd_sync("pactl", &["get-default-sink"])
            
            .unwrap_or_default();
        if current.trim() != device_name {
            run_cmd_sync("pactl", &["set-default-sink", &device_name])?;
            eprintln!("set_channel_device: Master sink → {device_name}");
        }
        return Ok(());
    }

    let sink = format!("OpenGG_{channel}");

    // ★ Idempotent: only modify links that are actually different.
    //   Query current links, unlink from wrong devices, link to target if missing.
    for p in ["FL", "FR"] {
        let current = get_linked_device_for_monitor(&sink, p);
        if current == device_name {
            // Already linked to the target device — nothing to do.
            continue;
        }
        // Unlink from the current wrong device if any.
        if !current.is_empty() {
            let _ = run_cmd_sync(
                "pw-link",
                &[
                    "-d",
                    &format!("{sink}:monitor_{p}"),
                    &format!("{current}:playback_{p}"),
                ],
            )
            ;
        }
        // Create the new link.
        match run_cmd_sync(
            "pw-link",
            &[
                &format!("{sink}:monitor_{p}"),
                &format!("{device_name}:playback_{p}"),
            ],
        )
        
        {
            Ok(_) => {
                eprintln!(
                    "set_channel_device: linked {sink}:monitor_{p} → {device_name}:playback_{p}"
                );
            }
            Err(err) => {
                eprintln!("set_channel_device: pw-link failed: {err}");
            }
        }
    }
    Ok(())
}
// ═══ VU ═══

pub fn check_virtual_audio_status() -> Result<bool, String> {
    let sinks = run_cmd_sync("pactl", &["list", "sinks", "short"]).unwrap_or_default();
    let sources = run_cmd_sync("pactl", &["list", "sources", "short"]).unwrap_or_default();
    let any_present = sinks.contains("OpenGG_") || sources.contains("OpenGG_");
    Ok(any_present)
}

/// Create all OpenGG virtual null sinks via pactl (idempotent — skips existing).

pub fn create_virtual_audio() -> Result<(), String> {
    let existing = run_cmd_sync("pactl", &["list", "sinks", "short"]).unwrap_or_default();
    for ch in VIRTUAL_CHANNELS {
        let sink_name = format!("OpenGG_{ch}");
        if existing.contains(&sink_name) {
            continue;
        }
        let display_name = live_display_name(ch);
        run_cmd_sync("pactl", &[
            "load-module", "module-null-sink",
            &format!("sink_name={sink_name}"),
            &format!(
                "sink_properties=node.description={} node.nick={} device.description={} media.name={}",
                sink_prop_value(&display_name),
                sink_prop_value(&display_name),
                sink_prop_value(&display_name),
                sink_prop_value(&display_name),
            ),
            "channels=2", "channel_map=front-left,front-right",
        ])?;
    }
    log::info!("Virtual audio sinks created");
    Ok(())
}

/// Unload only the OpenGG virtual sinks without restarting PipeWire/WirePlumber.

pub fn remove_virtual_audio() -> Result<(), String> {
    // Step 1: Try daemon-side teardown via D-Bus
    // This is the preferred path — the daemon owns the module_ids vec
    // and has full knowledge of what it created.
    match call_dbus_void("RemoveVirtualAudio", AU_PATH, AU_IFACE, ()) {
        Ok(()) => {
            log::info!("Virtual audio teardown completed via daemon");
            return Ok(());
        }
        Err(e) => {
            log::warn!("Daemon teardown failed ({}), falling back to local scan", e);
        }
    }

    // Step 2: Fallback — daemon unreachable; enumerate and destroy mechanism-agnostically
    // Delete legacy config, unload OpenGG modules, destroy nodes, retry bounded

    // Delete legacy config (same logic as daemon)
    if let Ok(cfg_home) = std::env::var("XDG_CONFIG_HOME") {
        let pw_dir = std::path::PathBuf::from(&cfg_home).join("pipewire/pipewire.conf.d");
        if let Ok(entries) = std::fs::read_dir(&pw_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name == "opengg-sinks.conf" || (name.starts_with("opengg-") && name.ends_with(".conf")) {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
    } else if let Some(home) = dirs::home_dir() {
        let pw_dir = home.join(".config/pipewire/pipewire.conf.d");
        if let Ok(entries) = std::fs::read_dir(&pw_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name == "opengg-sinks.conf" || (name.starts_with("opengg-") && name.ends_with(".conf")) {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
    }

    const MAX_PASSES: u32 = 4;
    const RETRY_DELAY_MS: u64 = 150;

    for pass in 1..=MAX_PASSES {
        // Unload all OpenGG pactl modules (generalized: any module with OpenGG_ in args)
        let modules_output = run_cmd_sync("pactl", &["list", "modules"])
            
            .unwrap_or_default();

        let mut current_index: Option<u64> = None;
        let mut module_args = String::new();

        for line in modules_output.lines() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("Module #") {
                if let Some(idx) = current_index {
                    if module_args.contains("OpenGG_") {
                        let _ = run_cmd_sync("pactl", &["unload-module", &idx.to_string()]);
                    }
                }
                current_index = rest.parse().ok();
                module_args.clear();
            } else if trimmed.starts_with("Argument:") {
                module_args = trimmed.strip_prefix("Argument:").unwrap_or("").trim().to_string();
            }
        }
        // Emit the last module
        if let Some(idx) = current_index {
            if module_args.contains("OpenGG_") {
                let _ = run_cmd_sync("pactl", &["unload-module", &idx.to_string()]);
            }
        }

        // Enumerate live OpenGG nodes and destroy them
        let sinks = run_cmd_sync("pactl", &["list", "sinks", "short"])
            
            .unwrap_or_default();
        let sources = run_cmd_sync("pactl", &["list", "sources", "short"])
            
            .unwrap_or_default();

        let node_ids = list_opengg_node_ids_async(&sinks, &sources);
        for id in node_ids {
            let _ = run_cmd_sync("pw-cli", &["destroy", &id.to_string()]);
        }

        // Check if clean
        let sinks = run_cmd_sync("pactl", &["list", "sinks", "short"])
            
            .unwrap_or_default();
        let sources = run_cmd_sync("pactl", &["list", "sources", "short"])
            
            .unwrap_or_default();

        let remaining = list_opengg_node_ids_async(&sinks, &sources);
        let modules = run_cmd_sync("pactl", &["list", "modules", "short"])
            
            .unwrap_or_default();
        let has_opengg_modules = modules.lines().any(|l| l.contains("OpenGG_"));

        if remaining.is_empty() && !has_opengg_modules {
            break;
        }

        if pass < MAX_PASSES {
            std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
        }
    }

    // Restore OS defaults
    let sinks = run_cmd_sync("pactl", &["list", "sinks", "short"])
        
        .map_err(|e| format!("Failed to list sinks: {e}"))?;

    if let Some(first_real_sink) = sinks
        .lines()
        .find(|line| !line.contains("OpenGG_"))
        .and_then(|line| line.split_whitespace().nth(1))
    {
        let _ = run_cmd_sync("pactl", &["set-default-sink", first_real_sink]);
        log::info!("Restored default sink: {first_real_sink}");
    } else {
        log::warn!("No non-OpenGG sinks found to restore as default");
    }

    let sources = run_cmd_sync("pactl", &["list", "sources", "short"])
        
        .map_err(|e| format!("Failed to list sources: {e}"))?;

    if let Some(first_real_source) = sources
        .lines()
        .find(|line| !line.contains("OpenGG_") && !line.contains(".monitor"))
        .and_then(|line| line.split_whitespace().nth(1))
    {
        let _ = run_cmd_sync("pactl", &["set-default-source", first_real_source]);
        log::info!("Restored default source: {first_real_source}");
    } else {
        log::warn!("No non-OpenGG non-monitor sources found to restore as default");
    }

    // Verify
    let final_sinks = run_cmd_sync("pactl", &["list", "sinks", "short"])
        
        .map_err(|e| format!("Failed to verify sinks: {e}"))?;

    let stragglers: Vec<&str> = final_sinks
        .lines()
        .filter(|line| line.contains("OpenGG_"))
        .collect();

    if !stragglers.is_empty() {
        let straggler_list = stragglers.join("; ");
        log::error!(
            "Teardown verification failed: {} OpenGG sink(s) still present",
            stragglers.len()
        );
        return Err(format!(
            "Teardown incomplete — {} sink(s) still present: {}",
            stragglers.len(),
            straggler_list
        ));
    }

    log::info!("Virtual audio teardown completed via fallback (daemon unreachable)");
    Ok(())
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 3: Startup audio hydration
//
//  Reads the saved per-channel device map from ui-settings.json
//  and re-applies pw-link connections so virtual sinks stay
//  routed to the correct physical output after a restart.
// ══════════════════════════════════════════════════════════════

pub fn hydrate_audio_routing() {
    let settings_path = dirs::config_dir()
        .unwrap_or_default()
        .join("opengg/ui-settings.json");

    let json = match std::fs::read_to_string(&settings_path) {
        Ok(j) => j,
        Err(_) => return, // no saved settings yet — nothing to hydrate
    };
    let v: serde_json::Value = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => return,
    };
    let devices = match v["mixer"]["devices"].as_object() {
        Some(d) => d.clone(),
        None => return,
    };

    for (channel, device_val) in &devices {
        let device = match device_val.as_str() {
            Some(d) if !d.is_empty() => d.to_string(),
            _ => continue,
        };

        if channel == "Mic" {
            let current = run_cmd_sync("pactl", &["get-default-source"]).unwrap_or_default();
            if current.trim() != device {
                Command::new("pactl")
                    .args(["set-default-source", &device])
                    .output()
                    .ok();
                eprintln!("hydrate: Mic source → {device}");
            }
        } else if channel == "Master" {
            // Setting default sink is intentionally skipped — it changes the
            // system-wide default and surprises the user.  The frontend will
            // call set_channel_device if the user actively changes it.
        } else if !VIRTUAL_CHANNELS.contains(&channel.as_str()) {
            continue;
        } else {
            // Virtual sink (Game/Chat/Media/Aux): idempotent reconnection.
            // Only unlink/link if the current device differs from the saved one.
            let sink = format!("OpenGG_{channel}");
            let mut changed = false;
            for p in ["FL", "FR"] {
                let current = get_linked_device_for_monitor(&sink, p);
                if current == device {
                    continue;
                }
                if !current.is_empty() {
                    Command::new("pw-link")
                        .args([
                            "-d",
                            &format!("{sink}:monitor_{p}"),
                            &format!("{current}:playback_{p}"),
                        ])
                        .output()
                        .ok();
                }
                Command::new("pw-link")
                    .args([
                        &format!("{sink}:monitor_{p}"),
                        &format!("{device}:playback_{p}"),
                    ])
                    .output()
                    .ok();
                changed = true;
            }
            if changed {
                eprintln!("hydrate: {channel} → {device}");
            }
        }
    }

    // ── Restore saved per-channel volumes/mutes ONCE at startup ──────────────────
    // After a reboot the daemon recreates the OpenGG_* null-sinks at a fresh 100%.
    // The UI's persisted levels (ui-settings.json → mixer.volumes/mutes) are the source
    // of truth, so re-apply them here a single time. Only the OpenGG channel sinks are
    // touched (never @DEFAULT_SINK@ / Master) so we don't change the system output level.
    // We compare against the live value first and skip no-ops to avoid spurious OS toasts.
    const VOLUME_CHANNELS: &[&str] = &["Game", "Chat", "Media", "Aux", "Mic"];
    let volumes = v["mixer"]["volumes"].as_object();
    let mutes = v["mixer"]["mutes"].as_object();
    for &channel in VOLUME_CHANNELS {
        let sink = format!("OpenGG_{channel}");
        if let Some(saved) = volumes.and_then(|m| m.get(channel)).and_then(|x| x.as_u64()) {
            let saved = saved.min(150) as u32;
            if current_sink_volume_pct(&sink).map(|c| c != saved).unwrap_or(true) {
                Command::new("pactl")
                    .args(["set-sink-volume", &sink, &format!("{saved}%")])
                    .output()
                    .ok();
                eprintln!("hydrate: {channel} volume → {saved}%");
            }
        }
        if let Some(saved) = mutes.and_then(|m| m.get(channel)).and_then(|x| x.as_bool()) {
            if current_sink_muted(&sink).map(|c| c != saved).unwrap_or(true) {
                Command::new("pactl")
                    .args(["set-sink-mute", &sink, if saved { "1" } else { "0" }])
                    .output()
                    .ok();
            }
        }
    }
}

/// Live volume percentage of a sink via `pactl get-sink-volume` (first channel's `%`).

pub fn list_audio_sinks() -> Result<Vec<String>, String> {
    let output = std::process::Command::new("pactl")
        .args(["list", "sinks", "short"])
        .output()
        .map_err(|e| format!("pactl not found: {e}"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let sinks: Vec<String> = text
        .lines()
        .filter_map(|line| {
            // Format: "<id>\t<name>\t<driver>\t<sample_spec>\t<state>"
            line.split('\t').nth(1).map(|s| s.trim().to_string())
        })
        .filter(|s| !s.is_empty())
        .collect();
    if sinks.is_empty() {
        Err("No audio sinks found via pactl".into())
    } else {
        Ok(sinks)
    }
}

pub fn list_capture_sources() -> Result<Vec<CaptureSource>, String> {
    let output = std::process::Command::new("pactl")
        .args(["list", "sources"])
        .output()
        .map_err(|e| format!("pactl not found: {e}"))?;
    let text = String::from_utf8_lossy(&output.stdout);

    // Collect (node.name, friendly description) for every source. In `pactl list sources`,
    // each block has a top-level "Name:" then "Description:" line (properties use the
    // lowercase "device.description" form, which we deliberately don't match).
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut cur_name: Option<String> = None;
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Name: ") {
            cur_name = Some(rest.trim().to_string());
        } else if let Some(rest) = t.strip_prefix("Description: ") {
            if let Some(n) = cur_name.take() {
                pairs.push((n, rest.trim().to_string()));
            }
        }
    }

    let (mut og, mut inputs, mut monitors) = (Vec::new(), Vec::new(), Vec::new());
    for (name, desc) in pairs {
        let lower = name.to_lowercase();
        if name == "OpenGG_Virtual_Mic"
            || lower.contains("gsr-")
            || lower.contains("pw-cat")
            || lower.contains("loopback")
        {
            continue; // internal helper or duplicate of OpenGG_Mic.monitor
        }
        if let Some(ch) = name
            .strip_prefix("OpenGG_")
            .and_then(|s| s.strip_suffix(".monitor"))
        {
            og.push(CaptureSource { value: name.clone(), label: ch.to_string() });
        } else if name.ends_with(".monitor") {
            monitors.push(CaptureSource { value: name.clone(), label: format!("{desc} (Monitor)") });
        } else {
            inputs.push(CaptureSource { value: name.clone(), label: desc });
        }
    }

    // Order: OpenGG channels, then hardware inputs, then hardware output monitors.
    let mut out = og;
    out.extend(inputs);
    out.extend(monitors);
    if out.is_empty() {
        Err("No audio sources found via pactl".into())
    } else {
        Ok(out)
    }
}

pub fn get_session_type() -> String {
    if let Ok(t) = std::env::var("XDG_SESSION_TYPE") {
        let t = t.trim().to_lowercase();
        if !t.is_empty() {
            return t;
        }
    }
    // Fallback: check well-known environment variables
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return "wayland".to_string();
    }
    if std::env::var_os("DISPLAY").is_some() {
        return "x11".to_string();
    }
    "unknown".to_string()
}

fn is_blacklisted_binary(binary: &str) -> bool {
    let b = binary.to_lowercase();
    ROUTING_BLACKLIST.iter().any(|bl| b == *bl)
}

pub fn get_sink_index_by_name(sink_name: &str) -> Result<u32, String> {
    let j = run_cmd_sync("pactl", &["-f", "json", "list", "sinks"])?;
    let sinks: Vec<serde_json::Value> = serde_json::from_str(&j).map_err(|e| format!("{e}"))?;
    for s in &sinks {
        if s["name"].as_str() == Some(sink_name) {
            if let Some(idx) = s["index"].as_u64() {
                return Ok(idx as u32);
            }
        }
    }
    Err(format!("sink '{sink_name}' not found"))
}

/// List all OpenGG PipeWire node IDs (from pactl list sinks/sources short)

fn list_opengg_node_ids_async(sinks: &str, sources: &str) -> Vec<u32> {
    let mut ids = Vec::new();
    for line in sinks.lines() {
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
    for line in sources.lines() {
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
    ids
}

/// Extended sink-input info for cross-referencing PipeWire IDs

pub fn live_display_name(channel: &str) -> String {
    if matches!(channel, "Game" | "Chat" | "Media" | "Aux") {
        channel.to_string()
    } else {
        format!("OpenGG - {channel}")
    }
}

pub fn sink_prop_value(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

/// Helper to parse pw-dump JSON and extract stream info with fallback to pactl.
/// Returns a map of pactl sink-input index → StreamInfo.
/// Uses pw-dump for fast enumeration when available, falls back to pactl on any parse error.
fn get_streams_from_pw_dump() -> Result<HashMap<u32, StreamInfo>, String> {
    use crate::subprocess;

    // First, try pw-dump if available
    if !subprocess::is_available("pw-dump") {
        eprintln!("pw-dump not available, using pactl fallback");
        return Err("pw-dump unavailable".into());
    }

    match subprocess::run("pw-dump", &[]) {
        Ok(output) => {
            let dump_str = String::from_utf8_lossy(&output.stdout);
            match parse_pw_dump_streams(&dump_str) {
                Ok(map) => {
                    eprintln!("pw-dump: enumerated {} streams", map.len());
                    return Ok(map);
                }
                Err(e) => {
                    eprintln!("pw-dump parse failed: {}, falling back to pactl", e);
                }
            }
        }
        Err(e) => {
            eprintln!("pw-dump execution failed: {}, falling back to pactl", e);
        }
    }

    // Fallback: use pactl
    Err("pw-dump unavailable".into())
}

/// Extract volume percent (0-100) from PipeWire Props object.
/// PipeWire stores volumes as linear float values in channelVolumes array.
/// The percent is calculated via cubic root: percent = round(cbrt(linear) * 100).
/// Also checks the mute status — if muted, returns 0%.
/// Verified empirically: pactl 57% = linear 0.19 (cbrt(0.19)*100 ≈ 57%), pactl 30% = 0.027 linear.

fn extract_volume_from_pw_props(pw_props: &serde_json::Map<String, serde_json::Value>) -> u32 {
    // Check mute status first
    if let Some(mute) = pw_props.get("mute").and_then(|m| m.as_bool()) {
        if mute {
            return 0;
        }
    }

    // Try to extract volume from channelVolumes array
    if let Some(ch_vols) = pw_props
        .get("channelVolumes")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_f64())
    {
        // Cubic root: percent = round(cbrt(linear) * 100)
        let percent = (ch_vols.cbrt() * 100.0).round() as u32;
        return percent.min(100); // Cap at 100 for safety
    }

    // Fallback to master volume if no channelVolumes
    if let Some(vol) = pw_props.get("volume").and_then(|v| v.as_f64()) {
        let percent = (vol.cbrt() * 100.0).round() as u32;
        return percent.min(100);
    }

    // Ultimate fallback: 100% (unmuted, no volume info)
    100
}

/// Parse pw-dump output to extract Stream/Output/Audio nodes and map them to sink assignments.
/// Returns a HashMap where the key is the pactl sink-input index (object.serial for streams)
/// and the value is StreamInfo (name, binary, sink_idx, volume, auto_channel).
///
/// pw-dump format: Array of objects with "type", "id", "info" { "props": {...}, "params": {"Props": [...]}} }
/// Volumes are extracted from params.Props[0] using the cubic root formula verified against pactl.

fn parse_pw_dump_streams(dump_json: &str) -> Result<HashMap<u32, StreamInfo>, String> {
    let data: Vec<serde_json::Value> =
        serde_json::from_str(dump_json).map_err(|e| format!("parse pw-dump: {e}"))?;

    let mut streams = HashMap::new();

    // Extract streams (app audio nodes)
    for obj in &data {
        let obj_type = obj.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if obj_type != "PipeWire:Interface:Node" {
            continue;
        }

        let info = match obj.get("info") {
            Some(i) => i,
            None => continue,
        };

        let props = match info
            .get("props")
            .and_then(|p| p.as_object())
        {
            Some(p) => p,
            None => continue, // Skip malformed objects
        };

        let media_class = props
            .get("media.class")
            .and_then(|mc| mc.as_str())
            .unwrap_or("");

        // Only collect output streams (apps playing audio)
        if !media_class.starts_with("Stream/Output/Audio") && media_class != "Stream/Output/Audio" {
            continue;
        }

        let app_name = props
            .get("application.name")
            .and_then(|a| a.as_str())
            .unwrap_or("");
        let media_name = props
            .get("media.name")
            .and_then(|m| m.as_str())
            .unwrap_or("");
        let binary = props
            .get("application.process.binary")
            .and_then(|b| b.as_str())
            .unwrap_or("");

        // Skip internal streams
        if is_internal_stream(Some(app_name), Some(media_name), Some(binary)) {
            continue;
        }

        // Extract the stream's pactl index (stored as object.serial in pw-dump)
        let pactl_index = match props
            .get("object.serial")
            .and_then(|s| s.as_str())
            .and_then(|s| s.parse::<u32>().ok())
        {
            Some(idx) => idx,
            None => continue, // Skip streams without object.serial
        };

        // Determine which sink this stream is routed to
        // Priority: target.object → pulse.sink (PW node ID → try pactl via fallback)
        let sink_idx = props
            .get("target.object")
            .and_then(|t| t.as_str())
            .and_then(|target_name| get_sink_index_by_name(target_name).ok())
            .unwrap_or(0); // Unknown sink — will show in Master

        let name = normalized_stream_name(Some(app_name), Some(media_name), Some(binary));
        let binary_owned = binary.to_string();

        // Extract real volume from params.Props[0] using cubic root formula
        let volume = info
            .get("params")
            .and_then(|p| p.get("Props"))
            .and_then(|props_arr| props_arr.as_array())
            .and_then(|arr| arr.first())
            .and_then(|pw_props| pw_props.as_object())
            .map(extract_volume_from_pw_props)
            .unwrap_or(100); // Fallback to 100% if no Props available

        let auto_channel = classify_channel(&serde_json::json!(props));

        streams.insert(pactl_index, (name, binary_owned, sink_idx, volume, auto_channel));
    }

    Ok(streams)
}

/// Scan sink-inputs — uses pw-dump for fast enumeration with pactl fallback.
/// ★ FIX 2: Filters out streams going to OpenGG virtual sinks' monitors
/// ★ MODERNIZATION: Try pw-dump first for ~single-process speed, fall back to pactl on error.

fn scan_sink_inputs() -> Result<String, String> {
    let mut apps = Vec::new();

    // Try pw-dump path first (faster, single subprocess)
    if let Ok(streams) = get_streams_from_pw_dump() {
        for (idx, (name, binary, sink_idx, vol, auto_channel)) in streams {
            let channel = lookup_sink_channel(sink_idx);
            apps.push(serde_json::json!({
                "id": idx, "name": name, "binary": binary,
                "channel": channel, "icon": "", "volume": vol,
                "auto_channel": auto_channel,
                "locked": is_blacklisted_binary(&binary)
            }));
        }
    } else {
        // Fallback: original pactl path (two subprocesses: pactl list sink-inputs, pactl list sinks)
        eprintln!("scan_sink_inputs: falling back to pactl");
        let j = run_cmd_sync("pactl", &["-f", "json", "list", "sink-inputs"])?;
        let sis: Vec<serde_json::Value> = serde_json::from_str(&j).map_err(|e| format!("{e}"))?;
        for si in &sis {
            let idx = si["index"].as_u64().unwrap_or(0) as u32;
            let p = &si["properties"];
            if is_internal_stream(
                p["application.name"].as_str(),
                p["media.name"].as_str(),
                p["application.process.binary"].as_str(),
            ) {
                continue;
            }
            let binary = p["application.process.binary"].as_str().unwrap_or("");
            let name = normalized_stream_name(
                p["application.name"].as_str(),
                p["media.name"].as_str(),
                Some(binary),
            );

            let sink_idx = si["sink"].as_u64().unwrap_or(0) as u32;
            let channel = lookup_sink_channel(sink_idx);
            let auto_channel = classify_channel(p);

            // Include volume info (0-100) for per-app volume control
            let vol = si["volume"]
                .as_object()
                .and_then(|v| v.values().next())
                .and_then(|ch| ch["value_percent"].as_str())
                .and_then(|s| s.trim_end_matches('%').parse::<u32>().ok())
                .unwrap_or(100);

            apps.push(serde_json::json!({
                "id": idx, "name": name, "binary": binary,
                "channel": channel, "icon": "", "volume": vol,
                "auto_channel": auto_channel,
                "locked": is_blacklisted_binary(binary)
            }));
        }
    }

    // ★ Epic 3: Also scan source-outputs — apps recording from the mic
    // IDs are offset by 90000 to avoid conflicts with sink-input IDs.
    if let Ok(sj) = run_cmd_sync("pactl", &["-f", "json", "list", "source-outputs"]) {
        if let Ok(sos) = serde_json::from_str::<Vec<serde_json::Value>>(&sj) {
            for (idx, so) in sos.iter().enumerate() {
                let p = &so["properties"];
                if is_internal_stream(
                    p["application.name"].as_str(),
                    p["media.name"].as_str(),
                    p["application.process.binary"].as_str(),
                ) {
                    continue;
                }
                let binary = p["application.process.binary"].as_str().unwrap_or("");
                let name = normalized_stream_name(
                    p["application.name"].as_str(),
                    p["media.name"].as_str(),
                    Some(binary),
                );
                let fake_id = 90000u32 + idx as u32;
                apps.push(serde_json::json!({
                    "id": fake_id, "name": name, "binary": binary,
                    "channel": "Mic", "icon": "", "volume": 100
                }));
            }
        }
    }

    Ok(serde_json::to_string(&apps).unwrap_or("[]".into()))
}

fn normalized_stream_name(
    app_name: Option<&str>,
    media_name: Option<&str>,
    binary_name: Option<&str>,
) -> String {
    let app = app_name.map(str::trim).filter(|v| !v.is_empty());
    let media = media_name.map(str::trim).filter(|v| !v.is_empty());
    let binary = binary_name.map(str::trim).filter(|v| !v.is_empty());

    match app {
        Some(name) if !name.eq_ignore_ascii_case("opengg") => name.to_string(),
        _ => media.or(binary).unwrap_or("Unknown").to_string(),
    }
}

fn is_internal_stream(
    app_name: Option<&str>,
    media_name: Option<&str>,
    binary_name: Option<&str>,
) -> bool {
    let matches_internal = |value: Option<&str>| {
        value
            .map(str::to_lowercase)
            .map(|raw| {
                raw.contains("opengg")
                    || raw.contains("wireplumber")
                    || raw.contains("pipewire")
                    || raw.contains("peak detect")
                    || raw.contains("monitor")
                    // OpenGG-internal helpers that must never show as user apps:
                    || raw.contains("pw-cat")          // native VU-meter readers
                    || raw.contains("gsr-")            // gpu-screen-recorder captures
                    || raw.contains("gpu-screen-recorder")
                    || raw.contains("loopback")        // mic loopback helper streams
            })
            .unwrap_or(false)
    };

    matches_internal(app_name)
        || matches_internal(media_name)
        || matches_internal(binary_name)
}

/// Suggest an OpenGG channel for an app based on PipeWire stream properties.
/// Returns an empty string when no confident classification can be made.

fn classify_channel(props: &serde_json::Value) -> &'static str {
    let role = props["media.role"].as_str().unwrap_or("").to_lowercase();
    let binary = props["application.process.binary"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    let name = props["application.name"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();

    // media.role is the most authoritative signal (set by the app itself)
    match role.as_str() {
        "game" => return "Game",
        "music" | "video" | "movie" => return "Media",
        "phone" | "communication" => return "Chat",
        _ => {}
    }

    // Binary / app-name heuristics
    const CHAT_BINS: &[&str] = &[
        "discord",
        "teamspeak",
        "mumble",
        "signal",
        "telegram",
        "zoom",
        "slack",
        "skype",
        "element",
        "hexchat",
    ];
    const GAME_BINS: &[&str] = &[
        "steam",
        "heroic",
        "lutris",
        "wine",
        "proton",
        "gameoverlayui",
        "gamescope",
        "mangohud",
    ];
    const MEDIA_BINS: &[&str] = &[
        "spotify",
        "rhythmbox",
        "clementine",
        "vlc",
        "mpv",
        "celluloid",
        "strawberry",
        "quodlibet",
        "cmus",
        "lollypop",
        "elisa",
        "audacious",
    ];

    if CHAT_BINS
        .iter()
        .any(|b| binary.contains(b) || name.contains(b))
    {
        return "Chat";
    }
    if GAME_BINS
        .iter()
        .any(|b| binary.contains(b) || name.contains(b))
    {
        return "Game";
    }
    if MEDIA_BINS
        .iter()
        .any(|b| binary.contains(b) || name.contains(b))
    {
        return "Media";
    }

    "" // No confident match — leave in Master
}

fn lookup_sink_channel(sink_idx: u32) -> String {
    if let Ok(j) = run_cmd_sync("pactl", &["-f", "json", "list", "sinks"]) {
        if let Ok(sinks) = serde_json::from_str::<Vec<serde_json::Value>>(&j) {
            for s in &sinks {
                if s["index"].as_u64() == Some(sink_idx as u64) {
                    let n = s["name"].as_str().unwrap_or("");
                    if let Some(ch) = n.strip_prefix("OpenGG_") {
                        return ch.into();
                    }
                }
            }
        }
    }
    String::new()
}

/// Verify that the sink-input `si_idx` (pactl integer index) is actually on the target
/// sink `sink_idx` (pactl integer index), by re-reading pactl's live `sink` field.
///
/// This is reality-based: pactl's `sink` field reflects where the stream is *actually*
/// linked (confirmed empirically), unlike a metadata write that may report success
/// without moving anything. Polls a few times to absorb the brief settle after a move.

pub fn get_linked_device_for_monitor(sink_name: &str, port: &str) -> String {
    let output = match run_cmd_sync("pw-link", &["-l"]) {
        Ok(o) => o,
        Err(_) => return String::new(),
    };
    let target_port = format!("{sink_name}:monitor_{port}");
    let mut in_section = false;
    for line in output.lines() {
        // Section header: port name with no leading whitespace
        if !line.starts_with(' ') && line.trim() == target_port {
            in_section = true;
            continue;
        }
        if in_section {
            if !line.starts_with("  |->") {
                break; // next port section
            }
            if let Some(rest) = line.trim_start().strip_prefix("|-> ") {
                if let Some((device, _)) = rest.rsplit_once(&format!(":playback_{port}")) {
                    return device.to_string();
                }
            }
        }
    }
    String::new()
}

/// Destroy every existing pw-link from `{sink_name}:monitor_FL/FR` to
/// any physical sink that is currently listed by PulseAudio.
/// If `preserve_device` is provided, links to that device are left intact.

fn current_sink_volume_pct(sink: &str) -> Option<u32> {
    let out = run_cmd_sync("pactl", &["get-sink-volume", sink]).ok()?;
    out.split('%')
        .next()
        .and_then(|s| s.rsplit('/').next())
        .and_then(|s| s.trim().parse::<u32>().ok())
}

/// Live mute state of a sink via `pactl get-sink-mute` ("Mute: yes/no").

fn current_sink_muted(sink: &str) -> Option<bool> {
    let out = run_cmd_sync("pactl", &["get-sink-mute", sink]).ok()?;
    Some(out.trim().ends_with("yes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalized_stream_name_prefers_app() {
        assert_eq!(
            normalized_stream_name(Some("Discord"), Some("media"), Some("discord")),
            "Discord"
        );
    }

    #[test]
    fn test_normalized_stream_name_skips_opengg() {
        assert_eq!(
            normalized_stream_name(Some("OpenGG"), Some("Video Player"), Some("mpv")),
            "Video Player"
        );
    }

    #[test]
    fn test_normalized_stream_name_fallback_to_binary() {
        assert_eq!(
            normalized_stream_name(None, None, Some("firefox")),
            "firefox"
        );
    }

    #[test]
    fn test_normalized_stream_name_unknown() {
        assert_eq!(
            normalized_stream_name(None, None, None),
            "Unknown"
        );
    }

    #[test]
    fn test_is_internal_stream_detects_opengg() {
        assert!(is_internal_stream(Some("OpenGG"), None, None));
    }

    #[test]
    fn test_is_internal_stream_detects_wireplumber() {
        assert!(is_internal_stream(None, Some("WirePlumber"), None));
    }

    #[test]
    fn test_is_internal_stream_detects_monitor() {
        assert!(is_internal_stream(None, None, Some("some-monitor")));
    }

    #[test]
    fn test_is_internal_stream_allows_regular_app() {
        assert!(!is_internal_stream(Some("Discord"), None, None));
    }

    #[test]
    fn test_classify_channel_by_role() {
        let mut props = serde_json::json!({"media.role": "game"});
        assert_eq!(classify_channel(&props), "Game");

        props = serde_json::json!({"media.role": "music"});
        assert_eq!(classify_channel(&props), "Media");

        props = serde_json::json!({"media.role": "phone"});
        assert_eq!(classify_channel(&props), "Chat");
    }

    #[test]
    fn test_classify_channel_by_binary() {
        let props = serde_json::json!({"application.process.binary": "discord"});
        assert_eq!(classify_channel(&props), "Chat");

        let props = serde_json::json!({"application.process.binary": "steam"});
        assert_eq!(classify_channel(&props), "Game");

        let props = serde_json::json!({"application.process.binary": "spotify"});
        assert_eq!(classify_channel(&props), "Media");
    }

    #[test]
    fn test_classify_channel_unknown() {
        let props = serde_json::json!({"application.process.binary": "unknown_app"});
        assert_eq!(classify_channel(&props), "");
    }

    #[test]
    fn test_parse_pw_dump_streams_basic() {
        // Minimal fixture: one stream (Zen browser) with volume 57% (channelVolumes [0.19, 0.19])
        // Empirically verified: pactl 57% = linear 0.19, cbrt(0.19)*100 ≈ 57.4%
        let fixture = r#"[
  {
    "id": 100,
    "type": "PipeWire:Interface:Node",
    "info": {
      "props": {
        "media.class": "Stream/Output/Audio",
        "application.name": "Zen",
        "application.process.binary": "zen",
        "media.name": "audio",
        "object.serial": "904490",
        "target.object": "OpenGG_Game"
      },
      "params": {
        "Props": [
          {
            "volume": 1.0,
            "mute": false,
            "channelVolumes": [0.19, 0.19]
          }
        ]
      }
    }
  }
]"#;

        let result = parse_pw_dump_streams(fixture).expect("parse failed");
        assert_eq!(result.len(), 1);
        let (name, binary, _sink_idx, vol, _auto) = result.get(&904490u32).expect("stream not found");
        assert_eq!(name, "Zen");
        assert_eq!(binary, "zen");
        assert_eq!(*vol, 57); // cbrt(0.19) * 100 ≈ 57.4, rounded to 57
    }

    #[test]
    fn test_parse_pw_dump_streams_filters_internal() {
        // Stream with "opengg" in app name should be filtered
        let fixture = r#"[
  {
    "id": 100,
    "type": "PipeWire:Interface:Node",
    "info": {
      "props": {
        "media.class": "Stream/Output/Audio",
        "application.name": "OpenGG",
        "application.process.binary": "opengg",
        "object.serial": "12345"
      },
      "params": {
        "Props": [{"volume": 1.0, "mute": false, "channelVolumes": [1.0, 1.0]}]
      }
    }
  }
]"#;

        let result = parse_pw_dump_streams(fixture).expect("parse failed");
        assert_eq!(result.len(), 0, "internal stream should be filtered");
    }

    #[test]
    fn test_parse_pw_dump_streams_multiple() {
        // Multiple streams with different volumes: Discord at 80%, Spotify at 50%
        let fixture = r#"[
  {
    "id": 1,
    "type": "PipeWire:Interface:Node",
    "info": {
      "props": {
        "media.class": "Stream/Output/Audio",
        "application.name": "Discord",
        "application.process.binary": "discord",
        "object.serial": "100",
        "target.object": "OpenGG_Chat"
      },
      "params": {
        "Props": [{"volume": 1.0, "mute": false, "channelVolumes": [0.512, 0.512]}]
      }
    }
  },
  {
    "id": 2,
    "type": "PipeWire:Interface:Node",
    "info": {
      "props": {
        "media.class": "Stream/Output/Audio",
        "application.name": "Spotify",
        "application.process.binary": "spotify",
        "object.serial": "200",
        "target.object": "OpenGG_Media"
      },
      "params": {
        "Props": [{"volume": 1.0, "mute": false, "channelVolumes": [0.125, 0.125]}]
      }
    }
  }
]"#;

        let result = parse_pw_dump_streams(fixture).expect("parse failed");
        assert_eq!(result.len(), 2);
        let (_, _, _, discord_vol, _) = result.get(&100u32).expect("Discord not found");
        let (_, _, _, spotify_vol, _) = result.get(&200u32).expect("Spotify not found");
        assert_eq!(*discord_vol, 80); // cbrt(0.512) * 100 ≈ 80
        assert_eq!(*spotify_vol, 50); // cbrt(0.125) * 100 = 50
    }

    #[test]
    fn test_parse_pw_dump_streams_missing_object_serial() {
        // Stream with missing object.serial should cause the loop to skip it (continue)
        // Mixed: one good stream and one without object.serial
        let fixture = r#"[
  {
    "id": 100,
    "type": "PipeWire:Interface:Node",
    "info": {
      "props": {
        "media.class": "Stream/Output/Audio",
        "application.name": "Firefox",
        "application.process.binary": "firefox"
      },
      "params": {
        "Props": [{"volume": 1.0, "mute": false, "channelVolumes": [1.0, 1.0]}]
      }
    }
  },
  {
    "id": 101,
    "type": "PipeWire:Interface:Node",
    "info": {
      "props": {
        "media.class": "Stream/Output/Audio",
        "application.name": "Chrome",
        "application.process.binary": "chrome",
        "object.serial": "500"
      },
      "params": {
        "Props": [{"volume": 1.0, "mute": false, "channelVolumes": [1.0, 1.0]}]
      }
    }
  }
]"#;

        let result = parse_pw_dump_streams(fixture).expect("parse failed");
        assert_eq!(result.len(), 1, "stream without object.serial should be skipped");
        assert!(result.contains_key(&500u32), "stream with object.serial should be present");
    }
}


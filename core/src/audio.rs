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
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};
use crate::daemon::{call_dbus, call_dbus_void, AU_IFACE, AU_PATH};
use crate::subprocess::run_cmd_sync;

const ROUTING_BLACKLIST: &[&str] = &[
    "plasmashell", "kwin_wayland", "kwin_x11", "swaync", "sway",
    "xdg-desktop-portal", "xdg-desktop-portal-gnome", "xdg-desktop-portal-kde",
    "wireplumber", "pipewire", "pipewire-pulse", "opengg", "peak detect",
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

/// Create all OpenGG virtual null sinks (idempotent — skips existing).
///
/// Prefers routing through the daemon's D-Bus `CreateVirtualAudio` call —
/// the daemon's `SinkManager` is the single owner of sink lifecycle and the
/// only thing that tracks module IDs for cleanup. Only if the daemon is
/// unreachable do we fall back to creating sinks directly from this
/// process via raw `pactl` calls; those sinks won't be tracked by anything
/// (this process has no equivalent of `SinkManager`/`Drop`-based cleanup),
/// so this fallback should be treated as best-effort, not the common case.
pub fn create_virtual_audio() -> Result<(), String> {
    // Step 1: Try daemon-side creation via D-Bus — preferred path.
    match call_dbus_void("CreateVirtualAudio", AU_PATH, AU_IFACE, ()) {
        Ok(()) => {
            log::info!("Virtual audio sinks created via daemon");
            return Ok(());
        }
        Err(e) => {
            log::warn!("Daemon-side sink creation failed ({}), falling back to local pactl calls", e);
        }
    }

    // Step 2: Fallback — daemon unreachable; create sinks directly.
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
    log::info!("Virtual audio sinks created locally (daemon unreachable)");
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

/// Real playback devices for a channel's output-device selector, as
/// {value: node.name, label: friendly description} pairs — the qt-shell
/// counterpart of `list_capture_sources` for sinks rather than sources.
///
/// `list_audio_sinks()` above returns raw pactl node names with no
/// filtering, which is exactly what it needs to be for its one existing
/// caller (GSR's audio-source picker, which legitimately wants to offer a
/// virtual channel's own sink as a capture target). Reusing it for a
/// channel's OUTPUT device selector produced two real bugs: the dropdown
/// showed technical names like "alsa_output.usb-..." instead of "Arctis
/// Nova 7 Analog Stereo", and it listed OpenGG's own virtual sinks
/// (OpenGG_Game/Chat/Media/Aux) as selectable playback devices — routing a
/// channel's output to another channel's virtual sink is nonsensical and
/// was never a real target. This is a new function rather than a change to
/// `list_audio_sinks` so GSR's existing behavior is untouched.
pub fn list_audio_sinks_friendly() -> Result<Vec<CaptureSource>, String> {
    let output = std::process::Command::new("pactl")
        .args(["list", "sinks"])
        .output()
        .map_err(|e| format!("pactl not found: {e}"))?;
    let text = String::from_utf8_lossy(&output.stdout);

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

    let out: Vec<CaptureSource> = pairs
        .into_iter()
        .filter(|(name, _)| !name.starts_with("OpenGG_"))
        .map(|(value, label)| CaptureSource { value, label })
        .collect();
    if out.is_empty() {
        Err("No audio sinks found via pactl".into())
    } else {
        Ok(out)
    }
}

/// The device currently in use for a channel, as a raw pactl node.name
/// matching a `value` field from `list_audio_sinks_friendly`/
/// `list_capture_sources` — so a ComboBox can find and highlight the real
/// current selection instead of always defaulting to index 0.
///
/// Master/Mic route through the system default sink/source, not a virtual
/// OpenGG sink, so those read `pactl get-default-{sink,source}` directly.
/// The other four channels are queried the same way `set_channel_device`
/// verifies its own writes: via `pw-link -l`, since pactl has no "what is
/// this virtual sink's monitor linked to" query of its own.
pub fn current_channel_device(channel: &str) -> String {
    match channel {
        "Master" => run_cmd_sync("pactl", &["get-default-sink"])
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
        "Mic" => run_cmd_sync("pactl", &["get-default-source"])
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
        _ => {
            let sink = format!("OpenGG_{channel}");
            let fl = get_linked_device_for_monitor(&sink, "FL");
            if !fl.is_empty() {
                fl
            } else {
                get_linked_device_for_monitor(&sink, "FR")
            }
        }
    }
}

/// `current_channel_device` for every channel strip the Mixer page renders,
/// as a single JSON object (`{"Master": "...", "Game": "...", ...}`) — one
/// bundle rather than six separate qinvokable round trips from QML.
pub fn get_channel_devices_json() -> String {
    let mut map = serde_json::Map::new();
    for ch in ["Master", "Game", "Chat", "Media", "Aux", "Mic"] {
        map.insert(
            ch.to_string(),
            serde_json::Value::String(current_channel_device(ch)),
        );
    }
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".into())
}

/// Every capture-capable node: real mic inputs, OpenGG virtual-channel
/// monitors, and hardware output monitors (e.g. "Headphones (Monitor)" for
/// system-audio capture). Used by the Capture Sound settings panel and GSR's
/// recording-source picker, where all three categories are valid choices.
pub fn list_capture_sources() -> Result<Vec<CaptureSource>, String> {
    let (og, inputs, monitors) = list_sources_categorized()?;
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

/// Real hardware/software microphone inputs only — excludes OpenGG's own
/// virtual-channel monitors (Game/Chat/Media/Aux) and hardware output
/// monitors (e.g. "Headphones (Monitor)"). Used for the Mixer's Mic-channel
/// device picker, where those other entries are meaningless (or actively
/// confusing — they aren't microphones).
pub fn list_mic_input_sources() -> Result<Vec<CaptureSource>, String> {
    let (_, inputs, _) = list_sources_categorized()?;
    if inputs.is_empty() {
        Err("No microphone input sources found via pactl".into())
    } else {
        Ok(inputs)
    }
}

type CategorizedSources = (Vec<CaptureSource>, Vec<CaptureSource>, Vec<CaptureSource>);

fn list_sources_categorized() -> Result<CategorizedSources, String> {
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

    Ok((og, inputs, monitors))
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

// ══════════════════════════════════════════════════════════════════════
//  ★ PER-APP ROUTING — ported from the Tauri host's `route_app` + `Router`
//  + `RouteState` (frontend/src-tauri/src/commands/audio.rs, src/main.rs).
//  Fully portable to core: `RouteState` has zero Tauri coupling (plain
//  Mutex<HashMap> state), and the only AppHandle usage in the original
//  (`app.emit("audio-mixer-refresh", ())`) is dropped here — callers
//  refresh their own view after a successful `route_app` the same way
//  every other mutating AudioController method already does.
//
//  Dropped from the port (dead code in the original, `#[allow(dead_code)]`
//  there too): `route_via_pw_metadata`, `get_pw_node_id_for_sink`,
//  `unload_null_sink_module`, `unlink_virtual_sink_from_all` — none of
//  these are on the live `pactl move-sink-input` code path.
// ══════════════════════════════════════════════════════════════════════

const ROUTE_COOLDOWN_SECS: u64 = 5;
const FAIL_THRESHOLD: u32 = 3;
const FAIL_WINDOW_SECS: u64 = 30;
const FAIL_COOLDOWN_SECS: u64 = 30;

/// Tracks routing attempts, successes, and failures per app to prevent the
/// infinite re-routing loop that spawns pactl/pw-metadata thousands of
/// times per second and eventually OOM-kills the system.
struct RouteState {
    /// routing-key → last routing attempt time. Prevents retry during cooldown.
    /// KEY = stable app identity (binary, lowercased) when known, else the stream id —
    /// keying by binary (not the volatile PipeWire object.serial / sink-input index) is
    /// what makes these guards actually work: a flood of short-lived streams from one
    /// app shares ONE key, so the cooldown + circuit breaker can finally engage.
    cooldown: Mutex<HashMap<String, SystemTime>>,
    /// routing-key → (channel, success_time). Tracks successfully routed apps.
    routed: Mutex<HashMap<String, (String, SystemTime)>>,
    /// routing-key → (fail_count, first_fail_time). Circuit breaker for repeated failures.
    fail_counts: Mutex<HashMap<String, (u32, SystemTime)>>,
}

impl RouteState {
    fn new() -> Self {
        Self {
            cooldown: Mutex::new(HashMap::new()),
            routed: Mutex::new(HashMap::new()),
            fail_counts: Mutex::new(HashMap::new()),
        }
    }

    fn is_on_cooldown(&self, key: &str) -> bool {
        let map = self.cooldown.lock().unwrap();
        map.get(key).is_some_and(|t| {
            SystemTime::now().duration_since(*t).unwrap_or(Duration::MAX)
                < Duration::from_secs(ROUTE_COOLDOWN_SECS)
        })
    }

    fn record_attempt(&self, key: &str) {
        self.cooldown.lock().unwrap().insert(key.to_string(), SystemTime::now());
    }

    fn record_success(&self, key: &str, channel: String) {
        self.routed.lock().unwrap().insert(key.to_string(), (channel, SystemTime::now()));
        self.fail_counts.lock().unwrap().remove(key);
    }

    fn is_already_routed(&self, key: &str, channel: &str) -> bool {
        let map = self.routed.lock().unwrap();
        map.get(key).is_some_and(|(ch, _)| ch == channel)
    }

    /// Records a failure and returns `true` if the circuit breaker is now
    /// open (this app should be blocked from further attempts).
    fn record_failure(&self, key: &str) -> bool {
        let mut map = self.fail_counts.lock().unwrap();
        let now = SystemTime::now();
        let entry = map.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(entry.1).unwrap_or(Duration::MAX) > Duration::from_secs(FAIL_WINDOW_SECS) {
            *entry = (1, now);
        } else {
            entry.0 += 1;
        }
        if entry.0 >= FAIL_THRESHOLD {
            self.cooldown
                .lock()
                .unwrap()
                .insert(key.to_string(), now + Duration::from_secs(FAIL_COOLDOWN_SECS));
            true
        } else {
            false
        }
    }

    fn is_circuit_open(&self, key: &str) -> bool {
        let map = self.fail_counts.lock().unwrap();
        let now = SystemTime::now();
        map.get(key).is_some_and(|(count, first)| {
            *count >= FAIL_THRESHOLD
                && now.duration_since(*first).unwrap_or(Duration::MAX) <= Duration::from_secs(FAIL_COOLDOWN_SECS)
        })
    }
}

fn route_state() -> &'static RouteState {
    static STATE: OnceLock<RouteState> = OnceLock::new();
    STATE.get_or_init(RouteState::new)
}

/// Build a map of pactl sink-input index → every PipeWire node id it's
/// known by (a stream can be identified by several id-shaped properties).
fn build_si_map() -> Result<HashMap<u32, Vec<u32>>, String> {
    let j = run_cmd_sync("pactl", &["-f", "json", "list", "sink-inputs"])?;
    let sis: Vec<serde_json::Value> = serde_json::from_str(&j).map_err(|e| format!("parse sink-inputs: {e}"))?;
    let mut map = HashMap::new();
    for si in sis {
        let idx = si["index"].as_u64().unwrap_or(0) as u32;
        let p = &si["properties"];

        let mut pw_ids = Vec::new();
        for key in [
            "object.serial",
            "object.id",
            "node.id",
            "pipewire.access.portal.app_id",
            "pipewire.client.access",
        ] {
            if let Some(s) = p[key].as_str() {
                if let Ok(id) = s.parse::<u32>() {
                    pw_ids.push(id);
                }
            }
        }
        if let Some(s) = p["media.name"].as_str() {
            if let Ok(id) = s.parse::<u32>() {
                pw_ids.push(id);
            }
        }

        map.insert(idx, pw_ids);
    }
    Ok(map)
}

/// Check whether a sink-input index currently exists in the system.
fn validate_sink_input_exists(si_idx: u32) -> bool {
    if let Ok(j) = run_cmd_sync("pactl", &["-f", "json", "list", "sink-inputs"]) {
        if let Ok(sis) = serde_json::from_str::<Vec<serde_json::Value>>(&j) {
            return sis.iter().any(|si| si["index"].as_u64() == Some(si_idx as u64));
        }
    }
    false
}

/// Cross-reference PipeWire node ID → pactl sink-input index using the full SI map.
fn find_pactl_si_for_pw_id(pw_id: u32) -> Result<u32, String> {
    let map = build_si_map()?;
    for (idx, pw_ids) in &map {
        if pw_ids.contains(&pw_id) {
            return Ok(*idx);
        }
    }
    Err(format!("no pactl si for PW#{pw_id}"))
}

/// Translate an incoming app id to a pactl sink-input index. The id may
/// already BE a pactl sink-input index, or it may be a PipeWire node.id —
/// e.g. when the daemon supplied a node id. Returns `None` if the id maps
/// to no current sink-input (e.g. a source-output / mic capture, which
/// cannot be moved to a playback sink).
fn resolve_pactl_si_index(app_id: u32) -> Option<u32> {
    if validate_sink_input_exists(app_id) {
        return Some(app_id);
    }
    find_pactl_si_for_pw_id(app_id).ok()
}

/// Ensure a channel's virtual sink exists (creating it with a 600ms
/// settling time + default-sink loopback links if not), so a freshly
/// created channel is immediately audible.
fn ensure_sink_exists(name: &str, ch: &str) -> Result<(), String> {
    if let Ok(o) = Command::new("pactl").args(["list", "sinks", "short"]).output() {
        if String::from_utf8_lossy(&o.stdout).contains(name) {
            return Ok(());
        }
    }
    let display_name = live_display_name(ch);
    let c = Command::new("pactl")
        .args([
            "load-module",
            "module-null-sink",
            &format!("sink_name={name}"),
            &format!(
                "sink_properties=node.description={} node.nick={} device.description={} media.name={}",
                sink_prop_value(&display_name),
                sink_prop_value(&display_name),
                sink_prop_value(&display_name),
                sink_prop_value(&display_name),
            ),
            "channels=2",
            "channel_map=front-left,front-right",
        ])
        .output()
        .map_err(|e| format!("{e}"))?;
    if !c.status.success() {
        return Err(String::from_utf8_lossy(&c.stderr).to_string());
    }
    std::thread::sleep(Duration::from_millis(600));

    if let Ok(def) = run_cmd_sync("pactl", &["get-default-sink"]) {
        for p in ["FL", "FR"] {
            if !get_linked_device_for_monitor(name, p).is_empty() {
                continue; // already linked to something — skip to avoid duplicates
            }
            let _ = Command::new("pw-link")
                .args([&format!("{name}:monitor_{p}"), &format!("{def}:playback_{p}")])
                .output();
        }
    }
    log::info!("ensure_sink_exists: sink '{name}' ready");
    Ok(())
}

/// Confirm a moved sink-input actually landed on the target sink, with retry
/// (WirePlumber can take a moment to apply the move).
fn verify_stream_routed(si_idx: u32, sink_idx: u32) -> bool {
    for attempt in 0..5 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(120));
        }
        let Ok(j) = run_cmd_sync("pactl", &["-f", "json", "list", "sink-inputs"]) else { continue };
        let Ok(sis) = serde_json::from_str::<Vec<serde_json::Value>>(&j) else { continue };
        if let Some(si) = sis.iter().find(|si| si["index"].as_u64() == Some(si_idx as u64)) {
            if si["sink"].as_u64() == Some(sink_idx as u64) {
                return true;
            }
        }
    }
    false
}

fn resolve_route_sink_name(channel: &str) -> Result<String, String> {
    if channel == "default" || channel == "Master" {
        run_cmd_sync("pactl", &["get-default-sink"])
    } else {
        let name = format!("OpenGG_{channel}");
        ensure_sink_exists(&name, channel)?;
        Ok(name)
    }
}

/// Move the stream to the target sink via `pactl move-sink-input` and
/// confirm it actually landed there (with retry). Both indices are pactl
/// integer indices.
fn move_and_verify(app_id: u32, channel: &str, si_idx: u32, sink_idx: u32) -> Result<(), String> {
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(150));
        }
        match run_cmd_sync("pactl", &["move-sink-input", &si_idx.to_string(), &sink_idx.to_string()]) {
            Ok(_) => {
                if verify_stream_routed(si_idx, sink_idx) {
                    log::info!("route_app[{app_id}→{channel}]: moved sink-input #{si_idx} → sink #{sink_idx} (verified)");
                    return Ok(());
                }
                log::debug!("route_app[{app_id}→{channel}]: move ok but not on target yet (attempt {}/3)", attempt + 1);
            }
            Err(e) => log::debug!("route_app[{app_id}→{channel}]: move-sink-input failed (attempt {}/3): {e}", attempt + 1),
        }
    }
    Err(format!("route_app: failed to move id {app_id} ({si_idx}→{channel}) after retries"))
}

/// Route one app's audio stream to a channel ("default"/"Master" routes back
/// to the system default sink). Guarded by a per-app cooldown + circuit
/// breaker (see `RouteState`) so a flood of short-lived streams from one app
/// can't spam `pactl`/D-Bus thousands of times per second.
///
/// `binary` is used as the stable routing-guard key (falls back to `app_id`
/// if empty — matches the Tauri original, which keys by binary specifically
/// so hundreds of short-lived streams from one app share a single guard).
pub fn route_app(app_id: u32, channel: String, binary: String) -> Result<(), String> {
    let key = if binary.trim().is_empty() { app_id.to_string() } else { binary.to_lowercase() };
    let state = route_state();

    if is_blacklisted_binary(&binary) {
        return Err(format!("route_app: {app_id} ({binary}) is blacklisted — system processes cannot be routed"));
    }
    if state.is_already_routed(&key, &channel) {
        return Ok(());
    }
    if state.is_on_cooldown(&key) {
        return Err(format!("route_app: {key} is on cooldown ({ROUTE_COOLDOWN_SECS}s)"));
    }
    if state.is_circuit_open(&key) {
        return Err(format!("route_app: {key} circuit breaker open (too many failures)"));
    }

    state.record_attempt(&key);

    let result = (|| -> Result<(), String> {
        // Strategy 0: D-Bus daemon (preferred — single source of truth)
        if call_dbus_void("RouteApp", AU_PATH, AU_IFACE, (app_id, channel.as_str())).is_ok() {
            return Ok(());
        }
        let sink_name = resolve_route_sink_name(&channel)?;
        let sink_idx = get_sink_index_by_name(&sink_name)?;
        let si_idx = resolve_pactl_si_index(app_id).ok_or_else(|| {
            format!("route_app: id {app_id} is not a movable sink-input — not routing to {channel}")
        })?;
        move_and_verify(app_id, &channel, si_idx, sink_idx)
    })();

    match result {
        Ok(()) => {
            state.record_success(&key, channel);
            Ok(())
        }
        Err(e) => {
            if state.record_failure(&key) {
                log::warn!(
                    "route_app: '{key}' circuit breaker OPEN after {FAIL_THRESHOLD} failures in {FAIL_WINDOW_SECS}s — suppressing further attempts for {FAIL_COOLDOWN_SECS}s"
                );
            } else {
                log::debug!("route_app: id={app_id} ('{key}') → failed: {e}");
            }
            Err(e)
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
//  Persisted app→channel routing rules
//
//  `pactl move-sink-input` is a live, stateless move: it lasts exactly as
//  long as that sink-input does. Nothing in the Qt shell ever wrote the
//  user's choice down or replayed it, so every app link was lost the moment
//  the stream (or the machine) restarted — the user had to re-drag every
//  app by hand. `mixer.appRules` in ui-settings.json already held exactly
//  this map (the removed Vue frontend wrote it), so these read and reapply
//  that existing key rather than inventing a second store.
// ══════════════════════════════════════════════════════════════════════

/// Saved binary/app-name → channel rules, keyed lowercase for matching.
///
/// Entries written by the old frontend are a mix of process binaries
/// ("vlc", "sd_dummy") and display names ("Discord", "Playback Stream"),
/// so [`apply_saved_app_rules`] deliberately matches a stream on *either*.
pub fn load_app_rules() -> std::collections::HashMap<String, String> {
    let path = crate::settings::settings_path();
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return std::collections::HashMap::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return std::collections::HashMap::new();
    };
    let Some(obj) = v["mixer"]["appRules"].as_object() else {
        return std::collections::HashMap::new();
    };
    obj.iter()
        .filter_map(|(k, val)| {
            let ch = val.as_str()?.trim();
            (!ch.is_empty()).then(|| (k.trim().to_lowercase(), ch.to_string()))
        })
        .collect()
}

/// Read a numeric field that may arrive as a JSON number *or* a JSON string.
///
/// The daemon's `GetApps` serializes `id` (and `volume`) as strings —
/// `{"id":"33191", ...}` — while the local `pactl` fallback emits real
/// numbers. Accepting only one shape silently skipped every app on the D-Bus
/// path, which is exactly how the first cut of `apply_saved_app_rules` did
/// nothing at all while appearing to succeed.
fn json_u32(v: &serde_json::Value) -> Option<u32> {
    if let Some(n) = v.as_u64() {
        return u32::try_from(n).ok();
    }
    v.as_str()?.trim().parse::<u32>().ok()
}

/// Re-apply every saved rule to any live stream currently sitting on the
/// wrong channel.
///
/// Safe to call repeatedly: `route_app` already no-ops when a stream is
/// where it belongs, and skips blacklisted system processes.
pub fn apply_saved_app_rules() {
    let rules = load_app_rules();
    if rules.is_empty() {
        return;
    }
    let Ok(apps_json) = get_apps() else { return };
    let Ok(apps) = serde_json::from_str::<Vec<serde_json::Value>>(&apps_json) else {
        return;
    };

    for app in &apps {
        let binary = app["binary"].as_str().unwrap_or("");
        let name = app["name"].as_str().unwrap_or("");
        let current = app["channel"].as_str().unwrap_or("");
        let Some(id) = json_u32(&app["id"]) else { continue };

        // Binary first: it is the stabler identifier. Display names drift
        // with whatever the app puts in media.name.
        let target = rules
            .get(&binary.to_lowercase())
            .or_else(|| rules.get(&name.to_lowercase()));
        let Some(target) = target else { continue };

        if target == current {
            continue;
        }
        if let Err(e) = route_app(id, target.clone(), binary.to_string()) {
            log::debug!("apply_saved_app_rules: {binary}/{name} → {target} failed: {e}");
        }
    }
}

/// Route an app back to the system default sink ("unroute").
pub fn unroute_app(app_id: u32, binary: String) -> Result<(), String> {
    route_app(app_id, "default".to_string(), binary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_u32_accepts_a_real_number() {
        assert_eq!(json_u32(&serde_json::json!(33191)), Some(33191));
    }

    #[test]
    fn json_u32_accepts_the_daemons_stringified_form() {
        // GetApps returns {"id":"33191"} — reading only as_u64() here made
        // apply_saved_app_rules skip every app while reporting success.
        assert_eq!(json_u32(&serde_json::json!("33191")), Some(33191));
        assert_eq!(json_u32(&serde_json::json!(" 42 ")), Some(42));
    }

    #[test]
    fn json_u32_rejects_values_that_are_not_ids() {
        assert_eq!(json_u32(&serde_json::json!("")), None);
        assert_eq!(json_u32(&serde_json::json!("abc")), None);
        assert_eq!(json_u32(&serde_json::json!(null)), None);
        assert_eq!(json_u32(&serde_json::json!(-1)), None);
        // Wider than u32 must not silently truncate into a wrong stream id.
        assert_eq!(json_u32(&serde_json::json!(u64::from(u32::MAX) + 1)), None);
    }

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

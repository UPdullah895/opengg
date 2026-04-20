//! Tauri Commands — complete backend for OpenGG.
//!
//! Audio routing ported from the working Python pulsectl code:
//!   pulse.sink_input_move(si.index, sink.index)
//! → pactl move-sink-input <si_index:u32> <sink_index:u32>

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use tauri::{command, AppHandle, Emitter, Manager};

const AU_PATH: &str = "/org/opengg/Daemon/Audio";
const AU_IFACE: &str = "org.opengg.Daemon.Audio";
const RP_PATH: &str = "/org/opengg/Daemon/Replay";
const RP_IFACE: &str = "org.opengg.Daemon.Replay";
const DV_PATH: &str = "/org/opengg/Daemon/Device";
const DV_IFACE: &str = "org.opengg.Daemon.Device";

// ═══ Audio ═══
#[command] pub async fn get_channels() -> Result<String, String> { call_dbus("GetChannels", AU_PATH, AU_IFACE, ()).await }
/// Volume control with pactl fallback — controls both sinks and sink-inputs
#[command]
pub async fn set_volume(channel: String, volume: u32) -> Result<(), String> {
    // Try D-Bus first
    if call_dbus_void("SetVolume", AU_PATH, AU_IFACE, (channel.as_str(), volume)).await.is_ok() {
        return Ok(());
    }
    // Direct pactl fallback
    let pct = format!("{volume}%");
    if channel == "Master" {
        run_cmd("pactl", &["set-sink-volume", "@DEFAULT_SINK@", &pct])?;
    } else if channel == "Mic" {
        run_cmd("pactl", &["set-source-volume", "@DEFAULT_SOURCE@", &pct])?;
    } else {
        let sink = format!("OpenGG_{channel}");
        run_cmd("pactl", &["set-sink-volume", &sink, &pct])?;
    }
    Ok(())
}

#[command]
pub async fn set_mute(channel: String, muted: bool) -> Result<(), String> {
    if call_dbus_void("SetMute", AU_PATH, AU_IFACE, (channel.as_str(), muted)).await.is_ok() {
        return Ok(());
    }
    let val = if muted { "1" } else { "0" };
    if channel == "Master" {
        run_cmd("pactl", &["set-sink-mute", "@DEFAULT_SINK@", val])?;
    } else if channel == "Mic" {
        run_cmd("pactl", &["set-source-mute", "@DEFAULT_SOURCE@", val])?;
    } else {
        run_cmd("pactl", &["set-sink-mute", &format!("OpenGG_{channel}"), val])?;
    }
    Ok(())
}

/// Unmute any WebKit video/webaudio sink-inputs belonging to this app.
/// Called whenever clip playback starts to counteract module-stream-restore
/// auto-muting the media.role="video" stream.
#[command]
pub async fn unmute_media_streams() -> Result<(), String> {
    let output = Command::new("pactl")
        .args(["list", "sink-inputs"])
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&output.stdout);

    let mut current_id: Option<u32> = None;
    let mut is_opengg = false;
    let mut is_media  = false;

    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Sink Input #") {
            if let Some(id) = current_id {
                if is_opengg && is_media {
                    let _ = Command::new("pactl")
                        .args(["set-sink-input-mute", &id.to_string(), "0"])
                        .output();
                }
            }
            current_id = rest.parse().ok();
            is_opengg  = false;
            is_media   = false;
        } else if t.contains(r#"application.name = "opengg""#) {
            is_opengg = true;
        } else if t.contains(r#"media.role = "video""#) || t.contains(r#"media.role = "webaudio""#) {
            is_media = true;
        }
    }
    // handle last block
    if let Some(id) = current_id {
        if is_opengg && is_media {
            let _ = Command::new("pactl")
                .args(["set-sink-input-mute", &id.to_string(), "0"])
                .output();
        }
    }
    Ok(())
}

/// Set volume for an individual app (sink-input) by its pactl index
#[command]
pub async fn set_app_volume(app_index: u32, volume: u32) -> Result<(), String> {
    let pct = format!("{volume}%");
    run_cmd("pactl", &["set-sink-input-volume", &app_index.to_string(), &pct])?;
    Ok(())
}
#[command] pub async fn get_apps() -> Result<String, String> {
    match call_dbus("GetApps", AU_PATH, AU_IFACE, ()).await {
        Ok(r) => Ok(r),
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

#[command]
pub async fn route_app(app: tauri::AppHandle, app_id: u32, channel: String) -> Result<(), String> {
    // Try D-Bus daemon first
    match call_dbus_void("RouteApp", AU_PATH, AU_IFACE, (app_id, channel.as_str())).await {
        Ok(()) => {
            let _ = app.emit("audio-mixer-refresh", ());
            return Ok(());
        }
        Err(e) => eprintln!("route_app: D-Bus route failed ({e}), falling back to pactl"),
    }

    log_routing_context(app_id, &channel);

    // Resolve the target sink name once — used by both pactl and pw-metadata paths
    let sink_name = if channel == "default" || channel == "Master" {
        run_cmd("pactl", &["get-default-sink"])?
    } else {
        let name = format!("OpenGG_{channel}");
        ensure_sink_exists(&name, &channel).await?;
        name
    };

    // Get target sink's INTEGER INDEX (needed by pactl move-sink-input)
    let sink_idx = get_sink_index_by_name(&sink_name)?;

    let cmd_str = format!("pactl move-sink-input {} {}", app_id, sink_idx);

    // Attempt 1: app_id IS the correct pactl sink-input index — with 3 retries
    for attempt in 0..=2 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(150));
        }

        if !validate_sink_input_exists(app_id) {
            if attempt == 0 {
                eprintln!("route_app: sink-input {app_id} not found in pactl list, skipping to PW-ID scan...");
            }
            break;
        }

        eprintln!("route_app: [attempt {}/3] executing: {cmd_str}", attempt + 1);
        let r = Command::new("pactl")
            .args(["move-sink-input", &app_id.to_string(), &sink_idx.to_string()])
            .output()
            .map_err(|e| format!("{cmd_str} — exec error: {e}"))?;

        if r.status.success() {
            eprintln!("route_app: ✓ attempt {}/3: sink-input {app_id} → sink #{sink_idx} ({channel})", attempt + 1);
            let _ = app.emit("audio-mixer-refresh", ());
            return Ok(());
        }

        let err = String::from_utf8_lossy(&r.stderr);
        eprintln!("route_app: attempt {}/3 failed ({err})", attempt + 1);

        if attempt == 2 {
            eprintln!("route_app: all 3 attempts exhausted for direct index, scanning for correct sink-input...");
        }
    }

    // Attempt 2: app_id might be a PipeWire node ID — find the correct pactl sink-input index
    if let Ok(si_idx) = find_pactl_si_for_pw_id(app_id) {
        let cmd2 = format!("pactl move-sink-input {} {}", si_idx, sink_idx);
        eprintln!("route_app: found PW#{app_id} → pactl si#{si_idx}, executing: {cmd2}");
        let r2 = Command::new("pactl")
            .args(["move-sink-input", &si_idx.to_string(), &sink_idx.to_string()])
            .output()
            .map_err(|e| format!("{cmd2} — exec error: {e}"))?;
        if r2.status.success() {
            eprintln!("route_app: ✓ PW#{app_id} → pactl si#{si_idx} → sink #{sink_idx} ({channel})");
            let _ = app.emit("audio-mixer-refresh", ());
            return Ok(());
        }
        let err2 = String::from_utf8_lossy(&r2.stderr);
        eprintln!("route_app: corrected pactl index also failed ({err2}), trying pw-metadata...");
    } else {
        eprintln!("route_app: no pactl sink-input found for PW#{app_id}, trying pw-metadata...");
    }

    // Attempt 3: pw-metadata target.node — WirePlumber-native stream move.
    // Resolves the sink's PipeWire node.id (different from the pactl integer index)
    // and writes it into the WirePlumber settings metadata so WirePlumber moves the stream.
    match get_pw_node_id_for_sink(&sink_name) {
        Ok(sink_pw_id) => {
            eprintln!(
                "route_app: [pw-metadata] node#{app_id} → target.node={sink_pw_id} (sink '{}', channel '{channel}')",
                sink_name
            );
            if let Err(e) = route_via_pw_metadata(app_id, sink_pw_id) {
                eprintln!("route_app: pw-metadata failed: {e}");
                return Err(format!(
                    "route_app: all routing methods failed for id {app_id} → {channel}. \
                     See logs above for diagnostics."
                ));
            }
            eprintln!("route_app: ✓ pw-metadata succeeded for {app_id} → {channel}");
            let _ = app.emit("audio-mixer-refresh", ());
            Ok(())
        }
        Err(e) => {
            eprintln!("route_app: cannot resolve sink '{}' PW node.id: {e}", sink_name);
            Err(format!(
                "route_app: all routing methods failed for id {app_id} → {channel}. \
                 Sink PW node.id unavailable: {e}"
            ))
        }
    }
}

/// Get default sink's pactl integer index
fn get_default_sink_index() -> Result<u32, String> {
    let name = run_cmd("pactl", &["get-default-sink"])?;
    get_sink_index_by_name(&name)
}

/// Look up sink's pactl integer index by name — mirrors pulsectl.sink_list()
fn get_sink_index_by_name(sink_name: &str) -> Result<u32, String> {
    let j = run_cmd("pactl", &["-f", "json", "list", "sinks"])?;
    let sinks: Vec<serde_json::Value> = serde_json::from_str(&j).map_err(|e| format!("{e}"))?;
    for s in &sinks {
        if s["name"].as_str() == Some(sink_name) {
            if let Some(idx) = s["index"].as_u64() { return Ok(idx as u32); }
        }
    }
    Err(format!("sink '{sink_name}' not found"))
}

/// Extended sink-input info for cross-referencing PipeWire IDs
#[derive(Debug)]
struct SiInfo {
    idx: u32,
    app_name: String,
    binary: String,
    pw_ids: Vec<u32>,
}

/// Build a comprehensive map of all sink-inputs with their PW node IDs and app metadata.
fn build_si_map() -> Result<HashMap<u32, SiInfo>, String> {
    let j = run_cmd("pactl", &["-f", "json", "list", "sink-inputs"])?;
    let sis: Vec<serde_json::Value> = serde_json::from_str(&j).map_err(|e| format!("parse sink-inputs: {e}"))?;
    let mut map = HashMap::new();
    for si in sis {
        let idx = si["index"].as_u64().unwrap_or(0) as u32;
        let p = &si["properties"];
        let app_name = p["application.name"].as_str()
            .or(p["media.name"].as_str())
            .unwrap_or("")
            .to_string();
        let binary = p["application.process.binary"].as_str().unwrap_or("").to_string();

        let mut pw_ids = Vec::new();
        for key in ["object.serial", "object.id", "node.id",
                    "pipewire.access.portal.app_id", "pipewire.client.access"] {
            if let Some(s) = p[key].as_str() {
                if let Ok(id) = s.parse::<u32>() {
                    pw_ids.push(id);
                }
            }
        }
        // Some clients expose the node ID embedded in media.name or application.name
        if let Some(s) = p["media.name"].as_str() {
            if let Ok(id) = s.parse::<u32>() { pw_ids.push(id); }
        }

        map.insert(idx, SiInfo { idx, app_name, binary, pw_ids });
    }
    Ok(map)
}

/// Check whether a sink-input index currently exists in the system.
fn validate_sink_input_exists(si_idx: u32) -> bool {
    if let Ok(j) = run_cmd("pactl", &["-f", "json", "list", "sink-inputs"]) {
        if let Ok(sis) = serde_json::from_str::<Vec<serde_json::Value>>(&j) {
            return sis.iter().any(|si| si["index"].as_u64() == Some(si_idx as u64));
        }
    }
    false
}

/// Cross-reference PipeWire node ID → pactl sink-input index using the full SI map.
fn find_pactl_si_for_pw_id(pw_id: u32) -> Result<u32, String> {
    let map = build_si_map()?;
    for (idx, info) in &map {
        if info.pw_ids.contains(&pw_id) {
            return Ok(*idx);
        }
    }
    Err(format!("no pactl si for PW#{pw_id}"))
}

/// Resolve a PipeWire sink's node.id (PW namespace) from its pactl name.
/// The PW node.id is stored in the sink's properties["node.id"] in pactl JSON output
/// and is the correct identifier for pw-metadata target.node writes.
fn get_pw_node_id_for_sink(sink_name: &str) -> Result<u32, String> {
    let j = run_cmd("pactl", &["-f", "json", "list", "sinks"])?;
    let sinks: Vec<serde_json::Value> = serde_json::from_str(&j)
        .map_err(|e| format!("parse sinks: {e}"))?;
    for s in &sinks {
        if s["name"].as_str() == Some(sink_name) {
            // WirePlumber surfaces the PW node.id as a string property
            if let Some(nid) = s["properties"]["node.id"].as_str()
                .and_then(|v| v.parse::<u32>().ok())
            {
                return Ok(nid);
            }
            // Fallback: object.id is the same value on older PipeWire builds
            if let Some(nid) = s["properties"]["object.id"].as_str()
                .and_then(|v| v.parse::<u32>().ok())
            {
                return Ok(nid);
            }
        }
    }
    Err(format!("no PW node.id found for sink '{sink_name}'"))
}

/// Route a stream via WirePlumber metadata — the correct PipeWire-native move.
///
/// `pw-metadata -n settings <stream_node_id> target.node <sink_node_id>`
///
/// WirePlumber watches the "settings" metadata namespace and moves the stream
/// when it sees a `target.node` entry for a managed stream node. This is
/// the same mechanism used internally by pavucontrol and the GNOME audio panel.
fn route_via_pw_metadata(stream_pw_id: u32, sink_pw_id: u32) -> Result<(), String> {
    eprintln!(
        "route_via_pw_metadata: pw-metadata -n settings {} target.node {}",
        stream_pw_id, sink_pw_id
    );
    let r = Command::new("pw-metadata")
        .args([
            "-n", "settings",
            &stream_pw_id.to_string(),
            "target.node",
            &sink_pw_id.to_string(),
        ])
        .output()
        .map_err(|e| format!("pw-metadata exec error: {e}"))?;
    if r.status.success() { return Ok(()); }
    Err(format!("pw-metadata: {}", String::from_utf8_lossy(&r.stderr).trim()))
}

/// Log full environment context and PipeWire state before routing attempts.
fn log_routing_context(app_id: u32, channel: &str) {
    eprintln!("=== route_app[{} → {channel}] environment context ===", app_id);
    eprintln!("  PULSE_SERVER:        {:?}", std::env::var("PULSE_SERVER").ok());
    eprintln!("  PIPEWIRE_DEBUG:      {:?}", std::env::var("PIPEWIRE_DEBUG").ok());
    eprintln!("  XDG_SESSION_TYPE:    {:?}", std::env::var("XDG_SESSION_TYPE").ok());
    eprintln!("  WAYLAND_DISPLAY:     {:?}", std::env::var("WAYLAND_DISPLAY").ok());
    eprintln!("  DISPLAY:             {:?}", std::env::var("DISPLAY").ok());
    eprintln!("  XDG_CURRENT_DESKTOP: {:?}", std::env::var("XDG_CURRENT_DESKTOP").ok());

    // Resolve stream identity: find node.name and binary for app_id in the SI map
    match build_si_map() {
        Ok(map) => {
            let matched: Vec<_> = map.values()
                .filter(|info| info.pw_ids.contains(&app_id) || info.idx == app_id)
                .collect();
            if matched.is_empty() {
                eprintln!("  stream id={app_id}: not found in current pactl sink-inputs (may be a PW node ID)");
            } else {
                for info in &matched {
                    eprintln!(
                        "  stream id={app_id}: pactl-idx={} name='{}' binary='{}'",
                        info.idx, info.app_name, info.binary
                    );
                }
            }
        }
        Err(e) => eprintln!("  stream id={app_id}: SI map unavailable ({e})"),
    }

    // Resolve target sink PW node.id (what pw-metadata will use)
    let sink_name = if channel == "default" || channel == "Master" {
        run_cmd("pactl", &["get-default-sink"]).unwrap_or_default()
    } else {
        format!("OpenGG_{channel}")
    };
    match get_pw_node_id_for_sink(&sink_name) {
        Ok(nid) => eprintln!("  target sink '{}' → PW node.id={}", sink_name, nid),
        Err(e)  => eprintln!("  target sink '{}' → PW node.id unavailable: {}", sink_name, e),
    }

    if let Ok(j) = run_cmd("pactl", &["-f", "json", "list", "sinks"]) {
        let count = serde_json::from_str::<Vec<serde_json::Value>>(&j).map(|v| v.len()).unwrap_or(0);
        eprintln!("  pactl sinks visible: {count}");
    }

    eprintln!("====================================================");
}

/// Ensure virtual sink exists (non-blocking, with extended settling time)
async fn ensure_sink_exists(name: &str, ch: &str) -> Result<(), String> {
    if let Ok(o) = Command::new("pactl").args(["list", "sinks", "short"]).output() {
        if String::from_utf8_lossy(&o.stdout).contains(name) { return Ok(()); }
    }
    eprintln!("ensure_sink_exists: creating virtual sink '{name}' for channel '{ch}'");
    let c = Command::new("pactl").args(["load-module", "module-null-sink",
        &format!("sink_name={name}"),
        &format!("sink_properties=node.description=\"OpenGG - {ch}\" node.nick=\"OpenGG - {ch}\" device.description=\"OpenGG - {ch}\" media.name=\"OpenGG - {ch}\" node.name=\"opengg_{}\"", ch.to_lowercase()),
        "channels=2", "channel_map=front-left,front-right"
    ]).output().map_err(|e| format!("{e}"))?;
    if !c.status.success() {
        let err = String::from_utf8_lossy(&c.stderr);
        eprintln!("ensure_sink_exists: pactl load-module FAILED: {err}");
        return Err(err.to_string());
    }
    // Increase settling time: 600ms to allow PipeWire to fully enumerate the new sink
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    // Set up loopback links to default sink
    if let Ok(def) = run_cmd("pactl", &["get-default-sink"]) {
        for p in ["FL", "FR"] {
            let result = Command::new("pw-link")
                .args([&format!("{name}:monitor_{p}"), &format!("{def}:playback_{p}")])
                .output();
            if let Err(e) = result {
                eprintln!("ensure_sink_exists: pw-link {name}:monitor_{p} → {def}:playback_{p}: {e}");
            }
        }
    }
    eprintln!("ensure_sink_exists: sink '{name}' ready");
    Ok(())
}

/// Scan sink-inputs — mirrors pulsectl.sink_input_list()
/// ★ FIX 2: Filters out streams going to OpenGG virtual sinks' monitors
fn scan_sink_inputs() -> Result<String, String> {
    let j = run_cmd("pactl", &["-f", "json", "list", "sink-inputs"])?;
    let sis: Vec<serde_json::Value> = serde_json::from_str(&j).map_err(|e| format!("{e}"))?;
    let mut apps = Vec::new();
    for si in &sis {
        let idx = si["index"].as_u64().unwrap_or(0) as u32;
        let p = &si["properties"];
        let name = p["application.name"].as_str()
            .or(p["media.name"].as_str())
            .or(p["application.process.binary"].as_str())
            .unwrap_or("Unknown");
        let binary = p["application.process.binary"].as_str().unwrap_or("");

        // ★ FIX 2: Skip internal PipeWire/WirePlumber streams
        let name_lower = name.to_lowercase();
        if name_lower.contains("opengg") || name_lower == "wireplumber"
            || name_lower == "pipewire" || name_lower.contains("peak detect") {
            continue;
        }

        let sink_idx = si["sink"].as_u64().unwrap_or(0) as u32;
        let channel = lookup_sink_channel(sink_idx);
        let auto_channel = classify_channel(&p);

        // Include volume info (0-100) for per-app volume control
        let vol = si["volume"].as_object()
            .and_then(|v| v.values().next())
            .and_then(|ch| ch["value_percent"].as_str())
            .and_then(|s| s.trim_end_matches('%').parse::<u32>().ok())
            .unwrap_or(100);

        apps.push(serde_json::json!({
            "id": idx, "name": name, "binary": binary,
            "channel": channel, "icon": "", "volume": vol,
            "auto_channel": auto_channel
        }));
    }

    // ★ Epic 3: Also scan source-outputs — apps recording from the mic
    // IDs are offset by 90000 to avoid conflicts with sink-input IDs.
    if let Ok(sj) = run_cmd("pactl", &["-f", "json", "list", "source-outputs"]) {
        if let Ok(sos) = serde_json::from_str::<Vec<serde_json::Value>>(&sj) {
            for (idx, so) in sos.iter().enumerate() {
                let p = &so["properties"];
                let name = p["application.name"].as_str()
                    .or(p["media.name"].as_str())
                    .or(p["application.process.binary"].as_str())
                    .unwrap_or("Unknown");
                let binary = p["application.process.binary"].as_str().unwrap_or("");
                let name_lower = name.to_lowercase();
                if name_lower.contains("opengg") || name_lower == "wireplumber"
                    || name_lower == "pipewire" || name_lower.contains("peak detect") {
                    continue;
                }
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

/// Suggest an OpenGG channel for an app based on PipeWire stream properties.
/// Returns an empty string when no confident classification can be made.
fn classify_channel(props: &serde_json::Value) -> &'static str {
    let role   = props["media.role"].as_str().unwrap_or("").to_lowercase();
    let binary = props["application.process.binary"].as_str().unwrap_or("").to_lowercase();
    let name   = props["application.name"].as_str().unwrap_or("").to_lowercase();

    // media.role is the most authoritative signal (set by the app itself)
    match role.as_str() {
        "game"                          => return "Game",
        "music" | "video" | "movie"    => return "Media",
        "phone" | "communication"      => return "Chat",
        _ => {}
    }

    // Binary / app-name heuristics
    const CHAT_BINS: &[&str]  = &["discord", "teamspeak", "mumble", "signal", "telegram",
                                   "zoom", "slack", "skype", "element", "hexchat"];
    const GAME_BINS: &[&str]  = &["steam", "heroic", "lutris", "wine", "proton",
                                   "gameoverlayui", "gamescope", "mangohud"];
    const MEDIA_BINS: &[&str] = &["spotify", "rhythmbox", "clementine", "vlc", "mpv",
                                   "celluloid", "strawberry", "quodlibet", "cmus", "lollypop",
                                   "elisa", "audacious"];

    if CHAT_BINS.iter().any(|b| binary.contains(b) || name.contains(b))  { return "Chat"; }
    if GAME_BINS.iter().any(|b| binary.contains(b) || name.contains(b))  { return "Game"; }
    if MEDIA_BINS.iter().any(|b| binary.contains(b) || name.contains(b)) { return "Media"; }

    ""  // No confident match — leave in Master
}

fn lookup_sink_channel(sink_idx: u32) -> String {
    if let Ok(j) = run_cmd("pactl", &["-f","json","list","sinks"]) {
        if let Ok(sinks) = serde_json::from_str::<Vec<serde_json::Value>>(&j) {
            for s in &sinks {
                if s["index"].as_u64() == Some(sink_idx as u64) {
                    let n = s["name"].as_str().unwrap_or("");
                    if let Some(ch) = n.strip_prefix("OpenGG_") { return ch.into(); }
                }
            }
        }
    }
    String::new()
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 1: Media Analysis via ffprobe
// ══════════════════════════════════════════════════════════════

#[derive(Serialize)]
pub struct MediaStream {
    pub index: u32,
    pub codec_type: String,  // "video" | "audio" | "subtitle"
    pub codec_name: String,  // "h264", "aac", "opus", etc.
    pub channels: u32,       // audio channel count (2=stereo)
    pub sample_rate: String,
    pub language: String,
    pub title: String,       // track title if set (e.g. "Game Audio", "Mic")
}

#[derive(Serialize)]
pub struct MediaInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub video_codec: String,
    pub streams: Vec<MediaStream>,
    pub video_streams: u32,
    pub audio_streams: u32,
}

#[command]
pub async fn analyze_media(filepath: String) -> Result<MediaInfo, String> {
    let output = Command::new("ffprobe")
        .args(["-v", "quiet", "-print_format", "json",
            "-show_format", "-show_streams", &filepath])
        .output()
        .map_err(|e| format!("ffprobe: {e}"))?;

    if !output.status.success() {
        return Err(format!("ffprobe failed: {}", String::from_utf8_lossy(&output.stderr)));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("parse: {e}"))?;

    let fmt = &json["format"];
    let duration: f64 = fmt["duration"].as_str()
        .and_then(|s| s.parse().ok()).unwrap_or(0.0);

    let mut streams = Vec::new();
    let mut width = 0u32;
    let mut height = 0u32;
    let mut fps = 0.0f64;
    let mut video_codec = String::new();
    let mut video_count = 0u32;
    let mut audio_count = 0u32;

    if let Some(arr) = json["streams"].as_array() {
        for s in arr {
            let codec_type = s["codec_type"].as_str().unwrap_or("").to_string();
            let codec_name = s["codec_name"].as_str().unwrap_or("").to_string();
            let idx = s["index"].as_u64().unwrap_or(0) as u32;
            let tags = &s["tags"];
            let lang = tags["language"].as_str().unwrap_or("").to_string();
            let title = tags["title"].as_str().unwrap_or("").to_string();

            match codec_type.as_str() {
                "video" => {
                    video_count += 1;
                    if width == 0 {
                        width = s["width"].as_u64().unwrap_or(0) as u32;
                        height = s["height"].as_u64().unwrap_or(0) as u32;
                        video_codec = codec_name.clone();
                        // Parse fps from r_frame_rate "60/1" or "30000/1001"
                        if let Some(rfr) = s["r_frame_rate"].as_str() {
                            let parts: Vec<&str> = rfr.split('/').collect();
                            if parts.len() == 2 {
                                let n: f64 = parts[0].parse().unwrap_or(0.0);
                                let d: f64 = parts[1].parse().unwrap_or(1.0);
                                if d > 0.0 { fps = n / d; }
                            }
                        }
                    }
                    streams.push(MediaStream {
                        index: idx, codec_type, codec_name,
                        channels: 0, sample_rate: String::new(), language: lang, title,
                    });
                }
                "audio" => {
                    audio_count += 1;
                    let ch = s["channels"].as_u64().unwrap_or(2) as u32;
                    let sr = s["sample_rate"].as_str().unwrap_or("48000").to_string();
                    let track_title = if title.is_empty() {
                        format!("Audio {audio_count}")
                    } else { title };
                    streams.push(MediaStream {
                        index: idx, codec_type, codec_name,
                        channels: ch, sample_rate: sr, language: lang, title: track_title,
                    });
                }
                _ => {}
            }
        }
    }

    Ok(MediaInfo {
        duration, width, height, fps, video_codec,
        streams, video_streams: video_count, audio_streams: audio_count,
    })
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 4: File Renaming
// ══════════════════════════════════════════════════════════════

#[command]
pub async fn rename_clip(old_path: String, new_name: String) -> Result<String, String> {
    let old = PathBuf::from(&old_path);
    if !old.exists() { return Err(format!("File not found: {old_path}")); }

    let ext = old.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let dir = old.parent().unwrap_or(Path::new("."));
    // Sanitize filename
    // ★ Epic 2: Allow Unicode (Arabic/CJK) — only strip OS-illegal path chars
    let safe_name: String = new_name.chars()
        .filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\0'))
        .collect::<String>().trim().to_string();
    let new_path = dir.join(format!("{safe_name}.{ext}"));

    if new_path.exists() { return Err("A file with that name already exists".into()); }

    std::fs::rename(&old, &new_path).map_err(|e| format!("rename: {e}"))?;

    // Update SQLite metadata
    if let Ok(db) = open_db() {
        let _ = db.execute("UPDATE clip_meta SET filepath=?1 WHERE filepath=?2",
            rusqlite::params![new_path.to_string_lossy().to_string(), old_path]);
        let _ = db.execute("UPDATE trim_state SET filepath=?1 WHERE filepath=?2",
            rusqlite::params![new_path.to_string_lossy().to_string(), old_path]);
    }

    // Rename thumbnail too
    let old_id = format!("{:x}", hash_str(&old_path));
    let new_fp = new_path.to_string_lossy().to_string();
    let new_id = format!("{:x}", hash_str(&new_fp));
    let td = thumb_dir();
    let old_thumb = td.join(format!("{old_id}.jpg"));
    let new_thumb = td.join(format!("{new_id}.jpg"));
    if old_thumb.exists() { let _ = std::fs::rename(&old_thumb, &new_thumb); }

    Ok(new_fp)
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 5: Timeline Export (placeholder — builds ffmpeg args)
// ══════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct TimelineExportClip {
    pub filepath: String,
    pub start: f64,
    pub end: f64,
    pub track: String,
}

#[command]
pub async fn export_timeline(
    clips: Vec<TimelineExportClip>,
    output_dir: String,
    output_name: String,
) -> Result<String, String> {
    if clips.is_empty() { return Err("No clips to export".into()); }

    let dir = if output_dir.is_empty() { default_clips_dir() } else { PathBuf::from(shexp(&output_dir)) };
    let _ = std::fs::create_dir_all(&dir);
    let safe: String = output_name.chars().filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\0')).collect::<String>().trim().to_string();
    let outfile = dir.join(format!("{}.mp4", if safe.is_empty() { "export" } else { &safe }));

    // For single-source trim (most common case), use stream copy
    let video_clips: Vec<&TimelineExportClip> = clips.iter().filter(|c| c.track == "video").collect();
    if video_clips.len() == 1 {
        let vc = video_clips[0];
        let r = Command::new("ffmpeg")
            .args(["-i", &vc.filepath, "-ss", &format!("{:.3}", vc.start),
                "-to", &format!("{:.3}", vc.end), "-c", "copy",
                "-avoid_negative_ts", "make_zero", "-y",
                &outfile.to_string_lossy()])
            .output().map_err(|e| format!("{e}"))?;
        if r.status.success() { return Ok(outfile.to_string_lossy().to_string()); }
        return Err(format!("ffmpeg: {}", String::from_utf8_lossy(&r.stderr)));
    }

    // Multi-clip: use ffmpeg concat demuxer (future — complex filter graph)
    // For now, return a descriptive error
    Err(format!("Multi-clip export ({} clips) coming soon. Use single-clip trim for now.", video_clips.len()))
}

/// Generate audio waveform peaks data for visualization.
/// Uses ffmpeg to extract PCM samples, then computes peaks.
/// Returns JSON array of peak values (0.0-1.0) for the given audio stream.
#[command]
pub async fn generate_waveform(filepath: String, stream_index: u32, num_peaks: u32) -> Result<Vec<f32>, String> {
    let peaks_count = num_peaks.max(100).min(2000);

    // Extract raw PCM audio from the specified stream
    let output = Command::new("ffmpeg")
        .args(["-i", &filepath, "-map", &format!("0:{stream_index}"),
            "-ac", "1", "-f", "s16le", "-ar", "8000", "-"])
        .output()
        .map_err(|e| format!("ffmpeg waveform: {e}"))?;

    if !output.status.success() || output.stdout.is_empty() {
        return Ok(vec![0.0; peaks_count as usize]);
    }

    // Parse raw s16le samples
    let samples: Vec<i16> = output.stdout.chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();

    if samples.is_empty() {
        return Ok(vec![0.0; peaks_count as usize]);
    }

    // Downsample to requested peak count
    let chunk_size = (samples.len() / peaks_count as usize).max(1);
    let peaks: Vec<f32> = (0..peaks_count as usize)
        .map(|i| {
            let start = i * chunk_size;
            let end = (start + chunk_size).min(samples.len());
            let max_abs = samples[start..end].iter()
                .map(|s| s.unsigned_abs() as f32)
                .fold(0.0f32, f32::max);
            (max_abs / 32768.0).min(1.0)
        })
        .collect();

    Ok(peaks)
}

// ══════════════════════════════════════════════════════════════
//  ★ Epic 3 P3+P4: Export with audio downmix + overlay burn
// ══════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct ExportOverlay {
    pub overlay_type: String,       // "text" | "image" | "gif"
    pub content: String,            // text string or file path
    pub x: f64,                    // percentage 0-100
    pub y: f64,
    pub scale: f64,
    pub start_sec: f64,
    pub dur_sec: f64,
    pub font_name: Option<String>, // e.g. "Impact", "Arial" — resolved to system font path
}

#[derive(Deserialize)]
pub struct ExportAudioTrack {
    pub stream_index: u32,
    pub volume: f64,  // 0.0-1.0
    pub muted: bool,
}

/// Export a clip with strict branching:
///
///  Condition A — No visual overlays → fast stream copy (`-c copy`).
///    Input-side seeking, zero quality loss, no re-encoding.
///
///  Condition B — Has visual overlays → filter_complex + libx264 re-encode.
///    Builds drawtext/overlay filter graph, burns in at CRF 18.
///
/// Stderr is captured to `stderr_log` so error details can be surfaced to the
/// user instead of silently disappearing into the terminal.
#[command]
pub async fn export_clip_with_filters(
    app: AppHandle,
    input_path: String,
    start_sec: f64,
    end_sec: f64,
    audio_tracks: Vec<ExportAudioTrack>,
    overlays: Vec<ExportOverlay>,
    target_mb: f64,
    output_path: String,
) -> Result<String, String> {
    let dur = end_sec - start_sec;
    if dur <= 0.0 { return Err("Invalid trim range".into()); }

    let mut out = if output_path.is_empty() { auto_name(&input_path, "_export") } else { smart_prefix(&output_path) };
    if out == input_path || Path::new(&out) == Path::new(&input_path) {
        out = auto_name(&input_path, "_export");
        eprintln!("export: output collided with input, renamed to {out}");
    }

    // Shared stderr accumulator — lets error path report the actual FFmpeg message.
    let stderr_log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // ─────────────────────────────────────────────────────────────
    // Condition A: NO valid visual overlays → stream copy (fast, lossless)
    // ─────────────────────────────────────────────────────────────
    let has_valid_overlays = overlays.iter().any(|o| match o.overlay_type.as_str() {
        "text"        => !o.content.is_empty(),
        "image" | "gif" => !o.content.is_empty() && Path::new(&o.content).exists(),
        _             => false,
    });

    if !has_valid_overlays {
        // Even with no visual overlays we still need correct audio handling:
        //   • Multiple audio tracks  → downmix to one AAC stream (amix).
        //   • Per-track volume/mute  → apply volume filter even in copy mode.
        //   • Single default track   → copy video + encode audio to AAC.
        // Without explicit -map ffmpeg only picks the "best" audio stream,
        // silently discarding any additional tracks.
        let audio_indices_a = get_audio_stream_global_indices(&input_path);
        let to_rel_a = |global_idx: u32| -> Option<u32> {
            audio_indices_a.iter().position(|&g| g == global_idx).map(|i| i as u32)
        };

        let valid_audio_a: Vec<(&ExportAudioTrack, u32)> = audio_tracks.iter()
            .filter(|t| !t.muted && t.volume > 0.0)
            .filter_map(|t| to_rel_a(t.stream_index).map(|rel| (t, rel)))
            .collect();

        let mut cond_a_fc: Vec<String> = Vec::new();
        let a_map: String = if audio_tracks.is_empty() {
            // No audio track info from frontend: map all source audio as-is.
            "0:a".to_string()
        } else if valid_audio_a.is_empty() {
            // All tracks muted — output silence.
            cond_a_fc.push("anullsrc=r=48000:cl=stereo:d=1[aout]".into());
            "[aout]".to_string()
        } else if valid_audio_a.len() == 1 && (valid_audio_a[0].0.volume - 1.0).abs() < 0.01 {
            // Single track at unity gain → no filter needed, direct map.
            format!("0:a:{}", valid_audio_a[0].1)
        } else {
            // Multiple tracks OR custom volume → build amix with per-track gain.
            let mut mix_ins = Vec::new();
            for (i, (t, rel)) in valid_audio_a.iter().enumerate() {
                let lbl = format!("[ca{i}]");
                cond_a_fc.push(format!("[0:a:{rel}]volume={:.4}{lbl}", t.volume));
                mix_ins.push(lbl);
            }
            let joined = mix_ins.join("");
            cond_a_fc.push(format!(
                "{joined}amix=inputs={}:normalize=0:duration=longest[amix]",
                mix_ins.len()
            ));
            "[amix]".to_string()
        };

        let mut args: Vec<String> = vec![
            "-y".into(),
            "-ss".into(), format!("{start_sec:.3}"),
            "-to".into(), format!("{end_sec:.3}"),
            "-i".into(), input_path.clone(),
        ];
        if !cond_a_fc.is_empty() {
            args.extend(["-filter_complex".into(), cond_a_fc.join(";")]);
        }
        args.extend([
            "-map".into(), "0:v".into(),
            "-map".into(), a_map,
            "-c:v".into(), "copy".into(),
            "-c:a".into(), "aac".into(),
            "-b:a".into(), "192k".into(),
            "-avoid_negative_ts".into(), "make_zero".into(),
            out.clone(),
        ]);
        eprintln!("export (copy+audio): ffmpeg {}", args.join(" "));
        let _ = app.emit("export-progress", serde_json::json!({"percent": 0, "stage": "copying", "speed": ""}));

        let mut child = Command::new("ffmpeg")
            .args(&args)
            .stderr(std::process::Stdio::piped())
            .spawn().map_err(|e| format!("ffmpeg: {e}"))?;

        let log_c = Arc::clone(&stderr_log);
        if let Some(stderr) = child.stderr.take() {
            tokio::task::spawn_blocking(move || {
                use std::io::{BufRead, BufReader};
                for line in BufReader::new(stderr).lines().flatten() {
                    eprintln!("ffmpeg: {line}");
                    log_c.lock().unwrap().push(line);
                }
            });
        }

        { *app.state::<crate::ExportProcess>().child.lock().unwrap() = Some((child, out.clone())); }

        let status = {
            let es = app.state::<crate::ExportProcess>();
            let mut lock = es.child.lock().unwrap();
            if let Some((ref mut c, _)) = *lock { Some(c.wait().map_err(|e| format!("ffmpeg: {e}"))?) } else { None }
        };
        { *app.state::<crate::ExportProcess>().child.lock().unwrap() = None; }
        for i in 0..20 { let _ = std::fs::remove_file(std::env::temp_dir().join(format!("opengg_text_{i}.txt"))); }

        return match status {
            Some(s) if s.success() => {
                let _ = app.emit("export-progress", serde_json::json!({"percent": 100, "stage": "done", "speed": ""}));
                Ok(out)
            }
            Some(_) => {
                std::thread::sleep(std::time::Duration::from_millis(80)); // let stderr thread flush
                let tail = stderr_log.lock().unwrap().iter().rev().take(8).rev().cloned().collect::<Vec<_>>().join("\n");
                let _ = app.emit("export-progress", serde_json::json!({"percent": -1, "stage": "error", "speed": "failed"}));
                Err(format!("FFmpeg stream copy failed.\n\nDetails:\n{tail}"))
            }
            None => Err("Export was cancelled".into()),
        };
    }

    // ─────────────────────────────────────────────────────────────
    // Condition B: Has overlays → filter_complex + libx264 re-encode
    // ─────────────────────────────────────────────────────────────
    let audio_global_indices = get_audio_stream_global_indices(&input_path);
    eprintln!("export (encode): audio streams {:?}", audio_global_indices);

    let to_audio_relative = |global_idx: u32| -> Option<u32> {
        audio_global_indices.iter().position(|&g| g == global_idx).map(|i| i as u32)
    };

    // Input-side seeking (before the main -i so PTS starts at 0)
    let mut inputs: Vec<String> = vec![
        "-ss".into(), format!("{start_sec:.3}"),
        "-to".into(), format!("{end_sec:.3}"),
        "-i".into(), input_path.clone(),
    ];
    let mut filter_parts: Vec<String> = Vec::new();
    let mut input_count = 1u32;
    let mut video_label = "[0:v]".to_string();

    // Audio filter graph
    let valid_audio: Vec<(&ExportAudioTrack, u32)> = audio_tracks.iter()
        .filter(|t| !t.muted && t.volume > 0.0)
        .filter_map(|t| to_audio_relative(t.stream_index).map(|rel| (t, rel)))
        .collect();
    let audio_label = if valid_audio.is_empty() {
        filter_parts.push("anullsrc=r=48000:cl=stereo:d=1[aout]".into());
        "[aout]".to_string()
    } else if valid_audio.len() == 1 {
        let (t, rel) = valid_audio[0];
        filter_parts.push(format!("[0:a:{rel}]volume={:.2}[amix]", t.volume));
        "[amix]".to_string()
    } else {
        let mut mix_ins = Vec::new();
        for (i, (t, rel)) in valid_audio.iter().enumerate() {
            let lbl = format!("[a{i}]");
            filter_parts.push(format!("[0:a:{rel}]volume={:.2}{lbl}", t.volume));
            mix_ins.push(lbl);
        }
        let mix_in = mix_ins.join("");
        filter_parts.push(format!("{mix_in}amix=inputs={}:duration=longest[amix]", valid_audio.len()));
        "[amix]".to_string()
    };

    // Probe resolution for proportional sizing
    let (src_w, src_h) = probe_resolution(&input_path);
    eprintln!("export (encode): source {src_w}x{src_h}");

    // Burn overlays into the video filter chain
    for ov in &overlays {
        let ov_start = (ov.start_sec - start_sec).max(0.0);
        let ov_end   = (ov.start_sec + ov.dur_sec - start_sec).min(dur);
        if ov_end <= ov_start { continue; }
        let enable = format!("between(t\\,{ov_start:.2}\\,{ov_end:.2})");
        match ov.overlay_type.as_str() {
            "text" => {
                let tmp = std::env::temp_dir().join(format!("opengg_text_{}.txt", filter_parts.len()));
                if std::fs::write(&tmp, &ov.content).is_err() { continue; }
                let fs = ((src_h as f64 / 1080.0) * 24.0 * ov.scale / 100.0).max(8.0) as u32;
                // Resolve font: honour user choice, fall back to best system font.
                let font = find_system_font_by_name(ov.font_name.as_deref());
                let x_expr = format!("(W*{}/100-tw/2)", ov.x as u32);
                let y_expr = format!("(H*{}/100-th/2)", ov.y as u32);
                let next = format!("[vov{}]", filter_parts.len());
                // Shadow (shadowx/y + alpha) replaces the old hard-coded black border.
                // Gives a natural depth effect without visual artefacts on bright bg.
                filter_parts.push(format!(
                    "{video_label}drawtext=fontfile='{font}':textfile='{}':fontsize={fs}:fontcolor=white:shadowx=2:shadowy=2:shadowcolor=black@0.65:x={x_expr}:y={y_expr}:enable='{enable}'{next}",
                    tmp.to_string_lossy()
                ));
                video_label = next;
            }
            "image" | "gif" => {
                if !ov.content.is_empty() && Path::new(&ov.content).exists() {
                    let is_gif = ov.overlay_type == "gif" || ov.content.to_lowercase().ends_with(".gif");
                    if is_gif { inputs.push("-ignore_loop".into()); inputs.push("0".into()); }
                    inputs.push("-i".into()); inputs.push(ov.content.clone());
                    let idx = input_count; input_count += 1;
                    // Force even pixel dimensions.
                    // `2*trunc(N/2)` is an FFmpeg expression that rounds N down
                    // to the nearest even integer — libx264 rejects odd dimensions.
                    let scale_w = ((src_w as f64 * ov.scale / 100.0 * 0.3) as u32 / 2 * 2).max(2);
                    let scale_lbl = format!("[sovl{}]", filter_parts.len());
                    // `format=rgba` converts GIF palette / PNG BGRA → RGBA before
                    // scale so the overlay filter always gets a well-defined pixel
                    // format. `-2` on height auto-rounds to even while preserving AR.
                    // DO NOT add a `loop` filter — `-ignore_loop 0` on the input
                    // is sufficient and the extra filter causes a fatal hang.
                    filter_parts.push(format!(
                        "[{idx}:v]format=rgba,scale=w=2*trunc({scale_w}/2):h=-2:flags=lanczos{scale_lbl}"
                    ));
                    let x_expr = format!("(W*{}/100-overlay_w/2)", ov.x as u32);
                    let y_expr = format!("(H*{}/100-overlay_h/2)", ov.y as u32);
                    let next = format!("[vov{}]", filter_parts.len());
                    filter_parts.push(format!(
                        "{video_label}{scale_lbl}overlay=x={x_expr}:y={y_expr}:enable='{enable}':shortest=1{next}"
                    ));
                    video_label = next;
                }
            }
            _ => {}
        }
    }

    // Belt-and-suspenders pixel format fix:
    //   1. format=yuv420p node at the END of the video chain (in-graph conversion).
    //   2. -pix_fmt yuv420p on the output side (encoder-level enforcement).
    // Together they handle every deprecated / unusual input format (yuvj420p,
    // bgra, indexed-color) that would otherwise make libx264 return -22.
    let fmt_label = format!("[vfmt{}]", filter_parts.len());
    filter_parts.push(format!("{video_label}format=yuv420p{fmt_label}"));

    let filter_complex = filter_parts.join(";");

    let mut args: Vec<String> = vec!["-y".into()];
    args.extend(inputs);
    args.extend(["-filter_complex".into(), filter_complex]);
    args.push("-map".into()); args.push(fmt_label);
    args.push("-map".into()); args.push(audio_label);

    if target_mb > 0.0 {
        let video_kbps = ((target_mb * 8192.0 / dur) - 128.0).max(100.0);
        args.extend(["-c:v".into(), "libx264".into(), "-pix_fmt".into(), "yuv420p".into(),
            "-b:v".into(), format!("{}k", video_kbps as u32), "-preset".into(), "fast".into()]);
    } else {
        args.extend(["-c:v".into(), "libx264".into(), "-pix_fmt".into(), "yuv420p".into(),
            "-crf".into(), "18".into(), "-preset".into(), "fast".into()]);
    }
    args.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "128k".into()]);
    args.push(out.clone());

    eprintln!("export (encode): ffmpeg {}", args.join(" "));
    let _ = app.emit("export-progress", serde_json::json!({"percent": 0, "stage": "encoding", "speed": ""}));

    let mut child = Command::new("ffmpeg")
        .args(&args)
        .stderr(std::process::Stdio::piped())
        .spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    if let Some(stderr) = child.stderr.take() {
        let app_c = app.clone();
        let log_c = Arc::clone(&stderr_log);
        tokio::task::spawn_blocking(move || {
            parse_ffmpeg_progress(stderr, dur, 0.0, 100.0, &app_c, Some(log_c));
        });
    }

    { *app.state::<crate::ExportProcess>().child.lock().unwrap() = Some((child, out.clone())); }

    let status = {
        let es = app.state::<crate::ExportProcess>();
        let mut lock = es.child.lock().unwrap();
        if let Some((ref mut c, _)) = *lock { Some(c.wait().map_err(|e| format!("ffmpeg: {e}"))?) } else { None }
    };
    { *app.state::<crate::ExportProcess>().child.lock().unwrap() = None; }
    for i in 0..20 { let _ = std::fs::remove_file(std::env::temp_dir().join(format!("opengg_text_{i}.txt"))); }

    match status {
        Some(s) if s.success() => {
            let _ = app.emit("export-progress", serde_json::json!({"percent": 100, "stage": "done", "speed": ""}));
            Ok(out)
        }
        Some(_) => {
            std::thread::sleep(std::time::Duration::from_millis(80));
            let tail = stderr_log.lock().unwrap().iter().rev().take(8).rev().cloned().collect::<Vec<_>>().join("\n");
            let _ = app.emit("export-progress", serde_json::json!({"percent": -1, "stage": "error", "speed": "failed"}));
            Err(format!("FFmpeg encode failed.\n\nDetails:\n{tail}"))
        }
        None => Err("Export was cancelled".into()),
    }
}

/// ★ Epic 3: Cancel the running FFmpeg export
#[command]
pub async fn cancel_export(app: AppHandle) -> Result<(), String> {
    let export_state = app.state::<crate::ExportProcess>();
    let mut lock = export_state.child.lock().unwrap();
    if let Some((mut child, output_path)) = lock.take() {
        eprintln!("cancel_export: killing FFmpeg process");
        let _ = child.kill();
        let _ = child.wait(); // reap zombie
        // Delete the partial/corrupt output file
        if std::path::Path::new(&output_path).exists() {
            let _ = std::fs::remove_file(&output_path);
            eprintln!("cancel_export: deleted partial file {output_path}");
        }
        let _ = app.emit("export-progress", serde_json::json!({"percent": -1, "stage": "cancelled", "speed": ""}));
        Ok(())
    } else {
        Ok(()) // nothing running
    }
}

// ═══ App Lifecycle ═══

/// ★ Epic 4: Graceful quit — shows window if hidden, then exits
#[command]
pub async fn quit_app(app: AppHandle) -> Result<(), String> {
    eprintln!("OpenGG: quit requested");
    {
        let state = app.state::<crate::ExportProcess>();
        let mut lock = state.child.lock().unwrap();
        if let Some((mut child, path)) = lock.take() {
            let _ = child.kill();
            let _ = child.wait();
            if std::path::Path::new(&path).exists() { let _ = std::fs::remove_file(&path); }
        }
    }
    std::process::exit(0);
}

// ═══ Replay ═══
#[command]
pub fn get_recorder_status(app: AppHandle) -> String {
    // Check live GsrProcess state first — covers the GPU Screen Recorder path.
    let gsr = app.state::<GsrProcess>();
    let mut lock = gsr.0.lock().unwrap();
    match lock.as_mut() {
        Some((child, params)) => match child.try_wait() {
            Ok(None) => {
                // Process still running — return replay:<secs>
                let secs = params.replay_secs;
                return format!("replay:{secs}");
            }
            _ => {
                // Exited or error — clean up state
                lock.take();
            }
        },
        None => {}
    }
    drop(lock);
    // Fallback to legacy D-Bus daemon path (non-GSR recorder)
    "idle".into()
}
#[command] pub async fn start_replay(duration: u32) -> Result<(), String> { call_dbus_void("StartReplay", RP_PATH, RP_IFACE, (duration,)).await }
#[command] pub async fn stop_recorder() -> Result<(), String> { call_dbus_void("Stop", RP_PATH, RP_IFACE, ()).await }
#[command] pub async fn save_replay() -> Result<(), String> { call_dbus_void("SaveReplay", RP_PATH, RP_IFACE, ()).await }

// ═══ SQLite ═══
fn clips_db_path() -> PathBuf { dirs::data_dir().unwrap_or_else(|| PathBuf::from("~/.local/share")).join("opengg/clips.db") }
fn open_db() -> Result<Connection, String> { Connection::open(clips_db_path()).map_err(|e| format!("DB: {e}")) }
pub fn init_clips_db() -> Result<(), String> {
    let p = clips_db_path(); if let Some(d) = p.parent() { std::fs::create_dir_all(d).ok(); }
    let db = open_db()?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS clip_meta(filepath TEXT PRIMARY KEY,custom_name TEXT DEFAULT '',favorite INTEGER DEFAULT 0,tags TEXT DEFAULT '',notes TEXT DEFAULT '');
     CREATE TABLE IF NOT EXISTS trim_state(filepath TEXT PRIMARY KEY,trim_start REAL DEFAULT 0,trim_end REAL DEFAULT 0);").map_err(|e| format!("{e}"))?;
    // Phase 3a: Add ffprobe cache columns (ALTER TABLE is a no-op if column already exists)
    let _ = db.execute("ALTER TABLE clip_meta ADD COLUMN duration REAL DEFAULT 0", []);
    let _ = db.execute("ALTER TABLE clip_meta ADD COLUMN width INTEGER DEFAULT 0", []);
    let _ = db.execute("ALTER TABLE clip_meta ADD COLUMN height INTEGER DEFAULT 0", []);
    let _ = db.execute("ALTER TABLE clip_meta ADD COLUMN mtime INTEGER DEFAULT 0", []);
    Ok(())
}
fn get_meta_map() -> HashMap<String,(String,bool,String)> {
    let mut m = HashMap::new();
    if let Ok(c) = open_db() {
        let _ = c.execute("ALTER TABLE clip_meta ADD COLUMN game_tag TEXT DEFAULT ''", []);
        if let Ok(mut s) = c.prepare("SELECT filepath,custom_name,favorite,COALESCE(game_tag,'') FROM clip_meta") {
            let _ = s.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,bool>(2)?,r.get::<_,String>(3)?))).map(|rows| { for r in rows.flatten() { m.insert(r.0,(r.1,r.2,r.3)); } });
        }
    }
    m
}

// ═══ Clips ═══
#[derive(Debug,Serialize,Clone)]
pub struct ClipInfo { pub id:String, pub filename:String, pub filepath:String, pub filesize:u64, pub created:String, pub duration:f64, pub width:u32, pub height:u32, pub game:String, pub custom_name:String, pub favorite:bool, pub thumbnail:String }
const VIDEO_EXTS: &[&str] = &["mp4","mkv","webm","avi","mov","ts","flv"];

// Phase 3a: ffprobe cache helpers
/// Read cached (duration, width, height) for a filepath if mtime matches.
fn probe_cache_get(db: &Connection, fp: &str, mtime: u64) -> Option<(f64, u32, u32)> {
    db.query_row(
        "SELECT duration, width, height FROM clip_meta WHERE filepath=?1 AND mtime=?2 AND duration>0",
        rusqlite::params![fp, mtime as i64],
        |r| Ok((r.get::<_, f64>(0)?, r.get::<_, u32>(1)?, r.get::<_, u32>(2)?)),
    ).ok()
}
/// Write (duration, width, height, mtime) to cache.
fn probe_cache_set(db: &Connection, fp: &str, dur: f64, w: u32, h: u32, mtime: u64) {
    let _ = db.execute(
        "INSERT INTO clip_meta(filepath,duration,width,height,mtime) VALUES(?1,?2,?3,?4,?5) \
         ON CONFLICT(filepath) DO UPDATE SET duration=?2,width=?3,height=?4,mtime=?5",
        rusqlite::params![fp, dur, w, h, mtime as i64],
    );
}

/// Lightweight clip counter — counts video files without reading metadata.
/// Used by the Home page dashboard so it doesn't trigger full ffprobe scans.
#[command]
pub async fn get_clips_count(folder: String) -> Result<usize, String> {
    let dir = resolve_clips_dir(&folder);
    if !dir.exists() { return Ok(0); }
    let count = walkdir::WalkDir::new(&dir)
        .min_depth(1)
        .into_iter()
        .flatten()
        .filter(|e| {
            let p = e.path();
            p.is_file() && {
                let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
                VIDEO_EXTS.contains(&ext.as_str())
            }
        })
        .count();
    Ok(count)
}

#[command] pub async fn get_clips(folder: String) -> Result<Vec<ClipInfo>, String> {
    // Phase 3a+3b: cache-first ffprobe, parallel for uncached clips
    use tokio::sync::Semaphore;
    #[cfg(debug_assertions)] let t_total = std::time::Instant::now();
    let dirs = get_all_clip_dirs(&folder);
    let meta = get_meta_map();
    let td = thumb_dir(); let _ = std::fs::create_dir_all(&td);

    // Collect all candidate files first (cheap filesystem scan)
    struct Entry { fp: String, fname: String, id: String, filesize: u64, created: String, mtime: u64, game_raw: String, cn: String, fav: bool, game_tag: String, thumbnail: String }
    let mut entries: Vec<Entry> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in &dirs {
        if !dir.exists() { continue; }
        for e in walkdir::WalkDir::new(dir).min_depth(1).into_iter().flatten() {
            let p = e.path().to_path_buf(); if !p.is_file() { continue; }
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if !VIDEO_EXTS.contains(&ext.as_str()) { continue; }
            let fp = p.to_string_lossy().to_string();
            if seen.contains(&fp) { continue; }
            seen.insert(fp.clone());
            let m = match e.metadata() { Ok(m) => m, Err(_) => continue };
            let fname = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            let id = format!("{:x}", hash_str(&fp));
            let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("Unknown");
            let created = date_from_stem(stem).unwrap_or_else(|| fmt_ts_local(mtime as i64));
            // SteelSeries: GameName__YYYY-MM-DD__HH-MM-SS — split on __ to get full game name.
            // Other formats: Prefix_YYYY-MM-DD_HH-MM-SS — split on _ to get prefix.
            let game_raw = if let Some(pos) = stem.find("__") {
                stem[..pos].replace('-', " ").replace('_', " ")
            } else {
                stem.split('_').next().unwrap_or("Unknown").replace('-', " ")
            };
            let (cn, fav, game_tag) = meta.get(&fp).cloned().unwrap_or_default();
            let thumb = td.join(format!("{id}.jpg"));
            let thumbnail = if thumb.exists() { thumb.to_string_lossy().to_string() } else { String::new() };
            entries.push(Entry { fp, fname, id, filesize: m.len(), created, mtime, game_raw, cn, fav, game_tag, thumbnail });
        }
    }
    #[cfg(debug_assertions)] let t_scan = t_total.elapsed().as_millis();

    // Phase 3a: check probe cache; collect uncached for parallel probing
    let db = open_db().ok();
    struct CachedEntry { entry_idx: usize, dur: f64, w: u32, h: u32 }
    let mut cached: Vec<CachedEntry> = Vec::new();
    let mut uncached_idxs: Vec<usize> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if let Some(ref db) = db {
            if let Some((dur, w, h)) = probe_cache_get(db, &e.fp, e.mtime) {
                cached.push(CachedEntry { entry_idx: i, dur, w, h });
                continue;
            }
        }
        uncached_idxs.push(i);
    }
    #[cfg(debug_assertions)] let t_cache = t_total.elapsed().as_millis();
    #[cfg(debug_assertions)] let n_uncached = uncached_idxs.len();

    // Phase 3b: parallel ffprobe for uncached clips (max 4 concurrent)
    // acquire().await blocks until a permit is free, properly limiting concurrency.
    let sem = Arc::new(Semaphore::new(4));
    let mut probe_tasks = Vec::new();
    for idx in uncached_idxs {
        let fp = entries[idx].fp.clone();
        let sem = Arc::clone(&sem);
        let task = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let fp2 = fp.clone();
            let result = tokio::task::spawn_blocking(move || {
                probe_video(std::path::Path::new(&fp2))
            }).await.unwrap_or((0.0, 0, 0));
            (idx, fp, result)
        });
        probe_tasks.push(task);
    }
    let mut probe_results: Vec<(usize, String, (f64, u32, u32))> = Vec::new();
    for task in probe_tasks {
        if let Ok(r) = task.await { probe_results.push(r); }
    }
    #[cfg(debug_assertions)] let t_probe = t_total.elapsed().as_millis();
    // Write new probe results to cache
    if let Some(ref db) = db {
        for (idx, fp, (dur, w, h)) in &probe_results {
            probe_cache_set(db, fp, *dur, *w, *h, entries[*idx].mtime);
        }
    }

    // Assemble final ClipInfo list
    let mut probe_map: std::collections::HashMap<usize,(f64,u32,u32)> = std::collections::HashMap::new();
    for c in cached { probe_map.insert(c.entry_idx, (c.dur, c.w, c.h)); }
    for (idx, _, dwh) in probe_results { probe_map.insert(idx, dwh); }

    let mut clips: Vec<ClipInfo> = entries.into_iter().enumerate().map(|(i, e)| {
        let (dur, w, h) = probe_map.get(&i).copied().unwrap_or((0.0, 0, 0));
        let game = if e.game_tag.is_empty() { e.game_raw } else { e.game_tag };
        ClipInfo { id: e.id, filename: e.fname, filepath: e.fp, filesize: e.filesize, created: e.created, duration: dur, width: w, height: h, game, custom_name: e.cn, favorite: e.fav, thumbnail: e.thumbnail }
    }).collect();

    clips.sort_by(|a,b| b.created.cmp(&a.created));
    #[cfg(debug_assertions)] {
        let t_total_ms = t_total.elapsed().as_millis();
        eprintln!("[perf] get_clips: scan={}ms cache={}ms ffprobe={}ms ({} uncached) assemble={}ms total={}ms clips={}",
            t_scan, t_cache - t_scan, t_probe - t_cache, n_uncached,
            t_total_ms - t_probe, t_total_ms, clips.len());
    }
    Ok(clips)
}

/// Fast clip list — skips ffprobe entirely for uncached clips.
/// Uncached clips get duration=0, width=0, height=0 so the grid can appear immediately.
/// Call probe_clips() afterward to fill in missing metadata in the background.
#[command] pub async fn get_clips_fast(folder: String) -> Result<Vec<ClipInfo>, String> {
    #[cfg(debug_assertions)] let t_total = std::time::Instant::now();
    let dirs = get_all_clip_dirs(&folder);
    let meta = get_meta_map();
    let td = thumb_dir(); let _ = std::fs::create_dir_all(&td);

    struct Entry { fp: String, fname: String, id: String, filesize: u64, created: String, mtime: u64, game_raw: String, cn: String, fav: bool, game_tag: String, thumbnail: String }
    let mut entries: Vec<Entry> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in &dirs {
        if !dir.exists() { continue; }
        for e in walkdir::WalkDir::new(dir).min_depth(1).into_iter().flatten() {
            let p = e.path().to_path_buf(); if !p.is_file() { continue; }
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if !VIDEO_EXTS.contains(&ext.as_str()) { continue; }
            let fp = p.to_string_lossy().to_string();
            if seen.contains(&fp) { continue; }
            seen.insert(fp.clone());
            let m = match e.metadata() { Ok(m) => m, Err(_) => continue };
            let fname = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            let id = format!("{:x}", hash_str(&fp));
            let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("Unknown");
            let created = date_from_stem(stem).unwrap_or_else(|| fmt_ts_local(mtime as i64));
            // SteelSeries: GameName__YYYY-MM-DD__HH-MM-SS — split on __ to get full game name.
            // Other formats: Prefix_YYYY-MM-DD_HH-MM-SS — split on _ to get prefix.
            let game_raw = if let Some(pos) = stem.find("__") {
                stem[..pos].replace('-', " ").replace('_', " ")
            } else {
                stem.split('_').next().unwrap_or("Unknown").replace('-', " ")
            };
            let (cn, fav, game_tag) = meta.get(&fp).cloned().unwrap_or_default();
            let thumb = td.join(format!("{id}.jpg"));
            let thumbnail = if thumb.exists() { thumb.to_string_lossy().to_string() } else { String::new() };
            entries.push(Entry { fp, fname, id, filesize: m.len(), created, mtime, game_raw, cn, fav, game_tag, thumbnail });
        }
    }

    // Check probe cache — cached clips get real values, uncached get (0,0,0)
    let db = open_db().ok();
    let mut clips: Vec<ClipInfo> = entries.into_iter().map(|e| {
        let (dur, w, h) = db.as_ref()
            .and_then(|db| probe_cache_get(db, &e.fp, e.mtime))
            .unwrap_or((0.0, 0, 0));
        let game = if e.game_tag.is_empty() { e.game_raw } else { e.game_tag };
        ClipInfo { id: e.id, filename: e.fname, filepath: e.fp, filesize: e.filesize, created: e.created, duration: dur, width: w, height: h, game, custom_name: e.cn, favorite: e.fav, thumbnail: e.thumbnail }
    }).collect();

    clips.sort_by(|a,b| b.created.cmp(&a.created));
    #[cfg(debug_assertions)] eprintln!("[perf] get_clips_fast: total={}ms clips={}", t_total.elapsed().as_millis(), clips.len());
    Ok(clips)
}

/// Probe duration/resolution for a list of files and write results to the SQLite cache.
/// Used after get_clips_fast() to fill in metadata for uncached clips in the background.
/// Returns Vec of (filepath, duration, width, height).
#[command] pub async fn probe_clips(filepaths: Vec<String>) -> Result<Vec<(String, f64, u32, u32)>, String> {
    use tokio::sync::Semaphore;
    if filepaths.is_empty() { return Ok(vec![]); }
    #[cfg(debug_assertions)] let t_start = std::time::Instant::now();
    let sem = Arc::new(Semaphore::new(4));
    let mut tasks = Vec::new();
    for fp in filepaths {
        let sem = Arc::clone(&sem);
        tasks.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let fp2 = fp.clone();
            let (dur, w, h) = tokio::task::spawn_blocking(move || probe_video(std::path::Path::new(&fp2)))
                .await.unwrap_or((0.0, 0, 0));
            (fp, dur, w, h)
        }));
    }
    let mut results = Vec::new();
    for t in tasks { if let Ok(r) = t.await { results.push(r); } }
    // Write to cache
    if let Ok(db) = open_db() {
        for (fp, dur, w, h) in &results {
            let mtime = std::fs::metadata(fp).ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()).unwrap_or(0);
            probe_cache_set(&db, fp, *dur, *w, *h, mtime);
        }
    }
    #[cfg(debug_assertions)] eprintln!("[perf] probe_clips: {}ms ({} clips)", t_start.elapsed().as_millis(), results.len());
    Ok(results.into_iter().map(|(fp, dur, w, h)| (fp, dur, w, h)).collect())
}

/// Fetch metadata for a single file — used by the frontend file-watcher listener.
#[command]
pub async fn get_clip_by_path(filepath: String) -> Result<Option<ClipInfo>, String> {
    let p = PathBuf::from(&filepath);
    if !p.is_file() { return Ok(None); }
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if !VIDEO_EXTS.contains(&ext.as_str()) { return Ok(None); }
    let meta = get_meta_map();
    let td = thumb_dir(); let _ = std::fs::create_dir_all(&td);
    let m = p.metadata().map_err(|e| format!("{e}"))?;
    let fname = p.file_name().unwrap_or_default().to_string_lossy().to_string();
    let fp = filepath.clone();
    let id = format!("{:x}", hash_str(&fp));
    let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("Unknown");
    let created = date_from_stem(stem).unwrap_or_else(|| fmt_ts_local(mtime as i64));
    let (dur,w,h) = probe_video(&p);
    let game_from_filename = stem.split('_').next().unwrap_or("Unknown").replace('-', " ");
    let (cn,fav,game_tag) = meta.get(&fp).cloned().unwrap_or_default();
    let game = if game_tag.is_empty() { game_from_filename } else { game_tag };
    let thumb = td.join(format!("{id}.jpg"));
    let thumbnail = if thumb.exists() { thumb.to_string_lossy().to_string() } else { String::new() };
    Ok(Some(ClipInfo{id,filename:fname,filepath:fp,filesize:m.len(),created,duration:dur,width:w,height:h,game,custom_name:cn,favorite:fav,thumbnail}))
}
#[command] pub async fn generate_thumbnail(filepath: String, duration: Option<f64>) -> Result<String, String> {
    let id = format!("{:x}",hash_str(&filepath)); let d = thumb_dir(); let _ = std::fs::create_dir_all(&d);
    let out = d.join(format!("{id}.jpg")); if out.exists() { return Ok(out.to_string_lossy().to_string()); }
    #[cfg(debug_assertions)] let t_start = std::time::Instant::now();
    // Use caller-provided duration to skip redundant probe_duration ffprobe subprocess
    let dur = duration.filter(|&d| d > 0.0).unwrap_or_else(|| probe_duration(&filepath));
    #[cfg(debug_assertions)] let t_probe_ms = t_start.elapsed().as_millis();
    let seek = if dur>1.0{dur*0.1}else{0.0};
    // 480p thumbnails: ~853x480 at q:v 3 (~90KB each). Matches SteelSeries quality.
    let r = Command::new("ffmpeg").args([
        "-ss", &format!("{seek:.2}"), "-i", &filepath,
        "-vframes", "1", "-vf", "scale=-2:480", "-q:v", "3", "-y",
        &out.to_string_lossy()
    ]).output().map_err(|e| format!("{e}"))?;
    #[cfg(debug_assertions)] {
        let fname = filepath.rfind('/').map(|i| &filepath[i+1..]).unwrap_or(&filepath);
        eprintln!("[perf] generate_thumbnail: probe={}ms ffmpeg={}ms total={}ms file={}",
            t_probe_ms, t_start.elapsed().as_millis() - t_probe_ms, t_start.elapsed().as_millis(), fname);
    }
    if r.status.success() && out.exists() { Ok(out.to_string_lossy().to_string()) }
    else { Err(format!("ffmpeg: {}", String::from_utf8_lossy(&r.stderr))) }
}
/// Phase 3d: Batch thumbnail generation — generates up to 3 concurrently.
/// `durations`: optional per-filepath duration hints. When provided and non-zero,
/// skips the redundant probe_duration ffprobe call for that clip.
#[command]
pub async fn generate_thumbnails_batch(filepaths: Vec<String>, durations: Option<Vec<f64>>) -> Result<Vec<String>, String> {
    use tokio::sync::Semaphore;
    let sem = Arc::new(Semaphore::new(3));
    let mut tasks = Vec::new();
    for (i, filepath) in filepaths.into_iter().enumerate() {
        let sem = Arc::clone(&sem);
        let provided_dur = durations.as_ref().and_then(|d| d.get(i).copied()).filter(|&d| d > 0.0);
        let task = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let fp = filepath.clone();
            tokio::task::spawn_blocking(move || {
                let id = format!("{:x}", hash_str(&fp));
                let d = thumb_dir(); let _ = std::fs::create_dir_all(&d);
                let out = d.join(format!("{id}.jpg"));
                if out.exists() { return out.to_string_lossy().to_string(); }
                let dur = provided_dur.unwrap_or_else(|| probe_duration(&fp));
                let seek = if dur > 1.0 { dur * 0.1 } else { 0.0 };
                let r = Command::new("ffmpeg").args([
                    "-ss", &format!("{seek:.2}"), "-i", &fp,
                    "-vframes", "1", "-vf", "scale=-2:480", "-q:v", "3", "-y",
                    &out.to_string_lossy(),
                ]).output();
                match r {
                    Ok(o) if o.status.success() && out.exists() => out.to_string_lossy().to_string(),
                    _ => String::new(),
                }
            }).await.unwrap_or_default()
        });
        tasks.push(task);
    }
    let mut results = Vec::new();
    for task in tasks {
        results.push(task.await.unwrap_or_default());
    }
    Ok(results)
}

#[derive(Deserialize)] pub struct ClipMetaUpdate { pub filepath:String, pub custom_name:Option<String>, pub favorite:Option<bool>, pub game_tag:Option<String>, pub notes:Option<String> }
#[command] pub async fn set_clip_meta(update: ClipMetaUpdate) -> Result<(), String> {
    let db = open_db()?;
    let _ = db.execute("ALTER TABLE clip_meta ADD COLUMN game_tag TEXT DEFAULT ''", []);
    // Build dynamic UPDATE to only set provided fields
    let cn = update.custom_name.unwrap_or_default();
    let fav = update.favorite.unwrap_or(false) as i32;
    let gt = update.game_tag.unwrap_or_default();
    let notes = update.notes.unwrap_or_default();
    db.execute(
        "INSERT INTO clip_meta(filepath,custom_name,favorite,game_tag,notes) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(filepath) DO UPDATE SET custom_name=CASE WHEN ?2='' THEN custom_name ELSE ?2 END,favorite=?3,game_tag=CASE WHEN ?4='' THEN game_tag ELSE ?4 END,notes=CASE WHEN ?5='' THEN notes ELSE ?5 END",
        rusqlite::params![update.filepath, cn, fav, gt, notes]
    ).map_err(|e| format!("{e}"))?;
    Ok(())
}

/// Get full clip metadata including notes (for overlay persistence)
#[command]
pub async fn get_clip_meta(filepath: String) -> Result<String, String> {
    let db = open_db()?;
    let _ = db.execute("ALTER TABLE clip_meta ADD COLUMN game_tag TEXT DEFAULT ''", []);
    match db.query_row("SELECT custom_name,favorite,COALESCE(game_tag,''),COALESCE(notes,'') FROM clip_meta WHERE filepath=?1", [&filepath], |r| {
        Ok(serde_json::json!({
            "custom_name": r.get::<_,String>(0)?,
            "favorite": r.get::<_,bool>(1)?,
            "game_tag": r.get::<_,String>(2).unwrap_or_default(),
            "notes": r.get::<_,String>(3).unwrap_or_default(),
        }))
    }) {
        Ok(v) => Ok(v.to_string()),
        Err(_) => Ok("null".into()),
    }
}

/// Take a screenshot at a specific timestamp.
/// `output_dir`: optional override; falls back to `~/Pictures`.
#[command]
pub async fn take_screenshot(filepath: String, time_sec: f64, output_dir: Option<String>) -> Result<String, String> {
    let pics_dir = match output_dir.as_deref().filter(|s| !s.is_empty()) {
        Some(d) => PathBuf::from(shexp(d)),
        None    => dirs::picture_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures")),
    };
    let _ = std::fs::create_dir_all(&pics_dir);
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let out = pics_dir.join(format!("opengg_screenshot_{ts}.png"));

    let r = Command::new("ffmpeg").args([
        "-ss", &format!("{time_sec:.3}"),
        "-i", &filepath,
        "-vframes", "1",
        "-q:v", "2",
        "-y", &out.to_string_lossy(),
    ]).output().map_err(|e| format!("ffmpeg: {e}"))?;

    if r.status.success() && out.exists() {
        Ok(out.to_string_lossy().to_string())
    } else {
        Err(format!("Screenshot failed: {}", String::from_utf8_lossy(&r.stderr)))
    }
}
#[command] pub async fn delete_clip(filepath: String) -> Result<(), String> { if Path::new(&filepath).exists(){std::fs::remove_file(&filepath).map_err(|e| format!("{e}"))?;} let id=format!("{:x}",hash_str(&filepath)); let t=thumb_dir().join(format!("{id}.jpg")); if t.exists(){let _=std::fs::remove_file(&t);} if let Ok(c)=open_db(){let _=c.execute("DELETE FROM clip_meta WHERE filepath=?1",[&filepath]); let _=c.execute("DELETE FROM trim_state WHERE filepath=?1",[&filepath]);} Ok(()) }
#[command] pub async fn save_trim_state(filepath: String, trim_start: f64, trim_end: f64) -> Result<(), String> { open_db()?.execute("INSERT INTO trim_state(filepath,trim_start,trim_end) VALUES(?1,?2,?3) ON CONFLICT(filepath) DO UPDATE SET trim_start=?2,trim_end=?3", rusqlite::params![filepath,trim_start,trim_end]).map_err(|e| format!("{e}"))?; Ok(()) }
#[derive(Serialize)] pub struct TrimState { pub trim_start: f64, pub trim_end: f64 }
#[command] pub async fn get_trim_state(filepath: String) -> Result<Option<TrimState>, String> { let c=open_db()?; let mut s=c.prepare("SELECT trim_start,trim_end FROM trim_state WHERE filepath=?1").map_err(|e| format!("{e}"))?; match s.query_row([&filepath],|r| Ok(TrimState{trim_start:r.get(0)?,trim_end:r.get(1)?})){Ok(t)=>Ok(Some(t)),Err(rusqlite::Error::QueryReturnedNoRows)=>Ok(None),Err(e)=>Err(format!("{e}"))} }
#[command] pub async fn trim_clip(input_path: String, start_sec: f64, end_sec: f64, output_path: String) -> Result<String, String> { let mut out=if output_path.is_empty(){auto_name(&input_path,"_trim")}else{output_path}; if out==input_path{out=auto_name(&input_path,"_trim")} let r=Command::new("ffmpeg").args(["-i",&input_path,"-ss",&format!("{start_sec:.3}"),"-to",&format!("{end_sec:.3}"),"-c","copy","-avoid_negative_ts","make_zero","-y",&out]).output().map_err(|e| format!("{e}"))?; if r.status.success(){Ok(out)}else{Err(format!("{}",String::from_utf8_lossy(&r.stderr)))} }
/// Export with target size + real-time progress via Tauri events.
/// Parses ffmpeg stderr for "time=HH:MM:SS.xx" to calculate %.
#[command]
pub async fn export_clip_sized(app: AppHandle, input_path: String, start_sec: f64, end_sec: f64, target_mb: f64, output_path: String) -> Result<String, String> {
    let dur = end_sec - start_sec;
    if dur <= 0.0 { return Err("Invalid trim range".into()); }

    let mut out = if output_path.is_empty() { auto_name(&input_path, &format!("_{}mb", target_mb as u32)) } else { smart_prefix(&output_path) };
    if out == input_path { out = auto_name(&input_path, &format!("_{}mb", target_mb as u32)); }

    if target_mb <= 0.0 {
        // Original quality — stream copy (fast, no progress needed)
        let _ = app.emit("export-progress", serde_json::json!({"percent": 50, "stage": "copying"}));
        let result = trim_clip(input_path, start_sec, end_sec, out.clone()).await;
        let _ = app.emit("export-progress", serde_json::json!({"percent": 100, "stage": "done"}));
        return result;
    }

    let audio_kbps: f64 = 128.0;
    let total_kbps = target_mb * 8192.0 / dur;
    let video_kbps = (total_kbps - audio_kbps).max(100.0);
    let vbr = format!("{}k", video_kbps as u32);

    // Pass 1 (analyze)
    let _ = app.emit("export-progress", serde_json::json!({"percent": 0, "stage": "pass1"}));
    let mut child1 = Command::new("ffmpeg")
        .args(["-y", "-i", &input_path, "-ss", &format!("{start_sec:.3}"), "-to", &format!("{end_sec:.3}"),
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-b:v", &vbr, "-preset", "fast", "-pass", "1",
            "-an", "-f", "null", "/dev/null"])
        .stderr(std::process::Stdio::piped())
        .spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    // Stream progress from pass 1 (0-45%)
    if let Some(stderr) = child1.stderr.take() {
        let app_clone = app.clone();
        let dur_clone = dur;
        tokio::task::spawn_blocking(move || {
            parse_ffmpeg_progress(stderr, dur_clone, 0.0, 45.0, &app_clone, None);
        });
    }
    let _status1 = child1.wait().map_err(|e| format!("ffmpeg wait: {e}"))?;

    // Pass 2 (encode)
    let _ = app.emit("export-progress", serde_json::json!({"percent": 45, "stage": "pass2"}));
    let mut child2 = Command::new("ffmpeg")
        .args(["-y", "-i", &input_path, "-ss", &format!("{start_sec:.3}"), "-to", &format!("{end_sec:.3}"),
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-b:v", &vbr, "-preset", "fast", "-pass", "2",
            "-c:a", "aac", "-b:a", "128k", &out])
        .stderr(std::process::Stdio::piped())
        .spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    // Stream progress from pass 2 (45-100%)
    if let Some(stderr) = child2.stderr.take() {
        let app_clone = app.clone();
        let dur_clone = dur;
        tokio::task::spawn_blocking(move || {
            parse_ffmpeg_progress(stderr, dur_clone, 45.0, 100.0, &app_clone, None);
        });
    }
    let status2 = child2.wait().map_err(|e| format!("ffmpeg wait: {e}"))?;

    // Cleanup 2-pass log files
    let _ = std::fs::remove_file("ffmpeg2pass-0.log");
    let _ = std::fs::remove_file("ffmpeg2pass-0.log.mbtree");

    let _ = app.emit("export-progress", serde_json::json!({"percent": 100, "stage": "done"}));

    if status2.success() { Ok(out) }
    else { Err("FFmpeg encoding failed".into()) }
}

/// Parse ffmpeg stderr for "time=HH:MM:SS.xx" and emit progress events.
///
/// ★ THE BUG: FFmpeg writes progress as `\r`-delimited lines (carriage return),
/// NOT `\n` (newline). BufReader::lines() splits on `\n` only, so the reader
/// blocks indefinitely waiting for a newline that never comes.
///
/// FIX: Read byte-by-byte, split on both `\r` and `\n`.
/// Parse FFmpeg progress from stderr, emitting progress events.
/// `log`: optional buffer that collects every stderr line for post-mortem error reporting.
fn parse_ffmpeg_progress(
    stderr: std::process::ChildStderr,
    total_dur: f64,
    pct_start: f64,
    pct_end: f64,
    app: &AppHandle,
    log: Option<Arc<Mutex<Vec<String>>>>,
) {
    use std::io::Read;
    let range = pct_end - pct_start;
    let mut buf = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    let mut reader = std::io::BufReader::new(stderr);

    loop {
        match reader.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {
                let ch = byte[0];
                if ch == b'\r' || ch == b'\n' {
                    if !buf.is_empty() {
                        let line = String::from_utf8_lossy(&buf).to_string();
                        buf.clear();
                        eprintln!("ffmpeg: {line}");
                        if let Some(ref l) = log { l.lock().unwrap().push(line.clone()); }

                        if let Some(pos) = line.find("time=") {
                            let rest = &line[pos + 5..];
                            let ts_end = rest.find(|c: char| c == ' ' || c == '\t')
                                .unwrap_or(rest.len());
                            if let Some(secs) = parse_time_to_secs(&rest[..ts_end]) {
                                let pct = pct_start + (secs / total_dur.max(0.1) * range).min(range);
                                let _ = app.emit("export-progress", serde_json::json!({
                                    "percent": pct.round() as u32,
                                    "stage": "encoding",
                                    "speed": extract_speed(&line),
                                }));
                            }
                        }
                    }
                } else {
                    buf.push(ch);
                }
            }
            Err(_) => break,
        }
    }
}

/// Extract "speed=1.23x" from ffmpeg line
fn extract_speed(line: &str) -> String {
    if let Some(pos) = line.find("speed=") {
        let rest = &line[pos + 6..];
        let end = rest.find(|c: char| c == ' ' || c == '\r' || c == '\n').unwrap_or(rest.len());
        rest[..end].trim().to_string()
    } else {
        String::new()
    }
}

/// Parse "HH:MM:SS.xx" or "MM:SS.xx" to seconds
fn parse_time_to_secs(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        3 => {
            let h: f64 = parts[0].parse().ok()?;
            let m: f64 = parts[1].parse().ok()?;
            let s: f64 = parts[2].parse().ok()?;
            Some(h * 3600.0 + m * 60.0 + s)
        }
        2 => {
            let m: f64 = parts[0].parse().ok()?;
            let s: f64 = parts[1].parse().ok()?;
            Some(m * 60.0 + s)
        }
        _ => parts[0].parse().ok()
    }
}

/// Calculate projected export settings (for UI preview)
#[command]
pub async fn calc_export_settings(duration_sec: f64, target_mb: f64, width: u32, height: u32) -> Result<String, String> {
    if duration_sec <= 0.0 { return Err("Invalid duration".into()); }
    let audio_kbps = 128.0;
    let total_kbps = target_mb * 8192.0 / duration_sec;
    let video_kbps = (total_kbps - audio_kbps).max(100.0);
    Ok(serde_json::json!({
        "resolution": format!("{width}x{height}"),
        "video_bitrate_kbps": video_kbps as u32,
        "audio_bitrate_kbps": audio_kbps as u32,
        "total_bitrate_kbps": total_kbps as u32,
        "codec": "H.264 (libx264)", "preset": "fast", "passes": 2
    }).to_string())
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 5: Export with progress events
// ══════════════════════════════════════════════════════════════

#[derive(Serialize, Clone)]
struct ExportProgress { percent: f64, fps_current: f64, time_processed: String, speed: String }

#[command]
pub async fn export_with_progress(
    app: AppHandle,
    input_path: String, start_sec: f64, end_sec: f64,
    target_mb: f64, output_path: String,
) -> Result<String, String> {
    let dur = end_sec - start_sec;
    if dur <= 0.0 { return Err("Invalid trim range".into()); }

    let out = if output_path.is_empty() { auto_name(&input_path, "_export") } else { smart_prefix(&output_path) };

    // Input-side seeking: -ss/-to BEFORE -i so FFmpeg seeks to the keyframe
    // before start_sec instead of decoding from 0 (fast + correct PTS).
    let mut args: Vec<String> = vec![
        "-y".into(), "-progress".into(), "pipe:1".into(),
        "-ss".into(), format!("{start_sec:.3}"),
        "-to".into(), format!("{end_sec:.3}"),
        "-i".into(), input_path.clone(),
    ];

    if target_mb > 0.0 {
        let vbr = format!("{}k", ((target_mb * 8192.0 / dur) - 128.0).max(100.0) as u32);
        args.extend(["-c:v".into(), "libx264".into(), "-pix_fmt".into(), "yuv420p".into(),
            "-b:v".into(), vbr, "-preset".into(), "fast".into(),
            "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into()]);
    } else {
        // Copy video stream as-is; downmix ALL source audio tracks → one AAC stream.
        // Without explicit mapping, FFmpeg only copies the "best" audio stream.
        let n_audio = (count_audio_streams(&input_path) as usize).max(1);
        if n_audio == 1 {
            args.extend([
                "-map".into(), "0:v".into(),
                "-map".into(), "0:a:0".into(),
                "-c:v".into(), "copy".into(),
                "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(),
                "-avoid_negative_ts".into(), "make_zero".into(),
            ]);
        } else {
            // Build amix filter over all audio streams
            let mix_ins: String = (0..n_audio).map(|i| format!("[0:a:{i}]")).collect();
            let fc = format!("{mix_ins}amix=inputs={n_audio}:normalize=0:duration=longest[amix]");
            args.extend([
                "-filter_complex".into(), fc,
                "-map".into(), "0:v".into(),
                "-map".into(), "[amix]".into(),
                "-c:v".into(), "copy".into(),
                "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(),
                "-avoid_negative_ts".into(), "make_zero".into(),
            ]);
        }
    }
    args.push(out.clone());

    let mut child = Command::new("ffmpeg")
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg: {e}"))?;

    // ★ Read progress from stdout in background
    let handle = app.clone();
    let total_dur = dur;
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(async move {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                // ffmpeg -progress outputs: out_time_ms=12345678
                if let Some(time_us) = line.strip_prefix("out_time_us=") {
                    if let Ok(us) = time_us.parse::<f64>() {
                        let secs = us / 1_000_000.0;
                        let pct = (secs / total_dur * 100.0).min(100.0);
                        let _ = handle.emit("export-progress", ExportProgress {
                            percent: pct,
                            fps_current: 0.0,
                            time_processed: format!("{secs:.1}s"),
                            speed: String::new(),
                        });
                    }
                }
                if line.starts_with("speed=") {
                    let spd = line.strip_prefix("speed=").unwrap_or("").trim().to_string();
                    let _ = handle.emit("export-progress", ExportProgress {
                        percent: -1.0, // -1 = speed update only
                        fps_current: 0.0,
                        time_processed: String::new(),
                        speed: spd,
                    });
                }
            }
        });
    }

    let status = child.wait().map_err(|e| format!("ffmpeg wait: {e}"))?;
    // Send 100% on completion
    let _ = app.emit("export-progress", ExportProgress { percent: 100.0, fps_current: 0.0, time_processed: "done".into(), speed: String::new() });

    if status.success() { Ok(out) } else { Err("FFmpeg export failed".into()) }
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 3: Generate audio waveform peaks via ffmpeg
#[command] pub async fn open_file_location(filepath: String) -> Result<(), String> {
    // Try org.freedesktop.FileManager1.ShowItems — highlights the file in Nautilus/Dolphin/Nemo
    let file_uri = format!("file://{}", filepath);
    let ok = Command::new("dbus-send")
        .args([
            "--session", "--print-reply",
            "--dest=org.freedesktop.FileManager1",
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1.ShowItems",
            &format!("array:string:{}", file_uri),
            "string:",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if ok { return Ok(()); }
    // Fallback: open the parent folder without file selection
    let parent = Path::new(&filepath).parent().unwrap_or(Path::new("/"));
    open::that(parent).map_err(|e| format!("{e}"))?;
    Ok(())
}

// ═══ Recording ═══
#[command] pub async fn start_screen_recording(save_dir: String, fps: u32, quality: String, replay_seconds: u32) -> Result<String, String> { let dir=if save_dir.is_empty(){let d=default_clips_dir(); let _=std::fs::create_dir_all(&d); d}else{let d=PathBuf::from(shexp(&save_dir)); if !d.exists(){std::fs::create_dir_all(&d).map_err(|e| format!("{e}"))?;} d}; let ts=chrono_now(); let outfile=dir.join(format!("opengg_{ts}.mp4")); let target=detect_target(); let qp=match quality.as_str(){"Low"=>"medium","Medium"=>"high","High"=>"very_high","Ultra"=>"ultra",_=>"high"}; let mut args=vec!["-w".into(),target,"-f".into(),fps.to_string(),"-q".into(),qp.into(),"-a".into(),"default_output".into(),"-o".into(),outfile.to_string_lossy().to_string()]; if replay_seconds>0{args.push("-r".into());args.push(replay_seconds.to_string());} Command::new("gpu-screen-recorder").args(&args).spawn().map_err(|e| if e.kind()==std::io::ErrorKind::NotFound{"gpu-screen-recorder not found".into()}else{format!("{e}")})?; Ok(outfile.to_string_lossy().to_string()) }
#[command] pub async fn stop_screen_recording() -> Result<(), String> { let _=Command::new("pkill").args(["-SIGINT","gpu-screen-recorder"]).output(); tokio::time::sleep(std::time::Duration::from_millis(500)).await; Ok(()) }
fn detect_target() -> String { if let Ok(o)=Command::new("sh").args(["-c","xdotool getactivewindow 2>/dev/null"]).output(){let w=String::from_utf8_lossy(&o.stdout).trim().to_string(); if !w.is_empty(){if let Ok(s)=Command::new("xprop").args(["-id",&w,"_NET_WM_STATE"]).output(){if String::from_utf8_lossy(&s.stdout).contains("FULLSCREEN"){return format!("window:{w}");}}}} "screen".into() }
fn chrono_now() -> String { let s=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(); format!("{}-{:02}-{:02}_{:02}-{:02}-{:02}",1970+s/31536000,(s%31536000)/2592000+1,(s%2592000)/86400+1,(s%86400)/3600,(s%3600)/60,s%60) }

// ═══ Theme ═══
fn theme_path() -> PathBuf { dirs::config_dir().unwrap_or_else(||PathBuf::from("~/.config")).join("opengg/theme.json") }
#[command] pub async fn load_theme() -> Result<String, String> { let p=theme_path(); if p.exists(){std::fs::read_to_string(&p).map_err(|e| format!("{e}"))}else{Ok("{\"colors\":{\"--accent\":\"#E94560\"},\"layout\":{\"--clips-grid-cols\":\"4\"}}".into())} }
#[command] pub async fn save_theme(theme_json: String) -> Result<(), String> { let p=theme_path(); if let Some(d)=p.parent(){std::fs::create_dir_all(d).ok();} std::fs::write(&p,&theme_json).map_err(|e| format!("{e}")) }
#[command] pub async fn get_media_server_port(app: AppHandle) -> Result<u16, String> { Ok(app.state::<crate::MediaServerPort>().0) }

// ═══ Audio Devices ═══
#[derive(Debug,Serialize,Deserialize,Clone)] pub struct AudioDevice{pub name:String,pub description:String,pub device_type:String,pub is_default:bool}
#[command] pub async fn get_audio_devices() -> Result<Vec<AudioDevice>, String> { let mut d=Vec::new(); let ds=run_cmd("pactl",&["get-default-sink"]).unwrap_or_default(); let dr=run_cmd("pactl",&["get-default-source"]).unwrap_or_default(); if let Ok(o)=run_cmd("pactl",&["-f","json","list","sinks"]){if let Ok(v)=serde_json::from_str::<serde_json::Value>(&o){for s in v.as_array().unwrap_or(&vec![]){let n=s["name"].as_str().unwrap_or("").to_string(); if n.starts_with("OpenGG_"){continue;} d.push(AudioDevice{is_default:n==ds,description:s["description"].as_str().unwrap_or(&n).into(),name:n,device_type:"sink".into()});}}} if let Ok(o)=run_cmd("pactl",&["-f","json","list","sources"]){if let Ok(v)=serde_json::from_str::<serde_json::Value>(&o){for s in v.as_array().unwrap_or(&vec![]){let n=s["name"].as_str().unwrap_or("").to_string(); if n.contains(".monitor"){continue;} d.push(AudioDevice{is_default:n==dr,description:s["description"].as_str().unwrap_or(&n).into(),name:n,device_type:"source".into()});}}} Ok(d) }
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

/// Destroy every existing pw-link from `{sink_name}:monitor_FL/FR` to
/// any physical sink that is currently listed by PulseAudio.
fn unlink_virtual_sink_from_all(sink_name: &str) {
    let json = match run_cmd("pactl", &["-f", "json", "list", "sinks"]) {
        Ok(j) => j,
        Err(_) => return,
    };
    let sinks: Vec<serde_json::Value> = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => return,
    };
    for s in &sinks {
        if let Some(target) = s["name"].as_str() {
            for p in ["FL", "FR"] {
                // pw-link -d silently exits 0 when the link doesn't exist
                Command::new("pw-link")
                    .args(["-d",
                        &format!("{sink_name}:monitor_{p}"),
                        &format!("{target}:playback_{p}"),
                    ])
                    .output()
                    .ok();
            }
        }
    }
    eprintln!("set_channel_device: unlinked {sink_name} from {} sinks", sinks.len());
}

#[command]
pub async fn set_channel_device(channel: String, device_name: String) -> Result<(), String> {
    if channel == "Mic" {
        let _ = Command::new("pactl").args(["set-default-source", &device_name]).output();
        return Ok(());
    }
    if channel == "Master" {
        let _ = Command::new("pactl").args(["set-default-sink", &device_name]).output();
        return Ok(());
    }

    let sink = format!("OpenGG_{channel}");

    // ★ Step 1: tear down ALL existing links for this virtual sink.
    //   This is the core fix for the routing leak / audio duplication bug.
    unlink_virtual_sink_from_all(&sink);

    // ★ Step 2: create fresh links to the newly selected device.
    for p in ["FL", "FR"] {
        let r = Command::new("pw-link")
            .args([
                &format!("{sink}:monitor_{p}"),
                &format!("{device_name}:playback_{p}"),
            ])
            .output();
        if let Ok(o) = r {
            if o.status.success() {
                eprintln!("set_channel_device: linked {sink}:monitor_{p} → {device_name}:playback_{p}");
            } else {
                eprintln!("set_channel_device: pw-link failed: {}", String::from_utf8_lossy(&o.stderr).trim());
            }
        }
    }
    Ok(())
}

// ═══ VU ═══
#[derive(Serialize, Clone)] struct VuLevels { channels: HashMap<String, f32> }

/// Real-time per-channel VU meters via native libpulse.
///
/// One `pa_simple` connection per channel reads S16LE PCM from the channel's
/// monitor source.  Each reader runs in `spawn_blocking` so the async runtime
/// is never stalled.  Stopping is cooperative: set the AtomicBool to false and
/// each thread exits on its next 32 ms read; the `Simple` handle is dropped at
/// thread exit, calling `pa_simple_free()` automatically (RAII).
///
/// Using native libpulse completely eliminates:
///  • Process spawns (old pw-cat approach spawned 6 OS processes per stream)
///  • Mic fallback bug (pw-cat silently routed unknown targets to the mic)
///  • SIGTERM dance on shutdown
#[command]
pub async fn start_vu_stream(app: AppHandle) -> Result<(), String> {
    use libpulse_binding::sample::{Format, Spec};
    use libpulse_binding::stream::Direction;
    use libpulse_simple_binding::Simple;

    let st = app.state::<crate::VuState>();

    // ── Generation-counter dedup ─────────────────────────────────────────────
    // Stop any live reader threads from the previous session by toggling the
    // running flag off, bumping the generation, then turning it back on.
    // Threads blocked in pa.read() will see their generation is stale and exit.
    st.0.store(false, Ordering::Relaxed);
    let my_gen = st.1.fetch_add(1, Ordering::SeqCst) + 1;
    // Give old threads one read-period (~32 ms) to notice the flag is false.
    std::thread::sleep(std::time::Duration::from_millis(50));
    st.0.store(true, Ordering::Relaxed);

    let running = st.0.clone();
    let gen     = st.1.clone();
    let handle  = app.clone();

    // Resolve default sink/source once — not inside the hot path.
    let master_monitor = run_cmd("pactl", &["get-default-sink"])
        .map(|s| format!("{s}.monitor"))
        .unwrap_or_default();
    let mic_source = run_cmd("pactl", &["get-default-source"]).unwrap_or_default();

    // Guard: only connect to sources that currently exist.
    let known_sources: std::collections::HashSet<String> = {
        let mut set = std::collections::HashSet::new();
        if let Ok(json) = run_cmd("pactl", &["-f", "json", "list", "sources"]) {
            if let Ok(sources) = serde_json::from_str::<Vec<serde_json::Value>>(&json) {
                for s in &sources {
                    if let Some(name) = s["name"].as_str() { set.insert(name.to_string()); }
                }
            }
        }
        set
    };
    eprintln!("start_vu_stream: {} PA sources: {:?}", known_sources.len(), known_sources);

    let levels: Arc<Mutex<HashMap<String, f32>>> = Arc::new(Mutex::new(HashMap::new()));

    let channel_targets: Vec<(&'static str, String)> = vec![
        ("Master", master_monitor),
        ("Game",   "OpenGG_Game.monitor".into()),
        ("Chat",   "OpenGG_Chat.monitor".into()),
        ("Media",  "OpenGG_Media.monitor".into()),
        ("Aux",    "OpenGG_Aux.monitor".into()),
        ("Mic",    mic_source),
    ];

    let spec = Spec { format: Format::S16le, rate: 8000, channels: 1 };

    for (name, target) in channel_targets {
        if target.is_empty() || !known_sources.contains(target.as_str()) {
            eprintln!("start_vu_stream: {name} → '{target}' not in PA sources, level=0");
            levels.lock().unwrap().insert(name.to_string(), 0.0);
            continue;
        }

        let levels_clone  = Arc::clone(&levels);
        let running_clone = running.clone();
        let gen_clone     = gen.clone();

        tokio::task::spawn_blocking(move || {
            // Create the PA simple connection inside spawn_blocking — Simple is !Send.
            let pa = match Simple::new(
                None, "OpenGG VU", Direction::Record,
                Some(target.as_str()), name,
                &spec, None, None,
            ) {
                Ok(p)  => p,
                Err(e) => {
                    eprintln!("libpulse: {name} → failed to open '{target}': {e:?}");
                    return;
                }
            };
            eprintln!("start_vu_stream: {name} → libpulse connected to '{target}' (gen={my_gen})");

            // 512 bytes = 256 i16 samples = 32 ms at 8 kHz mono — fast enough to
            // check `running` + generation frequently while producing smooth meter values.
            let mut buf = vec![0u8; 512];
            let mut prev = 0.0f32;

            // Check BOTH the running flag AND the generation counter so that stale
            // threads spawned by a previous `start_vu_stream` call stop cleanly even
            // if the flag was reset to `true` before they exited pa.read().
            while running_clone.load(Ordering::Relaxed)
                  && gen_clone.load(Ordering::Relaxed) == my_gen
            {
                if pa.read(&mut buf).is_err() { break; }

                // RMS for perceptual loudness (vs peak which looks jittery).
                let sum_sq: f32 = buf.chunks_exact(2)
                    .map(|c| { let s = i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0; s * s })
                    .sum();
                let rms = (sum_sq / (buf.len() / 2) as f32).sqrt().min(1.0);

                // Fast attack, slow decay for natural VU ballistics.
                let smoothed = if rms > prev { rms * 0.9 + prev * 0.1 }
                                         else { rms * 0.3 + prev * 0.7 };
                prev = smoothed;
                let db = (20.0_f32 * smoothed.max(1e-9_f32).log10()).max(-60.0_f32);
                levels_clone.lock().unwrap().insert(name.to_string(), db);
            }
            // `pa` is dropped here → pa_simple_free() called automatically (RAII).
        });
    }

    // Emitter: publishes the shared map at ~60 fps, stops when this generation ends.
    tokio::spawn(async move {
        while running.load(Ordering::Relaxed) && gen.load(Ordering::Relaxed) == my_gen {
            let snapshot = levels.lock().unwrap().clone();
            let _ = handle.emit("vu-levels", VuLevels { channels: snapshot });
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
        }
    });

    Ok(())
}

/// Stops the VU stream. Reader threads exit cooperatively within one read
/// period (~32 ms) and free their PA connections via RAII.
#[command]
pub async fn stop_vu_stream(app: AppHandle) -> Result<(), String> {
    app.state::<crate::VuState>().0.store(false, Ordering::Relaxed);
    Ok(())
}

// ═══ Settings ═══
fn settings_path() -> PathBuf { dirs::config_dir().unwrap_or_else(||PathBuf::from("~/.config")).join("opengg/ui-settings.json") }
#[command] pub async fn save_ui_settings(settings_json: String) -> Result<(), String> { let p=settings_path(); if let Some(d)=p.parent(){std::fs::create_dir_all(d).ok();} std::fs::write(&p,&settings_json).map_err(|e| format!("{e}")) }
#[command] pub async fn load_ui_settings() -> Result<String, String> { let p=settings_path(); if p.exists(){std::fs::read_to_string(&p).map_err(|e| format!("{e}"))}else{Ok("null".into())} }

/// Opens the user-facing locales directory in the system file manager.
/// Creates `~/.config/opengg/locales/` if it does not yet exist.
/// If `en.json` is absent, writes the bundled English template so translators
/// immediately have a complete base file to duplicate and translate.
#[command]
pub async fn open_locales_folder() -> Result<String, String> {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("opengg/locales");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create dir: {e}"))?;

    // Embed the shipped English locale at compile time — always stays in sync
    // with the app's built-in translations.
    const EN_TEMPLATE: &str = include_str!("../../src/locales/en.json");
    let template_path = dir.join("en.json");
    if !template_path.exists() {
        std::fs::write(&template_path, EN_TEMPLATE)
            .map_err(|e| format!("write en.json template: {e}"))?;
    }

    let path_str = dir.to_string_lossy().to_string();
    open::that(&dir).map_err(|e| format!("open folder: {e}"))?;
    Ok(path_str)
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 3: Modular Plugin / Extensions System
// ══════════════════════════════════════════════════════════════

/// Developer guide written to the extensions folder on first open.
const EXTENSIONS_GUIDE: &str = r#"# How to Create an OpenGG Extension

Extensions are self-contained directories placed here.
Each one is an IIFE JavaScript bundle that registers a Vue 3 component on `window.__ext_<id>`.

---

## Directory Structure

```
~/.local/share/opengg/extensions/
  my-extension/
    manifest.json        ← required — metadata + capability declarations
    index.iife.js        ← IIFE bundle built with Vite/Rollup
    icon.svg             ← optional — shown in Settings → Extensions
    locales/
      en.json            ← optional — i18n strings for this extension
      ar.json
```

The folder name is treated as the extension's `id`.

---

## manifest.json Schema

```json
{
  "id":          "my-extension",
  "name":        "My Extension",
  "description": "A one-line description shown in Settings → Extensions.",
  "version":     "1.0.0",
  "author":      "Your Name",
  "icon":        "icon.svg",
  "main":        "index.iife.js",
  "hasSettings": true
}
```

| Field          | Required | Description |
|----------------|----------|-------------|
| `id`           | ✓        | Unique kebab-case identifier. |
| `name`         | ✓        | Display name. |
| `description`  | ✗        | Short description (≤ 120 chars). |
| `version`      | ✗        | SemVer string e.g. `"1.0.0"`. |
| `author`       | ✗        | Author name or handle. |
| `icon`         | ✗        | Icon filename relative to the extension root (SVG/PNG). |
| `main`         | ✗        | IIFE bundle filename. Omit for metadata-only extensions. |
| `hasSettings`  | ✗        | Set `true` to show a gear button that opens your settings panel. |

---

## IIFE Bundle Pattern

Your bundle must set `window.__ext_<id>` (dashes → underscores in the key):

```js
// index.iife.js
(function () {
  const { defineComponent, ref, h } = window.Vue;

  const SettingsPanel = defineComponent({
    name: 'MyExtSettings',
    setup() {
      const count = ref(0);
      return () => h('div', { style: 'padding:16px' }, [
        h('p', `Count: ${count.value}`),
        h('button', { onClick: () => count.value++ }, 'Increment'),
      ]);
    },
  });

  // Extension id "my-extension" → global key "__ext_my_extension"
  window.__ext_my_extension = {
    settingsComponent: SettingsPanel,
  };
})();
```

`window.Vue` is populated by OpenGG before any extension loads and exposes the
full Vue 3 composition API (`ref`, `computed`, `defineComponent`, `h`, …).

---

## Extension API — window.opengg

OpenGG exposes a restricted bridge for read-only Tauri commands:

```js
const clips = await window.opengg.invoke('get_clip_list');
const port  = window.opengg.mediaPort;  // local media-server port
```

Only a whitelist of non-destructive commands are allowed. Calling a command
not on the whitelist returns a rejected promise with an explanatory error.

---

## Locales

Place `locales/<lang>.json` files alongside `manifest.json`.
OpenGG merges them into the running vue-i18n instance under the namespace
`ext.<your-extension-id>.*` so strings don't collide with core translations.

```json
// locales/en.json
{
  "settingsTitle": "My Extension Settings",
  "countLabel":    "Count"
}
```

Inside your component access them via the injected i18n instance or
`window.Vue.inject('$i18n')`.

---

## Build Setup (Vite)

```js
// vite.config.js
export default {
  build: {
    lib: {
      entry: 'src/index.ts',
      name:  'MyExt',
      formats: ['iife'],
      fileName: () => 'index.iife.js',
    },
    rollupOptions: {
      // Exclude Vue from the bundle — OpenGG provides it via window.Vue
      external: ['vue'],
      output: { globals: { vue: 'Vue' } },
    },
  },
};
```

See `extension-template/` in the OpenGG repository for a complete starter.
"#;

fn extensions_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("opengg/extensions")
}

/// Public re-export so `main.rs` can add the extensions directory to the watcher.
pub fn extensions_dir_pub() -> PathBuf { extensions_dir() }

#[derive(Serialize)]
pub struct ExtensionInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub path: String,
    #[serde(default)]
    pub has_settings: bool,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub ui: Option<String>,
}

/// Creates `~/.local/share/opengg/extensions/` if needed, writes the developer
/// guide on the first visit, then opens the folder in the file manager.
#[command]
pub async fn open_extensions_folder() -> Result<String, String> {
    let dir = extensions_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create dir: {e}"))?;

    let guide = dir.join("HOW_TO_CREATE_EXTENSIONS.md");
    if !guide.exists() {
        std::fs::write(&guide, EXTENSIONS_GUIDE)
            .map_err(|e| format!("write guide: {e}"))?;
    }

    let path_str = dir.to_string_lossy().to_string();
    open::that(&dir).map_err(|e| format!("open folder: {e}"))?;
    Ok(path_str)
}

/// Scans `~/.local/share/opengg/extensions/` for subdirectories containing a
/// `manifest.json`. Returns the parsed metadata for each valid extension.
#[command]
pub async fn scan_extensions() -> Result<Vec<ExtensionInfo>, String> {
    let dir = extensions_dir();
    if !dir.exists() { return Ok(vec![]); }

    let mut exts = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("read dir: {e}"))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }

        let manifest_path = path.join("manifest.json");
        if !manifest_path.exists() { continue; }

        let raw = match std::fs::read_to_string(&manifest_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let v: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue, // skip malformed manifests silently
        };

        let id   = v["id"].as_str().unwrap_or("").to_string();
        let name = v["name"].as_str().unwrap_or(&id).to_string();
        if id.is_empty() || name.is_empty() { continue; }

        exts.push(ExtensionInfo {
            id,
            name,
            description:  v["description"].as_str().unwrap_or("").to_string(),
            version:      v["version"].as_str().unwrap_or("0.0.0").to_string(),
            path:         path.to_string_lossy().to_string(),
            has_settings: v["hasSettings"].as_bool().unwrap_or(false),
            icon:         v["icon"].as_str().map(|s| s.to_string()),
            main:         v["main"].as_str().map(|s| s.to_string()),
            ui:           v["ui"].as_str().map(|s| s.to_string()),
        });
    }

    exts.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(exts)
}

/// Reads every `*.json` file in `~/.config/opengg/locales/` and returns their
/// raw content. The frontend parses each file, extracts `_meta.{name,dir}`,
/// and registers the locale dynamically via `i18n.global.setLocaleMessage`.
#[derive(Serialize)]
pub struct UserLocale {
    pub code: String,
    pub json_content: String,
}

#[command]
pub async fn list_user_locales() -> Result<Vec<UserLocale>, String> {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("opengg/locales");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut locales = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("read dir: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let code = match path.file_stem().and_then(|s| s.to_str()) {
            Some(c) if !c.is_empty() => c.to_string(),
            _ => continue,
        };
        if let Ok(content) = std::fs::read_to_string(&path) {
            locales.push(UserLocale { code, json_content: content });
        }
    }
    Ok(locales)
}

#[derive(Serialize)]
pub struct StorageInfo {
    pub clip_count: u64,
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

/// Returns disk usage for the clips folder plus filesystem free/total space.
#[command]
pub async fn get_storage_info(clip_directories: Vec<String>) -> Result<StorageInfo, String> {
    let mut total_count = 0u64;
    let mut total_used = 0u64;
    let mut first_existing: Option<PathBuf> = None;

    for dir_str in &clip_directories {
        let folder = PathBuf::from(shexp(dir_str));
        if !folder.exists() { continue; }
        if first_existing.is_none() { first_existing = Some(folder.clone()); }
        for e in walkdir::WalkDir::new(&folder).min_depth(1).into_iter().flatten() {
            let p = e.path().to_path_buf();
            if !p.is_file() { continue; }
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            if name.ends_with(".mp4") || name.ends_with(".mkv") || name.ends_with(".webm") || name.ends_with(".mov") {
                total_count += 1;
                if let Ok(meta) = e.metadata() { total_used += meta.len(); }
            }
        }
    }

    let fs_root = first_existing.unwrap_or_else(|| PathBuf::from("/"));
    let (total_bytes, free_bytes) = get_fs_stats(&fs_root);

    Ok(StorageInfo { clip_count: total_count, used_bytes: total_used, total_bytes, free_bytes })
}

#[cfg(unix)]
fn get_fs_stats(path: &PathBuf) -> (u64, u64) {
    use std::os::unix::ffi::OsStrExt;
    let mut stat: libc_statvfs = unsafe { std::mem::zeroed() };
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap_or_default();
    unsafe {
        if libc_statvfs_call(cpath.as_ptr(), &mut stat) == 0 {
            let bsize = stat.f_frsize as u64;
            return (stat.f_blocks * bsize, stat.f_bfree * bsize);
        }
    }
    (0, 0)
}
#[cfg(not(unix))]
fn get_fs_stats(_path: &PathBuf) -> (u64, u64) { (0, 0) }

// Thin statvfs wrapper to avoid adding libc as a direct dep.
#[cfg(unix)]
#[repr(C)]
struct libc_statvfs {
    f_bsize: u64, f_frsize: u64, f_blocks: u64, f_bfree: u64, f_bavail: u64,
    f_files: u64, f_ffree: u64, f_favail: u64, f_fsid: u64, f_flag: u64,
    f_namemax: u64, __spare: [u64; 6],
}
#[cfg(unix)]
extern "C" { fn statvfs(path: *const std::ffi::c_char, buf: *mut libc_statvfs) -> std::ffi::c_int; }
#[cfg(unix)]
unsafe fn libc_statvfs_call(path: *const std::ffi::c_char, buf: *mut libc_statvfs) -> std::ffi::c_int {
    unsafe { statvfs(path, buf) }
}

// ═══ Helpers ═══
fn thumb_dir() -> PathBuf { dirs::data_dir().unwrap_or_else(||PathBuf::from("~/.local/share")).join("opengg/thumbnails") }

/// Count actual audio streams in a file via ffprobe
fn count_audio_streams(path: &str) -> u32 {
    get_audio_stream_global_indices(path).len() as u32
}

/// Get the global stream indices of all audio streams.
fn get_audio_stream_global_indices(path: &str) -> Vec<u32> {
    if let Ok(o) = Command::new("ffprobe")
        .args(["-v", "quiet", "-select_streams", "a", "-show_entries", "stream=index", "-of", "csv=p=0", path])
        .output() {
        if o.status.success() {
            return String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|l| l.trim().parse::<u32>().ok())
                .collect();
        }
    }
    vec![0]
}

/// ★ Epic 5: Probe video resolution for normalized overlay sizing
fn probe_resolution(path: &str) -> (u32, u32) {
    if let Ok(o) = Command::new("ffprobe")
        .args(["-v", "quiet", "-select_streams", "v:0",
            "-show_entries", "stream=width,height", "-of", "csv=s=x:p=0", path])
        .output() {
        if o.status.success() {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let parts: Vec<&str> = s.split('x').collect();
            if parts.len() == 2 {
                let w = parts[0].parse().unwrap_or(1920);
                let h = parts[1].parse().unwrap_or(1080);
                return (w, h);
            }
        }
    }
    (1920, 1080) // fallback
}

/// Resolve a font by user-chosen name (e.g. "Impact") to a system path.
/// Falls back to the generic best-match font if the requested name is not found.
fn find_system_font_by_name(hint: Option<&str>) -> String {
    if let Some(name) = hint {
        let lower = name.to_lowercase();
        let candidates: &[&str] = match lower.as_str() {
            "impact" => &[
                "/usr/share/fonts/truetype/msttcorefonts/Impact.ttf",
                "/usr/share/fonts/truetype/impact.ttf",
                "/usr/share/fonts/impact.ttf",
                "/usr/share/fonts/TTF/Impact.ttf",
            ],
            "tahoma" => &[
                "/usr/share/fonts/truetype/msttcorefonts/Tahoma.ttf",
                "/usr/share/fonts/truetype/tahoma.ttf",
                "/usr/share/fonts/tahoma.ttf",
            ],
            "arial" | "liberation sans" => &[
                "/usr/share/fonts/truetype/msttcorefonts/Arial.ttf",
                "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
                "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
                "/usr/share/fonts/Liberation/LiberationSans-Regular.ttf",
            ],
            "dejavu sans" => &[
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/TTF/DejaVuSans.ttf",
            ],
            _ => &[],
        };
        for p in candidates {
            if Path::new(p).exists() { return p.to_string(); }
        }
        // Unknown name: ask fontconfig
        if let Ok(out) = Command::new("fc-match")
            .args(["--format=%{file}", name])
            .output()
        {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() && Path::new(&p).exists() { return p; }
            }
        }
    }
    find_system_font()
}

/// Find a system font that supports Arabic/CJK/Latin characters.
/// Tries common paths on Arch/Ubuntu/Fedora, falls back to fc-match.
fn find_system_font() -> String {
    let candidates = [
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/TTF/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ];
    for path in &candidates {
        if std::path::Path::new(path).exists() { return path.to_string(); }
    }
    // Fallback: use fc-match to find any available sans-serif font
    if let Ok(output) = Command::new("fc-match").args(["--format=%{file}", "sans"]).output() {
        if output.status.success() {
            let p = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !p.is_empty() && std::path::Path::new(&p).exists() { return p; }
        }
    }
    // Last resort
    "sans".to_string()
}

/// Clear the thumbnail cache directory
#[command]
pub async fn clear_thumbnail_cache() -> Result<u32, String> {
    let td = thumb_dir();
    let mut count = 0u32;
    if td.exists() {
        // Count files before deleting
        if let Ok(entries) = std::fs::read_dir(&td) {
            count = entries.filter_map(|e| e.ok()).filter(|e| e.path().is_file()).count() as u32;
        }
        std::fs::remove_dir_all(&td).map_err(|e| format!("remove: {e}"))?;
    }
    std::fs::create_dir_all(&td).map_err(|e| format!("create: {e}"))?;
    Ok(count)
}
fn resolve_clips_dir(f:&str) -> PathBuf { if !f.is_empty(){return PathBuf::from(shexp(f));} let sp=settings_path(); if sp.exists(){if let Ok(j)=std::fs::read_to_string(&sp){if let Ok(v)=serde_json::from_str::<serde_json::Value>(&j){ if let Some(arr)=v["settings"]["clip_directories"].as_array(){if let Some(first)=arr.first(){if let Some(f)=first.as_str(){return PathBuf::from(shexp(f));}}}}}} default_clips_dir() }
pub fn default_clips_dir() -> PathBuf { dirs::video_dir().unwrap_or_else(||dirs::home_dir().unwrap().join("Videos")).join("OpenGG") }
pub fn shexp(p:&str) -> String { if p.starts_with("~/"){if let Some(h)=dirs::home_dir(){return p.replacen("~",&h.to_string_lossy(),1);}} p.into() }
pub fn settings_path_pub() -> PathBuf { settings_path() }

/// Returns all directories to scan for clips: all entries from `clip_directories` in settings.
fn get_all_clip_dirs(primary: &str) -> Vec<PathBuf> {
    let sp = settings_path();
    if let Ok(j) = std::fs::read_to_string(&sp) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&j) {
            if let Some(arr) = v["settings"]["clip_directories"].as_array() {
                let dirs: Vec<PathBuf> = arr.iter()
                    .filter_map(|s| s.as_str())
                    .map(|p| PathBuf::from(shexp(p)))
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                if !dirs.is_empty() { return dirs; }
            }
        }
    }
    vec![resolve_clips_dir(primary)]
}
fn auto_name(input:&str,suffix:&str) -> String { let p=Path::new(input); let s=p.file_stem().unwrap_or_default().to_string_lossy(); let e=p.extension().unwrap_or_default().to_string_lossy(); p.parent().unwrap_or(Path::new(".")).join(format!("{s}{suffix}.{e}")).to_string_lossy().into() }

/// Diff current watched directories against the settings file and update the
/// notify watcher accordingly (watch new dirs, unwatch removed ones).
#[command]
pub async fn update_watch_dirs(app: tauri::AppHandle) -> Result<(), String> {
    use notify::{RecursiveMode, Watcher};
    let desired = get_all_clip_dirs("");
    let watcher_state = app.state::<crate::WatcherHandle>();
    let watched_state = app.state::<crate::WatchedDirs>();
    let mut guard = watcher_state.0.lock().map_err(|e| e.to_string())?;
    let mut current = watched_state.0.lock().map_err(|e| e.to_string())?;
    if let Some(watcher) = guard.as_mut() {
        for dir in current.iter() {
            if !desired.contains(dir) {
                let _ = watcher.unwatch(dir);
                log::info!("Watcher: unwatched {:?}", dir);
            }
        }
        for dir in &desired {
            if !current.contains(dir) {
                let _ = std::fs::create_dir_all(dir);
                if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
                    log::warn!("Watcher: cannot watch {:?}: {e}", dir);
                } else {
                    log::info!("Watcher: now watching {:?}", dir);
                }
            }
        }
        *current = desired;
    }
    Ok(())
}

/// Prepend "OpenGG_" to the filename if it doesn't already start with it.
fn smart_prefix(path: &str) -> String {
    let p = Path::new(path);
    let stem = p.file_stem().unwrap_or_default().to_string_lossy();
    if stem.starts_with("OpenGG_") { return path.to_string(); }
    let ext = p.extension().unwrap_or_default().to_string_lossy();
    let new_name = if ext.is_empty() { format!("OpenGG_{stem}") } else { format!("OpenGG_{stem}.{ext}") };
    p.parent().unwrap_or(Path::new(".")).join(new_name).to_string_lossy().into()
}

#[tauri::command]
pub async fn register_global_shortcuts(
    app: tauri::AppHandle,
    save_replay: String,
    toggle_recording: String,
    screenshot: String,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    let combos: &[(&str, &str)] = &[
        (&save_replay, "save_replay"),
        (&toggle_recording, "toggle_recording"),
        (&screenshot, "screenshot"),
    ];
    for (combo, action) in combos {
        if combo.is_empty() { continue; }
        let shortcut: tauri_plugin_global_shortcut::Shortcut = combo.parse()
            .map_err(|_| format!("Invalid shortcut: '{combo}'"))?;
        let a = action.to_string();
        let app2 = app.clone();
        gs.on_shortcut(shortcut, move |_app, _sc, event| {
            if event.state() == ShortcutState::Pressed {
                let _ = app2.emit(&format!("global-shortcut-{a}"), ());
            }
        }).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn probe_video(p:&Path) -> (f64,u32,u32) { let d=probe_duration(&p.to_string_lossy()); let dm=Command::new("ffprobe").args(["-v","quiet","-select_streams","v:0","-show_entries","stream=width,height","-of","csv=s=x:p=0",&p.to_string_lossy()]).output().ok().map(|o|String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default(); let ps:Vec<&str>=dm.split('x').collect(); (d,ps.first().and_then(|s|s.parse().ok()).unwrap_or(0),ps.get(1).and_then(|s|s.parse().ok()).unwrap_or(0)) }
fn probe_duration(p:&str) -> f64 { Command::new("ffprobe").args(["-v","quiet","-show_entries","format=duration","-of","default=noprint_wrappers=1:nokey=1",p]).output().ok().and_then(|o|String::from_utf8_lossy(&o.stdout).trim().parse().ok()).unwrap_or(0.0) }
fn hash_str(s:&str) -> u64 { let mut h:u64=5381; for b in s.bytes(){h=h.wrapping_mul(33).wrapping_add(b as u64);} h }

/// Extract "YYYY-MM-DD HH:MM" from a filename stem.
/// Handles two formats:
///   SteelSeries GG:        GameName__YYYY-MM-DD__HH-MM-SS  (double underscore)
///   gpu-screen-recorder:   Prefix_YYYY-MM-DD_HH-MM-SS      (single underscore)
/// Returns None if neither pattern matches.
fn date_from_stem(stem: &str) -> Option<String> {
    let b = stem.as_bytes();
    // SteelSeries: YYYY-MM-DD__HH-MM-SS (20 chars)
    // Pattern positions: DDDD-DD-DD__DD-DD-DD
    if b.len() >= 20 {
        for i in 0..=b.len()-20 {
            let s = &b[i..i+20];
            if s[4]==b'-' && s[7]==b'-' && s[10]==b'_' && s[11]==b'_' && s[14]==b'-' && s[17]==b'-'
                && s[..4].iter().all(u8::is_ascii_digit)
                && s[5..7].iter().all(u8::is_ascii_digit)
                && s[8..10].iter().all(u8::is_ascii_digit)
                && s[12..14].iter().all(u8::is_ascii_digit)
                && s[15..17].iter().all(u8::is_ascii_digit)
                && s[18..20].iter().all(u8::is_ascii_digit)
            {
                let t = std::str::from_utf8(&s[..20]).unwrap();
                return Some(format!("{} {}:{}", &t[..10], &t[12..14], &t[15..17]));
            }
        }
    }
    // gpu-screen-recorder: YYYY-MM-DD_HH-MM-SS (19 chars)
    // Pattern positions: DDDD-DD-DD_DD-DD-DD
    if b.len() >= 19 {
        for i in 0..=b.len()-19 {
            let s = &b[i..i+19];
            if s[4]==b'-' && s[7]==b'-' && s[10]==b'_' && s[13]==b'-' && s[16]==b'-'
                && s[..4].iter().all(u8::is_ascii_digit)
                && s[5..7].iter().all(u8::is_ascii_digit)
                && s[8..10].iter().all(u8::is_ascii_digit)
                && s[11..13].iter().all(u8::is_ascii_digit)
                && s[14..16].iter().all(u8::is_ascii_digit)
                && s[17..19].iter().all(u8::is_ascii_digit)
            {
                let t = std::str::from_utf8(&s[..19]).unwrap();
                return Some(format!("{} {}:{}", &t[..10], &t[11..13], &t[14..16]));
            }
        }
    }
    None
}

/// Convert Unix timestamp to local-time "YYYY-MM-DD HH:MM" using libc::localtime_r.
fn fmt_ts_local(s: i64) -> String {
    #[cfg(unix)]
    {
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&s, &mut tm); }
        return format!("{}-{:02}-{:02} {:02}:{:02}",
            tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday,
            tm.tm_hour, tm.tm_min);
    }
    #[allow(unreachable_code)]
    fmt_ts(s)
}

/// Accurate Unix-timestamp → "YYYY-MM-DD HH:MM" using Howard Hinnant's civil calendar algorithm.
fn fmt_ts(s: i64) -> String {
    let days    = s / 86400;
    let rem     = s % 86400;
    let (h, m)  = (rem / 3600, (rem % 3600) / 60);
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y   = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp  = (5 * doy + 2) / 153;
    let d   = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr  = if mth <= 2 { y + 1 } else { y };
    format!("{yr}-{mth:02}-{d:02} {h:02}:{m:02}")
}
fn get_vol(n:&str) -> Option<f32> { let o=Command::new("pactl").args(["get-sink-volume",n]).output().ok()?; for p in String::from_utf8_lossy(&o.stdout).split('/'){let s=p.trim();if s.ends_with('%'){if let Ok(v)=s.trim_end_matches('%').trim().parse::<f32>(){return Some(v/100.0);}}} None }
// ══════════════════════════════════════════════════════════════
//  ★ EPIC 2: Crash-log directory opener
// ══════════════════════════════════════════════════════════════

/// Returns the directory where per-session log files live.
/// Mirrors main::logs_dir — kept as a standalone helper so commands can call it
/// without pulling main into the module graph.
pub fn crash_log_dir() -> PathBuf {
    // Same compile-time path resolution as main::LOGS_DIR — lands at <repo>/Logs.
    const LOGS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Logs");
    let p = PathBuf::from(LOGS_DIR);
    let _ = std::fs::create_dir_all(&p);
    p.canonicalize().unwrap_or(p)
}

/// Opens the OS file manager at the crash-log directory so the user can
/// retrieve logs for bug reports.
#[command]
pub async fn open_crash_logs_folder() -> Result<(), String> {
    let dir = crash_log_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{e}"))?;
    open::that(&dir).map_err(|e| format!("{e}"))?;
    Ok(())
}

// ══════════════════════════════════════════════════════════════
//  ★ Virtual Audio Onboarding / Factory Reset
// ══════════════════════════════════════════════════════════════

const VIRTUAL_CHANNELS: &[&str] = &["Game", "Chat", "Media", "Aux"];

/// Returns true if all 4 OpenGG virtual sinks are present in PipeWire.
#[command]
pub async fn check_virtual_audio_status() -> Result<bool, String> {
    let out = run_cmd("pactl", &["list", "sinks", "short"]).unwrap_or_default();
    let all_present = VIRTUAL_CHANNELS.iter().all(|ch| out.contains(&format!("OpenGG_{ch}")));
    Ok(all_present)
}

/// Create all OpenGG virtual null sinks via pactl (idempotent — skips existing).
#[command]
pub async fn create_virtual_audio() -> Result<(), String> {
    let existing = run_cmd("pactl", &["list", "sinks", "short"]).unwrap_or_default();
    for ch in VIRTUAL_CHANNELS {
        let sink_name = format!("OpenGG_{ch}");
        if existing.contains(&sink_name) { continue; }
        run_cmd("pactl", &[
            "load-module", "module-null-sink",
            &format!("sink_name={sink_name}"),
            &format!("sink_properties=node.description=\"OpenGG - {ch}\" node.nick=\"OpenGG - {ch}\" device.description=\"OpenGG - {ch}\" media.name=\"OpenGG - {ch}\" node.name=\"opengg_{}\"", ch.to_lowercase()),
            "channels=2", "channel_map=front-left,front-right",
        ])?;
    }
    log::info!("Virtual audio sinks created");
    Ok(())
}

/// Unload only the OpenGG virtual sinks without restarting PipeWire/WirePlumber.
#[command]
pub async fn remove_virtual_audio() -> Result<(), String> {
    let modules_json = run_cmd("pactl", &["-f", "json", "list", "modules"]).unwrap_or_default();
    if let Ok(mods) = serde_json::from_str::<Vec<serde_json::Value>>(&modules_json) {
        for m in &mods {
            if m["name"].as_str() != Some("module-null-sink") { continue; }
            let args = m["argument"].as_str().unwrap_or("");
            if !args.contains("OpenGG_") { continue; }
            if let Some(idx) = m["index"].as_u64() {
                let _ = run_cmd("pactl", &["unload-module", &idx.to_string()]);
            }
        }
    }
    log::info!("Virtual audio removed (PipeWire/WirePlumber left running)");
    Ok(())
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 3: Startup audio hydration
//
//  Reads the saved per-channel device map from ui-settings.json
//  and re-applies pw-link connections so virtual sinks stay
//  routed to the correct physical output after a restart.
// ══════════════════════════════════════════════════════════════

#[command]
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
            Command::new("pactl")
                .args(["set-default-source", &device])
                .output()
                .ok();
            eprintln!("hydrate: Mic source → {device}");
        } else if channel == "Master" {
            // Setting default sink is intentionally skipped — it changes the
            // system-wide default and surprises the user.  The frontend will
            // call set_channel_device if the user actively changes it.
        } else {
            // Virtual sink (Game/Chat/Media/Aux): disconnect old links then
            // reconnect to the saved physical device.
            let sink = format!("OpenGG_{channel}");
            if let Ok(def) = run_cmd("pactl", &["get-default-sink"]) {
                for p in ["FL", "FR"] {
                    Command::new("pw-link")
                        .args(["-d", &format!("{sink}:monitor_{p}"), &format!("{def}:playback_{p}")])
                        .output()
                        .ok();
                }
            }
            for p in ["FL", "FR"] {
                Command::new("pw-link")
                    .args([&format!("{sink}:monitor_{p}"), &format!("{device}:playback_{p}")])
                    .output()
                    .ok();
            }
            eprintln!("hydrate: {channel} → {device}");
        }
    }
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 4: Background-daemon control commands
// ══════════════════════════════════════════════════════════════

/// Updates the in-process RunInBackground flag. Called from the frontend
/// whenever the "Keep running in background when closed" toggle changes.
#[command]
pub async fn set_run_in_background(app: AppHandle, val: bool) -> Result<(), String> {
    app.state::<crate::RunInBackground>()
        .0
        .store(val, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// Returns true if the XDG autostart entry for OpenGG exists.
#[command]
pub async fn get_autostart() -> Result<bool, String> {
    let desktop = dirs::home_dir()
        .ok_or_else(|| "no home dir".to_string())?
        .join(".config/autostart/opengg.desktop");
    Ok(desktop.exists())
}

/// Creates or removes the XDG autostart `.desktop` entry.
#[command]
pub async fn set_autostart(enable: bool) -> Result<(), String> {
    let dir = dirs::home_dir()
        .ok_or_else(|| "no home dir".to_string())?
        .join(".config/autostart");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{e}"))?;
    let desktop = dir.join("opengg.desktop");

    if enable {
        let exe = std::env::current_exe().map_err(|e| format!("{e}"))?;
        let content = format!(
            "[Desktop Entry]\nType=Application\nName=OpenGG\nExec={}\nHidden=false\nNoDisplay=false\nX-GNOME-Autostart-enabled=true\n",
            exe.display()
        );
        std::fs::write(&desktop, content).map_err(|e| format!("{e}"))?;
    } else if desktop.exists() {
        std::fs::remove_file(&desktop).map_err(|e| format!("{e}"))?;
    }
    Ok(())
}

// ══════════════════════════════════════════════════════════════
//  ★ GPU Screen Recorder integration
// ══════════════════════════════════════════════════════════════

use crate::GsrProcess;

/// Detect the primary monitor's resolution via xrandr for use with `-w focused`.
/// Returns "WxH" (e.g. "1920x1080"). Falls back to "1920x1080" if detection fails.
fn detect_primary_resolution() -> String {
    if let Ok(o) = std::process::Command::new("xrandr").arg("--current").output() {
        let text = String::from_utf8_lossy(&o.stdout);
        let mut any_connected: Option<String> = None;
        for line in text.lines() {
            if line.contains(" connected") && !line.contains(" disconnected") {
                for word in line.split_whitespace() {
                    // Resolution tokens look like "1920x1080+0+0"
                    if word.contains('x') && word.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                        let res = word.split('+').next().unwrap_or(word).to_string();
                        if res.split('x').count() == 2 {
                            if line.contains("primary") {
                                return res; // prefer explicitly marked primary
                            }
                            any_connected.get_or_insert(res);
                        }
                    }
                }
            }
        }
        if let Some(r) = any_connected { return r; }
    }
    log::warn!("detect_primary_resolution: xrandr failed or no connected display; defaulting to 1920x1080");
    "1920x1080".to_string()
}

/// List PipeWire/PulseAudio sink names for the audio capture devices dropdown.
#[command]
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

/// Returns the current display server session type: "wayland", "x11", or "unknown".
#[command]
pub fn get_session_type() -> String {
    if let Ok(t) = std::env::var("XDG_SESSION_TYPE") {
        let t = t.trim().to_lowercase();
        if !t.is_empty() { return t; }
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

/// Returns a list of connected monitors via Tauri's built-in monitor enumeration.
/// Each entry has a `name` (connector name as reported by the OS, e.g. "DP-1") and
/// a human-readable `label` (e.g. "Display 1 — 1920×1080 (DP-1)").
/// Falls back to a single "Primary Monitor / screen" entry if enumeration fails.
#[derive(Serialize)]
pub struct MonitorInfo {
    pub name: String,
    pub label: String,
}

#[command]
pub fn list_monitors(_app: AppHandle) -> Vec<MonitorInfo> {
    // Use gpu-screen-recorder's own monitor enumeration, NOT Tauri's available_monitors().
    //
    // Tauri's API returns EDID model names ("BenQ GW2780", "MASI251K03") which are NOT
    // valid GSR `-w` targets — passing them causes an immediate GSR crash.
    // GSR's --list-monitors returns the exact X11/Wayland connector names (e.g. "DP-1",
    // "HDMI-A-1", "screen") that its `-w` flag accepts.
    //
    // Example stdout from `gpu-screen-recorder --list-monitors`:
    //   screen
    //   DP-1
    //   DP-2
    //   HDMI-A-1
    // Each non-empty line is one valid `-w` argument.
    let output = std::process::Command::new("gpu-screen-recorder")
        .arg("--list-monitors")
        .output();

    let stdout = match output {
        Ok(o) if !o.stdout.is_empty() => String::from_utf8_lossy(&o.stdout).to_string(),
        Ok(o) => {
            log::warn!(
                "gpu-screen-recorder --list-monitors produced no output (exit={:?}); falling back",
                o.status.code()
            );
            String::new()
        }
        Err(e) => {
            log::warn!("gpu-screen-recorder --list-monitors failed to spawn: {e}; falling back");
            String::new()
        }
    };

    let mut monitors: Vec<MonitorInfo> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|name| {
            // "screen" is GSR's "capture all outputs / entire desktop" pseudo-target.
            let label = if name == "screen" {
                "Entire Desktop".to_string()
            } else {
                // Connector names are already human-readable ("DP-1", "HDMI-A-1").
                name.to_string()
            };
            MonitorInfo { name: name.to_string(), label }
        })
        .collect();

    if monitors.is_empty() {
        // GSR not installed, not in PATH, or no displays detected — safe fallback.
        monitors.push(MonitorInfo { name: "screen".into(), label: "Entire Desktop".into() });
    }

    monitors
}

/// Detect which monitor output (e.g. "DP-1") the currently focused window is on.
/// Uses xdotool to find the window position, then xrandr to match it to a monitor.
/// Returns None on Wayland, if tools are unavailable, or if detection fails.
fn get_focused_window_monitor() -> Option<String> {
    // Wayland: xdotool is unreliable — bail early
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        log::info!("Wayland detected — skipping xdotool monitor detection");
        return None;
    }

    // Step 1: get the focused window ID (decimal)
    let win_id_out = std::process::Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .ok()?;
    let win_id = String::from_utf8_lossy(&win_id_out.stdout).trim().to_string();
    if win_id.is_empty() { return None; }

    // Step 2: get the window's absolute position on the desktop
    // xdotool getwindowgeometry output: "Window NNN\n  Position: X,Y (screen N)\n  Geometry: WxH"
    let geom_out = std::process::Command::new("xdotool")
        .args(["getwindowgeometry", "--shell", &win_id])
        .output()
        .ok()?;
    let geom_text = String::from_utf8_lossy(&geom_out.stdout).into_owned();
    // --shell format: "X=123\nY=456\nWIDTH=...\nHEIGHT=..."
    let win_x: i64 = geom_text.lines()
        .find(|l| l.starts_with("X="))
        .and_then(|l| l[2..].parse().ok())?;
    let win_y: i64 = geom_text.lines()
        .find(|l| l.starts_with("Y="))
        .and_then(|l| l[2..].parse().ok())?;

    // Step 3: parse `xrandr --listmonitors` to get monitor names + offsets
    // Output format per monitor line: "  N: +*DP-1 1920/527x1080/296+0+0  ..."
    let xrandr_out = std::process::Command::new("xrandr")
        .arg("--listmonitors")
        .output()
        .ok()?;
    let xrandr_text = String::from_utf8_lossy(&xrandr_out.stdout).into_owned();

    // Each monitor line: "  0: +*DP-1 1920/527x1080/296+0+0   0"
    // Geometry token: "<w>/<mm>x<h>/<mm>+<ox>+<oy>"
    let mut best: Option<String> = None;
    for line in xrandr_text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 { continue; }
        // parts[1] is monitor name (may have leading '+' or '*' flags)
        let name = parts[1].trim_start_matches('+').trim_start_matches('*');
        // parts[2] is geometry token
        let geom_tok = parts[2];
        // Parse "WxH+OX+OY" or "W/mmxH/mm+OX+OY"
        let geom_clean = geom_tok
            .split('+')
            .collect::<Vec<_>>();
        if geom_clean.len() < 3 { continue; }
        let ox: i64 = geom_clean[1].parse().ok()?;
        let oy: i64 = geom_clean[2].parse().ok()?;
        // Width/height may be "1920/527" or plain "1920"
        let wh = geom_clean[0];
        let (w_part, h_part) = wh.split_once('x')?;
        let w: i64 = w_part.split('/').next()?.parse().ok()?;
        let h: i64 = h_part.split('/').next()?.parse().ok()?;
        // Check if window origin falls inside this monitor's rectangle
        if win_x >= ox && win_x < ox + w && win_y >= oy && win_y < oy + h {
            best = Some(name.to_string());
            // Prefer primary (marked with '*')
            if parts[1].contains('*') { break; }
        }
    }
    if let Some(ref mon) = best {
        log::info!("Focused window (id={win_id}) at ({win_x},{win_y}) → monitor {mon}");
    }
    best
}

/// Returns true if the gpu-screen-recorder binary is found in PATH.
#[command]
pub fn check_gsr_installed() -> bool {
    std::process::Command::new("which")
        .arg("gpu-screen-recorder")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Start gpu-screen-recorder in replay-buffer mode.
/// Quality is passed directly as a GSR preset string: cbr | medium | high | very_high | ultra.
/// When quality is "cbr", `bitrate_kbps` sets the target bitrate (e.g. 8000 = 8 Mbps).
/// `monitor_target` is passed to `-w` (e.g. "screen", "DP-1", "HDMI-1").
/// Audio sources are PipeWire sink names without the "OpenGG_" prefix (e.g. ["Game","Chat","Mic"]).
#[command]
pub fn start_gsr_replay(
    app: AppHandle,
    output_dir: String,
    replay_secs: u32,
    fps: u32,
    quality: String,
    bitrate_kbps: Option<u32>,
    monitor_target: String,
    audio_sources: Vec<String>,
) -> Result<(), String> {
    use crate::GsrSpawnParams;
    let state = app.state::<GsrProcess>();
    let mut lock = state.0.lock().unwrap();
    if lock.is_some() {
        return Err("gpu-screen-recorder is already running".into());
    }
    let expanded = shexp(&output_dir);
    std::fs::create_dir_all(&expanded)
        .map_err(|e| format!("Cannot create output dir '{expanded}': {e}"))?;

    // Guard against stale settings that contain EDID model names or resolution strings
    // (e.g. "1920x1080", "BenQ GW2780") left over from the old Tauri monitor API.
    // GSR only accepts connector names ("screen", "DP-1", "HDMI-A-1", "focused").
    // A valid connector name never starts with a digit and never contains a space.
    let monitor_target = {
        let looks_invalid = monitor_target.is_empty()
            || monitor_target.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
            || monitor_target.contains(' ');
        if looks_invalid {
            log::warn!(
                "gsrMonitorTarget {:?} is not a valid GSR connector name — resetting to 'screen'",
                monitor_target
            );
            "screen".to_string()
        } else {
            monitor_target
        }
    };

    // For "focused" target: resolve to an actual monitor name using xdotool + xrandr.
    // This fixes multi-monitor setups where `-w focused` captures the wrong display.
    // Falls back to "focused" if detection fails (Wayland, missing tools, or parse error).
    let target = match monitor_target.as_str() {
        "focused" => {
            get_focused_window_monitor()
                .unwrap_or_else(|| {
                    log::warn!("Could not detect monitor for focused window; falling back to -w focused");
                    "focused".to_string()
                })
        }
        "" | "screen" => "screen".to_string(),
        other => other.to_string(),
    };
    let fps_str = fps.to_string();
    let secs_str = replay_secs.to_string();

    // "-w focused" (fallback only) requires an explicit resolution; detect primary monitor
    let focused_resolution = if target == "focused" {
        Some(detect_primary_resolution())
    } else {
        None
    };

    let mut cmd = std::process::Command::new("gpu-screen-recorder");
    cmd.args([
        "-w", &target,
        "-f", &fps_str,
        "-r", &secs_str,
        "-c", "mp4",
        "-o", &expanded,
    ]);

    // CBR mode: GSR requires an integer -q index even when -bm cbr is set. 0 = lowest quality
    // index, which is effectively ignored when the bitrate is set via -ffmpeg-opts.
    if quality == "cbr" {
        cmd.args(["-bm", "cbr", "-q", "0"]);
        if let Some(kbps) = bitrate_kbps {
            let bv = format!("-b:v {}k", kbps);
            cmd.args(["-ffmpeg-opts", &bv]);
        }
    } else {
        cmd.args(["-q", quality.as_str()]);
    }

    // Append resolution when capturing focused window
    if let Some(ref res) = focused_resolution {
        cmd.args(["-s", res]);
    }

    for src in &audio_sources {
        // Legacy short names (e.g. "Game") are prefixed; full sink names are passed as-is
        let sink = if src.contains('_') || src.contains('.') || src.contains('-') {
            src.clone()
        } else {
            format!("OpenGG_{src}")
        };
        // PipeWire/PulseAudio virtual sinks require the .monitor suffix for capture
        let monitor = if sink.ends_with(".monitor") { sink } else { format!("{sink}.monitor") };
        cmd.args(["-a", &monitor]);
    }

    let child = cmd.spawn()
        .map_err(|e| format!("Failed to start gpu-screen-recorder: {e}"))?;
    log::info!(
        "GSR started (pid {}) replay={}s fps={fps} quality={quality} bitrate={bitrate_kbps:?}kbps target={target} dir={expanded} audio={:?}",
        child.id(), replay_secs, audio_sources
    );
    *lock = Some((child, GsrSpawnParams {
        output_dir, replay_secs, fps, quality, bitrate_kbps, monitor_target, audio_sources,
    }));
    let _ = app.emit("gsr-status-changed", serde_json::json!({"running": true}));
    Ok(())
}

/// Sanitize a string so it is safe to use as a filename component.
/// Replaces any character that is not alphanumeric, a hyphen, or an underscore with `_`.
/// Strips leading/trailing underscores and collapses consecutive underscores.
fn sanitize_filename(s: &str) -> String {
    let raw: String = s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect();
    // Collapse runs of underscores, then trim boundary underscores
    let mut out = String::with_capacity(raw.len());
    let mut prev_under = false;
    for c in raw.chars() {
        if c == '_' {
            if !prev_under { out.push(c); }
            prev_under = true;
        } else {
            out.push(c);
            prev_under = false;
        }
    }
    out.trim_matches('_').to_string()
}

/// Find the newest video file written *strictly after* `after_time` in `dir`.
/// Polls every 200 ms for up to `timeout_ms` milliseconds.
/// This avoids the mtime-collision bug where a previously-played clip appears
/// newer than the freshly flushed replay file.
fn newest_video_after(
    dir: &str,
    after_time: std::time::SystemTime,
    timeout_ms: u64,
) -> Option<std::path::PathBuf> {
    const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "webm", "avi", "mov", "ts", "flv"];
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        let found = std::fs::read_dir(dir)
            .ok()
            .and_then(|rd| {
                rd.filter_map(|e| e.ok())
                  .filter(|e| {
                      e.path().extension()
                          .and_then(|x| x.to_str())
                          .map(|x| VIDEO_EXTS.contains(&x.to_lowercase().as_str()))
                          .unwrap_or(false)
                  })
                  .filter(|e| {
                      e.metadata()
                          .and_then(|m| m.modified())
                          .map(|t| t > after_time)
                          .unwrap_or(false)
                  })
                  .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
                  .map(|e| e.path())
            });
        if found.is_some() {
            return found;
        }
        if std::time::Instant::now() >= deadline {
            log::warn!("newest_video_after: no new file in {dir} after {timeout_ms}ms");
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Save the current replay buffer via SIGUSR1.
/// After the file flushes, the saved clip is renamed to `<GameName>_<timestamp>.mp4`.
/// If `restart_on_save` is true, GSR is also killed and respawned so the next save
/// captures only footage recorded after this moment.
#[command]
pub fn save_gsr_replay(app: AppHandle, restart_on_save: bool) -> Result<(), String> {
    use crate::GsrSpawnParams;
    let state = app.state::<GsrProcess>();

    // Capture active window title BEFORE signalling (window focus may change after).
    let game_title = std::process::Command::new("xdotool")
        .args(["getactivewindow", "getwindowname"])
        .output()
        .ok()
        .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None })
        .unwrap_or_else(|| "Unknown".to_string());

    // Capture current time just before SIGUSR1 so we can identify the new file by mtime.
    // This avoids the mtime-collision bug where a previously-played clip has a newer
    // mtime than the freshly-flushed replay file.
    let pre_save_time = std::time::SystemTime::now();

    // Step 1: send SIGUSR1 and clone spawn params.
    // Lock scope is tight — we drop it before calling start_gsr_replay to avoid deadlock.
    let (output_dir_exp, restart_params): (String, Option<GsrSpawnParams>) = {
        let lock = state.0.lock().unwrap();
        match &*lock {
            Some((child, params)) => {
                let pid = child.id();
                #[cfg(unix)]
                unsafe { libc::kill(pid as libc::pid_t, libc::SIGUSR1); }
                log::info!("GSR SIGUSR1 → pid {pid}");
                let expanded = shexp(&params.output_dir);
                let rp = if restart_on_save {
                    Some(GsrSpawnParams {
                        output_dir:     params.output_dir.clone(),
                        replay_secs:    params.replay_secs,
                        fps:            params.fps,
                        quality:        params.quality.clone(),
                        bitrate_kbps:   params.bitrate_kbps,
                        monitor_target: params.monitor_target.clone(),
                        audio_sources:  params.audio_sources.clone(),
                    })
                } else {
                    None
                };
                (expanded, rp)
            }
            None => return Err("gpu-screen-recorder is not running".into()),
        }
    }; // lock dropped here

    // Step 2: poll for the new file (written after pre_save_time) for up to 5 s.
    // Using a polling loop instead of a fixed sleep makes this both more reliable
    // and faster on fast NVMe storage while still handling slow HDDs.

    // Step 3: rename the file that appeared after the SIGUSR1 signal.
    if let Some(src_path) = newest_video_after(&output_dir_exp, pre_save_time, 5000) {
        let safe_name = sanitize_filename(&game_title);
        let safe_name = if safe_name.is_empty() { "Clip".to_string() } else { safe_name };
        let now = {
            use std::time::{SystemTime, UNIX_EPOCH};
            let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            // Format as YYYY-MM-DD_HH-MM-SS using simple arithmetic (no chrono dep).
            let s = secs;
            let sec  = s % 60;
            let min  = (s / 60) % 60;
            let hour = (s / 3600) % 24;
            let days = s / 86400; // days since 1970-01-01
            // Rata Die algorithm → Gregorian calendar
            let z = days + 719468;
            let era = z / 146097;
            let doe = z - era * 146097;
            let yoe = (doe - doe/1460 + doe/36524 - doe/146096) / 365;
            let y   = yoe + era * 400;
            let doy = doe - (365*yoe + yoe/4 - yoe/100);
            let mp  = (5*doy + 2) / 153;
            let d   = doy - (153*mp + 2)/5 + 1;
            let m   = if mp < 10 { mp + 3 } else { mp - 9 };
            let y   = if m <= 2 { y + 1 } else { y };
            format!("{y:04}-{m:02}-{d:02}_{hour:02}-{min:02}-{sec:02}")
        };
        let new_name = format!("{safe_name}_{now}.mp4");
        let dest = std::path::Path::new(&output_dir_exp).join(&new_name);
        let (save_ok, filesize_mb) = if let Err(e) = std::fs::rename(&src_path, &dest) {
            log::warn!("GSR clip rename failed ({src_path:?} → {dest:?}): {e}");
            (false, 0.0f64)
        } else {
            log::info!("GSR clip saved as {new_name}");
            let mb = std::fs::metadata(&dest).map(|m| m.len() as f64 / 1_000_000.0).unwrap_or(0.0);
            (true, mb)
        };
        let _ = app.emit("clip-saved", serde_json::json!({
            "game":        game_title,
            "filename":    new_name,
            "filesize_mb": (filesize_mb * 10.0).round() / 10.0,
            "success":     save_ok,
        }));
    }

    // Step 4: if restart requested, kill and respawn GSR.
    if let Some(params) = restart_params {
        {
            let mut lock = state.0.lock().unwrap();
            if let Some((mut child, _)) = lock.take() {
                gsr_kill_graceful(&mut child);
                log::info!("GSR stopped for restart-on-save (SIGINT + wait)");
            }
        } // lock dropped before respawn
        start_gsr_replay(
            app,
            params.output_dir,
            params.replay_secs,
            params.fps,
            params.quality,
            params.bitrate_kbps,
            params.monitor_target,
            params.audio_sources,
        )?;
        log::info!("GSR restarted (restart_on_save=true)");
    }
    Ok(())
}

/// Gracefully kill and immediately respawn GSR with updated settings (hot-reload).
#[command]
pub fn restart_gsr_replay(
    app: AppHandle,
    output_dir: String,
    replay_secs: u32,
    fps: u32,
    quality: String,
    bitrate_kbps: Option<u32>,
    monitor_target: String,
    audio_sources: Vec<String>,
) -> Result<(), String> {
    {
        let state = app.state::<GsrProcess>();
        let mut lock = state.0.lock().unwrap();
        if let Some((mut child, _)) = lock.take() {
            gsr_kill_graceful(&mut child);
            log::info!("GSR stopped for restart (SIGINT + wait)");
        }
    }
    start_gsr_replay(app, output_dir, replay_secs, fps, quality, bitrate_kbps, monitor_target, audio_sources)
}

/// Send SIGINT to let GSR flush cleanly, wait up to 2 s, then SIGKILL as fallback.
/// Reaping with `.wait()` prevents zombie PIDs.
/// Also kills `gsr-kms-server` — a helper daemon spawned by GSR that survives
/// the parent process and must be cleaned up explicitly.
fn gsr_kill_graceful(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as libc::pid_t;
        unsafe { libc::kill(pid, libc::SIGINT); }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => {
                    // Exited cleanly — still need to reap gsr-kms-server
                    kill_kms_server();
                    return;
                }
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                _ => break, // timeout — fall through to SIGKILL
            }
        }
    }
    let _ = child.kill(); // SIGKILL — last resort
    let _ = child.wait(); // reap zombie
    kill_kms_server();
}

/// Kill the `gsr-kms-server` helper daemon that gpu-screen-recorder spawns.
/// It does not exit when the parent process is killed, so we must clean it up
/// explicitly to prevent dangling GPU capture sessions.
fn kill_kms_server() {
    let _ = std::process::Command::new("pkill")
        .args(["-9", "gsr-kms-server"])
        .output();
}

/// Stop the GSR process gracefully (SIGINT → SIGKILL fallback).
#[command]
pub fn stop_gsr_replay(app: AppHandle) -> Result<(), String> {
    let state = app.state::<GsrProcess>();
    let mut lock = state.0.lock().unwrap();
    if let Some((mut child, _)) = lock.take() {
        gsr_kill_graceful(&mut child);
        log::info!("GSR stopped (SIGINT + wait)");
    }
    drop(lock); // release mutex before emitting
    let _ = app.emit("gsr-status-changed", serde_json::json!({"running": false}));
    Ok(())
}

/// Returns true if the GSR process is currently running.
#[command]
pub fn is_gsr_running(app: AppHandle) -> bool {
    let state = app.state::<GsrProcess>();
    let mut lock = state.0.lock().unwrap();
    match lock.as_mut() {
        Some((child, _)) => match child.try_wait() {
            Ok(None) => true,
            _        => { lock.take(); false }
        },
        None => false,
    }
}

/// Returns the title of the currently focused window (uses xdotool).
#[command]
pub fn get_active_window_title() -> String {
    std::process::Command::new("xdotool")
        .args(["getactivewindow", "getwindowname"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

// ═══ DSP: jalv-based LV2 filter chain ═══
//
// Architecture: one `jalv` subprocess per channel, hosted as a PipeWire JACK client.
// jalv reads port updates from stdin: "<port_index> <value>\n"
//
// LSP para_equalizer_x10_stereo control port layout (verify with `lv2info`):
//   0        bypass (0.0 = active, 1.0 = bypass)
//   1..14    filter type / freq / Q for each of the 10 bands
//   15..24   gain (dB) for bands 0–9  ← what we write on every apply_eq call
//
// TODO: wire jalv output to the corresponding virtual sink via `pw-link`.
// Currently the EQ runs in-process (jalv jack client auto-connects via WirePlumber).

const LSP_EQ_URI: &str = "http://lsp-plug.in/plugins/lv2/para_equalizer_x10_stereo";

/// Spawn a jalv LV2 host for the given channel's EQ.
/// The child's stdin is kept open so `apply_eq` can push port updates at any time.
#[command]
pub async fn start_eq_engine(app: AppHandle, channel: String) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let procs = app.state::<crate::JalvProcesses>();
    let mut map = procs.0.lock().unwrap();
    // Kill stale instance for this channel before spawning a fresh one.
    if let Some((mut child, _stdin)) = map.remove(&channel) {
        let _ = child.kill();
    }
    let jack_name = format!("opengg_eq_{}", channel.to_lowercase());
    let mut child = Command::new("jalv")
        .args(["-n", &jack_name, "-i", LSP_EQ_URI])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("jalv spawn failed: {e}. Install with: sudo apt install jalv"))?;
    let stdin = child.stdin.take().ok_or("no stdin on jalv child")?;
    map.insert(channel.clone(), (child, stdin));
    eprintln!("[opengg] start_eq_engine: jalv started for channel={channel}, jack_name={jack_name}");
    Ok(())
}

/// Kill the jalv EQ host for the given channel.
#[command]
pub async fn stop_eq_engine(app: AppHandle, channel: String) -> Result<(), String> {
    let procs = app.state::<crate::JalvProcesses>();
    let mut map = procs.0.lock().unwrap();
    if let Some((mut child, _stdin)) = map.remove(&channel) {
        let _ = child.kill();
        eprintln!("[opengg] stop_eq_engine: jalv stopped for channel={channel}");
    }
    Ok(())
}

/// Push 10-band EQ gain values to the running jalv instance for `channel`.
/// Each value is in dB, range -12.0..+12.0.
/// Port indices 15–24 = bands 0–9 in LSP para_equalizer_x10_stereo.
#[command]
pub async fn apply_eq(app: AppHandle, channel: String, bands: Vec<f32>) -> Result<(), String> {
    use std::io::Write;
    let procs = app.state::<crate::JalvProcesses>();
    let mut map = procs.0.lock().unwrap();
    let entry = map.get_mut(&channel).ok_or_else(|| {
        format!("No EQ engine running for channel '{channel}'. Call start_eq_engine first.")
    })?;
    // Write each band gain to the corresponding jalv control port.
    // Port index 15 = band 0, ..., port index 24 = band 9.
    const GAIN_PORT_BASE: usize = 15;
    for (i, &gain_db) in bands.iter().enumerate().take(10) {
        writeln!(entry.1, "{} {:.4}", GAIN_PORT_BASE + i, gain_db)
            .map_err(|e| format!("jalv stdin write failed: {e}"))?;
    }
    entry.1.flush().map_err(|e| format!("jalv stdin flush failed: {e}"))?;
    Ok(())
}

#[command]
pub async fn apply_noise_gate(_channel: String, _enabled: bool, _threshold: f32, _auto_detect: bool) -> Result<(), String> { Ok(()) }
#[command]
pub async fn apply_compressor(_channel: String, _enabled: bool, _level: f32) -> Result<(), String> { Ok(()) }
#[command]
pub async fn apply_noise_reduction(_channel: String, _enabled: bool, _intensity: f32) -> Result<(), String> { Ok(()) }

/// Spawn a transient overlay notification window.
/// `mode` controls which backend: "auto"|"x11-overlay"|"gsr-notify"|"system"|"disabled".
/// `position` is one of: "top-right"|"top-left"|"bottom-right"|"bottom-left".
/// `duration_secs` controls how long the window stays visible (1–30s, clamped).
/// `enabled` is passed from the frontend (reflects the `enableClipNotifications` setting).
#[command]
pub fn show_clip_notification(
    app: AppHandle,
    game: String,
    filename: String,
    filesize_mb: f64,
    success: bool,
    enabled: bool,
    mode: Option<String>,
    position: Option<String>,
    duration_secs: Option<u64>,
) -> Result<(), String> {
    if !enabled { return Ok(()); }
    let mode = mode.as_deref().unwrap_or("auto");
    if mode == "disabled" { return Ok(()); }

    // Clamp duration to [1, 30] seconds; default 4s.
    let duration_ms = duration_secs.unwrap_or(4).clamp(1, 30) * 1000;

    // ── Lightweight notification helpers ───────────────────────────────
    // Nested fns (not closures) to avoid borrow conflicts with `app`.
    fn notify_system(success: bool, game: &str, filename: &str, duration_ms: u64) {
        let summary = if success { "Clip Saved" } else { "Clip Save Failed" };
        let body = format!("{game} — {filename}");
        let _ = std::process::Command::new("notify-send")
            .args([
                "--app-name=OpenGG",
                "--urgency=normal",
                &format!("--expire-time={duration_ms}"),
                summary,
                &body,
            ])
            .spawn();
    }
    /// Returns true if the gsr-notify binary was found and launched.
    fn try_gsr_notify(success: bool, game: &str, filename: &str, filesize_mb: f64, duration_ms: u64) -> bool {
        let summary = if success { "Clip Saved" } else { "Clip Save Failed" };
        let body = format!("{game} — {filename} ({filesize_mb:.1} MB)");
        std::process::Command::new("gsr-notify")
            .args([
                "--app-name=OpenGG",
                &format!("--expire-time={duration_ms}"),
                summary,
                &body,
            ])
            .spawn()
            .is_ok()
    }

    let on_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();

    match mode {
        "system" => {
            notify_system(success, &game, &filename, duration_ms);
            return Ok(());
        }
        "gsr-notify" => {
            if !try_gsr_notify(success, &game, &filename, filesize_mb, duration_ms) {
                notify_system(success, &game, &filename, duration_ms);
            }
            return Ok(());
        }
        _ if on_wayland => {
            // "auto" or "x11-overlay" on Wayland: cannot spawn an X11 overlay window.
            // Try gsr-notify (works on XWayland), then fall back to notify-send.
            if !try_gsr_notify(success, &game, &filename, filesize_mb, duration_ms) {
                notify_system(success, &game, &filename, duration_ms);
            }
            return Ok(());
        }
        _ => {} // "x11-overlay" / "auto" on X11 → fall through to WebviewWindow
    }

    // Build the overlay URL served from the same Vite dev/prod server on localhost:1420
    // Use a stable unique label so multiple notifications can stack
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let label = format!("notif-{ts}");

    // URL-encode params manually to avoid adding a new crate dependency
    fn enc(s: &str) -> String {
        s.chars().flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => vec![c],
            c => c.encode_utf8(&mut [0u8; 4]).bytes()
                  .flat_map(|b| [b'%', hex_nibble(b >> 4), hex_nibble(b & 0xf)])
                  .map(|b| b as char)
                  .collect(),
        }).collect()
    }
    fn hex_nibble(n: u8) -> u8 { if n < 10 { b'0' + n } else { b'A' + n - 10 } }

    // Query string shared between dev and prod builds
    let query = format!(
        "?overlay=1&game={}&filename={}&filesize={}&success={}",
        enc(&game), enc(&filename),
        enc(&format!("{:.1}", filesize_mb)),
        if success { "1" } else { "0" },
    );

    // Dev: Vite serves at http://localhost:1420 (External URL required)
    // Prod: Tauri serves the bundled app at tauri://localhost (App URL required)
    // Using the wrong scheme causes a blank white window.
    #[cfg(debug_assertions)]
    let webview_url = tauri::WebviewUrl::External(
        format!("http://localhost:1420/{query}")
            .parse()
            .map_err(|e| format!("Bad overlay URL: {e}"))?,
    );
    #[cfg(not(debug_assertions))]
    let webview_url = tauri::WebviewUrl::App(std::path::PathBuf::from(&query));

    // ── Position: corner of the primary monitor based on `position` setting ──
    let notif_w = 380.0_f64;
    let notif_h = 120.0_f64;
    let margin  = 20.0_f64;
    let pos_str = position.as_deref().unwrap_or("top-right");
    let (x, y) = app
        .get_webview_window("main")
        .and_then(|w| w.primary_monitor().ok().flatten())
        .map(|m| {
            let scale = m.scale_factor();
            let sz    = m.size();
            let pos   = m.position();
            let lw = sz.width  as f64 / scale;
            let lh = sz.height as f64 / scale;
            let ox = pos.x as f64 / scale;
            let oy = pos.y as f64 / scale;
            match pos_str {
                "top-left"     => (ox + margin,                   oy + margin),
                "bottom-right" => (ox + lw - notif_w - margin,    oy + lh - notif_h - margin),
                "bottom-left"  => (ox + margin,                   oy + lh - notif_h - margin),
                _              => (ox + lw - notif_w - margin,    oy + margin), // top-right (default)
            }
        })
        .unwrap_or_else(|| match pos_str {
            "top-left"     => (margin, margin),
            "bottom-right" => (1920.0 - notif_w - margin, 1080.0 - notif_h - margin),
            "bottom-left"  => (margin, 1080.0 - notif_h - margin),
            _              => (1920.0 - notif_w - margin, margin),
        });

    let win = tauri::WebviewWindowBuilder::new(&app, label, webview_url)
    .always_on_top(true)
    .focused(false)
    .decorations(false)
    .skip_taskbar(true)
    .transparent(true)
    .resizable(false)
    .inner_size(notif_w, notif_h)
    .position(x, y)
    .build()
    .map_err(|e| format!("Overlay window failed: {e}"))?;

    // Pass all mouse events through — user can keep playing without interruption.
    let _ = win.set_ignore_cursor_events(true);

    // Auto-close after duration_ms.
    let win_for_thread = win.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(duration_ms));
        let _ = win_for_thread.close();
    });

    drop(win); // thread holds the only remaining clone
    Ok(())
}

// rng() removed — VU meters now use real PipeWire peak data
async fn call_dbus<R:serde::de::DeserializeOwned+zbus::zvariant::Type>(m:&str,p:&str,i:&str,a:impl serde::Serialize+zbus::zvariant::Type)->Result<R,String>{let c=zbus::Connection::session().await.map_err(|e|format!("{e}"))?;let r:R=c.call_method(Some("org.opengg.Daemon"),p,Some(i),m,&a).await.map_err(|e|format!("{m}:{e}"))?.body().deserialize().map_err(|e|format!("{m}:{e}"))?;Ok(r)}
async fn call_dbus_void(m: &str, p: &str, i: &str, a: impl serde::Serialize + zbus::zvariant::Type) -> Result<(), String> {
    let conn = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("call_dbus: session connection failed — {e}");
            return Err(format!("D-Bus session connect: {e}"));
        }
    };
    match conn.call_method(Some("org.opengg.Daemon"), p, Some(i), m, &a).await {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("call_dbus: method '{m}' on {p}.{i} failed — {e}");
            Err(format!("D-Bus method {m}: {e}"))
        }
    }
}
fn run_cmd(c:&str,a:&[&str])->Result<String,String>{let o=Command::new(c).args(a).output().map_err(|e|format!("{c}:{e}"))?;if o.status.success(){Ok(String::from_utf8_lossy(&o.stdout).trim().into())}else{Err(String::from_utf8_lossy(&o.stderr).trim().into())}}

// ═══ Devices ═══
#[command] pub async fn get_devices() -> Result<String, String> { call_dbus("GetDevices", DV_PATH, DV_IFACE, ()).await }
#[command] pub async fn set_mouse_dpi(device_id: String, dpi: u32) -> Result<(), String> { call_dbus_void("SetDpi", DV_PATH, DV_IFACE, (device_id.as_str(), dpi)).await }
#[command] pub async fn set_mouse_polling_rate(device_id: String, rate: u32) -> Result<(), String> { call_dbus_void("SetPollingRate", DV_PATH, DV_IFACE, (device_id.as_str(), rate)).await }
#[command] pub async fn set_headset_sidetone(device_id: String, level: u32) -> Result<(), String> { call_dbus_void("SetSidetone", DV_PATH, DV_IFACE, (device_id.as_str(), level)).await }
#[command] pub async fn set_headset_chatmix(device_id: String, level: u32) -> Result<(), String> { call_dbus_void("SetChatmix", DV_PATH, DV_IFACE, (device_id.as_str(), level)).await }
#[command] pub async fn set_headset_inactive_time(device_id: String, minutes: u32) -> Result<(), String> { call_dbus_void("SetInactiveTime", DV_PATH, DV_IFACE, (device_id.as_str(), minutes)).await }
#[command] pub async fn set_headset_mic_volume(device_id: String, level: u32) -> Result<(), String> { call_dbus_void("SetMicrophoneVolume", DV_PATH, DV_IFACE, (device_id.as_str(), level)).await }
#[command] pub async fn set_headset_mic_mute_led(device_id: String, brightness: u32) -> Result<(), String> { call_dbus_void("SetMicMuteLedBrightness", DV_PATH, DV_IFACE, (device_id.as_str(), brightness)).await }
#[command] pub async fn set_headset_volume_limiter(device_id: String, enabled: bool) -> Result<(), String> { call_dbus_void("SetVolumeLimiter", DV_PATH, DV_IFACE, (device_id.as_str(), enabled)).await }
#[command] pub async fn set_headset_bt_powered_on(device_id: String, enabled: bool) -> Result<(), String> { call_dbus_void("SetBtWhenPoweredOn", DV_PATH, DV_IFACE, (device_id.as_str(), enabled)).await }
#[command] pub async fn set_headset_bt_call_volume(device_id: String, level: u32) -> Result<(), String> { call_dbus_void("SetBtCallVolume", DV_PATH, DV_IFACE, (device_id.as_str(), level)).await }
#[command] pub async fn set_headset_eq_preset(device_id: String, preset_idx: u32) -> Result<(), String> { call_dbus_void("SetEqPreset", DV_PATH, DV_IFACE, (device_id.as_str(), preset_idx)).await }
#[command] pub async fn set_headset_eq_curve(device_id: String, bands_json: String) -> Result<(), String> { call_dbus_void("SetEqCurve", DV_PATH, DV_IFACE, (device_id.as_str(), bands_json.as_str())).await }

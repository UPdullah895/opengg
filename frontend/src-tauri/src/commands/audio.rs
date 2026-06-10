//! Audio commands and helpers for OpenGG.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::Ordering;
use tauri::{command, AppHandle, Emitter, Manager, State};
use super::{AU_PATH, AU_IFACE, call_dbus, call_dbus_void, run_cmd_async, run_cmd_sync};

// ═══ Audio ═══
#[command]
pub async fn get_channels() -> Result<String, String> {
    call_dbus("GetChannels", AU_PATH, AU_IFACE, ()).await
}
/// Volume control with pactl fallback — controls both sinks and sink-inputs
#[command]
pub async fn set_volume(channel: String, volume: u32) -> Result<(), String> {
    // Try D-Bus first
    if call_dbus_void("SetVolume", AU_PATH, AU_IFACE, (channel.as_str(), volume))
        .await
        .is_ok()
    {
        return Ok(());
    }
    // Direct pactl fallback
    let pct = format!("{volume}%");
    let sink = format!("OpenGG_{channel}");
    run_cmd_async("pactl", &["set-sink-volume", &sink, &pct]).await?;
    Ok(())
}

#[command]
pub async fn set_mute(channel: String, muted: bool) -> Result<(), String> {
    if call_dbus_void("SetMute", AU_PATH, AU_IFACE, (channel.as_str(), muted))
        .await
        .is_ok()
    {
        return Ok(());
    }
    let val = if muted { "1" } else { "0" };
    run_cmd_async("pactl", &["set-sink-mute", &format!("OpenGG_{channel}"), val]).await?;
    Ok(())
}

/// Unmute any WebKit video/webaudio sink-inputs belonging to this app.
/// Called whenever clip playback starts to counteract module-stream-restore
/// auto-muting the media.role="video" stream.
#[command]
pub async fn unmute_media_streams() -> Result<(), String> {
    let output = run_cmd_async("pactl", &["list", "sink-inputs"]).await?;

    let mut current_id: Option<u32> = None;
    let mut is_opengg = false;
    let mut is_media = false;

    for line in output.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Sink Input #") {
            if let Some(id) = current_id {
                if is_opengg && is_media {
                    let _ = run_cmd_async("pactl", &["set-sink-input-mute", &id.to_string(), "0"]).await;
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
            let _ = run_cmd_async("pactl", &["set-sink-input-mute", &id.to_string(), "0"]).await;
        }
    }
    Ok(())
}

/// Set volume for an individual app (sink-input) by its pactl index
#[command]
pub async fn set_app_volume(app_index: u32, volume: u32) -> Result<(), String> {
    let pct = format!("{volume}%");
    run_cmd_async(
        "pactl",
        &["set-sink-input-volume", &app_index.to_string(), &pct],
    ).await?;
    Ok(())
}
/// System-critical processes that must never be routed. They appear in the
/// app list as "locked" so the user knows they cannot be moved.
const ROUTING_BLACKLIST: &[&str] = &[
    "plasmashell", "kwin_wayland", "kwin_x11", "swaync", "sway",
    "xdg-desktop-portal", "xdg-desktop-portal-gnome", "xdg-desktop-portal-kde",
];

fn is_blacklisted_binary(binary: &str) -> bool {
    let b = binary.to_lowercase();
    ROUTING_BLACKLIST.iter().any(|bl| b == *bl)
}

#[command]
pub async fn get_apps() -> Result<String, String> {
    match call_dbus::<String>("GetApps", AU_PATH, AU_IFACE, ()).await {
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
        Err(_) => tokio::task::spawn_blocking(scan_sink_inputs)
            .await
            .map_err(|e| format!("spawn_blocking: {e}"))?,
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
struct Router {
    app: AppHandle,
    app_id: u32,
    channel: String,
}

impl Router {
    async fn route(self) -> Result<(), String> {
        // Strategy 0: D-Bus daemon
        if let Ok(()) = self.try_dbus().await {
            return Ok(());
        }

        log_routing_context(self.app_id, &self.channel);

        // Resolve target sink once
        let sink_name = self.resolve_sink_name().await?;
        let sink_name_clone = sink_name.clone();
        let sink_idx = tokio::task::spawn_blocking(move || get_sink_index_by_name(&sink_name_clone))
            .await
            .map_err(|e| format!("spawn_blocking: {e}"))??;

        // Strategy 1: direct pactl index
        if let Ok(()) = self.try_direct_pactl(sink_idx).await {
            return Ok(());
        }

        // Strategy 2: PW-ID cross-reference
        if let Ok(()) = self.try_pw_id_resolve(sink_idx).await {
            return Ok(());
        }

        // Strategy 3: pw-metadata + verification
        self.try_pw_metadata(&sink_name, sink_idx).await
    }

    async fn try_dbus(&self) -> Result<(), String> {
        call_dbus_void("RouteApp", AU_PATH, AU_IFACE, (self.app_id, self.channel.as_str()))
            .await
            .map(|()| {
                let _ = self.app.emit("audio-mixer-refresh", ());
            })
            .map_err(|e| {
                eprintln!("route_app: D-Bus route failed ({e}), falling back to pactl");
                e
            })
    }

    async fn resolve_sink_name(&self) -> Result<String, String> {
        if self.channel == "default" || self.channel == "Master" {
            run_cmd_async("pactl", &["get-default-sink"]).await
        } else {
            let name = format!("OpenGG_{}", self.channel);
            ensure_sink_exists(&name, &self.channel).await?;
            Ok(name)
        }
    }

    async fn try_direct_pactl(&self, sink_idx: u32) -> Result<(), String> {
        if !validate_sink_input_exists(self.app_id) {
            eprintln!(
                "route_app: app_id {} not a pactl sink-input index — skipping direct attempt",
                self.app_id
            );
            return Err("not a direct index".into());
        }

        let cmd_str = format!("pactl move-sink-input {} {}", self.app_id, sink_idx);
        for attempt in 0..=2 {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            eprintln!("route_app: [attempt {}/3] executing: {cmd_str}", attempt + 1);

            match run_cmd_async("pactl", &[
                "move-sink-input",
                &self.app_id.to_string(),
                &sink_idx.to_string(),
            ]).await {
                Ok(_) => {
                    eprintln!(
                        "route_app: ✓ attempt {}/3: sink-input {} → sink #{sink_idx} ({})",
                        attempt + 1,
                        self.app_id,
                        self.channel
                    );
                    let _ = self.app.emit("audio-mixer-refresh", ());
                    return Ok(());
                }
                Err(err) => {
                    eprintln!("route_app: attempt {}/3 failed ({err})", attempt + 1);
                }
            }
        }

        eprintln!("route_app: all 3 direct attempts exhausted, scanning for correct sink-input...");
        Err("direct attempts exhausted".into())
    }

    async fn try_pw_id_resolve(&self, sink_idx: u32) -> Result<(), String> {
        let si_idx = match find_pactl_si_for_pw_id(self.app_id) {
            Ok(idx) => idx,
            Err(_) => {
                eprintln!(
                    "route_app: no pactl sink-input found for PW#{}, trying pw-metadata...",
                    self.app_id
                );
                return Err("PW ID not found".into());
            }
        };

        let cmd = format!("pactl move-sink-input {} {}", si_idx, sink_idx);
        eprintln!("route_app: found PW#{} → pactl si#{}, executing: {cmd}", self.app_id, si_idx);

        match run_cmd_async("pactl", &[
            "move-sink-input",
            &si_idx.to_string(),
            &sink_idx.to_string(),
        ]).await {
            Ok(_) => {
                eprintln!(
                    "route_app: ✓ PW#{} → pactl si#{si_idx} → sink #{sink_idx} ({})",
                    self.app_id,
                    self.channel
                );
                let _ = self.app.emit("audio-mixer-refresh", ());
                return Ok(());
            }
            Err(err) => {
                eprintln!("route_app: corrected pactl index also failed ({err}), trying pw-metadata...");
            }
        }
        Err("corrected index failed".into())
    }

    async fn try_pw_metadata(&self, sink_name: &str, sink_idx: u32) -> Result<(), String> {
        let sink_pw_id = match get_pw_node_id_for_sink(sink_name) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("route_app: cannot resolve sink '{sink_name}' PW node.id: {e}");
                return Err(format!(
                    "route_app: all routing methods failed for id {} → {}. \
                     Sink PW node.id unavailable: {e}",
                    self.app_id, self.channel
                ));
            }
        };

        eprintln!(
            "route_app: [pw-metadata] node#{} → target.node={sink_pw_id} (sink '{sink_name}', channel '{}')",
            self.app_id, self.channel
        );

        if let Err(e) = route_via_pw_metadata(self.app_id, sink_pw_id) {
            eprintln!("route_app: pw-metadata failed: {e}");
            return Err(format!(
                "route_app: all routing methods failed for id {} → {}. \
                 See logs above for diagnostics.",
                self.app_id, self.channel
            ));
        }

        eprintln!("route_app: ✓ pw-metadata write succeeded for {} → {}", self.app_id, self.channel);

        // Verification: pw-metadata can report success without actually moving the stream
        // if the stream isn't WirePlumber-managed.
        if !verify_stream_routed(self.app_id, sink_idx) {
            eprintln!("route_app: pw-metadata reported success but stream is not on target sink — marking as failed");
            return Err(format!(
                "route_app: routing to {} appeared to succeed via pw-metadata \
                 but verification showed the stream is still not on the target sink. \
                 app_id={}, sink_idx={sink_idx}",
                self.channel, self.app_id
            ));
        }

        let _ = self.app.emit("audio-mixer-refresh", ());
        Ok(())
    }
}

#[command]
pub async fn route_app(
    app: tauri::AppHandle,
    state: State<'_, crate::RouteState>,
    app_id: u32,
    channel: String,
    binary: String,
) -> Result<(), String> {
    // ── Guard 1: Blacklist ──
    if state.is_blacklisted(&binary) {
        return Err(format!(
            "route_app: PID {} ({}) is blacklisted — system processes cannot be routed",
            app_id, binary
        ));
    }

    // ── Guard 2: Already routed to same channel ──
    if state.is_already_routed(app_id, &channel) {
        return Ok(());
    }

    // ── Guard 3: Cooldown (even on failure, don't retry) ──
    if state.is_on_cooldown(app_id) {
        return Err(format!(
            "route_app: PID {} is on cooldown ({}s)",
            app_id, crate::ROUTE_COOLDOWN_SECS
        ));
    }

    // ── Guard 4: Circuit breaker (too many recent failures) ──
    if state.is_circuit_open(app_id) {
        return Err(format!(
            "route_app: PID {} circuit breaker open (too many failures)",
            app_id
        ));
    }

    // Record attempt BEFORE execution to prevent concurrent duplicate calls
    state.record_attempt(app_id);

    // ── Single execution block: exit on first success ──
    let result = Router {
        app: app.clone(),
        app_id,
        channel: channel.clone(),
    }
    .route()
    .await;

    match result {
        Ok(()) => {
            state.record_success(app_id, channel);
            Ok(())
        }
        Err(ref e) => {
            let circuit_open = state.record_failure(app_id);
            if circuit_open {
                eprintln!(
                    "route_app: PID {} circuit breaker OPEN after {} failures in {}s",
                    app_id,
                    crate::FAIL_THRESHOLD,
                    crate::FAIL_WINDOW_SECS
                );
            }
            Err(e.clone())
        }
    }
}

/// Get default sink's pactl integer index
#[allow(dead_code)]
fn get_default_sink_index() -> Result<u32, String> {
    let name = run_cmd_sync("pactl", &["get-default-sink"])?;
    get_sink_index_by_name(&name)
}

/// Look up sink's pactl integer index by name — mirrors pulsectl.sink_list()
fn get_sink_index_by_name(sink_name: &str) -> Result<u32, String> {
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
    let j = run_cmd_sync("pactl", &["-f", "json", "list", "sink-inputs"])?;
    let sis: Vec<serde_json::Value> =
        serde_json::from_str(&j).map_err(|e| format!("parse sink-inputs: {e}"))?;
    let mut map = HashMap::new();
    for si in sis {
        let idx = si["index"].as_u64().unwrap_or(0) as u32;
        let p = &si["properties"];
        let app_name = p["application.name"]
            .as_str()
            .or(p["media.name"].as_str())
            .unwrap_or("")
            .to_string();
        let binary = p["application.process.binary"]
            .as_str()
            .unwrap_or("")
            .to_string();

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
        // Some clients expose the node ID embedded in media.name or application.name
        if let Some(s) = p["media.name"].as_str() {
            if let Ok(id) = s.parse::<u32>() {
                pw_ids.push(id);
            }
        }

        map.insert(
            idx,
            SiInfo {
                idx,
                app_name,
                binary,
                pw_ids,
            },
        );
    }
    Ok(map)
}

/// Check whether a sink-input index currently exists in the system.
fn validate_sink_input_exists(si_idx: u32) -> bool {
    if let Ok(j) = run_cmd_sync("pactl", &["-f", "json", "list", "sink-inputs"]) {
        if let Ok(sis) = serde_json::from_str::<Vec<serde_json::Value>>(&j) {
            return sis
                .iter()
                .any(|si| si["index"].as_u64() == Some(si_idx as u64));
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
    let j = run_cmd_sync("pactl", &["-f", "json", "list", "sinks"])?;
    let sinks: Vec<serde_json::Value> =
        serde_json::from_str(&j).map_err(|e| format!("parse sinks: {e}"))?;
    for s in &sinks {
        if s["name"].as_str() == Some(sink_name) {
            // WirePlumber surfaces the PW node.id as a string property
            if let Some(nid) = s["properties"]["node.id"]
                .as_str()
                .and_then(|v| v.parse::<u32>().ok())
            {
                return Ok(nid);
            }
            // Fallback: object.id is the same value on older PipeWire builds
            if let Some(nid) = s["properties"]["object.id"]
                .as_str()
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
            "-n",
            "settings",
            &stream_pw_id.to_string(),
            "target.node",
            &sink_pw_id.to_string(),
        ])
        .output()
        .map_err(|e| format!("pw-metadata exec error: {e}"))?;
    if r.status.success() {
        return Ok(());
    }
    Err(format!(
        "pw-metadata: {}",
        String::from_utf8_lossy(&r.stderr).trim()
    ))
}

/// Log full environment context and PipeWire state before routing attempts.
fn log_routing_context(app_id: u32, channel: &str) {
    eprintln!(
        "=== route_app[{} → {channel}] environment context ===",
        app_id
    );
    eprintln!(
        "  PULSE_SERVER:        {:?}",
        std::env::var("PULSE_SERVER").ok()
    );
    eprintln!(
        "  PIPEWIRE_DEBUG:      {:?}",
        std::env::var("PIPEWIRE_DEBUG").ok()
    );
    eprintln!(
        "  XDG_SESSION_TYPE:    {:?}",
        std::env::var("XDG_SESSION_TYPE").ok()
    );
    eprintln!(
        "  WAYLAND_DISPLAY:     {:?}",
        std::env::var("WAYLAND_DISPLAY").ok()
    );
    eprintln!("  DISPLAY:             {:?}", std::env::var("DISPLAY").ok());
    eprintln!(
        "  XDG_CURRENT_DESKTOP: {:?}",
        std::env::var("XDG_CURRENT_DESKTOP").ok()
    );

    // Resolve stream identity: find node.name and binary for app_id in the SI map
    match build_si_map() {
        Ok(map) => {
            let matched: Vec<_> = map
                .values()
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
        run_cmd_sync("pactl", &["get-default-sink"]).unwrap_or_default()
    } else {
        format!("OpenGG_{channel}")
    };
    match get_pw_node_id_for_sink(&sink_name) {
        Ok(nid) => eprintln!("  target sink '{}' → PW node.id={}", sink_name, nid),
        Err(e) => eprintln!(
            "  target sink '{}' → PW node.id unavailable: {}",
            sink_name, e
        ),
    }

    if let Ok(j) = run_cmd_sync("pactl", &["-f", "json", "list", "sinks"]) {
        let count = serde_json::from_str::<Vec<serde_json::Value>>(&j)
            .map(|v| v.len())
            .unwrap_or(0);
        eprintln!("  pactl sinks visible: {count}");
    }

    eprintln!("====================================================");
}

/// Ensure virtual sink exists (non-blocking, with extended settling time)
async fn ensure_sink_exists(name: &str, ch: &str) -> Result<(), String> {
    if let Ok(o) = Command::new("pactl")
        .args(["list", "sinks", "short"])
        .output()
    {
        if String::from_utf8_lossy(&o.stdout).contains(name) {
            return Ok(());
        }
    }
    let display_name = live_display_name(ch);
    eprintln!("ensure_sink_exists: creating virtual sink '{name}' for channel '{ch}'");
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
        let err = String::from_utf8_lossy(&c.stderr);
        eprintln!("ensure_sink_exists: pactl load-module FAILED: {err}");
        return Err(err.to_string());
    }
    // Increase settling time: 600ms to allow PipeWire to fully enumerate the new sink
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    // Set up loopback links to default sink (idempotent)
    if let Ok(def) = run_cmd_sync("pactl", &["get-default-sink"]) {
        for p in ["FL", "FR"] {
            let current = get_linked_device_for_monitor(name, p);
            if !current.is_empty() {
                // Already linked to something — skip to avoid duplicates/noise
                continue;
            }
            let result = Command::new("pw-link")
                .args([
                    &format!("{name}:monitor_{p}"),
                    &format!("{def}:playback_{p}"),
                ])
                .output();
            if let Err(e) = result {
                eprintln!(
                    "ensure_sink_exists: pw-link {name}:monitor_{p} → {def}:playback_{p}: {e}"
                );
            }
        }
    }
    eprintln!("ensure_sink_exists: sink '{name}' ready");
    Ok(())
}

fn live_display_name(channel: &str) -> String {
    if matches!(channel, "Game" | "Chat" | "Media" | "Aux") {
        channel.to_string()
    } else {
        format!("OpenGG - {channel}")
    }
}

fn sink_prop_value(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

#[allow(dead_code)]
fn unload_null_sink_module(sink_name: &str) -> Result<(), String> {
    let modules = run_cmd_sync("pactl", &["list", "short", "modules"])?;
    for line in modules.lines() {
        let mut parts = line.split('\t');
        let Some(idx) = parts.next() else { continue };
        let Some(name) = parts.next() else { continue };
        let args = parts.next().unwrap_or("");
        if name != "module-null-sink" || !args.contains(&format!("sink_name={sink_name}")) {
            continue;
        }
        run_cmd_sync("pactl", &["unload-module", idx])?;
    }
    for _ in 0..20 {
        let modules = run_cmd_sync("pactl", &["list", "short", "modules"])?;
        let modules_cleared = modules.lines().all(|line| {
            let mut parts = line.split('\t');
            let _idx = parts.next();
            let name = parts.next().unwrap_or("");
            let args = parts.next().unwrap_or("");
            name != "module-null-sink" || !args.contains(&format!("sink_name={sink_name}"))
        });
        let sinks = run_cmd_sync("pactl", &["list", "sinks", "short"])?;
        if modules_cleared && !sinks.contains(sink_name) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(())
}

/// Scan sink-inputs — mirrors pulsectl.sink_input_list()
/// ★ FIX 2: Filters out streams going to OpenGG virtual sinks' monitors
fn scan_sink_inputs() -> Result<String, String> {
    let j = run_cmd_sync("pactl", &["-f", "json", "list", "sink-inputs"])?;
    let sis: Vec<serde_json::Value> = serde_json::from_str(&j).map_err(|e| format!("{e}"))?;
    let mut apps = Vec::new();
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
        let auto_channel = classify_channel(&p);

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

/// Verify that a stream (identified by PW node ID `stream_pw_id`) is currently on the
/// target sink (identified by pactl integer index `sink_idx`).
///
/// This prevents the "pw-metadata reports success but stream isn't actually moved" bug
/// which causes the frontend polling loop to re-trigger routing every 2 seconds forever.
///
/// Returns `true` if the stream is on the target sink, `false` otherwise.
fn verify_stream_routed(stream_pw_id: u32, sink_idx: u32) -> bool {
    let j = match run_cmd_sync("pactl", &["-f", "json", "list", "sink-inputs"]) {
        Ok(j) => j,
        Err(_) => return false,
    };
    let sis: Vec<serde_json::Value> = match serde_json::from_str(&j) {
        Ok(sis) => sis,
        Err(_) => return false,
    };
    for si in &sis {
        let _idx = match si["index"].as_u64() {
            Some(i) => i as u32,
            None => continue,
        };
        let sink_of_si = match si["sink"].as_u64() {
            Some(s) => s as u32,
            None => continue,
        };
        if sink_of_si != sink_idx {
            continue;
        }
        let p = &si["properties"];
        for key in &[
            "object.serial",
            "object.id",
            "node.id",
            "pipewire.access.portal.app_id",
        ] {
            if let Some(s) = p[key].as_str() {
                if let Ok(id) = s.parse::<u32>() {
                    if id == stream_pw_id {
                        return true;
                    }
                }
            }
        }
    }
    false
}
// ═══ Audio Devices ═══
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AudioDevice {
    pub name: String,
    pub description: String,
    pub device_type: String,
    pub is_default: bool,
}
#[command]
pub async fn get_audio_devices() -> Result<Vec<AudioDevice>, String> {
    let mut d = Vec::new();
    let ds = run_cmd_async("pactl", &["get-default-sink"]).await.unwrap_or_default();
    let dr = run_cmd_async("pactl", &["get-default-source"]).await.unwrap_or_default();
    if let Ok(o) = run_cmd_async("pactl", &["-f", "json", "list", "sinks"]).await {
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
    if let Ok(o) = run_cmd_async("pactl", &["-f", "json", "list", "sources"]).await {
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
fn get_linked_device_for_monitor(sink_name: &str, port: &str) -> String {
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
#[allow(dead_code)]
fn unlink_virtual_sink_from_all(sink_name: &str, preserve_device: Option<&str>) {
    let json = match run_cmd_sync("pactl", &["-f", "json", "list", "sinks"]) {
        Ok(j) => j,
        Err(_) => return,
    };
    let sinks: Vec<serde_json::Value> = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => return,
    };
    let mut unlinked = 0;
    for s in &sinks {
        if let Some(target) = s["name"].as_str() {
            if preserve_device == Some(target) {
                continue;
            }
            for p in ["FL", "FR"] {
                // pw-link -d silently exits 0 when the link doesn't exist
                Command::new("pw-link")
                    .args([
                        "-d",
                        &format!("{sink_name}:monitor_{p}"),
                        &format!("{target}:playback_{p}"),
                    ])
                    .output()
                    .ok();
                unlinked += 1;
            }
        }
    }
    eprintln!(
        "set_channel_device: unlinked {sink_name} from {} sink(s)",
        unlinked / 2
    );
}

#[command]
pub async fn set_channel_device(channel: String, device_name: String) -> Result<(), String> {
    if channel == "Mic" {
        let current = run_cmd_async("pactl", &["get-default-source"])
            .await
            .unwrap_or_default();
        if current.trim() != device_name {
            run_cmd_async("pactl", &["set-default-source", &device_name]).await?;
            eprintln!("set_channel_device: Mic source → {device_name}");
        }
        return Ok(());
    }
    if channel == "Master" {
        let current = run_cmd_async("pactl", &["get-default-sink"])
            .await
            .unwrap_or_default();
        if current.trim() != device_name {
            run_cmd_async("pactl", &["set-default-sink", &device_name]).await?;
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
            let _ = run_cmd_async(
                "pw-link",
                &[
                    "-d",
                    &format!("{sink}:monitor_{p}"),
                    &format!("{current}:playback_{p}"),
                ],
            )
            .await;
        }
        // Create the new link.
        match run_cmd_async(
            "pw-link",
            &[
                &format!("{sink}:monitor_{p}"),
                &format!("{device_name}:playback_{p}"),
            ],
        )
        .await
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
#[derive(Serialize, Clone)]
struct VuLevels {
    channels: Vec<(String, f32)>,
}

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
    let gen = st.1.clone();
    let handle = app.clone();

    // Resolve default sink/source once — not inside the hot path.
    let master_monitor = run_cmd_async("pactl", &["get-default-sink"]).await
        .map(|s| format!("{s}.monitor"))
        .unwrap_or_default();
    let mic_source = run_cmd_async("pactl", &["get-default-source"]).await.unwrap_or_default();

    // Guard: only connect to sources that currently exist.
    let known_sources: std::collections::HashSet<String> = {
        let mut set = std::collections::HashSet::new();
        if let Ok(json) = run_cmd_async("pactl", &["-f", "json", "list", "sources"]).await {
            if let Ok(sources) = serde_json::from_str::<Vec<serde_json::Value>>(&json) {
                for s in &sources {
                    if let Some(name) = s["name"].as_str() {
                        set.insert(name.to_string());
                    }
                }
            }
        }
        set
    };
    eprintln!(
        "start_vu_stream: {} PA sources: {:?}",
        known_sources.len(),
        known_sources
    );

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(String, f32)>();

    let channel_targets: Vec<(&'static str, String)> = vec![
        ("Master", master_monitor),
        ("Game", "OpenGG_Game.monitor".into()),
        ("Chat", "OpenGG_Chat.monitor".into()),
        ("Media", "OpenGG_Media.monitor".into()),
        ("Aux", "OpenGG_Aux.monitor".into()),
        ("Mic", mic_source),
    ];

    let spec = Spec {
        format: Format::S16le,
        rate: 8000,
        channels: 1,
    };

    for (name, target) in channel_targets {
        if target.is_empty() || !known_sources.contains(target.as_str()) {
            eprintln!("start_vu_stream: {name} → '{target}' not in PA sources, level=0");
            let _ = tx.send((name.to_string(), 0.0));
            continue;
        }

        let tx_clone = tx.clone();
        let running_clone = running.clone();
        let gen_clone = gen.clone();

        tokio::task::spawn_blocking(move || {
            // Create the PA simple connection inside spawn_blocking — Simple is !Send.
            let pa = match Simple::new(
                None,
                "OpenGG VU",
                Direction::Record,
                Some(target.as_str()),
                name,
                &spec,
                None,
                None,
            ) {
                Ok(p) => p,
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
                if pa.read(&mut buf).is_err() {
                    break;
                }

                // RMS for perceptual loudness (vs peak which looks jittery).
                let sum_sq: f32 = buf
                    .chunks_exact(2)
                    .map(|c| {
                        let s = i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0;
                        s * s
                    })
                    .sum();
                let rms = (sum_sq / (buf.len() / 2) as f32).sqrt().min(1.0);

                // Fast attack, slow decay for natural VU ballistics.
                let smoothed = if rms > prev {
                    rms * 0.9 + prev * 0.1
                } else {
                    rms * 0.3 + prev * 0.7
                };
                prev = smoothed;
                let db = (20.0_f32 * smoothed.max(1e-9_f32).log10()).max(-60.0_f32);
                let _ = tx_clone.send((name.to_string(), db));
            }
            // `pa` is dropped here → pa_simple_free() called automatically (RAII).
        });
    }

    // Emitter: drains channel updates and publishes at ~30 fps.
    // 32 ms is perceptually identical to 16 ms for audio meters while halving
    // serialization + IPC + frontend reactivity overhead.
    tokio::spawn(async move {
        let mut levels_vec: Vec<(String, f32)> = Vec::with_capacity(6);
        while running.load(Ordering::Relaxed) && gen.load(Ordering::Relaxed) == my_gen {
            // Drain all pending reader updates into a small fixed-capacity Vec.
            while let Ok((ch, db)) = rx.try_recv() {
                if let Some(entry) = levels_vec.iter_mut().find(|(name, _)| name == &ch) {
                    entry.1 = db;
                } else {
                    levels_vec.push((ch, db));
                }
            }
            // ★ Ear blast protection: check each channel's level
            for (ch, db) in &levels_vec {
                let _ = check_ear_blast(&handle, ch, *db).await;
            }
            let _ = handle.emit("vu-levels", VuLevels { channels: levels_vec.clone() });
            tokio::time::sleep(std::time::Duration::from_millis(32)).await;
        }
    });

    Ok(())
}

/// Stops the VU stream. Reader threads exit cooperatively within one read
/// period (~32 ms) and free their PA connections via RAII.
#[command]
pub async fn stop_vu_stream(app: AppHandle) -> Result<(), String> {
    app.state::<crate::VuState>()
        .0
        .store(false, Ordering::Relaxed);
    Ok(())
}

// ══════════════════════════════════════════════════════════════
//  ★ Ear Blast Protection
// ══════════════════════════════════════════════════════════════

/// Query the current volume percentage (0-150) of a sink by name.
fn get_sink_volume_percent(sink_name: &str) -> Option<u32> {
    // `pactl list sinks short` has only 5 tab-separated fields (index/name/driver/spec/state),
    // not 6 — use `get-sink-volume` which outputs "... / 100% ..." directly.
    let out = run_cmd_sync("pactl", &["get-sink-volume", sink_name]).ok()?;
    // "Volume: front-left: 65536 / 100% / 0.00 dB,   front-right: ..."
    let pct_str = out.split('%').next()?.split_whitespace().last()?;
    pct_str.parse::<u32>().ok().map(|v| v.min(150))
}

/// Query the current volume percentage (0-150) of a source by name.
fn get_source_volume_percent(source_name: &str) -> Option<u32> {
    let out = run_cmd_sync("pactl", &["get-source-volume", source_name]).ok()?;
    let pct_str = out.split('%').next()?.split_whitespace().last()?;
    pct_str.parse::<u32>().ok().map(|v| v.min(150))
}

/// Resolve the PulseAudio object name for a channel.
fn pa_object_name_for_channel(channel: &str) -> Option<String> {
    match channel {
        "Master" => run_cmd_sync("pactl", &["get-default-sink"]).ok(),
        "Mic" => run_cmd_sync("pactl", &["get-default-source"]).ok(),
        _ => Some(format!("OpenGG_{channel}")),
    }
}

/// Volume-limiting check called from the VU emitter task (~30 fps).
/// Prevents oscillation via dynamic release margin.
async fn check_ear_blast(app: &AppHandle, channel: &str, db: f32) {
    let state = app.state::<crate::EarBlastState>();

    if !state.enabled.load(Ordering::Relaxed) {
        return;
    }

    {
        let channels = state.channels.lock().unwrap();
        if !channels.contains(channel) {
            return;
        }
    }

    let threshold_pct = state.threshold_percent.load(Ordering::Relaxed).max(1) as f32;
    let target_pct = state.target_percent.load(Ordering::Relaxed).min(100) as f32;
    let threshold_db = 20.0 * (threshold_pct / 100.0).max(1e-9).log10();

    // Dynamic margin: must exceed the volume-reduction delta + 3 dB buffer
    let reduction_db: f32 = if target_pct > 0.0 {
        20.0 * (target_pct / 100.0).max(1e-9).log10()
    } else {
        -60.0
    };
    let margin_db = reduction_db.abs() + 3.0;
    let release_db = threshold_db - margin_db;

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    // ── Phase 1: read & mutate state (synchronous, no await) ──
    enum EarBlastAction {
        Activate { sink_name: String, target: u32, _orig_vol: u32 },
        Deactivate { sink_name: String, orig_vol: u32, skip_restore: bool },
        Nothing,
    }

    let action = {
        let mut active_map = state.active.lock().unwrap();
        let mut orig_map = state.original_volumes.lock().unwrap();
        let mut trigger_map = state.last_trigger_ms.lock().unwrap();

        let is_active = *active_map.get(channel).unwrap_or(&false);
        let last_trigger = *trigger_map.get(channel).unwrap_or(&0);
        let hold_elapsed = now_ms.saturating_sub(last_trigger) >= 500;

        if !is_active && db > threshold_db {
            if let Some(sink_name) = pa_object_name_for_channel(channel) {
                let current_vol = if channel == "Mic" {
                    get_source_volume_percent(&sink_name)
                } else {
                    get_sink_volume_percent(&sink_name)
                };
                if let Some(vol) = current_vol {
                    if vol > target_pct as u32 {
                        orig_map.insert(channel.to_string(), vol);
                        active_map.insert(channel.to_string(), true);
                        trigger_map.insert(channel.to_string(), now_ms);
                        EarBlastAction::Activate {
                            sink_name,
                            target: target_pct.round() as u32,
                            _orig_vol: vol,
                        }
                    } else {
                        EarBlastAction::Nothing
                    }
                } else {
                    EarBlastAction::Nothing
                }
            } else {
                EarBlastAction::Nothing
            }
        } else if is_active && db < release_db && hold_elapsed {
            if let Some(sink_name) = pa_object_name_for_channel(channel) {
                if let Some(orig_vol) = orig_map.remove(channel) {
                    let current_vol = if channel == "Mic" {
                        get_source_volume_percent(&sink_name)
                    } else {
                        get_sink_volume_percent(&sink_name)
                    };
                    let skip_restore = if let Some(cur) = current_vol {
                        let diff = if cur > target_pct as u32 {
                            cur - target_pct as u32
                        } else {
                            target_pct as u32 - cur
                        };
                        diff > 5 // user manually changed volume
                    } else {
                        false
                    };
                    active_map.insert(channel.to_string(), false);
                    EarBlastAction::Deactivate {
                        sink_name,
                        orig_vol,
                        skip_restore,
                    }
                } else {
                    active_map.insert(channel.to_string(), false);
                    EarBlastAction::Nothing
                }
            } else {
                EarBlastAction::Nothing
            }
        } else {
            EarBlastAction::Nothing
        }
    };

    // ── Phase 2: async I/O (all MutexGuards dropped) ──
    match action {
        EarBlastAction::Activate { sink_name, target, .. } => {
            let pactl_target = format!("{}%", target);
            if channel == "Mic" {
                let _ = run_cmd_async("pactl", &["set-source-volume", &sink_name, &pactl_target]).await;
            } else {
                let _ = run_cmd_async("pactl", &["set-sink-volume", &sink_name, &pactl_target]).await;
            }
            let _ = app.emit(
                "ear-blast-state",
                serde_json::json!({ "channel": channel, "active": true }),
            );
            eprintln!(
                "ear_blast: activated on {channel} (level={db:.1} dB > threshold={threshold_db:.1} dB) → {target}%"
            );
        }
        EarBlastAction::Deactivate {
            sink_name,
            orig_vol,
            skip_restore,
        } => {
            if !skip_restore {
                let pactl_vol = format!("{}%", orig_vol);
                if channel == "Mic" {
                    let _ = run_cmd_async("pactl", &["set-source-volume", &sink_name, &pactl_vol]).await;
                } else {
                    let _ = run_cmd_async("pactl", &["set-sink-volume", &sink_name, &pactl_vol]).await;
                }
            }
            let _ = app.emit(
                "ear-blast-state",
                serde_json::json!({ "channel": channel, "active": false }),
            );
            eprintln!(
                "ear_blast: deactivated on {channel} (level={db:.1} dB < release={release_db:.1} dB) → restored {orig_vol}%"
            );
        }
        EarBlastAction::Nothing => {}
    }
}

#[command]
pub async fn set_ear_blast_enabled(
    enabled: bool,
    state: State<'_, crate::EarBlastState>,
) -> Result<(), String> {
    let was_enabled = state.enabled.load(Ordering::Relaxed);
    state.enabled.store(enabled, Ordering::Relaxed);

    if was_enabled && !enabled {
        // Restore all active channels — collect data first, then await
        let restores: Vec<(String, u32)> = {
            let mut orig_map = state.original_volumes.lock().unwrap();
            let mut active_map = state.active.lock().unwrap();
            let active_channels: Vec<String> = active_map
                .iter()
                .filter(|(_, v)| **v)
                .map(|(k, _)| k.clone())
                .collect();
            let mut result = Vec::new();
            for ch in active_channels {
                if let Some(orig_vol) = orig_map.remove(&ch) {
                    result.push((ch.clone(), orig_vol));
                    active_map.insert(ch, false);
                }
            }
            result
        };
        for (ch, orig_vol) in restores {
            if let Some(name) = pa_object_name_for_channel(&ch) {
                let vol_str = format!("{}%", orig_vol);
                if ch == "Mic" {
                    let _ = run_cmd_async("pactl", &["set-source-volume", &name, &vol_str]).await;
                } else {
                    let _ = run_cmd_async("pactl", &["set-sink-volume", &name, &vol_str]).await;
                }
            }
        }
    }
    Ok(())
}

#[command]
pub fn set_ear_blast_channels(
    channels: Vec<String>,
    state: State<'_, crate::EarBlastState>,
) {
    let mut chs = state.channels.lock().unwrap();
    chs.clear();
    for c in channels {
        chs.insert(c);
    }
}

#[command]
pub fn set_ear_blast_threshold(percent: u32, state: State<'_, crate::EarBlastState>) {
    state
        .threshold_percent
        .store(percent.min(100).max(1), Ordering::Relaxed);
}

#[command]
pub fn set_ear_blast_target(percent: u32, state: State<'_, crate::EarBlastState>) {
    state
        .target_percent
        .store(percent.min(100), Ordering::Relaxed);
}

#[command]
pub fn get_ear_blast_state(state: State<'_, crate::EarBlastState>) -> Result<String, String> {
    let enabled = state.enabled.load(Ordering::Relaxed);
    let channels: Vec<String> = state.channels.lock().unwrap().iter().cloned().collect();
    let threshold = state.threshold_percent.load(Ordering::Relaxed);
    let target = state.target_percent.load(Ordering::Relaxed);
    let json = serde_json::json!({
        "enabled": enabled,
        "channels": channels,
        "threshold": threshold,
        "target": target,
    });
    Ok(json.to_string())
}

// ══════════════════════════════════════════════════════════════
//  ★ Virtual Audio Onboarding / Factory Reset
// ══════════════════════════════════════════════════════════════

const VIRTUAL_CHANNELS: &[&str] = &["Game", "Chat", "Media", "Aux"];

/// Returns true if all 4 OpenGG virtual sinks are present in PipeWire.
#[command]
pub async fn check_virtual_audio_status() -> Result<bool, String> {
    let out = run_cmd_async("pactl", &["list", "sinks", "short"]).await.unwrap_or_default();
    let all_present = VIRTUAL_CHANNELS
        .iter()
        .all(|ch| out.contains(&format!("OpenGG_{ch}")));
    Ok(all_present)
}

/// Create all OpenGG virtual null sinks via pactl (idempotent — skips existing).
#[command]
pub async fn create_virtual_audio() -> Result<(), String> {
    let existing = run_cmd_async("pactl", &["list", "sinks", "short"]).await.unwrap_or_default();
    for ch in VIRTUAL_CHANNELS {
        let sink_name = format!("OpenGG_{ch}");
        if existing.contains(&sink_name) {
            continue;
        }
        let display_name = live_display_name(ch);
        run_cmd_async("pactl", &[
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
        ]).await?;
    }
    log::info!("Virtual audio sinks created");
    Ok(())
}

/// Unload only the OpenGG virtual sinks without restarting PipeWire/WirePlumber.
#[command]
pub async fn remove_virtual_audio() -> Result<(), String> {
    let output = run_cmd_async("pactl", &["list", "modules"])
        .await
        .map_err(|e| format!("Failed to list PulseAudio modules: {e}"))?;

    let mut indices_to_remove: Vec<u64> = Vec::new();
    let mut current_index: Option<u64> = None;
    let mut is_null_sink = false;
    let mut is_opengg = false;

    for line in output.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("Module #") {
            // Emit previous module if it matched
            if current_index.is_some() && is_null_sink && is_opengg {
                indices_to_remove.push(current_index.unwrap());
            }
            current_index = rest.parse().ok();
            is_null_sink = false;
            is_opengg = false;
        } else if trimmed.starts_with("Name:") {
            is_null_sink = trimmed.trim() == "Name: module-null-sink";
        } else if trimmed.starts_with("Argument:") {
            is_opengg = trimmed.contains("OpenGG_");
        }
    }
    // Emit the last module
    if current_index.is_some() && is_null_sink && is_opengg {
        indices_to_remove.push(current_index.unwrap());
    }

    if indices_to_remove.is_empty() {
        return Err("No OpenGG virtual audio modules found. They may have already been removed.".into());
    }

    let mut removed = 0;
    for idx in &indices_to_remove {
        run_cmd_async("pactl", &["unload-module", &idx.to_string()])
            .await
            .map_err(|e| format!("Failed to unload module {idx}: {e}"))?;
        removed += 1;
    }

    log::info!("Virtual audio removed: {removed} module(s) unloaded");
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
}

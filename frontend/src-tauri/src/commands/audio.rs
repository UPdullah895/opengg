//! Audio Tauri commands. The Tauri-free business logic (daemon dbus + pactl
//! mixer/device/virtual-sink queries) lives in `opengg_core::audio`; the
//! commands below are thin `#[command]` wrappers that delegate there (async
//! ones via `spawn_blocking`, since the core API is blocking).
//!
//! Kept here (AppHandle/State/Emitter- and libpulse-coupled, deferred from the
//! core extraction): per-app routing (`route_app` + `Router`), VU-meter
//! streaming, and the ear-blast protection loop. These share four `pub` helpers
//! from `opengg_core::audio` (`get_sink_index_by_name`, `live_display_name`,
//! `sink_prop_value`, `get_linked_device_for_monitor`).

use serde::Serialize;
use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{command, AppHandle, Emitter, Manager, State};
use super::{AU_PATH, AU_IFACE, call_dbus_void, run_cmd_async, run_cmd_sync};
use opengg_core::audio::{
    get_linked_device_for_monitor, get_sink_index_by_name, live_display_name, sink_prop_value,
};
pub use opengg_core::audio::{AudioDevice, CaptureSource};

// ═══════════════════════════════════════════════════════════════════════════
//  Thin wrappers over opengg_core::audio (async → spawn_blocking; sync direct)
// ═══════════════════════════════════════════════════════════════════════════

#[command]
pub async fn get_channels() -> Result<String, String> {
    tokio::task::spawn_blocking(opengg_core::audio::get_channels)
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[command]
pub async fn set_volume(channel: String, volume: u32) -> Result<(), String> {
    tokio::task::spawn_blocking(move || opengg_core::audio::set_volume(channel, volume))
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[command]
pub async fn set_mute(channel: String, muted: bool) -> Result<(), String> {
    tokio::task::spawn_blocking(move || opengg_core::audio::set_mute(channel, muted))
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[command]
pub async fn unmute_media_streams() -> Result<(), String> {
    tokio::task::spawn_blocking(opengg_core::audio::unmute_media_streams)
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[command]
pub async fn set_app_volume(app_index: u32, volume: u32) -> Result<(), String> {
    tokio::task::spawn_blocking(move || opengg_core::audio::set_app_volume(app_index, volume))
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[command]
pub async fn get_apps() -> Result<String, String> {
    tokio::task::spawn_blocking(opengg_core::audio::get_apps)
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[command]
pub async fn get_audio_devices() -> Result<Vec<AudioDevice>, String> {
    tokio::task::spawn_blocking(opengg_core::audio::get_audio_devices)
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[command]
pub async fn set_channel_device(channel: String, device_name: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || opengg_core::audio::set_channel_device(channel, device_name))
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[command]
pub async fn check_virtual_audio_status() -> Result<bool, String> {
    tokio::task::spawn_blocking(opengg_core::audio::check_virtual_audio_status)
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[command]
pub async fn create_virtual_audio() -> Result<(), String> {
    tokio::task::spawn_blocking(opengg_core::audio::create_virtual_audio)
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[command]
pub async fn remove_virtual_audio() -> Result<(), String> {
    tokio::task::spawn_blocking(opengg_core::audio::remove_virtual_audio)
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[command]
pub fn hydrate_audio_routing() {
    opengg_core::audio::hydrate_audio_routing()
}

#[command]
pub fn list_audio_sinks() -> Result<Vec<String>, String> {
    opengg_core::audio::list_audio_sinks()
}

#[command]
pub fn list_capture_sources() -> Result<Vec<CaptureSource>, String> {
    opengg_core::audio::list_capture_sources()
}

#[command]
pub fn get_session_type() -> String {
    opengg_core::audio::get_session_type()
}

// ═══════════════════════════════════════════════════════════════════════════
//  Kept in the Tauri host (deferred): route_app, VU streaming, ear-blast.
// ═══════════════════════════════════════════════════════════════════════════

struct Router {
    app: AppHandle,
    app_id: u32,
    channel: String,
}

impl Router {
    async fn route(self) -> Result<(), String> {
        // Strategy 0: D-Bus daemon (preferred — single source of truth)
        if let Ok(()) = self.try_dbus().await {
            return Ok(());
        }

        log_routing_context(self.app_id, &self.channel);

        // Resolve target sink's pactl integer index once.
        let sink_name = self.resolve_sink_name().await?;
        let sink_name_clone = sink_name.clone();
        let sink_idx = tokio::task::spawn_blocking(move || get_sink_index_by_name(&sink_name_clone))
            .await
            .map_err(|e| format!("spawn_blocking: {e}"))??;

        // ── ONE identifier space ──
        // The incoming `app_id` may be a pactl sink-input index OR a PipeWire node.id
        // (object.id). Translate it to the pactl sink-input index up front so every
        // downstream call (move + verify) speaks the same namespace. `pactl
        // move-sink-input <si_idx> <sink_idx>` is the reliable, verifiable mechanism
        // (pw-metadata target.node was unreliable — it writes metadata WirePlumber may
        // not act on, "succeeding" without moving the stream).
        let app_id = self.app_id;
        let si_idx = tokio::task::spawn_blocking(move || resolve_pactl_si_index(app_id))
            .await
            .map_err(|e| format!("spawn_blocking: {e}"))?
            .ok_or_else(|| {
                format!(
                    "route_app: id {} is not a movable sink-input (no pactl index nor PW node.id match) — not routing to {}",
                    self.app_id, self.channel
                )
            })?;

        self.move_and_verify(si_idx, sink_idx).await
    }

    /// Move the stream to the target sink via `pactl move-sink-input` and confirm it
    /// actually landed there by re-reading the sink-input's `sink` field (with retry).
    /// Both arguments are pactl integer indices.
    async fn move_and_verify(&self, si_idx: u32, sink_idx: u32) -> Result<(), String> {
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            }
            match run_cmd_async(
                "pactl",
                &["move-sink-input", &si_idx.to_string(), &sink_idx.to_string()],
            )
            .await
            {
                Ok(_) => {
                    let verified =
                        tokio::task::spawn_blocking(move || verify_stream_routed(si_idx, sink_idx))
                            .await
                            .unwrap_or(false);
                    if verified {
                        eprintln!(
                            "route_app[{}→{}]: ✓ moved sink-input #{si_idx} → sink #{sink_idx} (verified)",
                            self.app_id, self.channel
                        );
                        let _ = self.app.emit("audio-mixer-refresh", ());
                        return Ok(());
                    }
                    eprintln!(
                        "route_app[{}→{}]: move command ok but stream not on target yet (attempt {}/3)",
                        self.app_id, self.channel, attempt + 1
                    );
                }
                Err(e) => eprintln!(
                    "route_app[{}→{}]: pactl move-sink-input failed (attempt {}/3): {e}",
                    self.app_id, self.channel, attempt + 1
                ),
            }
        }
        Err(format!(
            "route_app: failed to move id {} ({}→{}) after retries",
            self.app_id, si_idx, self.channel
        ))
    }

    async fn try_dbus(&self) -> Result<(), String> {
        match call_dbus_void("RouteApp", AU_PATH, AU_IFACE, (self.app_id, self.channel.as_str())).await {
            Ok(()) => {
                log::info!("route_app[{}→{}]: D-Bus call succeeded, emitting refresh", self.app_id, self.channel);
                let _ = self.app.emit("audio-mixer-refresh", ());
                Ok(())
            }
            Err(e) => {
                // Debug, not error: the per-app circuit breaker / cooldown above limits how
                // often we get here, and a failure usually just means a short-lived stream
                // already closed. The pactl fallback below is the real attempt.
                log::debug!("route_app[{}→{}]: D-Bus route failed ({e}), falling back to pactl", self.app_id, self.channel);
                Err(e)
            }
        }
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

}

/// Translate an incoming app id to a pactl sink-input index.
///
/// The id may already BE a pactl sink-input index, or it may be a PipeWire node.id
/// (object.id) — e.g. when the daemon supplied a node id. Resolving to one namespace
/// here is what fixes the "pactl move-sink-input <pw_node_id> → No such entity" failure.
/// Returns `None` if the id maps to no current sink-input (e.g. a source-output / mic
/// capture, which cannot be moved to a playback sink).
fn resolve_pactl_si_index(app_id: u32) -> Option<u32> {
    if validate_sink_input_exists(app_id) {
        return Some(app_id);
    }
    find_pactl_si_for_pw_id(app_id).ok()
}

#[command]
pub async fn route_app(
    app: tauri::AppHandle,
    state: State<'_, crate::RouteState>,
    app_id: u32,
    channel: String,
    binary: String,
) -> Result<(), String> {
    // Stable routing key: the app's binary (lowercased) when known, else the volatile
    // stream id. Keying every guard by this — NOT the per-stream PipeWire object.serial —
    // is what stops the runaway flood: hundreds of short-lived streams from one app now
    // share a single key, so the cooldown + circuit breaker actually engage instead of
    // seeing a "fresh" id every time and waving each one through.
    let key = if binary.trim().is_empty() {
        app_id.to_string()
    } else {
        binary.to_lowercase()
    };

    // Identity log: what we're about to route and why it resolved to this key/channel.
    log::debug!("route_app: id={app_id} binary='{binary}' → channel='{channel}' key='{key}'");

    // ── Guard 1: Blacklist ──
    if state.is_blacklisted(&binary) {
        return Err(format!(
            "route_app: PID {} ({}) is blacklisted — system processes cannot be routed",
            app_id, binary
        ));
    }

    // ── Guard 2: Already routed to same channel ──
    if state.is_already_routed(&key, &channel) {
        return Ok(());
    }

    // ── Guard 3: Cooldown (even on failure, don't retry) ──
    if state.is_on_cooldown(&key) {
        log::debug!("route_app: '{key}' on cooldown ({}s) — skipping", crate::ROUTE_COOLDOWN_SECS);
        return Err(format!(
            "route_app: {} is on cooldown ({}s)",
            key, crate::ROUTE_COOLDOWN_SECS
        ));
    }

    // ── Guard 4: Circuit breaker (too many recent failures for this app) ──
    if state.is_circuit_open(&key) {
        log::debug!("route_app: '{key}' circuit breaker open — skipping");
        return Err(format!(
            "route_app: {} circuit breaker open (too many failures)",
            key
        ));
    }

    // Record attempt BEFORE execution to prevent concurrent duplicate calls
    state.record_attempt(&key);

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
            state.record_success(&key, channel);
            Ok(())
        }
        Err(ref e) => {
            let circuit_open = state.record_failure(&key);
            if circuit_open {
                // First time the breaker trips for this app: warn once. Subsequent
                // attempts short-circuit at Guard 4 (debug), so no error spam.
                log::warn!(
                    "route_app: '{key}' circuit breaker OPEN after {} failures in {}s — \
                     suppressing further attempts for {}s (likely transient/short-lived streams)",
                    crate::FAIL_THRESHOLD,
                    crate::FAIL_WINDOW_SECS,
                    crate::FAIL_COOLDOWN_SECS,
                );
            } else {
                // Expected for vanished short-lived streams — debug, not error spam.
                log::debug!("route_app: id={app_id} ('{key}') → {channel} failed: {e}");
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
#[allow(dead_code)]
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
///
/// NOTE: kept for reference only. In practice this proved unreliable (the metadata
/// write reports success without WirePlumber moving the stream), so routing now uses
/// `pactl move-sink-input` by translated pactl index — see `Router::move_and_verify`.
#[allow(dead_code)]
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

/// Stream info from pw-dump: (name, binary, sink_idx, volume, auto_channel).

fn verify_stream_routed(si_idx: u32, sink_idx: u32) -> bool {
    for attempt in 0..5 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
        let Ok(j) = run_cmd_sync("pactl", &["-f", "json", "list", "sink-inputs"]) else {
            continue;
        };
        let Ok(sis) = serde_json::from_str::<Vec<serde_json::Value>>(&j) else {
            continue;
        };
        if let Some(si) = sis
            .iter()
            .find(|si| si["index"].as_u64() == Some(si_idx as u64))
        {
            if si["sink"].as_u64() == Some(sink_idx as u64) {
                return true;
            }
        }
    }
    false
}

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

#[derive(Serialize, Clone)]
struct VuLevels {
    channels: Vec<(String, f32)>,
}

/// Spawns a libpulse reader thread for a single channel (fallback when native PipeWire fails).
fn spawn_libpulse_reader(
    spec: &libpulse_binding::sample::Spec,
    name: &'static str,
    target: &str,
    my_gen: u64,
    tx_clone: tokio::sync::mpsc::UnboundedSender<(String, f32)>,
    running_clone: Arc<AtomicBool>,
    gen_clone: Arc<AtomicU64>,
) {
    use libpulse_binding::stream::Direction;
    use libpulse_simple_binding::Simple;

    let target = target.to_string();
    let spec = *spec;

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
        while running_clone.load(Ordering::Relaxed) && gen_clone.load(Ordering::Relaxed) == my_gen {
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

/// Real-time per-channel VU meters with native PipeWire fallback to libpulse.
///
/// Attempts to use native PipeWire (pipewire-rs) for low-latency capture streams.
/// Each channel thread opens a PipeWire context, creates a capture stream (F32 mono),
/// and reads RMS levels with attack/decay smoothing identical to libpulse.
///
/// If PipeWire initialization fails at runtime (e.g., missing library), falls back to
/// libpulse with `pa_simple` connections (S16LE PCM 8kHz). Each reader runs in
/// `spawn_blocking` so the async runtime is never stalled. Stopping is cooperative:
/// set the AtomicBool to false and threads exit within one read period (~32ms).
/// Handles are dropped at thread exit, freeing system resources via RAII.
///
/// Generation counter (VuState.1) deduplicates stale threads from prior calls.
#[command]
pub async fn start_vu_stream(app: AppHandle) -> Result<(), String> {
    use libpulse_binding::sample::{Format, Spec};

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

    // Mic source: prefer OpenGG_Virtual_Mic when it exists, fall back to hardware default
    let mut mic_source = run_cmd_async("pactl", &["get-default-source"]).await.unwrap_or_default();
    if let Ok(sources_list) = run_cmd_async("pactl", &["list", "sources", "short"]).await {
        if sources_list.lines().any(|l| l.contains("OpenGG_Virtual_Mic")) {
            mic_source = "OpenGG_Virtual_Mic".to_string();
        }
    }

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

        // Try native PipeWire first
        let is_source = name == "Mic";
        match crate::vu_native::spawn_channel_reader(
            name.to_string(),
            target.clone(),
            is_source,
            tx_clone.clone(),
            running_clone.clone(),
            gen_clone.clone(),
            my_gen,
        ) {
            Ok(()) => {
                eprintln!("start_vu_stream: {name} → native PipeWire reader spawned");
            }
            Err(e) => {
                eprintln!("native PipeWire init failed for {name}: {e}; falling back to libpulse");
                // Fallback: spawn libpulse reader
                spawn_libpulse_reader(&spec, name, &target, my_gen, tx_clone, running_clone, gen_clone);
            }
        }
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
                        let diff = cur.abs_diff(target_pct as u32);
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
        .store(percent.clamp(1, 100), Ordering::Relaxed);
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



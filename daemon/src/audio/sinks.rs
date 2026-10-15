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
//!
//! Nothing here runs at construction time: `SinkManager::new` is pure, and
//! `tick` (driven by `AudioHub`'s background reconciler) is what waits for
//! pipewire-pulse, creates whatever is missing and records a `Health` snapshot.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::env;
use std::path::PathBuf;
use crate::subprocess;
use super::coalesce::Coalescer;

/// Minimum interval between `pactl` spawns for the same channel.
/// Prevents process storm during slider drags (≤10 spawns/sec instead of 60+).
/// Trailing-edge: the last value of a burst is always applied (see `coalesce`).
const VOLUME_DEBOUNCE: Duration = Duration::from_millis(100);

/// While the graph is degraded (links or mic loopback missing) a full
/// create-and-link pass is retried at most this often.
const DEGRADED_RETRY: Duration = Duration::from_secs(60);

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
    /// True when volume/mute were read from the live sink this call; false
    /// when the sink was not found and these are cached in-memory values (not
    /// real). Lets the UI show "audio backend unavailable" instead of
    /// presenting a dead mixer as healthy.
    pub live: bool,
}

#[derive(Debug, Clone)]
pub struct AppInfo {
    pub id: u32,
    pub name: String,
    pub binary: String,
}

/// Snapshot of what the audio backend looks like, produced by the reconciler
/// and exposed through `GetDiagnostics`. "Ready" is derived from it, never
/// assumed.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Health {
    pub pulse_reachable: bool,
    pub sinks_present: Vec<String>,
    pub sinks_missing: Vec<String>,
    /// Output channel links (OpenGG_x:monitor -> default sink) still missing
    /// after the verify pass; `None` = not evaluated (no default output).
    pub output_links_missing: Option<usize>,
    pub default_sink: Option<String>,
    /// `None` = mic loopback + virtual mic in place; `Some(reason)` otherwise.
    pub mic_loopback_error: Option<String>,
    pub last_check_unix: u64,
}

impl Health {
    /// Everything that is wrong, in words. Empty = fully ready.
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        if !self.pulse_reachable {
            p.push("pipewire-pulse is not reachable".to_string());
            return p;
        }
        if !self.sinks_missing.is_empty() {
            p.push(format!(
                "{}/{} virtual sinks present, missing: {}",
                self.sinks_present.len(),
                SINK_CHANNELS.len(),
                self.sinks_missing.join(", ")
            ));
        }
        match self.output_links_missing {
            Some(0) => {}
            Some(n) => p.push(format!(
                "{n} output link(s) to the default sink missing (those channels will be silent)"
            )),
            None => p.push("no default output device - channels are not linked to speakers".into()),
        }
        if let Some(e) = &self.mic_loopback_error {
            p.push(format!("mic loopback: {e}"));
        }
        p
    }

    pub fn is_ready(&self) -> bool {
        self.problems().is_empty()
    }
}

/// Names from `pactl list sinks short` output (second whitespace column).
pub(crate) fn sink_names_from_short(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.split_whitespace().nth(1))
        .map(str::to_string)
        .collect()
}

/// Which `OpenGG_<channel>` sinks are absent from `present` (exact name match;
/// `contains` would let `OpenGG_Mic` be satisfied by a longer unrelated name).
pub(crate) fn missing_sinks(present: &[String]) -> Vec<String> {
    SINK_CHANNELS
        .iter()
        .map(|ch| format!("OpenGG_{ch}"))
        .filter(|name| !present.iter().any(|p| p == name))
        .collect()
}

/// `pactl list sinks short`, distinguishing "tool missing" / "server down" from
/// "no sinks". This doubles as the reachability probe: it needs a live
/// pipewire-pulse to answer.
pub fn list_sink_names() -> std::result::Result<Vec<String>, String> {
    let out = subprocess::command("pactl")
        .args(["list", "sinks", "short"])
        .output()
        .map_err(|e| subprocess::spawn_error("pactl", &e))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("pactl list sinks failed: {}", err.trim()));
    }
    Ok(sink_names_from_short(&String::from_utf8_lossy(&out.stdout)))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// What one `tick` found.
pub enum Tick {
    /// pipewire-pulse / pactl cannot be reached right now (reason attached).
    PulseDown(String),
    Ok,
}

pub struct SinkManager {
    pub channels: Arc<Mutex<HashMap<String, ChannelInfo>>>,
    /// Module IDs from pactl load-module (for cleanup if needed)
    module_ids: Arc<Mutex<Vec<u32>>>,
    /// Trailing-edge coalescer in front of `pactl set-sink-volume`.
    volume: Coalescer,
    /// False after an explicit teardown ("Danger Zone"), so the reconciler does
    /// not fight the user by re-creating sinks they just removed. Set again by
    /// `ensure_created`.
    desired: Arc<AtomicBool>,
    health: Arc<Mutex<Health>>,
    /// When the last full create-and-link pass ran.
    last_full: Mutex<Option<Instant>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

/// `pactl set-sink-volume`, with failures turned into text the caller can
/// return over D-Bus (previously discarded, so a dead backend looked fine).
fn apply_volume(channel: &str, volume: f32) -> std::result::Result<(), String> {
    let pct_str = format!("{}%", (volume * 100.0) as u32);
    let sink = sink_for(channel);
    run_pactl(&["set-sink-volume", &sink, &pct_str])
}

fn sink_for(channel: &str) -> String {
    // Master has no virtual sink — it controls the hardware default output directly.
    if channel == "Master" { "@DEFAULT_SINK@".to_string() } else { format!("OpenGG_{channel}") }
}

/// Run `pactl <args>`; Err carries tool-missing / stderr text.
fn run_pactl(args: &[&str]) -> std::result::Result<(), String> {
    let out = subprocess::command("pactl")
        .args(args)
        .output()
        .map_err(|e| subprocess::spawn_error("pactl", &e))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "pactl {} failed: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Log a pactl failure once per distinct message (a slider drag would
/// otherwise repeat it 60 times a second).
fn warn_pactl_failure(what: &str, err: &str) {
    if crate::statelog::changed(&format!("audio.{what}"), err) {
        tracing::warn!("{what}: {err}");
    }
}

/// Outcome of one create-and-link pass.
struct CreateReport {
    module_ids: Vec<u32>,
    /// Output links still missing after verify; `None` = no default sink.
    links_missing: Option<usize>,
    default_sink: Option<String>,
    mic_error: Option<String>,
}

/// Create (idempotently) every OpenGG virtual sink and its loopbacks, and
/// return the pactl module IDs created along the way. Shared by the
/// reconciler (`SinkManager::tick`) and `SinkManager::ensure_created` (the
/// on-demand "Create Virtual Audio" D-Bus call), so there is exactly one code
/// path that knows how to bring sinks into existence and exactly one place
/// their module IDs get recorded for later cleanup.
fn create_sinks_and_loopbacks() -> CreateReport {
    remove_legacy_sink_config();

    let mut module_ids = Vec::new();

    // One listing up front instead of a pactl spawn per channel.
    let present = list_sink_names().unwrap_or_default();

    for &ch in SINK_CHANNELS {
        let sink_name = format!("OpenGG_{ch}");

        // Step 1: Check if this sink already exists (idempotent)
        if !present.contains(&sink_name) {
            // Step 2: Create via pactl load-module (non-destructive)
            match create_null_sink(&sink_name, ch) {
                Ok(module_id) => {
                    module_ids.push(module_id);
                    tracing::info!("Created sink {sink_name} (module {module_id})");
                }
                Err(e) => {
                    tracing::warn!("Failed to create {sink_name}: {e}");
                }
            }
        } else {
            tracing::debug!("Sink {sink_name} already exists — skipping creation");
        }
    }

    // Step 3: Wait briefly for sinks to register, then set up loopbacks
    std::thread::sleep(std::time::Duration::from_millis(500));
    let (links_missing, default_sink) = match setup_loopbacks() {
        Ok(st) => (st.missing, st.default_sink),
        Err(e) => {
            tracing::warn!("Loopback setup had issues: {e:#}");
            (None, None)
        }
    };
    // Dedup: WirePlumber session policies may auto-route system apps (plasmashell, kwin_*)
    // to OpenGG sinks every time they are created. Move them back to default output.
    cleanup_blacklisted_routing();

    // Step 4: Wire hardware mic → OpenGG_Mic virtual sink via loopback.
    // The sink's monitor port is what GSR and DSP chains capture from.
    let mic_error = match setup_mic_loopback() {
        Ok(mic_module_ids) => {
            module_ids.extend(mic_module_ids);
            None
        }
        Err(e) => {
            let msg = format!("{e:#}");
            // Fires on every degraded retry; only the transition is news.
            if crate::statelog::changed("audio.mic-loopback", &msg) {
                tracing::warn!("Mic loopback setup failed (raw HW mic will be used as fallback): {msg}");
            }
            Some(msg)
        }
    };
    if mic_error.is_none() {
        crate::statelog::changed("audio.mic-loopback", "ok");
    }

    CreateReport { module_ids, links_missing, default_sink, mic_error }
}

impl SinkManager {
    /// Build the manager WITHOUT touching PipeWire. Creation is the
    /// reconciler's job (`tick`), so daemon startup — and with it D-Bus name
    /// acquisition — never waits on pipewire-pulse.
    pub fn new() -> Self {
        let channels = Arc::new(Mutex::new(HashMap::new()));
        for &name in CHANNEL_NAMES {
            lock(&channels).insert(
                name.to_string(),
                ChannelInfo {
                    name: name.to_string(),
                    volume: 1.0,
                    muted: false,
                    assigned_apps: Vec::new(),
                    live: false,
                },
            );
        }
        Self {
            channels,
            module_ids: Arc::new(Mutex::new(Vec::new())),
            volume: Coalescer::new(VOLUME_DEBOUNCE, apply_volume),
            desired: Arc::new(AtomicBool::new(true)),
            health: Arc::new(Mutex::new(Health::default())),
            last_full: Mutex::new(None),
        }
    }

    pub fn health(&self) -> Health {
        lock(&self.health).clone()
    }

    pub fn health_handle(&self) -> Arc<Mutex<Health>> {
        Arc::clone(&self.health)
    }

    /// One reconcile step (blocking; call from `spawn_blocking`).
    ///
    /// * pipewire-pulse unreachable -> `Tick::PulseDown(reason)` and nothing
    ///   else; the caller backs off and retries.
    /// * any OpenGG sink missing (first start, pipewire-pulse restarted, a
    ///   module crashed), or `force`, or a degraded graph whose retry interval
    ///   elapsed -> run the full idempotent create-and-link pass.
    /// * otherwise just refresh the health snapshot (one `pactl` spawn).
    ///
    /// Never restarts PipeWire; every step is a stateless pactl/pw-link call.
    pub fn tick(&self, force: bool) -> Tick {
        let present = match list_sink_names() {
            Ok(p) => p,
            Err(reason) => {
                lock(&self.health).pulse_reachable = false;
                return Tick::PulseDown(reason);
            }
        };

        if !self.desired.load(Ordering::SeqCst) {
            let mut h = lock(&self.health);
            h.pulse_reachable = true;
            h.sinks_missing = missing_sinks(&present);
            h.sinks_present = present.into_iter().filter(|n| n.starts_with("OpenGG_")).collect();
            h.last_check_unix = unix_now();
            return Tick::Ok;
        }

        let missing = missing_sinks(&present);
        let degraded_retry_due = {
            let degraded = !self.health().is_ready();
            degraded && lock(&self.last_full).is_none_or(|t| t.elapsed() >= DEGRADED_RETRY)
        };

        if force || !missing.is_empty() || degraded_retry_due {
            self.create_and_record();
        } else {
            let mut h = lock(&self.health);
            h.pulse_reachable = true;
            h.sinks_missing = Vec::new();
            h.sinks_present = present.into_iter().filter(|n| n.starts_with("OpenGG_")).collect();
            h.last_check_unix = unix_now();
        }
        Tick::Ok
    }

    /// Run the create-and-link pass, fold module ids in, record `Health`, and
    /// log ONE summary whose level reflects the worst state ("ready" only when
    /// every sink and link is actually there).
    fn create_and_record(&self) {
        let report = create_sinks_and_loopbacks();
        *lock(&self.last_full) = Some(Instant::now());
        lock(&self.module_ids).extend(report.module_ids);

        let present = list_sink_names().unwrap_or_default();
        let health = Health {
            pulse_reachable: true,
            sinks_missing: missing_sinks(&present),
            sinks_present: present.into_iter().filter(|n| n.starts_with("OpenGG_")).collect(),
            output_links_missing: report.links_missing,
            default_sink: report.default_sink,
            mic_loopback_error: report.mic_error,
            last_check_unix: unix_now(),
        };
        let problems = health.problems();
        let summary = problems.join("; ");
        if crate::statelog::changed("audio.health", &summary) {
            if problems.is_empty() {
                tracing::info!(
                    "Virtual audio ready: {}/{} sinks, all output links, mic loopback active",
                    health.sinks_present.len(),
                    SINK_CHANNELS.len()
                );
            } else if !health.sinks_missing.is_empty() {
                tracing::error!("Virtual audio NOT ready: {summary}");
            } else {
                tracing::warn!("Virtual audio degraded: {summary}");
            }
        }
        *lock(&self.health) = health;
    }

    /// (Re)create any missing OpenGG virtual sinks on an already-running
    /// `SinkManager`, e.g. after the user tore them down via the Settings
    /// "Danger Zone" and then asks to recreate them. Newly created module
    /// IDs are folded into `self.module_ids` so the same instance that owns
    /// `Drop`-based cleanup knows about them — unlike the pre-fix behavior
    /// where the Qt UI process created these sinks directly via
    /// `opengg_core::audio::create_virtual_audio()`, invisible to this
    /// struct entirely.
    ///
    /// Returns an error naming what is still missing, so the caller sees a
    /// failed creation instead of a silent success.
    pub fn ensure_created(&self) -> Result<()> {
        self.desired.store(true, Ordering::SeqCst);
        if let Err(reason) = list_sink_names() {
            anyhow::bail!("audio backend unavailable: {reason}");
        }
        self.create_and_record();

        // Re-seed channel state for any channel that isn't already tracked
        // (fresh SinkManager instances always have all of them; this only
        // matters if CHANNEL_NAMES ever grows without a daemon restart).
        let mut channels = lock(&self.channels);
        for &name in CHANNEL_NAMES {
            channels.entry(name.to_string()).or_insert_with(|| ChannelInfo {
                name: name.to_string(),
                volume: 1.0,
                muted: false,
                assigned_apps: Vec::new(),
                live: false,
            });
        }
        drop(channels);

        let h = self.health();
        if !h.sinks_missing.is_empty() {
            anyhow::bail!("could not create virtual sinks: {}", h.problems().join("; "));
        }
        Ok(())
    }

    /// Set a channel's volume. The in-memory value is updated immediately; the
    /// `pactl` call is coalesced per channel (leading + trailing edge) so a
    /// slider drag neither storms pactl nor loses its final position. An error
    /// from the immediate apply (tool missing, server down, sink absent) is
    /// returned so the UI/core fallback sees it instead of a fake success.
    pub fn set_volume(&self, channel: &str, volume: f32) -> Result<()> {
        let volume = volume.clamp(0.0, 1.5);
        if let Some(info) = lock(&self.channels).get_mut(channel) {
            info.volume = volume;
        }
        self.volume.submit(channel, volume).map_err(|e| {
            warn_pactl_failure("set-volume", &e);
            anyhow::anyhow!(e)
        })
    }

    pub fn set_mute(&self, channel: &str, muted: bool) -> Result<()> {
        if let Some(info) = lock(&self.channels).get_mut(channel) {
            info.muted = muted;
        }
        let val = if muted { "1" } else { "0" };
        let sink_name = sink_for(channel);
        run_pactl(&["set-sink-mute", &sink_name, val]).map_err(|e| {
            warn_pactl_failure("set-mute", &e);
            anyhow::anyhow!(e)
        })?;
        // Muting the Mic channel's virtual sink only silences consumers reading
        // from OpenGG_Mic.monitor. GSR and other capture tools still subscribe
        // directly to the hardware capture source, so the actual mic input must
        // also be muted at the source for the mute to have any real effect.
        if channel == "Mic" {
            if let Err(e) = run_pactl(&["set-source-mute", "@DEFAULT_SOURCE@", val]) {
                // The sink itself is muted; a machine with no default capture
                // source should not turn that into a failed call.
                warn_pactl_failure("set-source-mute", &e);
            }
        }
        Ok(())
    }

    /// Return channel state with **live** volume/mute read from pactl, so the mixer
    /// always reflects reality (including external changes via pavucontrol) instead of a
    /// stale in-memory default. When a sink is not found the cached value is kept but
    /// `live` is false and the state change is logged, so cached numbers are never
    /// presented as if they were real.
    pub fn get_channels(&self) -> Vec<ChannelInfo> {
        let live = read_live_sink_state();
        let default_sink = get_default_sink_name();
        let mut channels: Vec<ChannelInfo> = lock(&self.channels).values().cloned().collect();
        let mut not_live = Vec::new();
        for ch in &mut channels {
            // Master controls the hardware default output directly; everything else is OpenGG_{name}.
            let sink_name = if ch.name == "Master" {
                default_sink.clone()
            } else {
                Some(format!("OpenGG_{}", ch.name))
            };
            ch.live = false;
            if let Some(name) = sink_name {
                if let Some(&(vol, muted)) = live.get(&name) {
                    ch.volume = vol;
                    ch.muted = muted;
                    ch.live = true;
                }
            }
            if !ch.live && ch.name != "Master" {
                not_live.push(ch.name.clone());
            }
        }
        not_live.sort();
        let state = if not_live.is_empty() { "all live".to_string() } else { not_live.join(",") };
        if crate::statelog::changed("audio.get-channels-live", &state) {
            if not_live.is_empty() {
                tracing::info!("GetChannels: all channels are reading live sink state again");
            } else {
                tracing::warn!(
                    "GetChannels: no live sink for [{state}] — returning cached values flagged live=false \
                     (audio backend down or sinks not created yet)"
                );
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
        // The user asked for the sinks to be gone: stop the reconciler from
        // re-creating them until `ensure_created` is called again.
        self.desired.store(false, Ordering::SeqCst);

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
        .map_err(|e| anyhow::anyhow!("{}", subprocess::spawn_error("pactl", &e)))?;

    if output.status.success() {
        let id_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Do not fall back to 0: teardown would later `unload-module 0`, i.e.
        // some unrelated module on the server.
        let module_id = id_str
            .parse::<u32>()
            .map_err(|_| anyhow::anyhow!("pactl load-module returned a non-numeric module id: {id_str:?}"))?;
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
        tracing::debug!("Mic loopback already active — skipping loopback creation");
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
            tracing::debug!("Virtual mic source already exists and is wired to OpenGG_Mic — skipping");
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
        .map_err(|e| anyhow::anyhow!("{}", subprocess::spawn_error("pw-link", &e)))?;

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
pub fn setup_loopbacks() -> Result<LoopStatus> {
    let output = subprocess::command("pactl")
        .args(["get-default-sink"])
        .output()
        .map_err(|e| anyhow::anyhow!("{}", subprocess::spawn_error("pactl", &e)))?;
    let default_sink = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if default_sink.is_empty() {
        if crate::statelog::changed("audio.no-default-sink", "none") {
            tracing::warn!("No default audio output detected — loopbacks skipped");
        }
        return Ok(LoopStatus { default_sink: None, missing: None });
    }
    crate::statelog::changed("audio.no-default-sink", "present");

    // Output channels only — Mic is a capture sink, not a playback sink.
    const OUTPUT_CHANNELS: &[&str] = &["Game", "Chat", "Media", "Aux"];
    if crate::statelog::changed("audio.link-target", &default_sink) {
        tracing::info!("Linking virtual sinks → {default_sink}");
    }

    // Pre-check: read existing links so we can skip ones already present.
    let existing_links = parse_pw_links().unwrap_or_default();

    // Collect intended links for verification
    let mut intended_links: Vec<(String, String, &str)> = Vec::new();

    for &ch in OUTPUT_CHANNELS {
        let sink_name = format!("OpenGG_{ch}");
        for port in ["FL", "FR"] {
            let source = format!("{sink_name}:monitor_{port}");
            let dest = format!("{default_sink}:playback_{port}");
            intended_links.push((source.clone(), dest.clone(), ch));

            // Skip if this exact link already exists in the PipeWire graph.
            if existing_links.contains(&(source.clone(), dest.clone())) {
                tracing::debug!("Skipping already-linked {source} → {dest}");
                continue;
            }

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
                Err(e) => {
                    // Missing pw-link means NO channel gets linked to the
                    // speakers; say so (once) with the package to install.
                    let msg = subprocess::spawn_error("pw-link", &e);
                    if crate::statelog::changed("audio.pw-link-spawn", &msg) {
                        tracing::error!("audio routing disabled: {msg}");
                    }
                }
                _ => {}
            }
        }
    }

    // Unverified links count as missing: "ready" must be earned.
    let mut last_missing = intended_links.len();

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

                last_missing = missing.len();
                if missing.is_empty() {
                    tracing::debug!("Loopback verification passed (attempt {attempt}/{MAX_RETRIES})");
                    crate::statelog::changed("audio.links-missing", "none");
                    return Ok(LoopStatus { default_sink: Some(default_sink.clone()), missing: Some(0) });
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
                    // Final attempt: log as error (non-fatal, but unmissable) —
                    // once per distinct set, since a degraded graph is re-tried
                    // every minute and would otherwise repeat this forever.
                    let key: Vec<String> = missing.iter().map(|(s, d, _)| format!("{s}>{d}")).collect();
                    if crate::statelog::changed("audio.links-missing", &key.join(",")) {
                        for (source, dest, channel) in missing.iter() {
                            tracing::error!(
                                "Failed to link after {} retries: {} → {} (channel {}) — this channel may be silent \
                                 (sinks without playback_FL/FR ports, e.g. pro-audio profiles, cannot be linked)",
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
            }
            Err(e) => {
                tracing::debug!(
                    "Failed to verify loopback links on attempt {}/{}: {}",
                    attempt,
                    MAX_RETRIES,
                    e
                );
                if attempt == MAX_RETRIES
                    && crate::statelog::changed("audio.links-verify", &format!("{e:#}"))
                {
                    tracing::error!(
                        "Could not verify loopback setup after {} attempts ({e:#}) — continuing anyway",
                        MAX_RETRIES
                    );
                }
            }
        }
    }

    Ok(LoopStatus { default_sink: Some(default_sink), missing: Some(last_missing) })
}

/// Result of linking the output channels to the default sink.
pub struct LoopStatus {
    pub default_sink: Option<String>,
    /// Links still absent after the verify pass; `None` = nothing to link to.
    pub missing: Option<usize>,
}

/// Move blacklisted system-app streams (plasmashell, kwin_*, etc.) off OpenGG
/// virtual sinks back to the hardware default. WirePlumber session policies may
/// auto-route these apps to OpenGG null-sinks every time sinks are (re)created,
/// causing duplicated Media links and unexpected audio routing.
fn cleanup_blacklisted_routing() {
    const BLACKLIST: &[&str] = &[
        "plasmashell",
        "kwin_wayland",
        "kwin_x11",
        "swaync",
        "xdg-desktop-portal",
        "xdg-desktop-portal-gnome",
        "xdg-desktop-portal-kde",
    ];

    // Get the hardware default sink to move blacklisted streams onto.
    let Ok(out) = subprocess::command("pactl").args(["get-default-sink"]).output() else {
        return;
    };
    let default_sink = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if default_sink.is_empty() {
        return;
    }

    // Build sink-index → name map so we can check if a stream is on an OpenGG sink.
    let sink_map: std::collections::HashMap<u32, String> = {
        let Ok(out) = subprocess::command("pactl").args(["-f", "json", "list", "sinks"]).output() else {
            return;
        };
        let Ok(sinks) = serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout) else {
            return;
        };
        sinks.iter().filter_map(|s| {
            Some((s["index"].as_u64()? as u32, s["name"].as_str()?.to_string()))
        }).collect()
    };

    let Ok(out) = subprocess::command("pactl").args(["-f", "json", "list", "sink-inputs"]).output() else {
        return;
    };
    let Ok(sis) = serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout) else {
        return;
    };

    for si in &sis {
        let idx = si["index"].as_u64().unwrap_or(0) as u32;
        let p = &si["properties"];
        let binary = p["application.process.binary"].as_str().unwrap_or("");

        if !BLACKLIST.iter().any(|&b| binary.contains(b)) {
            continue;
        }

        let sink_idx = si["sink"].as_u64().unwrap_or(u64::MAX) as u32;
        let sink_name = sink_map.get(&sink_idx).map(|s| s.as_str()).unwrap_or("");

        if !sink_name.starts_with("OpenGG_") {
            continue;
        }

        tracing::info!(
            "Cleanup: moving blacklisted '{}' (sink-input #{}) off {} → {}",
            binary, idx, sink_name, default_sink
        );
        let _result = subprocess::command("pactl")
            .args(["move-sink-input", &idx.to_string(), &default_sink])
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHORT: &str = "\
60\talsa_output.pci-0000_00_1f.3.analog-stereo\tPipeWire\ts32le 2ch 48000Hz\tRUNNING
61\tOpenGG_Game\tPipeWire\ts16le 2ch 48000Hz\tIDLE
62\tOpenGG_Chat\tPipeWire\ts16le 2ch 48000Hz\tIDLE
";

    #[test]
    fn parses_sink_names_and_finds_exactly_what_is_missing() {
        let names = sink_names_from_short(SHORT);
        assert_eq!(names.len(), 3);
        assert_eq!(missing_sinks(&names), vec!["OpenGG_Media", "OpenGG_Aux", "OpenGG_Mic"]);
    }

    #[test]
    fn missing_match_is_exact_not_substring() {
        // A longer name must not satisfy OpenGG_Mic.
        let names = vec!["OpenGG_Mic_extra".to_string()];
        assert!(missing_sinks(&names).contains(&"OpenGG_Mic".to_string()));
    }

    #[test]
    fn health_is_ready_only_when_everything_is_there() {
        let all: Vec<String> = SINK_CHANNELS.iter().map(|c| format!("OpenGG_{c}")).collect();
        let mut h = Health {
            pulse_reachable: true,
            sinks_present: all,
            sinks_missing: vec![],
            output_links_missing: Some(0),
            default_sink: Some("x".into()),
            mic_loopback_error: None,
            last_check_unix: 0,
        };
        assert!(h.is_ready(), "{:?}", h.problems());

        h.sinks_present.truncate(1);
        h.sinks_missing = missing_sinks(&h.sinks_present);
        let p = h.problems().join("; ");
        assert!(p.contains("1/5 virtual sinks present") && p.contains("OpenGG_Chat"), "{p}");

        h.sinks_missing.clear();
        h.output_links_missing = Some(3);
        assert!(h.problems().join(";").contains("3 output link"));
        h.output_links_missing = None;
        assert!(h.problems().join(";").contains("no default output"));
        h.output_links_missing = Some(0);
        h.mic_loopback_error = Some("No default source".into());
        assert!(h.problems().join(";").contains("mic loopback: No default source"));
        h.pulse_reachable = false;
        assert_eq!(h.problems(), vec!["pipewire-pulse is not reachable".to_string()]);
    }
}

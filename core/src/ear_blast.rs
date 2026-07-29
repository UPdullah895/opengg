//! Ear Blast Protection — auto-ducks a channel's PipeWire volume when its
//! live VU level exceeds a threshold, restoring it once the level drops
//! back below a release margin. Ported from the Tauri host's
//! `check_ear_blast` (`frontend/src-tauri/src/commands/audio.rs`), which
//! runs inside its VU emitter task at ~30fps; this runs the same way inside
//! qt-shell's `AudioController::start_vu_stream` aggregator loop
//! (`qt-shell/src/audio.rs`).
//!
//! Config (`enabled`/`channels`/`threshold`/`target`) lives at
//! `mixer.earBlast` in the shared `ui-settings.json` envelope — a sibling
//! of `"settings"`, not nested under it, same as `modules`/
//! `extensionConsents` (see `qt-shell/src/extensions.rs`'s header comment).
//! Runtime state (which channels are currently ducked, their original
//! volumes, last-trigger timestamps) is deliberately kept separate from
//! config so a config reload mid-stream doesn't reset an in-progress duck.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct EarBlastState {
    pub enabled: bool,
    pub channels: HashSet<String>,
    pub threshold_pct: u32,
    pub target_pct: u32,
    active: HashMap<String, bool>,
    original_volumes: HashMap<String, u32>,
    last_trigger_ms: HashMap<String, u64>,
}

impl Default for EarBlastState {
    fn default() -> Self {
        Self {
            enabled: false,
            channels: HashSet::new(),
            threshold_pct: 85,
            target_pct: 60,
            active: HashMap::new(),
            original_volumes: HashMap::new(),
            last_trigger_ms: HashMap::new(),
        }
    }
}

impl EarBlastState {
    pub fn set_config(&mut self, enabled: bool, channels: HashSet<String>, threshold_pct: u32, target_pct: u32) {
        self.enabled = enabled;
        self.channels = channels;
        self.threshold_pct = threshold_pct;
        self.target_pct = target_pct;
    }

    pub fn is_active(&self, channel: &str) -> bool {
        *self.active.get(channel).unwrap_or(&false)
    }
}

/// Reads `mixer.earBlast` out of the shared `ui-settings.json` envelope.
/// Defaults match `DEFAULTS.mixer.earBlast` in `frontend/src/stores/
/// persistence.ts` when the key is absent.
pub fn load_config() -> (bool, HashSet<String>, u32, u32) {
    let raw = crate::settings::load_ui_settings().unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    let eb = &v["mixer"]["earBlast"];

    let enabled = eb["enabled"].as_bool().unwrap_or(false);
    let channels: HashSet<String> = eb["channels"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .filter(|s: &HashSet<String>| !s.is_empty())
        .unwrap_or_else(|| HashSet::from(["Game".to_string()]));
    let threshold_pct = eb["threshold"].as_u64().unwrap_or(85) as u32;
    let target_pct = eb["target"].as_u64().unwrap_or(60) as u32;

    (enabled, channels, threshold_pct, target_pct)
}

fn pa_object_name_for_channel(channel: &str) -> Option<String> {
    match channel {
        "Master" => crate::subprocess::run_cmd_sync("pactl", &["get-default-sink"]).ok().map(|s| s.trim().to_string()),
        "Mic" => crate::subprocess::run_cmd_sync("pactl", &["get-default-source"]).ok().map(|s| s.trim().to_string()),
        _ => Some(format!("OpenGG_{channel}")),
    }
}

fn get_sink_volume_percent(sink_name: &str) -> Option<u32> {
    let out = crate::subprocess::run_cmd_sync("pactl", &["get-sink-volume", sink_name]).ok()?;
    let pct_str = out.split('%').next()?.split_whitespace().last()?;
    pct_str.parse::<u32>().ok().map(|v| v.min(150))
}

fn get_source_volume_percent(source_name: &str) -> Option<u32> {
    let out = crate::subprocess::run_cmd_sync("pactl", &["get-source-volume", source_name]).ok()?;
    let pct_str = out.split('%').next()?.split_whitespace().last()?;
    pct_str.parse::<u32>().ok().map(|v| v.min(150))
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

/// Check one channel's live dB level against the configured threshold and
/// duck/restore its PipeWire volume as needed. Same activation/release/
/// hold-time state machine as the Tauri original — dynamic release margin
/// (threshold minus the volume-reduction delta plus a 3dB buffer) prevents
/// oscillation right at the threshold line, and a 500ms hold time after
/// activation prevents rapid re-triggering.
pub fn check(state: &mut EarBlastState, channel: &str, db: f32) {
    if !state.enabled || !state.channels.contains(channel) {
        return;
    }

    let threshold_pct = state.threshold_pct.max(1) as f32;
    let target_pct = state.target_pct.min(100) as f32;
    let threshold_db = 20.0 * (threshold_pct / 100.0).max(1e-9).log10();
    let reduction_db = if target_pct > 0.0 {
        20.0 * (target_pct / 100.0).max(1e-9).log10()
    } else {
        -60.0
    };
    let margin_db = reduction_db.abs() + 3.0;
    let release_db = threshold_db - margin_db;

    let now = now_ms();
    let is_active = state.is_active(channel);
    let last_trigger = *state.last_trigger_ms.get(channel).unwrap_or(&0);
    let hold_elapsed = now.saturating_sub(last_trigger) >= 500;

    if !is_active && db > threshold_db {
        let Some(sink_name) = pa_object_name_for_channel(channel) else { return };
        let current_vol = if channel == "Mic" {
            get_source_volume_percent(&sink_name)
        } else {
            get_sink_volume_percent(&sink_name)
        };
        let Some(vol) = current_vol else { return };
        if vol <= target_pct as u32 {
            return;
        }
        state.original_volumes.insert(channel.to_string(), vol);
        state.active.insert(channel.to_string(), true);
        state.last_trigger_ms.insert(channel.to_string(), now);

        let pactl_target = format!("{}%", target_pct.round() as u32);
        let _ = if channel == "Mic" {
            crate::subprocess::run_cmd_sync("pactl", &["set-source-volume", &sink_name, &pactl_target])
        } else {
            crate::subprocess::run_cmd_sync("pactl", &["set-sink-volume", &sink_name, &pactl_target])
        };
        log::info!("ear_blast: activated on {channel} (level={db:.1} dB > threshold={threshold_db:.1} dB) -> {}%", target_pct.round() as u32);
    } else if is_active && db < release_db && hold_elapsed {
        let Some(sink_name) = pa_object_name_for_channel(channel) else { return };
        let Some(orig_vol) = state.original_volumes.remove(channel) else {
            state.active.insert(channel.to_string(), false);
            return;
        };
        let current_vol = if channel == "Mic" {
            get_source_volume_percent(&sink_name)
        } else {
            get_sink_volume_percent(&sink_name)
        };
        // If the user manually changed the volume while ducked, don't stomp
        // their choice on release.
        let skip_restore = current_vol.map(|cur| cur.abs_diff(target_pct as u32) > 5).unwrap_or(false);
        state.active.insert(channel.to_string(), false);

        if !skip_restore {
            let pactl_vol = format!("{}%", orig_vol);
            let _ = if channel == "Mic" {
                crate::subprocess::run_cmd_sync("pactl", &["set-source-volume", &sink_name, &pactl_vol])
            } else {
                crate::subprocess::run_cmd_sync("pactl", &["set-sink-volume", &sink_name, &pactl_vol])
            };
        }
        log::info!("ear_blast: deactivated on {channel} (level={db:.1} dB < release={release_db:.1} dB) -> restored {orig_vol}%");
    }
}

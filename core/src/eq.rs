//! Per-channel EQ — client side.
//!
//! The engine runs in the daemon (`daemon/src/audio/eq.rs`): one PipeWire
//! filter-chain per enabled channel, linked into the graph by the
//! reconciler, surviving the UI closing. This side asks the daemon to apply
//! a state and remembers it in `ui-settings.json` (`mixer.eq`), which the
//! daemon reads back when it starts.
//!
//! The previous engine lived here, hosted by the UI process: it stopped
//! when the window closed, kept no state, and removed the channel's direct
//! output link, which the daemon's reconciler then restored — so the
//! equalised and dry signal played together.

use crate::daemon::{call_dbus, call_dbus_void, AU_IFACE, AU_PATH};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Band centres, for labels and tests. The daemon owns the filter design.
pub const EQ_FREQS: [f32; 10] = [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];

/// One channel's EQ, as stored and as sent to the daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EqState {
    pub enabled: bool,
    pub bands: Vec<f32>,
    pub preamp: f32,
}

impl Default for EqState {
    fn default() -> Self {
        EqState { enabled: false, bands: vec![0.0; EQ_FREQS.len()], preamp: 0.0 }
    }
}

/// Apply `state` to `channel` in the daemon. Errors carry the daemon's
/// reason (EQ unavailable on this system, daemon not running, …).
pub fn apply(channel: &str, state: &EqState) -> Result<(), String> {
    let json = serde_json::to_string(state).map_err(|e| e.to_string())?;
    call_dbus_void("SetEq", AU_PATH, AU_IFACE, (channel, json.as_str()))
}

/// The daemon's EQ status JSON:
/// `{"available", "reason", "channels": {ch: {enabled, bands, preamp, running}}}`.
pub fn status() -> Result<String, String> {
    call_dbus::<String>("GetEq", AU_PATH, AU_IFACE, ())
}

/// Saved state for every channel, from `mixer.eq`.
pub fn saved() -> HashMap<String, EqState> {
    let raw = crate::settings::load_ui_settings().unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);
    parse_saved(&v["mixer"]["eq"])
}

fn parse_saved(v: &serde_json::Value) -> HashMap<String, EqState> {
    v.as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), serde_json::from_value::<EqState>(v.clone()).ok()?)))
                .collect()
        })
        .unwrap_or_default()
}

/// States waiting to be written, and whether a writer is already scheduled.
fn pending() -> &'static Mutex<(HashMap<String, EqState>, bool)> {
    static P: OnceLock<Mutex<(HashMap<String, EqState>, bool)>> = OnceLock::new();
    P.get_or_init(|| Mutex::new((HashMap::new(), false)))
}

/// Remember `state` for `channel`. Slider drags call this many times a
/// second; writes are coalesced to one per half second, off the caller's
/// thread.
pub fn save(channel: &str, state: &EqState) {
    let mut p = pending().lock().unwrap();
    p.0.insert(channel.to_string(), state.clone());
    if p.1 {
        return;
    }
    p.1 = true;
    drop(p);
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(500));
        let states = {
            let mut p = pending().lock().unwrap();
            p.1 = false;
            std::mem::take(&mut p.0)
        };
        if let Err(e) = write_states(&states) {
            log::warn!("Saving EQ settings failed: {e}");
        }
    });
}

/// Merge `states` into `mixer.eq`, keeping the rest of the file.
fn write_states(states: &HashMap<String, EqState>) -> Result<(), String> {
    let raw = crate::settings::load_ui_settings()?;
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);
    merge_into(&mut v, states);
    crate::settings::save_ui_settings(&serde_json::to_string(&v).map_err(|e| e.to_string())?)
}

fn merge_into(v: &mut serde_json::Value, states: &HashMap<String, EqState>) {
    if !v.is_object() {
        *v = serde_json::json!({});
    }
    if !v["mixer"].is_object() {
        v["mixer"] = serde_json::json!({});
    }
    if !v["mixer"]["eq"].is_object() {
        v["mixer"]["eq"] = serde_json::json!({});
    }
    for (ch, st) in states {
        v["mixer"]["eq"][ch] = serde_json::to_value(st).unwrap_or_default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keeps_other_settings_and_channels() {
        let mut v = serde_json::json!({
            "settings": {"theme": "dark"},
            "mixer": {"appRules": {"vlc": "Media"}, "eq": {"Chat": {"enabled": true, "bands": [1], "preamp": 0}}}
        });
        let mut s = HashMap::new();
        s.insert("Game".to_string(), EqState { enabled: true, bands: vec![2.0; 10], preamp: -1.0 });
        merge_into(&mut v, &s);
        assert_eq!(v["settings"]["theme"], "dark");
        assert_eq!(v["mixer"]["appRules"]["vlc"], "Media");
        assert_eq!(v["mixer"]["eq"]["Chat"]["enabled"], true);
        assert_eq!(v["mixer"]["eq"]["Game"]["preamp"], -1.0);
        let back = parse_saved(&v["mixer"]["eq"]);
        assert_eq!(back["Game"].bands.len(), 10);
        // A file that is not an object yet still gets the key.
        let mut empty = serde_json::Value::Null;
        merge_into(&mut empty, &s);
        assert!(empty["mixer"]["eq"]["Game"].is_object());
    }

    #[test]
    fn partial_saved_state_fills_defaults() {
        let m = parse_saved(&serde_json::json!({"Aux": {"enabled": true}, "bad": 3}));
        assert_eq!(m.len(), 1);
        assert!(m["Aux"].enabled);
        assert_eq!(m["Aux"].bands, vec![0.0; 10]);
    }
}

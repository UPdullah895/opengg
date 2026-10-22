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

/// Remember `state` for `channel` (`mixer.eq.<channel>`). Coalesced and
/// off-thread, so slider drags can call it freely.
pub fn save(channel: &str, state: &EqState) {
    crate::settings::save_mixer_later(&["eq", channel], serde_json::to_value(state).unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_saved_state_fills_defaults() {
        let m = parse_saved(&serde_json::json!({"Aux": {"enabled": true}, "bad": 3}));
        assert_eq!(m.len(), 1);
        assert!(m["Aux"].enabled);
        assert_eq!(m["Aux"].bands, vec![0.0; 10]);
    }
}

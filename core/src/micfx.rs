//! Microphone processing — client side.
//!
//! The chain (RNNoise noise suppression, a gate, a compressor, each only
//! where its plugin is installed) runs in the daemon
//! (`daemon/src/audio/micfx.rs`). This side sends the user's settings,
//! reads what the system supports, and remembers the settings in
//! `ui-settings.json` (`mixer.micfx`), which the daemon reads at start.

use crate::daemon::{call_dbus, call_dbus_void, AU_IFACE, AU_PATH};

/// Apply settings (JSON in the daemon's `MicFx` shape).
pub fn apply(json: &str) -> Result<(), String> {
    call_dbus_void("SetMicFx", AU_PATH, AU_IFACE, (json,))
}

/// `{"caps": {nr, gate, comp}, "state": {...}, "running": bool}`
pub fn status() -> Result<String, String> {
    call_dbus::<String>("GetMicFx", AU_PATH, AU_IFACE, ())
}

/// Remember settings (coalesced, off-thread).
pub fn save(json: &str) {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
        crate::settings::save_mixer_later(&["micfx"], v);
    }
}

/// Saved settings as JSON, or "{}".
pub fn saved() -> String {
    let raw = crate::settings::load_ui_settings().unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);
    let fx = &v["mixer"]["micfx"];
    if fx.is_object() {
        fx.to_string()
    } else {
        "{}".into()
    }
}

//! SettingsController — generic read/write bridge over the shared
//! `~/.config/opengg/ui-settings.json` (`opengg_core::settings::{load,save}_ui_settings`),
//! backing the Settings page panels. Mirrors how `usePersistenceStore` exposes
//! `persist.state.settings` as one reactive object in the Vue app: QML reads
//! `JSON.parse(SettingsController.settingsJson)` and writes back field-by-field
//! via `setValue`, rather than each panel getting bespoke typed properties.

#[cxx_qt::bridge]
pub mod qobject {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, settings_json, cxx_name = "settingsJson")]
        type SettingsController = super::SettingsControllerRust;

        /// Reload `settingsJson` from disk.
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>);

        /// Set `settings.<key>` (or `settings.<a>.<b>` for one nested level) to
        /// `value_json` (a JSON-encoded scalar/array/object), save, and refresh.
        #[qinvokable]
        #[cxx_name = "setValue"]
        fn set_value(self: Pin<&mut Self>, key: QString, value_json: QString);

        /// Restore the built-in default keyboard shortcut bindings.
        #[qinvokable]
        #[cxx_name = "resetShortcuts"]
        fn reset_shortcuts(self: Pin<&mut Self>);

        /// The built-in bindings as JSON, so the panel can grey out "Reset to
        /// Defaults" when nothing has been changed. Returned rather than
        /// compared in Rust because the panel already re-reads `settingsJson`
        /// on every change — a bool invokable would have no change signal and
        /// would latch at its startup value.
        #[qinvokable]
        #[cxx_name = "defaultShortcutsJson"]
        fn default_shortcuts_json(self: &Self) -> QString;

        /// Put the timeline tracks back to the built-in defaults.
        #[qinvokable]
        #[cxx_name = "resetTrackDefs"]
        fn reset_track_defs(self: Pin<&mut Self>);
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;
use serde_json::{json, Value};

#[derive(Default)]
pub struct SettingsControllerRust {
    settings_json: QString,
}

/// Mirrors `DEFAULTS.settings.shortcuts` in `stores/persistence.ts`.
fn default_shortcuts() -> Value {
    json!({
        "saveReplay": "Alt+F10",
        "toggleRecording": "Alt+F9",
        "screenshot": "Alt+F12",
        "splitClip": "S",
        "exportClip": "Ctrl+E",
        "toggleMic": "Alt+M",
        "undo": "Ctrl+Z",
        "redo": "Ctrl+Shift+Z",
        "toggleEarBlast": "",
    })
}

/// Load the full on-disk envelope, or `{"settings":{}}` when absent/corrupt.
fn load_envelope() -> Value {
    let raw = opengg_core::settings::load_ui_settings().unwrap_or_default();
    let mut v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    if !v.is_object() {
        v = json!({});
    }
    if !v["settings"].is_object() {
        v["settings"] = json!({});
    }
    opengg_core::settings::ensure_required_tracks(&mut v);
    v
}

fn save_envelope(v: &Value) {
    if let Ok(s) = serde_json::to_string(v) {
        let _ = opengg_core::settings::save_ui_settings(&s);
    }
}

impl qobject::SettingsController {
    pub fn refresh(self: Pin<&mut Self>) {
        let v = load_envelope();
        let settings_str = serde_json::to_string(&v["settings"]).unwrap_or_else(|_| "{}".into());
        self.set_settings_json(QString::from(&settings_str));
    }

    pub fn set_value(self: Pin<&mut Self>, key: QString, value_json: QString) {
        let mut v = load_envelope();
        let parsed: Value =
            serde_json::from_str(&value_json.to_string()).unwrap_or(Value::Null);
        let key = key.to_string();

        if let Some((outer, inner)) = key.split_once('.') {
            if !v["settings"][outer].is_object() {
                v["settings"][outer] = json!({});
            }
            v["settings"][outer][inner] = parsed;
        } else {
            v["settings"][key] = parsed;
        }
        // The panel already refuses to delete Video/Overlays; this keeps a
        // stale or scripted write from saving a file without them.
        opengg_core::settings::ensure_required_tracks(&mut v);

        save_envelope(&v);
        self.refresh();
    }

    pub fn default_shortcuts_json(&self) -> QString {
        QString::from(&default_shortcuts().to_string())
    }

    pub fn reset_track_defs(self: Pin<&mut Self>) {
        let mut v = load_envelope();
        v["settings"]["trackDefs"] = opengg_core::settings::default_track_defs();
        save_envelope(&v);
        self.refresh();
    }

    pub fn reset_shortcuts(self: Pin<&mut Self>) {
        let mut v = load_envelope();
        v["settings"]["shortcuts"] = default_shortcuts();
        save_envelope(&v);
        self.refresh();
    }
}

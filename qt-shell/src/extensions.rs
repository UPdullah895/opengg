//! ExtensionsController — QML singleton over `opengg_core::extensions`
//! (manifest scanning, enable/disable, folder opening). Mirrors
//! `useExtensionStore`'s metadata half in the Vue app; deliberately does NOT
//! port dynamic runtime-loading of an extension's own settings UI
//! (`extStore.loadExtension()` mounts an arbitrary Vue component at runtime —
//! there is no QML/cxx-qt equivalent yet, see the migration plan doc) or the
//! Vite dev-mode hot-reload button (no equivalent build pipeline here).
//!
//! `extensionConsents` lives at the top level of the shared
//! `ui-settings.json` envelope (a sibling of `"settings"`, not nested under
//! it — see `ui-settings.json` on disk), so it is read/written directly here
//! via `opengg_core::settings::{load_ui_settings, save_ui_settings}` rather
//! than through `SettingsController`, which only ever touches the `"settings"`
//! sub-object.

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
        #[qproperty(QString, extensions_json, cxx_name = "extensionsJson")]
        #[qproperty(QString, consents_json, cxx_name = "consentsJson")]
        #[qproperty(QString, modules_json, cxx_name = "modulesJson")]
        type ExtensionsController = super::ExtensionsControllerRust;

        /// Re-scan the extensions folder and reload granted consents + module toggles.
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>);

        /// Set `modules.<key>` (audio/device/replay — currently inert, purely
        /// a persisted preference; see the "Core Modules" card doc comment
        /// in ExtensionsPanel.qml).
        #[qinvokable]
        #[cxx_name = "setModule"]
        fn set_module(self: Pin<&mut Self>, key: &QString, enabled: bool);

        /// Enable or disable an extension (writes the shared state file and
        /// best-effort notifies the daemon).
        #[qinvokable]
        #[cxx_name = "setEnabled"]
        fn set_enabled(self: Pin<&mut Self>, id: &QString, enabled: bool);

        /// Record that the user granted a daemon extension's permission
        /// request, then enable it.
        #[qinvokable]
        #[cxx_name = "grantConsent"]
        fn grant_consent(self: Pin<&mut Self>, id: &QString);

        /// Open the extensions folder in the system file manager, then refresh.
        #[qinvokable]
        #[cxx_name = "openFolder"]
        fn open_folder(self: Pin<&mut Self>);
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;
use serde_json::{json, Value};

#[derive(Default)]
pub struct ExtensionsControllerRust {
    extensions_json: QString,
    consents_json: QString,
    modules_json: QString,
}

fn load_envelope() -> Value {
    let raw = opengg_core::settings::load_ui_settings().unwrap_or_default();
    let v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    if v.is_object() {
        v
    } else {
        json!({})
    }
}

fn save_envelope(v: &Value) {
    if let Ok(s) = serde_json::to_string(v) {
        let _ = opengg_core::settings::save_ui_settings(&s);
    }
}

impl qobject::ExtensionsController {
    pub fn refresh(mut self: Pin<&mut Self>) {
        let list = opengg_core::extensions::scan_extensions().unwrap_or_default();
        let list_json = serde_json::to_string(&list).unwrap_or_else(|_| "[]".into());
        self.as_mut().set_extensions_json(QString::from(&list_json));

        let v = load_envelope();
        let consents = v
            .get("extensionConsents")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let consents_json = serde_json::to_string(&consents).unwrap_or_else(|_| "{}".into());
        self.as_mut().set_consents_json(QString::from(&consents_json));

        let modules = v.get("modules").cloned().unwrap_or_else(|| {
            json!({ "audio": true, "device": true, "replay": true })
        });
        let modules_json = serde_json::to_string(&modules).unwrap_or_else(|_| "{}".into());
        self.as_mut().set_modules_json(QString::from(&modules_json));
    }

    pub fn set_module(mut self: Pin<&mut Self>, key: &QString, enabled: bool) {
        let mut v = load_envelope();
        if !v["modules"].is_object() {
            v["modules"] = json!({ "audio": true, "device": true, "replay": true });
        }
        v["modules"][key.to_string()] = json!(enabled);
        save_envelope(&v);
        self.as_mut().refresh();
    }

    pub fn set_enabled(mut self: Pin<&mut Self>, id: &QString, enabled: bool) {
        let _ = opengg_core::extensions::set_extension_enabled(id.to_string(), enabled);
        self.as_mut().refresh();
    }

    pub fn grant_consent(mut self: Pin<&mut Self>, id: &QString) {
        let mut v = load_envelope();
        if !v["extensionConsents"].is_object() {
            v["extensionConsents"] = json!({});
        }
        v["extensionConsents"][id.to_string()] = json!(true);
        save_envelope(&v);
        self.as_mut().set_enabled(id, true);
    }

    pub fn open_folder(mut self: Pin<&mut Self>) {
        let _ = opengg_core::extensions::open_extensions_folder();
        self.as_mut().refresh();
    }
}

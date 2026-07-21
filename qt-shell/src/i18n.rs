//! I18n — a QML singleton exposing the JSON translation catalogs to QML.
//!
//! Design note (deviation from plan §3.1, recorded intentionally): the plan
//! specified a C++ `QTranslator` subclass + `qsTrId`. cxx-qt 0.9 cannot subclass
//! QTranslator in Rust, and the C++ shim is build-fragile. This equivalent
//! approach keeps ALL i18n logic in Rust (where JSON parsing is trivial) and
//! exposes it as a cxx-qt singleton: QML binds `I18n.t("nav.home")` with a
//! dependency on `I18n.language` so bindings re-evaluate live on language change.
//! User-visible behavior (live en↔ar switch, RTL, user-droppable JSON locales)
//! is identical. Catalogs are still the single source of truth (§3.1); they will
//! move to opengg-core during the core-extraction task.

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
        #[qproperty(QString, language)]
        #[qproperty(bool, rtl)]
        type I18n = super::I18nRust;

        /// Translate a dotted catalog key (e.g. "nav.home") for the current
        /// language, falling back to English, then to the key itself.
        /// Exposed to QML as `I18n.t(key)`.
        #[qinvokable]
        #[cxx_name = "t"]
        fn translate(self: &Self, key: &QString) -> QString;

        /// Switch the active language (no-op if the code is unknown). Updates
        /// `language` and `rtl`, emitting their change signals.
        #[qinvokable]
        #[cxx_name = "applyLanguage"]
        fn apply_language(self: Pin<&mut Self>, code: &QString);

        /// Human-readable display name for a language code (from `_meta.name`).
        #[qinvokable]
        #[cxx_name = "languageName"]
        fn language_name(self: &Self, code: &QString) -> QString;
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;
use std::collections::HashMap;

pub struct I18nRust {
    language: QString,
    rtl: bool,
    // Non-property internal state: flattened catalogs + metadata.
    catalogs: HashMap<String, HashMap<String, String>>,
    names: HashMap<String, String>,
    rtl_dirs: HashMap<String, bool>,
}

impl Default for I18nRust {
    fn default() -> Self {
        let (catalogs, names, rtl_dirs) = load_catalogs();
        // OPENGG_LANG allows launching directly in a given language (used for
        // verification without needing UI input); defaults to English.
        let initial = std::env::var("OPENGG_LANG")
            .ok()
            .filter(|c| catalogs.contains_key(c))
            .unwrap_or_else(|| "en".to_string());
        let rtl = *rtl_dirs.get(&initial).unwrap_or(&false);
        Self {
            language: QString::from(&initial),
            rtl,
            catalogs,
            names,
            rtl_dirs,
        }
    }
}

impl qobject::I18n {
    pub fn translate(&self, key: &QString) -> QString {
        let key = key.to_string();
        let lang = self.language.to_string();
        let value = self
            .catalogs
            .get(&lang)
            .and_then(|m| m.get(&key))
            .or_else(|| self.catalogs.get("en").and_then(|m| m.get(&key)));
        match value {
            Some(v) => QString::from(v),
            None => QString::from(&key),
        }
    }

    pub fn apply_language(mut self: Pin<&mut Self>, code: &QString) {
        let code = code.to_string();
        if !self.catalogs.contains_key(&code) {
            return;
        }
        let rtl = *self.rtl_dirs.get(&code).unwrap_or(&false);
        self.as_mut().set_language(QString::from(&code));
        self.as_mut().set_rtl(rtl);
    }

    pub fn language_name(&self, code: &QString) -> QString {
        let code = code.to_string();
        match self.names.get(&code) {
            Some(n) => QString::from(n),
            None => QString::from(&code),
        }
    }
}

type Catalogs = (
    HashMap<String, HashMap<String, String>>,
    HashMap<String, String>,
    HashMap<String, bool>,
);

fn load_catalogs() -> Catalogs {
    // Dev default: the crate's bundled locales. Override with OPENGG_LOCALES_DIR
    // (an installed share path in production; user locales are merged later).
    let dir = std::env::var("OPENGG_LOCALES_DIR")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/locales").to_string());

    let mut catalogs = HashMap::new();
    let mut names = HashMap::new();
    let mut rtl_dirs = HashMap::new();

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let code = match path.file_stem().and_then(|s| s.to_str()) {
                Some(c) => c.to_string(),
                None => continue,
            };
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let val: serde_json::Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let mut flat = HashMap::new();
            flatten(&val, String::new(), &mut flat);

            let name = val
                .get("_meta")
                .and_then(|m| m.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or(&code)
                .to_string();
            let is_rtl = val
                .get("_meta")
                .and_then(|m| m.get("dir"))
                .and_then(|v| v.as_str())
                == Some("rtl");

            names.insert(code.clone(), name);
            rtl_dirs.insert(code.clone(), is_rtl);
            catalogs.insert(code, flat);
        }
    }

    (catalogs, names, rtl_dirs)
}

/// Flatten nested catalog objects into dotted keys ("nav" -> "home" => "nav.home").
/// Skips the top-level `_meta` object; only string leaves are recorded.
fn flatten(val: &serde_json::Value, prefix: String, out: &mut HashMap<String, String>) {
    if let Some(obj) = val.as_object() {
        for (k, v) in obj {
            if prefix.is_empty() && k == "_meta" {
                continue;
            }
            let key = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            match v {
                serde_json::Value::String(s) => {
                    out.insert(key, s.clone());
                }
                serde_json::Value::Object(_) => flatten(v, key, out),
                _ => {}
            }
        }
    }
}

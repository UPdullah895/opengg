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
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
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

        /// Raw catalog subtree at a dotted key, serialized as JSON for QML to
        /// parse — the analogue of vue-i18n's `tm()`. Needed because `flatten`
        /// only keeps string leaves, so ARRAY-valued content (the dashboard
        /// changelog, and any future list) is otherwise unreachable from QML.
        /// Returns "null" when the key is absent in both the active language
        /// and the English fallback.
        #[qinvokable]
        #[cxx_name = "tRaw"]
        fn translate_raw(self: &Self, key: &QString) -> QString;

        /// Codes of all loaded languages (English first, then alphabetical),
        /// for the Settings language picker.
        #[qinvokable]
        #[cxx_name = "availableLanguages"]
        fn available_languages(self: &Self) -> QStringList;

        /// Text direction for a language code ("rtl" or "ltr"), independent of
        /// which language is currently active — the Settings language list
        /// shows this per row, not just for the selected language.
        #[qinvokable]
        #[cxx_name = "languageDir"]
        fn language_dir(self: &Self, code: &QString) -> QString;
    }
}

use core::pin::Pin;
use cxx_qt_lib::{QString, QStringList};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct I18nRust {
    language: QString,
    rtl: bool,
    // Non-property internal state: flattened catalogs + metadata.
    catalogs: HashMap<String, HashMap<String, String>>,
    /// Unflattened parsed catalogs, so `tRaw` can return arrays/objects.
    trees: HashMap<String, serde_json::Value>,
    names: HashMap<String, String>,
    rtl_dirs: HashMap<String, bool>,
}

impl Default for I18nRust {
    fn default() -> Self {
        let (catalogs, trees, names, rtl_dirs) = load_catalogs();
        // Initial language: OPENGG_LANG (verification override) → persisted
        // choice in the shared ui-settings.json → English.
        let initial = std::env::var("OPENGG_LANG")
            .ok()
            .filter(|c| catalogs.contains_key(c))
            .or_else(|| read_settings_language(&settings_path()).filter(|c| catalogs.contains_key(c)))
            .unwrap_or_else(|| "en".to_string());
        let rtl = *rtl_dirs.get(&initial).unwrap_or(&false);
        Self {
            language: QString::from(&initial),
            rtl,
            catalogs,
            trees,
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

    pub fn translate_raw(&self, key: &QString) -> QString {
        let key = key.to_string();
        let lang = self.language.to_string();
        let lookup = |code: &str| -> Option<&serde_json::Value> {
            let mut node = self.trees.get(code)?;
            for part in key.split('.') {
                node = node.get(part)?;
            }
            Some(node)
        };
        let node = lookup(&lang).or_else(|| lookup("en"));
        match node.and_then(|n| serde_json::to_string(n).ok()) {
            Some(json) => QString::from(&json),
            None => QString::from("null"),
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
        // Persist to the shared ui-settings.json so the choice survives restart
        // and stays compatible with the Tauri UI (R11 / Phase 1 acceptance).
        write_settings_language(&settings_path(), &code, rtl);
    }

    pub fn language_name(&self, code: &QString) -> QString {
        let code = code.to_string();
        match self.names.get(&code) {
            Some(n) => QString::from(n),
            None => QString::from(&code),
        }
    }

    pub fn available_languages(&self) -> QStringList {
        let mut codes: Vec<String> = self.catalogs.keys().cloned().collect();
        codes.sort();
        if let Some(pos) = codes.iter().position(|c| c == "en") {
            let en = codes.remove(pos);
            codes.insert(0, en);
        }
        codes.iter().map(QString::from).collect()
    }

    pub fn language_dir(&self, code: &QString) -> QString {
        let is_rtl = *self.rtl_dirs.get(&code.to_string()).unwrap_or(&false);
        QString::from(if is_rtl { "rtl" } else { "ltr" })
    }
}

type Catalogs = (
    HashMap<String, HashMap<String, String>>,
    HashMap<String, serde_json::Value>,
    HashMap<String, String>,
    HashMap<String, bool>,
);

fn load_catalogs() -> Catalogs {
    // Dev default: the crate's bundled locales. Override with OPENGG_LOCALES_DIR
    // (an installed share path in production; user locales are merged later).
    let dir = std::env::var("OPENGG_LOCALES_DIR")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/locales").to_string());

    let mut catalogs = HashMap::new();
    let mut trees = HashMap::new();
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
            trees.insert(code.clone(), val);
            catalogs.insert(code, flat);
        }
    }

    (catalogs, trees, names, rtl_dirs)
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

/// Path to the UI settings file shared with the Tauri app
/// (`$XDG_CONFIG_HOME/opengg/ui-settings.json`, falling back to `~/.config`).
fn settings_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    base.join("opengg").join("ui-settings.json")
}

/// Read `settings.language` from the shared settings file, if present.
fn read_settings_language(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("settings")?
        .get("language")?
        .as_str()
        .map(|s| s.to_string())
}

/// Write `settings.language` and `settings.rtlMode` into the shared settings
/// file, **preserving every other key** (reads the whole document, edits only
/// those two fields, writes it back). Creates the file if absent.
fn write_settings_language(path: &Path, code: &str, rtl: bool) {
    let mut root: serde_json::Value = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(|v: &serde_json::Value| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}));

    let obj = root.as_object_mut().expect("root is an object");
    let settings = obj
        .entry("settings")
        .or_insert_with(|| serde_json::json!({}));
    if !settings.is_object() {
        *settings = serde_json::json!({});
    }
    let s = settings.as_object_mut().expect("settings is an object");
    s.insert("language".into(), serde_json::Value::String(code.to_string()));
    s.insert("rtlMode".into(), serde_json::Value::Bool(rtl));

    if let Ok(text) = serde_json::to_string_pretty(&root) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::{read_settings_language, write_settings_language};

    #[test]
    fn language_round_trips_and_preserves_other_keys() {
        let dir = std::env::temp_dir().join(format!("opengg-i18n-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ui-settings.json");
        std::fs::write(
            &path,
            r#"{"_schemaVersion":3,"settings":{"language":"en","tutorialSeen":true},"mixer":{"vol":7}}"#,
        )
        .unwrap();

        write_settings_language(&path, "ar", true);

        assert_eq!(read_settings_language(&path).as_deref(), Some("ar"));
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["settings"]["rtlMode"], serde_json::json!(true));
        // untouched keys survive
        assert_eq!(v["settings"]["tutorialSeen"], serde_json::json!(true));
        assert_eq!(v["_schemaVersion"], serde_json::json!(3));
        assert_eq!(v["mixer"]["vol"], serde_json::json!(7));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_is_created_with_settings() {
        let dir = std::env::temp_dir().join(format!("opengg-i18n-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("ui-settings.json");

        write_settings_language(&path, "ar", true);

        assert_eq!(read_settings_language(&path).as_deref(), Some("ar"));
        std::fs::remove_dir_all(&dir).ok();
    }
}

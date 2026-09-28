//! Extension discovery and management.
//!
//! Handles reading extension manifests from `~/.local/share/opengg/extensions/`,
//! managing per-extension enable state, and fetching the remote registry index.

use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

/// Developer guide written to the extensions folder on first open.
const EXTENSIONS_GUIDE: &str = r#"# How to Create an OpenGG Extension

Each extension is a self-contained folder placed here. An extension can add a
**UI part** (an IIFE bundle that registers a Vue 3 component on `window.__ext_<id>`),
a **background part** (an executable the daemon runs and supervises, via the
`daemon` field), or both.

> The complete, authoritative authoring contract — including the daemon model and
> the `window.opengg` whitelist — lives in `AGENTS.md` in the OpenGG
> `extension-template/`. Building with AI? Use `PROMPT.md` from the same template.
> Quickest start of all: run `make new-extension NAME=my-extension` in the repo.

---

## Directory Structure

```
~/.local/share/opengg/extensions/
  my-extension/
    manifest.json        ← required — metadata + capability declarations
    index.iife.js        ← IIFE bundle built with Vite/Rollup
    icon.svg             ← optional — shown in Settings → Extensions
    locales/
      en.json            ← optional — i18n strings for this extension
      ar.json
```

The folder name is treated as the extension's `id`.

---

## manifest.json Schema

```json
{
  "id":          "my-extension",
  "name":        "My Extension",
  "description": "A one-line description shown in Settings → Extensions.",
  "version":     "1.0.0",
  "author":      "Your Name",
  "icon":        "assets/icon.svg",
  "main":        "dist/index.iife.js",
  "hasSettings": true,
  "daemon":      "bin/my-daemon"
}
```

| Field          | Required | Description |
|----------------|----------|-------------|
| `id`           | ✓        | Unique kebab-case identifier. |
| `name`         | ✓        | Display name. |
| `description`  | ✗        | Short description (≤ 120 chars). |
| `version`      | ✗        | SemVer string e.g. `"1.0.0"`. |
| `author`       | ✗        | Author name or handle. |
| `icon`         | ✗        | Icon path relative to the extension root (SVG/PNG). |
| `main`         | ✗        | UI part — IIFE bundle path. Omit for daemon-only extensions. |
| `hasSettings`  | ✗        | Set `true` to show a gear button that opens your settings panel. |
| `daemon`       | ✗        | Background part — path to a `chmod +x` executable the daemon runs & supervises. Omit for UI-only extensions. |

---

## IIFE Bundle Pattern

Your bundle must set `window.__ext_<id>` (dashes → underscores in the key):

```js
// index.iife.js
(function () {
  const { defineComponent, ref, h } = window.Vue;

  const SettingsPanel = defineComponent({
    name: 'MyExtSettings',
    setup() {
      const count = ref(0);
      return () => h('div', { style: 'padding:16px' }, [
        h('p', `Count: ${count.value}`),
        h('button', { onClick: () => count.value++ }, 'Increment'),
      ]);
    },
  });

  // Extension id "my-extension" → global key "__ext_my_extension"
  window.__ext_my_extension = {
    settingsComponent: SettingsPanel,
  };
})();
```

`window.Vue` is populated by OpenGG before any extension loads and exposes the
full Vue 3 composition API (`ref`, `computed`, `defineComponent`, `h`, …).

---

## Extension API — window.opengg

OpenGG exposes a restricted bridge for read-only Tauri commands:

```js
const clips = await window.opengg.invoke('get_clip_list');
const port  = window.opengg.mediaPort;  // local media-server port
```

Only a whitelist of non-destructive commands are allowed. Calling a command
not on the whitelist returns a rejected promise with an explanatory error.

---

## Locales

Place `locales/<lang>.json` files alongside `manifest.json`.
OpenGG merges them into the running vue-i18n instance under the namespace
`ext.<your-extension-id>.*` so strings don't collide with core translations.

```json
// locales/en.json
{
  "settingsTitle": "My Extension Settings",
  "countLabel":    "Count"
}
```

Inside your component access them via the injected i18n instance or
`window.Vue.inject('$i18n')`.

---

## Build Setup (Vite)

```js
// vite.config.js
export default {
  build: {
    lib: {
      entry: 'src/index.ts',
      name:  'MyExt',
      formats: ['iife'],
      fileName: () => 'index.iife.js',
    },
    rollupOptions: {
      // Exclude Vue from the bundle — OpenGG provides it via window.Vue
      external: ['vue'],
      output: { globals: { vue: 'Vue' } },
    },
  },
};
```

See `extension-template/` in the OpenGG repository for a complete starter.
"#;

pub fn extensions_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("opengg/extensions")
}

/// Shared enable-state file (`{ "<id>": bool }`, absent ⇒ enabled). Read and
/// written by both this Tauri layer and the daemon's `ExtensionManager`.
fn ext_state_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("opengg/extensions.json")
}

fn load_ext_enabled_map() -> HashMap<String, bool> {
    std::fs::read_to_string(ext_state_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[derive(Serialize)]
pub struct ExtensionInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub path: String,
    #[serde(default)]
    pub has_settings: bool,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub ui: Option<String>,
    /// Whether the extension is enabled (per the shared state file; default true).
    #[serde(default)]
    pub enabled: bool,
    /// True if the manifest declares a `daemon` (background) executable part.
    #[serde(default)]
    pub has_daemon: bool,
    /// Permission tiers declared by the manifest (empty = legacy all-read behavior).
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Creates `~/.local/share/opengg/extensions/` if needed, (re)writes the
/// auto-generated developer guide so it stays current, then opens the folder in
/// the file manager.
pub fn open_extensions_folder() -> Result<String, String> {
    let dir = extensions_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create dir: {e}"))?;

    // Always refresh the generated guide — it is not meant to be user-edited, and
    // overwriting ensures existing installs pick up new docs (e.g. the daemon field).
    let guide = dir.join("HOW_TO_CREATE_EXTENSIONS.md");
    std::fs::write(&guide, EXTENSIONS_GUIDE).map_err(|e| format!("write guide: {e}"))?;

    let path_str = dir.to_string_lossy().to_string();
    open::that(&dir).map_err(|e| format!("open folder: {e}"))?;
    Ok(path_str)
}

/// Scans `~/.local/share/opengg/extensions/` for subdirectories containing a
/// `manifest.json`. Returns the parsed metadata for each valid extension.
pub fn scan_extensions() -> Result<Vec<ExtensionInfo>, String> {
    let dir = extensions_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut exts = Vec::new();
    let enabled_map = load_ext_enabled_map();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("read dir: {e}"))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }

        let raw = match std::fs::read_to_string(&manifest_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let v: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue, // skip malformed manifests silently
        };

        let id = v["id"].as_str().unwrap_or("").to_string();
        let name = v["name"].as_str().unwrap_or(&id).to_string();
        if id.is_empty() || name.is_empty() {
            continue;
        }

        let enabled = *enabled_map.get(&id).unwrap_or(&true);
        let has_daemon = v["daemon"].as_str().map(|s| !s.is_empty()).unwrap_or(false);

        // Extract permissions array from manifest (optional field; empty = legacy behavior)
        let permissions: Vec<String> = v["permissions"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        exts.push(ExtensionInfo {
            id,
            name,
            description: v["description"].as_str().unwrap_or("").to_string(),
            version: v["version"].as_str().unwrap_or("0.0.0").to_string(),
            path: path.to_string_lossy().to_string(),
            has_settings: v["hasSettings"].as_bool().unwrap_or(false),
            icon: v["icon"].as_str().map(|s| s.to_string()),
            main: v["main"].as_str().map(|s| s.to_string()),
            ui: v["ui"].as_str().map(|s| s.to_string()),
            enabled,
            has_daemon,
            permissions,
        });
    }

    exts.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(exts)
}

/// Enable or disable an extension. Writes the shared `extensions.json` state file
/// (the single source of truth, read by both this layer and the daemon) and then
/// best-effort asks the daemon to start/stop the extension's background part live.
pub fn set_extension_enabled(id: String, enabled: bool) -> Result<(), String> {
    // 1. Persist to the shared state file so the decision survives even if the
    //    daemon isn't running (the frontend reads `enabled` from `scan_extensions`).
    let mut map = load_ext_enabled_map();
    map.insert(id.clone(), enabled);
    let path = ext_state_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
    }
    let json = serde_json::to_string_pretty(&map).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("write state: {e}"))?;

    // 2. Tell the daemon to start/stop the daemon part now (no restart). Best-effort:
    //    a UI-only extension or an absent daemon simply makes this a no-op.
    let _ = crate::daemon::call_dbus_void("SetEnabled", crate::daemon::EX_PATH, crate::daemon::EX_IFACE, (id.as_str(), enabled));
    Ok(())
}

/// Fetches the extension registry index from a remote URL.
/// Validates the JSON structure and returns the parsed index.
///
/// # Arguments
/// - `url` (optional): Custom registry URL. Defaults to the main OpenGG registry.
///
/// # Returns
/// The parsed registry index as a JSON value, or an error (offline, timeout, bad JSON, etc.)
///
/// # Security
/// - Fetches remote JSON for display only (no auto-install)
/// - Enforces 1 MB size cap to prevent memory exhaustion
/// - Validates structure before returning to frontend
pub async fn fetch_extension_registry(url: Option<String>) -> Result<serde_json::Value, String> {
    const DEFAULT_REGISTRY_URL: &str = "https://raw.githubusercontent.com/UPdullah895/opengg/main/registry/index.json";
    const SIZE_CAP: u64 = 1024 * 1024; // 1 MB
    const TIMEOUT_SECS: u64 = 10;

    let fetch_url = url.as_deref().unwrap_or(DEFAULT_REGISTRY_URL);

    // Create an async HTTP client with timeout and size limits
    let client = reqwest::ClientBuilder::new()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    // Fetch the registry index
    let response = client
        .get(fetch_url)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "Network timeout: registry server did not respond within 10 seconds".to_string()
            } else if e.is_connect() {
                "Network error: unable to reach registry server (offline?)".to_string()
            } else {
                format!("Network error: {e}")
            }
        })?;

    // Check Content-Length to avoid excessive memory use
    if let Some(content_length) = response.content_length() {
        if content_length > SIZE_CAP {
            return Err(format!(
                "Registry index too large: {} bytes (max {} MB)",
                content_length,
                SIZE_CAP / (1024 * 1024)
            ));
        }
    }

    // Read response body
    let body = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    if body.len() > SIZE_CAP as usize {
        return Err(format!(
            "Registry index too large: {} bytes (max {} MB)",
            body.len(),
            SIZE_CAP / (1024 * 1024)
        ));
    }

    // Parse JSON
    let parsed: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| format!("Invalid JSON: {e}"))?;

    // Validate registry version and structure
    match parsed.get("version").and_then(|v| v.as_number()) {
        Some(v) if v.is_u64() => {
            if v.as_u64().unwrap_or(0) != 1 {
                return Err(format!(
                    "Unsupported registry version: {} (expected 1)",
                    v
                ));
            }
        }
        _ => return Err("Missing or invalid 'version' field in registry".to_string()),
    }

    if !parsed.get("extensions").map(|e| e.is_array()).unwrap_or(false) {
        return Err("Missing or invalid 'extensions' array in registry".to_string());
    }

    Ok(parsed)
}

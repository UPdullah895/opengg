//! Settings persistence logic.
//!
//! Ported verbatim (behavior-preserving) from `frontend/src/stores/persistence.ts`
//! per plan §2.1 rule 4 ("frontend-store logic that is actually business logic
//! must move to core with its tests"). The JS operated on untyped objects, so the
//! Rust port works on `serde_json::Value` to keep parity with the on-disk
//! `ui-settings.json` shape shared across both UIs.

use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

/// Current settings schema version. Migrations run sequentially up to this.
/// Keep in lockstep with `CURRENT_SCHEMA_VERSION` in `persistence.ts`.
pub const CURRENT_SCHEMA_VERSION: i64 = 12;

// ── On-disk settings IO (moved verbatim from src-tauri/commands.rs) ──
//
// Canonical paths + read/write for theme.json, ui-settings.json, and the
// user-droppable locales directory. These are shared verbatim by the Tauri
// host and the Qt shell; each stack keeps only a thin wrapper.

/// `$XDG_CONFIG_HOME` (or `~/.config`) root.
fn config_root() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"))
}

/// `~/.config/opengg/theme.json`.
pub fn theme_path() -> PathBuf {
    config_root().join("opengg/theme.json")
}

/// `~/.config/opengg/ui-settings.json`.
pub fn settings_path() -> PathBuf {
    config_root().join("opengg/ui-settings.json")
}

/// `~/.config/opengg/locales/` — user-droppable runtime locale catalogs.
pub fn locales_dir() -> PathBuf {
    config_root().join("opengg/locales")
}

/// `~/.config/opengg/button-hotspots.json` — per-device button-hotspot
/// placements for the Devices roadmap Phase 5 button-mapping editor (see
/// [`crate::button_hotspots`]). Same directory as `theme.json`/
/// `ui-settings.json`, its own file — this is structured per-device data
/// keyed by vendor:product, not a flat UI preference, matching how
/// `daemon/src/device/identity_overrides.rs`'s
/// `device-identity-overrides.json` is also its own file alongside (not
/// folded into) the daemon's own config.
pub fn button_hotspots_path() -> PathBuf {
    config_root().join("opengg/button-hotspots.json")
}

/// Fallback theme served when no `theme.json` exists yet.
pub const DEFAULT_THEME_JSON: &str =
    "{\"colors\":{\"--accent\":\"#E94560\"},\"layout\":{\"--clips-grid-cols\":\"4\"}}";

// ── Safe persistence: atomic writes, corrupt-file quarantine, one lock ──
//
// Both files used to be read with "any failure == empty" and written with a
// truncating `fs::write`. A crash mid-write left a partial file, the next load
// treated it as empty, and the next toggle overwrote it — wiping every
// shortcut, track and route with no trace. The rules now:
//   * writes go through `atomic_write` (same-dir temp + fsync + rename);
//   * a file that does not parse is MOVED ASIDE to `<name>.corrupt-<ts>`
//     (never deleted), an ERROR is logged and recorded for the UI, and the
//     caller starts from defaults;
//   * every read-modify-write of ui-settings.json holds `SETTINGS_LOCK`, so the
//     UI thread and worker threads cannot lose each other's updates.

static SETTINGS_LOCK: Mutex<()> = Mutex::new(());
static THEME_LOCK: Mutex<()> = Mutex::new(());
/// Sticky for the session: the file that was quarantined on load.
static SETTINGS_LOAD_ERROR: Mutex<String> = Mutex::new(String::new());
/// Cleared by the next successful save.
static SETTINGS_SAVE_ERROR: Mutex<String> = Mutex::new(String::new());
static THEME_LOAD_ERROR: Mutex<String> = Mutex::new(String::new());
static THEME_SAVE_ERROR: Mutex<String> = Mutex::new(String::new());
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding the lock must not take settings down with it.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn set_msg(slot: &Mutex<String>, msg: &str) {
    *lock(slot) = msg.to_string();
}

fn either(a: &Mutex<String>, b: &Mutex<String>) -> String {
    let a = lock(a).clone();
    if a.is_empty() {
        lock(b).clone()
    } else {
        a
    }
}

/// Human-readable problem with `ui-settings.json` (unreadable file that was
/// set aside, or a failed save); empty when everything is fine.
pub fn settings_error() -> String {
    either(&SETTINGS_LOAD_ERROR, &SETTINGS_SAVE_ERROR)
}

/// Same as [`settings_error`] for `theme.json`.
pub fn theme_error() -> String {
    either(&THEME_LOAD_ERROR, &THEME_SAVE_ERROR)
}

/// Write `data` to `path` so a reader (or a crash) only ever sees the old or
/// the new complete file: temp file in the same directory, fsync, rename, then
/// a best-effort fsync of the directory. Creates the parent directory.
pub fn atomic_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(
        ".{name}.tmp-{}-{}",
        std::process::id(),
        TMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// Move `path` to `<path>.corrupt-<unix-ts>` (suffixing `-N` on collision) and
/// return the new location. The file is kept, never deleted.
pub fn quarantine(path: &Path) -> std::io::Result<PathBuf> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let base = path.as_os_str().to_string_lossy().into_owned();
    let mut dest = PathBuf::from(format!("{base}.corrupt-{ts}"));
    let mut n = 1;
    while dest.exists() {
        dest = PathBuf::from(format!("{base}.corrupt-{ts}-{n}"));
        n += 1;
    }
    std::fs::rename(path, &dest)?;
    Ok(dest)
}

/// Set an unparseable `path` aside, log at ERROR, and return the message.
fn quarantine_for_user(path: &Path, why: &str) -> String {
    let shown = path.display();
    let m = match quarantine(path) {
        Ok(dest) => format!(
            "{shown} could not be read ({why}); it was kept as {} and defaults are in use",
            dest.display()
        ),
        Err(e) => format!(
            "{shown} could not be read ({why}) and could not be set aside ({e}); defaults are in use for now"
        ),
    };
    log::error!("{m}");
    m
}

/// Read `theme.json`, or the built-in default when absent or unreadable.
pub fn load_theme() -> Result<String, String> {
    let _g = lock(&THEME_LOCK);
    load_theme_at(&theme_path())
}

fn load_theme_at(p: &Path) -> Result<String, String> {
    let raw = match std::fs::read_to_string(p) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(DEFAULT_THEME_JSON.into()),
        Err(e) => {
            let m = format!("{} could not be read: {e}", p.display());
            log::error!("{m}");
            set_msg(&THEME_LOAD_ERROR, &m);
            return Err(m);
        }
    };
    match serde_json::from_str::<Value>(&raw) {
        Ok(v) if v.is_object() => Ok(raw),
        Ok(_) => {
            set_msg(&THEME_LOAD_ERROR, &quarantine_for_user(p, "not a JSON object"));
            Ok(DEFAULT_THEME_JSON.into())
        }
        Err(e) => {
            set_msg(&THEME_LOAD_ERROR, &quarantine_for_user(p, &e.to_string()));
            Ok(DEFAULT_THEME_JSON.into())
        }
    }
}

/// Write `theme.json` atomically (creating the parent directory if needed).
pub fn save_theme(theme_json: &str) -> Result<(), String> {
    let _g = lock(&THEME_LOCK);
    let p = theme_path();
    match atomic_write(&p, theme_json.as_bytes()) {
        Ok(()) => {
            set_msg(&THEME_SAVE_ERROR, "");
            Ok(())
        }
        Err(e) => {
            let m = format!("could not save {}: {e}", p.display());
            log::error!("{m}");
            set_msg(&THEME_SAVE_ERROR, &m);
            Err(m)
        }
    }
}

/// Write `ui-settings.json` atomically (creating the parent directory if needed).
pub fn save_ui_settings(settings_json: &str) -> Result<(), String> {
    let _g = lock(&SETTINGS_LOCK);
    save_settings_locked(&settings_path(), settings_json)
}

fn save_settings_locked(p: &Path, settings_json: &str) -> Result<(), String> {
    match atomic_write(p, settings_json.as_bytes()) {
        Ok(()) => {
            set_msg(&SETTINGS_SAVE_ERROR, "");
            Ok(())
        }
        Err(e) => {
            let m = format!("could not save {}: {e}", p.display());
            log::error!("{m}");
            set_msg(&SETTINGS_SAVE_ERROR, &m);
            Err(m)
        }
    }
}

/// Read `ui-settings.json`, or the string `"null"` when absent (or when it was
/// unreadable and has been set aside). Settings written by an older schema are
/// migrated and the result persisted first, so every caller sees current-shape
/// data and migrations run once per file, not once per read.
pub fn load_ui_settings() -> Result<String, String> {
    let _g = lock(&SETTINGS_LOCK);
    load_settings_at(&settings_path())
}

fn load_settings_at(p: &Path) -> Result<String, String> {
    let raw = match std::fs::read_to_string(p) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok("null".into()),
        Err(e) => {
            // Unreadable is not the same as corrupt (permissions, I/O error):
            // don't move anything, just report it.
            let m = format!("{} could not be read: {e}", p.display());
            log::error!("{m}");
            set_msg(&SETTINGS_LOAD_ERROR, &m);
            return Err(m);
        }
    };
    let mut v: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            set_msg(&SETTINGS_LOAD_ERROR, &quarantine_for_user(p, &e.to_string()));
            return Ok("null".into());
        }
    };
    if v.is_null() {
        return Ok(raw);
    }
    if !v.is_object() {
        set_msg(&SETTINGS_LOAD_ERROR, &quarantine_for_user(p, "not a JSON object"));
        return Ok("null".into());
    }
    let saved = v.get("_schemaVersion").and_then(Value::as_i64).unwrap_or(0);
    // Only older files: one from a NEWER build keeps its version, since
    // stamping it down would make that build re-run migrations it already did.
    if saved >= CURRENT_SCHEMA_VERSION {
        return Ok(raw);
    }
    let backup = PathBuf::from(format!("{}.bak-v{saved}", p.display()));
    if !backup.exists() {
        if let Err(e) = std::fs::copy(p, &backup) {
            log::warn!("could not back up {} before migrating: {e}", p.display());
        }
    }
    run_migrations(&mut v);
    log::info!("migrated {} from schema v{saved} to v{CURRENT_SCHEMA_VERSION}", p.display());
    let out = serde_json::to_string(&v).map_err(|e| format!("{e}"))?;
    // A failed persist is logged and recorded by save_settings_locked; the
    // migrated data is still returned so this session works.
    let _ = save_settings_locked(p, &out);
    Ok(out)
}

/// Load -> modify -> save of the whole `ui-settings.json` envelope under the
/// process-wide settings lock, so concurrent writers (UI thread + workers)
/// cannot overwrite each other's keys. `f` receives an object that always has
/// an object at `"settings"`. Prefer this over `load_ui_settings` +
/// `save_ui_settings` for any read-modify-write.
pub fn update_ui_settings(f: impl FnOnce(&mut Value)) -> Result<(), String> {
    let _g = lock(&SETTINGS_LOCK);
    update_settings_at(&settings_path(), f)
}

fn update_settings_at(p: &Path, f: impl FnOnce(&mut Value)) -> Result<(), String> {
    let raw = load_settings_at(p)?;
    let mut v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    if !v.is_object() {
        v = json!({});
    }
    if !v["settings"].is_object() {
        v["settings"] = json!({});
    }
    f(&mut v);
    if v.get("_schemaVersion").is_none() {
        v["_schemaVersion"] = json!(CURRENT_SCHEMA_VERSION);
    }
    let out = serde_json::to_string(&v).map_err(|e| format!("{e}"))?;
    save_settings_locked(p, &out)
}

/// Ensure `~/.config/opengg/locales/` exists, seed it with the bundled English
/// template (`en_template`) if `en.json` is missing, open it in the system file
/// manager, and return the directory path.
///
/// The template is passed in rather than `include_str!`'d here so this crate
/// stays decoupled from any UI's locale assets (plan §2.3): the Tauri wrapper
/// supplies `frontend/src/locales/en.json`, the Qt wrapper its own catalog.
pub fn open_locales_folder(en_template: &str) -> Result<String, String> {
    let dir = locales_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create dir: {e}"))?;

    let template_path = dir.join("en.json");
    if !template_path.exists() {
        std::fs::write(&template_path, en_template)
            .map_err(|e| format!("write en.json template: {e}"))?;
    }

    let path_str = dir.to_string_lossy().to_string();
    open::that(&dir).map_err(|e| format!("open folder: {e}"))?;
    Ok(path_str)
}

/// A user-supplied locale catalog: its language `code` (filename stem) and the
/// raw JSON `json_content` for the frontend to parse and register.
#[derive(serde::Serialize)]
pub struct UserLocale {
    pub code: String,
    pub json_content: String,
}

/// Read every `*.json` in `~/.config/opengg/locales/` and return their raw
/// content keyed by filename stem.
pub fn list_user_locales() -> Result<Vec<UserLocale>, String> {
    let dir = locales_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut locales = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("read dir: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let code = match path.file_stem().and_then(|s| s.to_str()) {
            Some(c) if !c.is_empty() => c.to_string(),
            _ => continue,
        };
        if let Ok(content) = std::fs::read_to_string(&path) {
            locales.push(UserLocale {
                code,
                json_content: content,
            });
        }
    }
    Ok(locales)
}

/// Deep-merge `b` into `a`, mirroring `deepMerge` in `persistence.ts`.
///
/// Semantics carried over from the JS (including its quirks):
/// - `b == null` → returns `a` unchanged.
/// - if either side is not a JS "object" (i.e. a scalar) → returns `b`.
/// - arrays are **replaced wholesale**, never merged.
/// - objects merge key-by-key, recursing only when both sides at a key are
///   JS "objects" (which, per `typeof`, includes arrays and null).
pub fn deep_merge(a: Value, b: Value) -> Value {
    if b.is_null() {
        return a;
    }
    // `typeof x === 'object'` in JS is true for objects, arrays, and null.
    // null is already handled above for `b`; `a` being null lands here as an
    // "object" and produces an empty base map below (spread of null == {}).
    if !is_js_object(&a) || !is_js_object(&b) {
        return b;
    }
    if a.is_array() {
        return b;
    }
    let mut r = match a {
        Value::Object(map) => map,
        // `a` was JS-object-ish but not a real object (e.g. null) → `{...a}` == {}
        _ => serde_json::Map::new(),
    };
    if let Value::Object(bmap) = b {
        for (k, bv) in bmap {
            let merged = match r.get(k.as_str()) {
                Some(rv) if is_js_object(rv) && is_js_object(&bv) => {
                    deep_merge(rv.clone(), bv)
                }
                _ => bv,
            };
            r.insert(k, merged);
        }
    }
    Value::Object(r)
}

/// JS `typeof v === 'object'`: true for objects, arrays, and null.
fn is_js_object(v: &Value) -> bool {
    v.is_object() || v.is_array() || v.is_null()
}

/// Run all pending schema migrations on a parsed settings object, in place.
/// Mirrors `runMigrations` in `persistence.ts`: applies migrations
/// `(savedVersion + 1)..=CURRENT_SCHEMA_VERSION` sequentially, then stamps the
/// current schema version. A failing migration is skipped (JS swallowed the
/// throw with a `console.warn`); here migrations are written to be infallible.
pub fn run_migrations(parsed: &mut Value) {
    let saved_version = parsed
        .get("_schemaVersion")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    for v in (saved_version + 1)..=CURRENT_SCHEMA_VERSION {
        migrate(v, parsed);
    }
    if let Value::Object(map) = parsed {
        map.insert("_schemaVersion".into(), json!(CURRENT_SCHEMA_VERSION));
    }
}

/// Mutable borrow of `parsed.settings` as an object, if present.
fn settings_mut(s: &mut Value) -> Option<&mut serde_json::Map<String, Value>> {
    s.get_mut("settings").and_then(Value::as_object_mut)
}

/// The default timeline tracks, in order (from `DEFAULTS.settings.trackDefs`
/// in `persistence.ts`). Also what Settings → Timeline Tracks' "Reset to
/// defaults" restores.
pub fn default_track_defs() -> Value {
    json!([
        { "id": "O1", "name": "Overlays", "color": "#f97316", "icon": "overlay", "visible": true },
        { "id": "V1", "name": "Video",    "color": "#E94560", "icon": "video",   "visible": true },
        { "id": "A1", "name": "Audio 1",  "color": "#10b981", "icon": "game",    "visible": true },
        { "id": "A2", "name": "Audio 2",  "color": "#3b82f6", "icon": "chat",    "visible": true },
        { "id": "A3", "name": "Audio 3",  "color": "#f59e0b", "icon": "mic",     "visible": true },
        { "id": "A4", "name": "Audio 4",  "color": "#8b5cf6", "icon": "media",   "visible": true },
        { "id": "A5", "name": "Audio 5",  "color": "#ec4899", "icon": "media",   "visible": true }
    ])
}

/// One default track definition by id — needed by migrations 5 and 6.
fn default_track_def(id: &str) -> Option<Value> {
    let defs = default_track_defs();
    defs.as_array()
        .unwrap()
        .iter()
        .find(|d| d.get("id").and_then(Value::as_str) == Some(id))
        .cloned()
}

/// Apply a single numbered migration in place. No-ops (per the JS optional
/// chaining) when the referenced fields are absent.
fn migrate(version: i64, s: &mut Value) {
    match version {
        1 => {
            // clipsFolder → clip_directories
            if let Some(settings) = settings_mut(s) {
                let clips_folder = settings
                    .get("clipsFolder")
                    .and_then(Value::as_str)
                    .filter(|f| !f.is_empty())
                    .map(str::to_string);
                let has_dirs = settings
                    .get("clip_directories")
                    .and_then(Value::as_array)
                    .map(|a| !a.is_empty())
                    .unwrap_or(false);
                if let Some(folder) = clips_folder {
                    if !has_dirs {
                        settings.insert("clip_directories".into(), json!([folder]));
                    }
                }
                settings.remove("clipsFolder");
                settings.remove("clipSources");
            }
        }
        2 => {
            // screenshotDir (string) → screenshotDirs (array)
            if let Some(settings) = settings_mut(s) {
                let dir = settings
                    .get("screenshotDir")
                    .and_then(Value::as_str)
                    .filter(|d| !d.is_empty())
                    .map(str::to_string);
                let has_dirs = settings
                    .get("screenshotDirs")
                    .and_then(Value::as_array)
                    .map(|a| !a.is_empty())
                    .unwrap_or(false);
                if let Some(d) = dir {
                    if !has_dirs {
                        settings.insert("screenshotDirs".into(), json!([d]));
                    }
                }
                settings.remove("screenshotDir");
            }
        }
        3 => {
            // drop stale showTrackIcons
            if let Some(settings) = settings_mut(s) {
                settings.remove("showTrackIcons");
            }
        }
        4 => {
            // fix stale gsrMonitorTarget values (pure resolution / spaced values)
            if let Some(settings) = settings_mut(s) {
                if let Some(target) = settings.get("gsrMonitorTarget").and_then(Value::as_str) {
                    if is_resolution_like(target) || target.contains(' ') {
                        settings.insert("gsrMonitorTarget".into(), json!("screen"));
                    }
                }
            }
        }
        5 => {
            // ensure O1 (Overlays) exists and is positioned before V1 (Video)
            if let Some(defs) = settings_mut(s)
                .and_then(|st| st.get_mut("trackDefs"))
                .and_then(Value::as_array_mut)
            {
                let o1i = index_of_id(defs, "O1");
                let v1i = index_of_id(defs, "V1");
                if o1i.is_none() {
                    if let Some(o1_def) = default_track_def("O1") {
                        defs.insert(v1i.unwrap_or(0), o1_def);
                    }
                } else if let (Some(o1i), Some(v1i)) = (o1i, v1i) {
                    if o1i > v1i {
                        let o1 = defs.remove(o1i);
                        defs.insert(v1i, o1);
                    }
                }
            }
        }
        6 => {
            // ensure A1 exists
            if let Some(defs) = settings_mut(s)
                .and_then(|st| st.get_mut("trackDefs"))
                .and_then(Value::as_array_mut)
            {
                if index_of_id(defs, "A1").is_none() {
                    if let Some(a1_def) = default_track_def("A1") {
                        let pos = index_of_id(defs, "V1").map(|i| i + 1).unwrap_or(defs.len());
                        defs.insert(pos, a1_def);
                    }
                }
            }
        }
        7 => {
            // ensure all tracks have the visible field
            if let Some(defs) = settings_mut(s)
                .and_then(|st| st.get_mut("trackDefs"))
                .and_then(Value::as_array_mut)
            {
                for def in defs.iter_mut() {
                    if let Value::Object(map) = def {
                        if !map.contains_key("visible") {
                            map.insert("visible".into(), json!(true));
                        }
                    }
                }
            }
        }
        8 => {
            // schema version field introduced — no data changes, just bookkeeping
            if let Value::Object(map) = s {
                map.insert("_schemaVersion".into(), json!(CURRENT_SCHEMA_VERSION));
            }
        }
        9 => {
            // ★ Ear Blast Protection defaults
            let has_ear_blast = s
                .get("mixer")
                .and_then(|m| m.get("earBlast"))
                .map(|v| !v.is_null())
                .unwrap_or(false);
            if !has_ear_blast {
                if !s.get("mixer").map(Value::is_object).unwrap_or(false) {
                    if let Value::Object(map) = s {
                        map.insert("mixer".into(), json!({}));
                    }
                }
                if let Some(mixer) = s.get_mut("mixer").and_then(Value::as_object_mut) {
                    mixer.insert(
                        "earBlast".into(),
                        json!({ "enabled": false, "channels": ["Game"], "threshold": 85, "target": 60 }),
                    );
                }
            }
        }
        10 => {
            // Normalize legacy "connector|resolution" gsrMonitorTarget composites.
            if let Some(settings) = settings_mut(s) {
                if let Some(t) = settings.get("gsrMonitorTarget").and_then(Value::as_str) {
                    let t = t.to_string();
                    let connector = t.split('|').next().unwrap_or("").trim().to_string();
                    if connector.is_empty()
                        || connector.starts_with(|c: char| c.is_ascii_digit())
                        || connector.contains(' ')
                    {
                        settings.insert("gsrMonitorTarget".into(), json!("screen"));
                    } else if connector != t {
                        settings.insert("gsrMonitorTarget".into(), json!(connector));
                    }
                }
            }
        }
        11 => {
            // captureTracks store the real PipeWire source node.name; convert legacy
            // friendly labels to the OpenGG channel monitor node names.
            if let Some(tracks) = settings_mut(s)
                .and_then(|st| st.get_mut("captureTracks"))
                .and_then(Value::as_array_mut)
            {
                const LEGACY: [&str; 5] = ["Game", "Chat", "Media", "Aux", "Mic"];
                for tr in tracks.iter_mut() {
                    if let Some(source) = tr.get("source").and_then(Value::as_str) {
                        if LEGACY.contains(&source) {
                            let new_source = format!("OpenGG_{source}.monitor");
                            if let Value::Object(map) = tr {
                                map.insert("source".into(), json!(new_source));
                            }
                        }
                    }
                }
            }
        }
        12 => {
            // Mark existing users as having already picked a language.
            if let Some(settings) = settings_mut(s) {
                settings.insert("languagePicked".into(), json!(true));
            }
        }
        _ => {}
    }
}

/// Mirrors `/^\d+x\d+/` — a value that begins with `<digits>x<digits>`.
fn is_resolution_like(s: &str) -> bool {
    let mut chars = s.chars().peekable();
    let mut saw_digit = false;
    while chars.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        chars.next();
        saw_digit = true;
    }
    if !saw_digit || chars.next() != Some('x') {
        return false;
    }
    chars.peek().map(|c| c.is_ascii_digit()).unwrap_or(false)
}

/// Index of the first track def with `id == id`.
/// Timeline tracks the editor cannot work without: `O1` (Overlays) and `V1`
/// (Video). The editor draws its picture lane from `V1` and its overlay lane
/// from `O1`, so the settings panel refuses to delete them.
pub const REQUIRED_TRACK_IDS: [&str; 2] = ["O1", "V1"];

/// Put back any required timeline track missing from `envelope`
/// (`{ "settings": { "trackDefs": [...] } }`), and seed the full default set
/// when there is no `trackDefs` array at all. Returns true if it changed
/// anything.
///
/// This runs on every load rather than as a numbered migration because the
/// Qt shell never ran the migrations, and the panel used to allow deleting
/// both tracks, so there are settings files in the wild without them. It is
/// idempotent, and leaves the user's own tracks, order and colours alone.
pub fn ensure_required_tracks(envelope: &mut Value) -> bool {
    let Some(settings) = settings_mut(envelope) else {
        return false;
    };
    if !settings.get("trackDefs").is_some_and(Value::is_array) {
        settings.insert("trackDefs".into(), default_track_defs());
        return true;
    }
    let defs = settings
        .get_mut("trackDefs")
        .and_then(Value::as_array_mut)
        .expect("checked above");

    let mut changed = false;
    if index_of_id(defs, "O1").is_none() {
        let pos = index_of_id(defs, "V1").unwrap_or(0);
        defs.insert(pos, default_track_def("O1").expect("O1 default"));
        changed = true;
    }
    if index_of_id(defs, "V1").is_none() {
        let pos = index_of_id(defs, "O1").map_or(0, |i| i + 1);
        defs.insert(pos, default_track_def("V1").expect("V1 default"));
        changed = true;
    }
    changed
}

fn index_of_id(defs: &[Value], id: &str) -> Option<usize> {
    defs.iter()
        .position(|d| d.get("id").and_then(Value::as_str) == Some(id))
}

#[cfg(test)]
mod persistence_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_required_tracks_restores_deleted_video_and_overlay() {
        let mut v = json!({ "settings": { "trackDefs": [
            { "id": "A5", "name": "Audio 5", "color": "#94a3b8", "icon": "game", "visible": true }
        ] } });
        assert!(ensure_required_tracks(&mut v));
        let ids: Vec<&str> = v["settings"]["trackDefs"]
            .as_array().unwrap().iter()
            .map(|d| d["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["O1", "V1", "A5"]);
        // The user's own track is untouched.
        assert_eq!(v["settings"]["trackDefs"][2]["color"], "#94a3b8");
        // Idempotent.
        assert!(!ensure_required_tracks(&mut v));
    }

    #[test]
    fn ensure_required_tracks_keeps_existing_order_and_colours() {
        let mut v = json!({ "settings": { "trackDefs": [
            { "id": "A1", "name": "Audio 1", "color": "#123456", "icon": "game", "visible": true },
            { "id": "V1", "name": "Picture", "color": "#abcdef", "icon": "video", "visible": false }
        ] } });
        assert!(ensure_required_tracks(&mut v));
        let defs = v["settings"]["trackDefs"].as_array().unwrap();
        let ids: Vec<&str> = defs.iter().map(|d| d["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["A1", "O1", "V1"]);
        assert_eq!(defs[2]["color"], "#abcdef");
        assert_eq!(defs[2]["name"], "Picture");
    }

    #[test]
    fn ensure_required_tracks_seeds_defaults_when_absent() {
        let mut v = json!({ "settings": {} });
        assert!(ensure_required_tracks(&mut v));
        assert_eq!(v["settings"]["trackDefs"].as_array().unwrap().len(), 7);
        let mut none = json!(null);
        assert!(!ensure_required_tracks(&mut none));
    }

    // ── runMigrations (mirrors stores/persistence.test.ts) ──

    #[test]
    fn migrates_clips_folder_to_clip_directories() {
        let mut state = json!({ "settings": { "clipsFolder": "/old/path" }, "_schemaVersion": 0 });
        run_migrations(&mut state);
        assert_eq!(state["settings"]["clip_directories"], json!(["/old/path"]));
        assert!(state["settings"].get("clipsFolder").is_none());
    }

    #[test]
    fn migrates_screenshot_dir_to_screenshot_dirs() {
        let mut state = json!({ "settings": { "screenshotDir": "/old/pic" }, "_schemaVersion": 0 });
        run_migrations(&mut state);
        assert_eq!(state["settings"]["screenshotDirs"], json!(["/old/pic"]));
        assert!(state["settings"].get("screenshotDir").is_none());
    }

    #[test]
    fn resets_stale_gsr_monitor_target() {
        let mut state = json!({ "settings": { "gsrMonitorTarget": "1920x1080" }, "_schemaVersion": 0 });
        run_migrations(&mut state);
        assert_eq!(state["settings"]["gsrMonitorTarget"], "screen");
    }

    #[test]
    fn normalizes_connector_resolution_composite_to_bare_connector() {
        let mut state = json!({ "settings": { "gsrMonitorTarget": "DP-1|1920x1080" }, "_schemaVersion": 9 });
        run_migrations(&mut state);
        assert_eq!(state["settings"]["gsrMonitorTarget"], "DP-1");
    }

    #[test]
    fn resets_composite_whose_connector_half_is_invalid() {
        let mut state = json!({ "settings": { "gsrMonitorTarget": "1920x1080|DP-1" }, "_schemaVersion": 9 });
        run_migrations(&mut state);
        assert_eq!(state["settings"]["gsrMonitorTarget"], "screen");
    }

    #[test]
    fn migrates_legacy_capture_track_source_labels() {
        let mut state = json!({
            "settings": { "captureTracks": [
                { "name": "Track 1", "source": "Game" },
                { "name": "Track 2", "source": "Mic" },
                { "name": "Track 3", "source": "OpenGG_Aux.monitor" }
            ] },
            "_schemaVersion": 10
        });
        run_migrations(&mut state);
        let sources: Vec<&str> = state["settings"]["captureTracks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["source"].as_str().unwrap())
            .collect();
        assert_eq!(
            sources,
            ["OpenGG_Game.monitor", "OpenGG_Mic.monitor", "OpenGG_Aux.monitor"]
        );
    }

    #[test]
    fn sets_schema_version_after_running() {
        let mut state = json!({ "settings": {}, "_schemaVersion": 0 });
        run_migrations(&mut state);
        assert!(state["_schemaVersion"].as_i64().unwrap() > 0);
    }

    #[test]
    fn skips_already_applied_migrations() {
        let mut state = json!({ "settings": { "clipsFolder": "/old/path" }, "_schemaVersion": 8 });
        run_migrations(&mut state);
        assert_eq!(state["settings"]["clipsFolder"], "/old/path");
    }

    // ── deepMerge (mirrors stores/persistence.test.ts) ──

    #[test]
    fn merges_nested_objects() {
        let a = json!({ "mixer": { "volumes": { "Game": 100 } } });
        let b = json!({ "mixer": { "volumes": { "Chat": 50 } } });
        let r = deep_merge(a, b);
        assert_eq!(r["mixer"]["volumes"], json!({ "Game": 100, "Chat": 50 }));
    }

    #[test]
    fn overrides_primitive_values() {
        let a = json!({ "name": "old" });
        let b = json!({ "name": "new" });
        assert_eq!(deep_merge(a, b), json!({ "name": "new" }));
    }

    #[test]
    fn replaces_arrays_not_merges_them() {
        let a = json!({ "items": [1, 2] });
        let b = json!({ "items": [3] });
        assert_eq!(deep_merge(a, b), json!({ "items": [3] }));
    }

    #[test]
    fn returns_a_when_b_is_null() {
        let a = json!({ "key": "value" });
        assert_eq!(deep_merge(a.clone(), Value::Null), a);
    }

    #[test]
    fn returns_b_when_a_is_not_an_object() {
        assert_eq!(
            deep_merge(json!("string"), json!({ "key": "value" })),
            json!({ "key": "value" })
        );
    }

    #[test]
    fn handles_deeply_nested_structures() {
        let a = json!({ "a": { "b": { "c": 1 } } });
        let b = json!({ "a": { "b": { "d": 2 } } });
        assert_eq!(deep_merge(a, b), json!({ "a": { "b": { "c": 1, "d": 2 } } }));
    }
}

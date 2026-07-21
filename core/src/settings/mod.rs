//! Settings persistence logic.
//!
//! Ported verbatim (behavior-preserving) from `frontend/src/stores/persistence.ts`
//! per plan §2.1 rule 4 ("frontend-store logic that is actually business logic
//! must move to core with its tests"). The JS operated on untyped objects, so the
//! Rust port works on `serde_json::Value` to keep parity with the on-disk
//! `ui-settings.json` shape shared across both UIs.

use serde_json::{json, Value};

/// Current settings schema version. Migrations run sequentially up to this.
/// Keep in lockstep with `CURRENT_SCHEMA_VERSION` in `persistence.ts`.
pub const CURRENT_SCHEMA_VERSION: i64 = 12;

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

/// Default track definitions (from `DEFAULTS.settings.trackDefs` in
/// `persistence.ts`) — needed by migrations 5 and 6.
fn default_track_def(id: &str) -> Option<Value> {
    let defs = json!([
        { "id": "O1", "name": "Overlays", "color": "#f97316", "icon": "overlay", "visible": true },
        { "id": "V1", "name": "Video",    "color": "#E94560", "icon": "video",   "visible": true },
        { "id": "A1", "name": "Audio 1",  "color": "#10b981", "icon": "game",    "visible": true },
        { "id": "A2", "name": "Audio 2",  "color": "#3b82f6", "icon": "chat",    "visible": true },
        { "id": "A3", "name": "Audio 3",  "color": "#f59e0b", "icon": "mic",     "visible": true },
        { "id": "A4", "name": "Audio 4",  "color": "#8b5cf6", "icon": "media",   "visible": true },
        { "id": "A5", "name": "Audio 5",  "color": "#ec4899", "icon": "media",   "visible": true }
    ]);
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
fn index_of_id(defs: &[Value], id: &str) -> Option<usize> {
    defs.iter()
        .position(|d| d.get("id").and_then(Value::as_str) == Some(id))
}

#[cfg(test)]
mod tests {
    use super::*;

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

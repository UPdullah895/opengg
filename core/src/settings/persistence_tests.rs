//! Tests for the safe-persistence layer (atomic writes, quarantine, migration
//! on load). They drive the `*_at(path)` functions so nothing touches the
//! real `~/.config/opengg`.

use super::*;

/// Unique scratch dir under the system temp dir, removed on drop.
struct Scratch(PathBuf);
impl Scratch {
    fn new(tag: &str) -> Self {
        let d = std::env::temp_dir().join(format!(
            "opengg-settings-test-{tag}-{}-{}",
            std::process::id(),
            TMP_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn names(&self) -> Vec<String> {
        let mut n: Vec<String> = std::fs::read_dir(&self.0)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        n.sort();
        n
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn corrupt_settings_are_quarantined_not_overwritten() {
    let d = Scratch::new("corrupt");
    let p = d.file("ui-settings.json");
    std::fs::write(&p, "{\"settings\":").unwrap(); // truncated mid-write
    assert_eq!(load_settings_at(&p).unwrap(), "null");
    assert!(!p.exists(), "bad file must be moved out of the way");
    let kept: Vec<_> = d.names().into_iter().filter(|n| n.contains(".corrupt-")).collect();
    assert_eq!(kept.len(), 1);
    assert_eq!(
        std::fs::read_to_string(d.file(&kept[0])).unwrap(),
        "{\"settings\":",
        "quarantined copy keeps the original bytes"
    );
    assert!(settings_error().contains("could not be read"));
}

#[test]
fn non_object_settings_are_quarantined() {
    let d = Scratch::new("array");
    let p = d.file("ui-settings.json");
    std::fs::write(&p, "[1,2]").unwrap();
    assert_eq!(load_settings_at(&p).unwrap(), "null");
    assert!(d.names().iter().any(|n| n.contains(".corrupt-")));
}

#[test]
fn corrupt_theme_is_quarantined_and_default_served() {
    let d = Scratch::new("theme");
    let p = d.file("theme.json");
    std::fs::write(&p, "not json").unwrap();
    assert_eq!(load_theme_at(&p).unwrap(), DEFAULT_THEME_JSON);
    assert!(!p.exists());
    assert!(d.names().iter().any(|n| n.starts_with("theme.json.corrupt-")));
}

#[test]
fn quarantine_never_clobbers_an_earlier_one() {
    let d = Scratch::new("twice");
    let p = d.file("x.json");
    std::fs::write(&p, "a").unwrap();
    let first = quarantine(&p).unwrap();
    std::fs::write(&p, "b").unwrap();
    let second = quarantine(&p).unwrap();
    assert_ne!(first, second);
    assert_eq!(std::fs::read_to_string(first).unwrap(), "a");
    assert_eq!(std::fs::read_to_string(second).unwrap(), "b");
}

#[test]
fn atomic_write_replaces_whole_file_and_leaves_no_temp() {
    let d = Scratch::new("atomic");
    let p = d.file("nested/dir/f.json");
    atomic_write(&p, b"one").unwrap();
    atomic_write(&p, b"two-longer").unwrap();
    atomic_write(&p, b"3").unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "3");
    let siblings: Vec<String> = std::fs::read_dir(p.parent().unwrap())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(siblings, ["f.json"], "no .tmp leftovers: {siblings:?}");
}

#[test]
fn failed_atomic_write_keeps_old_content_and_cleans_up() {
    let d = Scratch::new("atomic-fail");
    let p = d.file("f.json");
    atomic_write(&p, b"keep").unwrap();
    // Renaming a file over a directory fails: the old file must survive
    // untouched and the temp must be removed.
    let blocked = d.file("blocked");
    std::fs::create_dir(&blocked).unwrap();
    assert!(atomic_write(&blocked, b"x").is_err());
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "keep");
    assert_eq!(d.names(), ["blocked", "f.json"]);
}

#[test]
fn migrates_unversioned_file_once_and_persists_with_backup() {
    let d = Scratch::new("migrate");
    let p = d.file("ui-settings.json");
    std::fs::write(
        &p,
        r#"{"settings":{"clipsFolder":"/old","gsrMonitorTarget":"1920x1080"}}"#,
    )
    .unwrap();
    let out: Value = serde_json::from_str(&load_settings_at(&p).unwrap()).unwrap();
    assert_eq!(out["settings"]["clip_directories"], json!(["/old"]));
    assert_eq!(out["settings"]["gsrMonitorTarget"], "screen");
    assert_eq!(out["_schemaVersion"], CURRENT_SCHEMA_VERSION);
    // persisted, and the pre-migration file is kept
    let on_disk: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    assert_eq!(on_disk, out);
    assert!(std::fs::read_to_string(d.file("ui-settings.json.bak-v0"))
        .unwrap()
        .contains("clipsFolder"));
}

#[test]
fn migration_is_idempotent_across_loads() {
    let d = Scratch::new("idem");
    let p = d.file("ui-settings.json");
    // A realistic "today" file: never stamped, current-shape, user data.
    std::fs::write(
        &p,
        r#"{"settings":{"clip_directories":["/v"],"gsrMonitorTarget":"DP-1","shortcuts":{"saveReplay":"Alt+F10"},
           "trackDefs":[{"id":"O1"},{"id":"V1"},{"id":"A1","visible":false}],
           "captureTracks":[{"source":"OpenGG_Game.monitor"}]},
          "mixer":{"earBlast":{"enabled":true,"channels":["Game"],"threshold":70,"target":50}}}"#,
    )
    .unwrap();
    let first = load_settings_at(&p).unwrap();
    let second = load_settings_at(&p).unwrap();
    assert_eq!(first, second, "second load must not change anything");
    let v: Value = serde_json::from_str(&first).unwrap();
    // user data survived the first (migrating) pass
    assert_eq!(v["settings"]["clip_directories"], json!(["/v"]));
    assert_eq!(v["settings"]["gsrMonitorTarget"], "DP-1");
    assert_eq!(v["settings"]["trackDefs"][2]["visible"], false);
    assert_eq!(v["mixer"]["earBlast"]["threshold"], 70);
    // and running the migrations directly on migrated data is a no-op
    let mut again = v.clone();
    again["_schemaVersion"] = json!(0);
    run_migrations(&mut again);
    assert_eq!(again, v);
}

#[test]
fn newer_schema_is_left_alone() {
    let d = Scratch::new("newer");
    let p = d.file("ui-settings.json");
    let raw = r#"{"settings":{"clipsFolder":"/keep"},"_schemaVersion":99}"#;
    std::fs::write(&p, raw).unwrap();
    assert_eq!(load_settings_at(&p).unwrap(), raw);
    assert_eq!(std::fs::read_to_string(&p).unwrap(), raw);
}

#[test]
fn update_stamps_new_files_and_preserves_other_keys() {
    let d = Scratch::new("update");
    let p = d.file("ui-settings.json");
    update_settings_at(&p, |v| v["settings"]["a"] = json!(1)).unwrap();
    update_settings_at(&p, |v| v["settings"]["b"] = json!(2)).unwrap();
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    assert_eq!(v["settings"], json!({"a":1,"b":2}));
    assert_eq!(v["_schemaVersion"], CURRENT_SCHEMA_VERSION);
}

#[test]
fn update_does_not_overwrite_a_corrupt_file() {
    let d = Scratch::new("update-corrupt");
    let p = d.file("ui-settings.json");
    std::fs::write(&p, "{broken").unwrap();
    update_settings_at(&p, |v| v["settings"]["k"] = json!(1)).unwrap();
    let kept = d.names().into_iter().find(|n| n.contains(".corrupt-")).expect("kept");
    assert_eq!(std::fs::read_to_string(d.file(&kept)).unwrap(), "{broken");
}

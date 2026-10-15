//! clips.db schema/migration/corruption tests. They drive `prepare_db_at` with
//! a scratch path so nothing touches the real `~/.local/share/opengg`.

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let d = std::env::temp_dir().join(format!(
            "opengg-db-test-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d)
    }
    fn db(&self) -> PathBuf {
        self.0.join("clips.db")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn columns(p: &Path) -> Vec<String> {
    let c = Connection::open(p).unwrap();
    let mut s = c.prepare("PRAGMA table_info(clip_meta)").unwrap();
    let cols: Vec<String> = s
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .flatten()
        .collect();
    cols
}

#[test]
fn fresh_db_gets_the_full_schema() {
    let d = Scratch::new();
    assert_eq!(prepare_db_at(&d.db()).unwrap(), None);
    let cols = columns(&d.db());
    for want in ["favorite", "notes", "duration", "width", "height", "mtime", "game_tag"] {
        assert!(cols.iter().any(|c| c == want), "missing {want} in {cols:?}");
    }
}

#[test]
fn migration_rerun_is_a_no_op_and_keeps_data() {
    let d = Scratch::new();
    prepare_db_at(&d.db()).unwrap();
    let c = Connection::open(d.db()).unwrap();
    c.execute("INSERT INTO clip_meta(filepath,favorite) VALUES('/a.mp4',1)", []).unwrap();
    drop(c);
    // second and third runs hit "duplicate column name" internally
    assert_eq!(prepare_db_at(&d.db()).unwrap(), None);
    assert_eq!(prepare_db_at(&d.db()).unwrap(), None);
    let c = Connection::open(d.db()).unwrap();
    let fav: i64 = c
        .query_row("SELECT favorite FROM clip_meta WHERE filepath='/a.mp4'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fav, 1);
}

#[test]
fn old_schema_without_game_tag_is_upgraded_in_place() {
    let d = Scratch::new();
    let c = Connection::open(d.db()).unwrap();
    c.execute_batch(
        "CREATE TABLE clip_meta(filepath TEXT PRIMARY KEY,custom_name TEXT DEFAULT '',favorite INTEGER DEFAULT 0,tags TEXT DEFAULT '',notes TEXT DEFAULT '');
         INSERT INTO clip_meta(filepath,custom_name) VALUES('/old.mp4','kept');",
    )
    .unwrap();
    drop(c);
    prepare_db_at(&d.db()).unwrap();
    assert!(columns(&d.db()).iter().any(|c| c == "game_tag"));
    let c = Connection::open(d.db()).unwrap();
    let name: String = c
        .query_row("SELECT custom_name FROM clip_meta WHERE filepath='/old.mp4'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(name, "kept");
}

#[test]
fn duplicate_column_is_expected_but_other_alter_errors_are_real() {
    let d = Scratch::new();
    prepare_db_at(&d.db()).unwrap();
    let c = Connection::open(d.db()).unwrap();

    // Raw duplicate: classified as "already there"...
    let dup = c.execute("ALTER TABLE clip_meta ADD COLUMN width INTEGER", []).unwrap_err();
    assert!(is_duplicate_column(&dup), "{dup:?}");
    assert!(!is_corrupt_db(&dup));
    // ...and add_column swallows exactly that.
    assert!(add_column(&c, "clip_meta", "width", "INTEGER DEFAULT 0").is_ok());

    // A missing table is a real error and must come through.
    let real = add_column(&c, "no_such_table", "x", "INTEGER").unwrap_err();
    assert!(!is_duplicate_column(&real), "{real:?}");
    assert!(!is_corrupt_db(&real));
}

#[test]
fn read_only_database_surfaces_instead_of_being_swallowed() {
    let d = Scratch::new();
    // An old-schema DB missing game_tag, opened read-only: the ALTER fails for
    // a real reason, which must not be mistaken for "column exists".
    let c = Connection::open(d.db()).unwrap();
    c.execute_batch("CREATE TABLE clip_meta(filepath TEXT PRIMARY KEY);").unwrap();
    drop(c);
    let ro = Connection::open_with_flags(d.db(), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let err = add_column(&ro, "clip_meta", "game_tag", "TEXT DEFAULT ''").unwrap_err();
    assert!(!is_duplicate_column(&err), "{err:?}");
}

#[test]
fn corrupt_db_is_kept_aside_and_replaced() {
    let d = Scratch::new();
    std::fs::write(d.db(), b"this is definitely not a sqlite database file at all").unwrap();
    let note = prepare_db_at(&d.db()).unwrap().expect("corruption reported");
    assert!(note.contains("corrupt"), "{note}");
    // original bytes preserved for manual recovery
    let kept: Vec<_> = std::fs::read_dir(&d.0)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("clips.db.corrupt-"))
        .collect();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(
        std::fs::read(d.0.join(&kept[0])).unwrap(),
        b"this is definitely not a sqlite database file at all"
    );
    // and the app has a working database again
    assert!(columns(&d.db()).iter().any(|c| c == "game_tag"));
}

/// The editor stores master volume and per-track gains next to the trim
/// window; a fresh or upgraded database must have those columns.
#[test]
fn trim_state_has_editor_columns() {
    let d = Scratch::new();
    prepare_db_at(&d.db()).unwrap();
    let c = Connection::open(d.db()).unwrap();
    let mut s = c.prepare("PRAGMA table_info(trim_state)").unwrap();
    let cols: Vec<String> = s
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .flatten()
        .collect();
    for want in ["trim_start", "trim_end", "volume", "track_gains"] {
        assert!(cols.iter().any(|c| c == want), "missing {want}: {cols:?}");
    }
}

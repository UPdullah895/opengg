//! Clip SQLite database (moved verbatim from src-tauri/commands.rs).
//!
//! Schema init, metadata + trim-state CRUD, the ffprobe result cache, and the
//! thumbnail-ID hash. The probe-driven listing commands (`get_clips` et al.)
//! stay in the Tauri host until the media helpers are extracted; they call the
//! `pub` helpers here.

use crate::paths::{resolve_clips_dir, thumb_dir};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// Video extensions recognized as clips.
pub const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "webm", "avi", "mov", "ts", "flv"];

/// `~/.local/share/opengg/clips.db`.
pub fn clips_db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("opengg/clips.db")
}

// ── Open / schema / corruption handling ──
//
// `clips.db` is NOT a pure cache: favourites, custom names, game tags, notes
// and trim windows live only here (the clip files themselves carry none of
// it). So a database SQLite reports as corrupt is never deleted — it is moved
// to `clips.db.corrupt-<ts>` for manual recovery, a fresh one is created so
// the app keeps working, and `db_error()` carries the story to the UI.
// Any other failure (read-only disk, permissions) is returned as-is and
// retried on the next call rather than papered over.
//
// The schema used to be created by `init_clips_db`, which nothing called, so
// a fresh install had no tables at all; `open_db` now does it on first use.

static DB_READY: Mutex<bool> = Mutex::new(false);
static DB_ERROR: Mutex<String> = Mutex::new(String::new());

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Non-empty when the clip database had to be set aside, or could not be
/// opened/migrated. Sticky for the session so the UI can still show it.
pub fn db_error() -> String {
    lock(&DB_ERROR).clone()
}

/// Open a connection to the clips DB, creating/migrating the schema on first
/// use in this process.
pub fn open_db() -> Result<Connection, String> {
    {
        let mut ready = lock(&DB_READY);
        if !*ready {
            match prepare_db_at(&clips_db_path()) {
                Ok(note) => {
                    if let Some(n) = note {
                        *lock(&DB_ERROR) = n;
                    }
                    *ready = true;
                }
                Err(e) => {
                    log::error!("clips.db unavailable: {e}");
                    *lock(&DB_ERROR) = e.clone();
                    return Err(e);
                }
            }
        }
    }
    connect(&clips_db_path()).map_err(|e| format!("DB: {e}"))
}

fn connect(p: &Path) -> rusqlite::Result<Connection> {
    let c = Connection::open(p)?;
    // The daemon-side scanner and the UI can touch the file at once; wait
    // briefly instead of failing with SQLITE_BUSY.
    c.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(c)
}

/// SQLite said the file is not a database / is malformed.
fn is_corrupt_db(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(f, _)
        if matches!(f.code, rusqlite::ErrorCode::NotADatabase | rusqlite::ErrorCode::DatabaseCorrupt))
}

/// `ALTER TABLE .. ADD COLUMN` on a column that already exists — the expected
/// outcome when the migration re-runs, as opposed to a real failure.
fn is_duplicate_column(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(_, Some(msg)) if msg.contains("duplicate column name"))
}

/// Add a column, treating "already there" as success and surfacing anything
/// else (read-only DB, full disk, missing table).
fn add_column(db: &Connection, table: &str, col: &str, decl: &str) -> rusqlite::Result<()> {
    match db.execute(&format!("ALTER TABLE {table} ADD COLUMN {col} {decl}"), []) {
        Ok(_) => Ok(()),
        Err(e) if is_duplicate_column(&e) => Ok(()),
        Err(e) => Err(e),
    }
}

/// Create the schema if missing and add any later columns (idempotent).
fn init_schema(p: &Path) -> rusqlite::Result<()> {
    let db = connect(p)?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS clip_meta(filepath TEXT PRIMARY KEY,custom_name TEXT DEFAULT '',favorite INTEGER DEFAULT 0,tags TEXT DEFAULT '',notes TEXT DEFAULT '');
     CREATE TABLE IF NOT EXISTS trim_state(filepath TEXT PRIMARY KEY,trim_start REAL DEFAULT 0,trim_end REAL DEFAULT 0);")?;
    // Phase 3a: ffprobe cache columns; later: game_tag.
    add_column(&db, "clip_meta", "duration", "REAL DEFAULT 0")?;
    add_column(&db, "clip_meta", "width", "INTEGER DEFAULT 0")?;
    add_column(&db, "clip_meta", "height", "INTEGER DEFAULT 0")?;
    add_column(&db, "clip_meta", "mtime", "INTEGER DEFAULT 0")?;
    add_column(&db, "clip_meta", "game_tag", "TEXT DEFAULT ''")?;
    // Editor state kept per clip alongside the trim window: master volume
    // (-1 = never set) and per-track gains as a JSON object.
    add_column(&db, "trim_state", "volume", "REAL DEFAULT -1")?;
    add_column(&db, "trim_state", "track_gains", "TEXT DEFAULT ''")?;
    Ok(())
}

/// Ensure the database at `p` is usable. `Ok(Some(msg))` means it was corrupt
/// and has been set aside (`msg` says where); `Ok(None)` means all was well.
fn prepare_db_at(p: &Path) -> Result<Option<String>, String> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("DB: create {}: {e}", d.display()))?;
    }
    match init_schema(p) {
        Ok(()) => Ok(None),
        Err(e) if is_corrupt_db(&e) => {
            let dest = crate::settings::quarantine(p).map_err(|re| {
                format!("DB: {} is corrupt ({e}) and could not be set aside: {re}", p.display())
            })?;
            // A stale -wal/-shm next to a brand-new database would corrupt it.
            for suffix in ["-wal", "-shm"] {
                let side = PathBuf::from(format!("{}{suffix}", p.display()));
                if side.exists() {
                    let _ = std::fs::rename(&side, PathBuf::from(format!("{}{suffix}", dest.display())));
                }
            }
            init_schema(p).map_err(|e2| format!("DB: could not recreate {}: {e2}", p.display()))?;
            let msg = format!(
                "The clip database was corrupt ({e}). It was kept as {} and a new one was created; favourites, custom names, game tags and trims saved in it are not shown.",
                dest.display()
            );
            log::error!("{msg}");
            Ok(Some(msg))
        }
        Err(e) => Err(format!("DB: {e}")),
    }
}

/// Create the clip DB schema if missing (idempotent). `open_db` does this on
/// first use; this is for callers that want to fail early.
pub fn init_clips_db() -> Result<(), String> {
    open_db().map(|_| ())
}

/// Map of filepath → (custom_name, favorite, game_tag) for all clip metadata.
pub fn get_meta_map() -> HashMap<String, (String, bool, String)> {
    let mut m = HashMap::new();
    if let Ok(c) = open_db() {
        if let Ok(mut s) =
            c.prepare("SELECT filepath,custom_name,favorite,COALESCE(game_tag,'') FROM clip_meta")
        {
            let _ = s
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, bool>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })
                .map(|rows| {
                    for r in rows.flatten() {
                        m.insert(r.0, (r.1, r.2, r.3));
                    }
                });
        }
    }
    m
}

/// A clip row as surfaced to the UI.
#[derive(Debug, Serialize, Clone)]
pub struct ClipInfo {
    pub id: String,
    pub filename: String,
    pub filepath: String,
    pub filesize: u64,
    pub created: String,
    #[serde(rename = "createdTs")]
    pub created_ts: u64,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub game: String,
    pub custom_name: String,
    pub favorite: bool,
    pub thumbnail: String,
    pub probing: bool,
}

// Phase 3a: ffprobe cache helpers
/// Read cached (duration, width, height) for a filepath if mtime matches.
pub fn probe_cache_get(db: &Connection, fp: &str, mtime: u64) -> Option<(f64, u32, u32)> {
    db.query_row(
        "SELECT duration, width, height FROM clip_meta WHERE filepath=?1 AND mtime=?2 AND duration>0",
        rusqlite::params![fp, mtime as i64],
        |r| Ok((r.get::<_, f64>(0)?, r.get::<_, u32>(1)?, r.get::<_, u32>(2)?)),
    ).ok()
}
/// Write (duration, width, height, mtime) to cache.
pub fn probe_cache_set(db: &Connection, fp: &str, dur: f64, w: u32, h: u32, mtime: u64) {
    let _ = db.execute(
        "INSERT INTO clip_meta(filepath,duration,width,height,mtime) VALUES(?1,?2,?3,?4,?5) \
         ON CONFLICT(filepath) DO UPDATE SET duration=?2,width=?3,height=?4,mtime=?5",
        rusqlite::params![fp, dur, w, h, mtime as i64],
    );
}

/// djb2 hash used to derive thumbnail file IDs from a clip path.
pub fn hash_str(s: &str) -> u64 {
    let mut h: u64 = 5381;
    for b in s.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    h
}

/// Count video files (non-recursive of metadata) under the resolved clip folder.
pub fn count_clips(folder: &str) -> usize {
    let dir = resolve_clips_dir(folder);
    if !dir.exists() {
        return 0;
    }
    walkdir::WalkDir::new(&dir)
        .min_depth(1)
        .into_iter()
        .flatten()
        .filter(|e| {
            let p = e.path();
            p.is_file() && {
                let ext = p
                    .extension()
                    .and_then(|x| x.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                VIDEO_EXTS.contains(&ext.as_str())
            }
        })
        .count()
}

/// Fields the UI sends to update a clip's metadata row.
#[derive(Deserialize)]
pub struct ClipMetaUpdate {
    pub filepath: String,
    pub custom_name: Option<String>,
    pub favorite: Option<bool>,
    pub game_tag: Option<String>,
    pub notes: Option<String>,
}

/// Upsert clip metadata (only non-empty text fields overwrite existing values).
pub fn set_clip_meta(update: ClipMetaUpdate) -> Result<(), String> {
    let db = open_db()?;
    // Build dynamic UPDATE to only set provided fields
    let cn = update.custom_name.unwrap_or_default();
    let fav = update.favorite.unwrap_or(false) as i32;
    let gt = update.game_tag.unwrap_or_default();
    let notes = update.notes.unwrap_or_default();
    db.execute(
        "INSERT INTO clip_meta(filepath,custom_name,favorite,game_tag,notes) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(filepath) DO UPDATE SET custom_name=CASE WHEN ?2='' THEN custom_name ELSE ?2 END,favorite=?3,game_tag=CASE WHEN ?4='' THEN game_tag ELSE ?4 END,notes=CASE WHEN ?5='' THEN notes ELSE ?5 END",
        rusqlite::params![update.filepath, cn, fav, gt, notes]
    ).map_err(|e| format!("{e}"))?;
    Ok(())
}

/// Return a clip's full metadata as a JSON string (`"null"` if absent).
pub fn get_clip_meta(filepath: &str) -> Result<String, String> {
    let db = open_db()?;
    match db.query_row("SELECT custom_name,favorite,COALESCE(game_tag,''),COALESCE(notes,'') FROM clip_meta WHERE filepath=?1", [&filepath], |r| {
        Ok(serde_json::json!({
            "custom_name": r.get::<_,String>(0)?,
            "favorite": r.get::<_,bool>(1)?,
            "game_tag": r.get::<_,String>(2).unwrap_or_default(),
            "notes": r.get::<_,String>(3).unwrap_or_default(),
        }))
    }) {
        Ok(v) => Ok(v.to_string()),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok("null".into()),
        Err(e) => Err(format!("{e}")),
    }
}

/// Move a clip to the system Trash (via `gio`), remove its thumbnail, and drop
/// its DB rows.
pub fn delete_clip(filepath: &str) -> Result<(), String> {
    let path = crate::paths::shexp(filepath);
    if Path::new(&path).exists() {
        // Move the clip to the system Trash (recoverable) instead of permanently deleting it.
        // `gio` ships with the glib stack that Tauri/WebKitGTK already require, so it is
        // reliably present. On failure we return Err so the frontend keeps the card instead
        // of removing it from the UI while the file still exists.
        let status = std::process::Command::new("gio")
            .args(["trash", "--"])
            .arg(&path)
            .status()
            .map_err(|e| format!("Could not move clip to Trash (gio unavailable): {e}"))?;
        if !status.success() {
            return Err(format!("Failed to move clip to Trash (gio trash exited {status})"));
        }
    }
    let id = format!("{:x}", hash_str(filepath));
    let t = thumb_dir().join(format!("{id}.jpg"));
    if t.exists() {
        let _ = std::fs::remove_file(&t);
    }
    if let Ok(c) = open_db() {
        let _ = c.execute("DELETE FROM clip_meta WHERE filepath=?1", [&filepath]);
        let _ = c.execute("DELETE FROM trim_state WHERE filepath=?1", [&filepath]);
    }
    Ok(())
}

/// Rename a clip file (sanitizing the new name), carrying its metadata, trim
/// state, and thumbnail across; returns the new filepath.
pub fn rename_clip(old_path: &str, new_name: &str) -> Result<String, String> {
    let old = PathBuf::from(old_path);
    if !old.exists() {
        return Err(format!("File not found: {old_path}"));
    }

    let ext = old.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let dir = old.parent().unwrap_or(Path::new("."));
    // Sanitize filename
    // ★ Epic 2: Allow Unicode (Arabic/CJK) — only strip OS-illegal path chars
    let safe_name: String = new_name
        .chars()
        .filter(|c| {
            !matches!(
                c,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\0'
            )
        })
        .collect::<String>()
        .trim()
        .to_string();
    let new_path = dir.join(format!("{safe_name}.{ext}"));

    if new_path.exists() {
        return Err("A file with that name already exists".into());
    }

    std::fs::rename(&old, &new_path).map_err(|e| format!("rename: {e}"))?;

    // Update SQLite metadata
    if let Ok(db) = open_db() {
        let _ = db.execute(
            "UPDATE clip_meta SET filepath=?1 WHERE filepath=?2",
            rusqlite::params![new_path.to_string_lossy().to_string(), old_path],
        );
        let _ = db.execute(
            "UPDATE trim_state SET filepath=?1 WHERE filepath=?2",
            rusqlite::params![new_path.to_string_lossy().to_string(), old_path],
        );
    }

    // Rename thumbnail too
    let old_id = format!("{:x}", hash_str(old_path));
    let new_fp = new_path.to_string_lossy().to_string();
    let new_id = format!("{:x}", hash_str(&new_fp));
    let td = thumb_dir();
    let old_thumb = td.join(format!("{old_id}.jpg"));
    let new_thumb = td.join(format!("{new_id}.jpg"));
    if old_thumb.exists() {
        let _ = std::fs::rename(&old_thumb, &new_thumb);
    }

    Ok(new_fp)
}

/// A clip's saved editor trim window.
#[derive(Serialize)]
pub struct TrimState {
    pub trim_start: f64,
    pub trim_end: f64,
}

/// Everything the clip editor restores when a clip is reopened.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EditorState {
    pub trim_start: f64,
    pub trim_end: f64,
    /// Master playback volume 0..1, or `None` if never changed.
    pub volume: Option<f64>,
    /// Per-track gains keyed by audio track index, as the editor holds them.
    pub track_gains: serde_json::Value,
}

/// Upsert a clip's editor state (trim window, volume, track gains).
pub fn save_editor_state(filepath: &str, st: &EditorState) -> Result<(), String> {
    let gains = if st.track_gains.is_null() {
        String::new()
    } else {
        st.track_gains.to_string()
    };
    open_db()?
        .execute(
            "INSERT INTO trim_state(filepath,trim_start,trim_end,volume,track_gains) \
             VALUES(?1,?2,?3,?4,?5) ON CONFLICT(filepath) DO UPDATE SET \
             trim_start=?2,trim_end=?3,volume=?4,track_gains=?5",
            rusqlite::params![
                filepath,
                st.trim_start,
                st.trim_end,
                st.volume.unwrap_or(-1.0),
                gains
            ],
        )
        .map_err(|e| format!("{e}"))?;
    Ok(())
}

/// Read a clip's saved editor state, if any.
pub fn get_editor_state(filepath: &str) -> Result<Option<EditorState>, String> {
    let c = open_db()?;
    let r = c.query_row(
        "SELECT trim_start,trim_end,COALESCE(volume,-1),COALESCE(track_gains,'') \
         FROM trim_state WHERE filepath=?1",
        [&filepath],
        |r| {
            let vol: f64 = r.get(2)?;
            let gains: String = r.get(3)?;
            Ok(EditorState {
                trim_start: r.get(0)?,
                trim_end: r.get(1)?,
                volume: (vol >= 0.0).then_some(vol),
                track_gains: serde_json::from_str(&gains).unwrap_or(serde_json::Value::Null),
            })
        },
    );
    match r {
        Ok(t) => Ok(Some(t)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("{e}")),
    }
}

/// Upsert a clip's trim window.
pub fn save_trim_state(filepath: &str, trim_start: f64, trim_end: f64) -> Result<(), String> {
    open_db()?.execute("INSERT INTO trim_state(filepath,trim_start,trim_end) VALUES(?1,?2,?3) ON CONFLICT(filepath) DO UPDATE SET trim_start=?2,trim_end=?3", rusqlite::params![filepath,trim_start,trim_end]).map_err(|e| format!("{e}"))?;
    Ok(())
}

/// Read a clip's saved trim window, if any.
pub fn get_trim_state(filepath: &str) -> Result<Option<TrimState>, String> {
    let c = open_db()?;
    let mut s = c
        .prepare("SELECT trim_start,trim_end FROM trim_state WHERE filepath=?1")
        .map_err(|e| format!("{e}"))?;
    match s.query_row([&filepath], |r| {
        Ok(TrimState {
            trim_start: r.get(0)?,
            trim_end: r.get(1)?,
        })
    }) {
        Ok(t) => Ok(Some(t)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("{e}")),
    }
}

#[cfg(test)]
#[path = "db_tests.rs"]
mod db_tests;

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

/// Video extensions recognized as clips.
pub const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "webm", "avi", "mov", "ts", "flv"];

/// `~/.local/share/opengg/clips.db`.
pub fn clips_db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("opengg/clips.db")
}

/// Open a connection to the clips DB.
pub fn open_db() -> Result<Connection, String> {
    Connection::open(clips_db_path()).map_err(|e| format!("DB: {e}"))
}

/// Create the clip DB schema if missing (idempotent).
pub fn init_clips_db() -> Result<(), String> {
    let p = clips_db_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).ok();
    }
    let db = open_db()?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS clip_meta(filepath TEXT PRIMARY KEY,custom_name TEXT DEFAULT '',favorite INTEGER DEFAULT 0,tags TEXT DEFAULT '',notes TEXT DEFAULT '');
     CREATE TABLE IF NOT EXISTS trim_state(filepath TEXT PRIMARY KEY,trim_start REAL DEFAULT 0,trim_end REAL DEFAULT 0);").map_err(|e| format!("{e}"))?;
    // Phase 3a: Add ffprobe cache columns (ALTER TABLE is a no-op if column already exists)
    let _ = db.execute(
        "ALTER TABLE clip_meta ADD COLUMN duration REAL DEFAULT 0",
        [],
    );
    let _ = db.execute(
        "ALTER TABLE clip_meta ADD COLUMN width INTEGER DEFAULT 0",
        [],
    );
    let _ = db.execute(
        "ALTER TABLE clip_meta ADD COLUMN height INTEGER DEFAULT 0",
        [],
    );
    let _ = db.execute(
        "ALTER TABLE clip_meta ADD COLUMN mtime INTEGER DEFAULT 0",
        [],
    );
    Ok(())
}

/// Map of filepath → (custom_name, favorite, game_tag) for all clip metadata.
pub fn get_meta_map() -> HashMap<String, (String, bool, String)> {
    let mut m = HashMap::new();
    if let Ok(c) = open_db() {
        let _ = c.execute(
            "ALTER TABLE clip_meta ADD COLUMN game_tag TEXT DEFAULT ''",
            [],
        );
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
    let _ = db.execute(
        "ALTER TABLE clip_meta ADD COLUMN game_tag TEXT DEFAULT ''",
        [],
    );
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
    let _ = db.execute(
        "ALTER TABLE clip_meta ADD COLUMN game_tag TEXT DEFAULT ''",
        [],
    );
    match db.query_row("SELECT custom_name,favorite,COALESCE(game_tag,''),COALESCE(notes,'') FROM clip_meta WHERE filepath=?1", [&filepath], |r| {
        Ok(serde_json::json!({
            "custom_name": r.get::<_,String>(0)?,
            "favorite": r.get::<_,bool>(1)?,
            "game_tag": r.get::<_,String>(2).unwrap_or_default(),
            "notes": r.get::<_,String>(3).unwrap_or_default(),
        }))
    }) {
        Ok(v) => Ok(v.to_string()),
        Err(_) => Ok("null".into()),
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

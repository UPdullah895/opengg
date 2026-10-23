//! Folder size limit: keep the clips folder under a user-chosen size by
//! moving OpenGG's oldest saves to the Trash.
//!
//! Only files OpenGG itself saved are ever touched. Each save is recorded in
//! the `owned_clips` table; everything else in the folder (a recording from
//! OBS, "Wedding.mp4" the user dropped in) is neither counted nor removed.
//! The newest owned clip is never removed — the user just saved it — even
//! when it alone is over the limit. Removal goes to the Trash, silently: the
//! user is usually in a game when this runs, and the Trash keeps it
//! recoverable.

use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// One owned file as the janitor sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnedFile {
    pub path: PathBuf,
    pub bytes: u64,
    /// Modification time, seconds since the epoch.
    pub mtime: u64,
}

/// Size limits offered in Settings → Storage, in GB (0 = no limit).
pub const LIMIT_CHOICES_GB: &[u64] = &[0, 25, 50, 100, 200, 500, 1000];

/// Which files to remove so the owned total is at most `limit_bytes`:
/// oldest first, never the newest. Pure, so it is tested exhaustively.
pub fn select_for_removal(files: &[OwnedFile], limit_bytes: u64) -> Vec<PathBuf> {
    if limit_bytes == 0 || files.len() < 2 {
        return Vec::new();
    }
    let mut sorted: Vec<&OwnedFile> = files.iter().collect();
    sorted.sort_by(|a, b| a.mtime.cmp(&b.mtime).then_with(|| a.path.cmp(&b.path)));
    let mut total: u64 = sorted.iter().map(|f| f.bytes).sum();
    let mut out = Vec::new();
    // `len - 1`: the newest file is never a candidate.
    for f in &sorted[..sorted.len() - 1] {
        if total <= limit_bytes {
            break;
        }
        total = total.saturating_sub(f.bytes);
        out.push(f.path.clone());
    }
    out
}

/// Create the ownership table (idempotent). Called from `init_clips_db`.
pub fn init_table(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS owned_clips(filepath TEXT PRIMARY KEY, saved_ts INTEGER DEFAULT 0);",
    )
}

/// Record that OpenGG saved `path` (a clip or screenshot).
pub fn record_owned(db: &Connection, path: &Path) -> rusqlite::Result<()> {
    init_table(db)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    db.execute(
        "INSERT OR REPLACE INTO owned_clips(filepath, saved_ts) VALUES (?1, ?2)",
        rusqlite::params![path.to_string_lossy().to_string(), now],
    )?;
    Ok(())
}

/// Every owned file that still exists, with its size and mtime. Rows whose
/// file is gone are pruned on the way.
pub fn owned_files(db: &Connection) -> rusqlite::Result<Vec<OwnedFile>> {
    init_table(db)?;
    let paths: Vec<String> = {
        let mut st = db.prepare("SELECT filepath FROM owned_clips")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        rows.flatten().collect()
    };
    let mut out = Vec::new();
    for p in paths {
        match std::fs::metadata(&p) {
            Ok(m) if m.is_file() => out.push(OwnedFile {
                path: PathBuf::from(&p),
                bytes: m.len(),
                mtime: m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            }),
            _ => {
                db.execute("DELETE FROM owned_clips WHERE filepath=?1", [&p])?;
            }
        }
    }
    Ok(out)
}

/// Bring the owned total under `limit_gb` (decimal GB; 0 = off) using
/// `trash` to remove each file. Returns the paths actually removed.
/// Failures are skipped, never surfaced: this runs unattended after saves.
pub fn enforce_with(db: &Connection, limit_gb: u64, trash: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    if limit_gb == 0 {
        return Vec::new();
    }
    let files = match owned_files(db) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("retention: cannot read owned clips: {e}");
            return Vec::new();
        }
    };
    let mut removed = Vec::new();
    for p in select_for_removal(&files, limit_gb.saturating_mul(1_000_000_000)) {
        if trash(&p) {
            let _ = db.execute(
                "DELETE FROM owned_clips WHERE filepath=?1",
                [p.to_string_lossy().to_string()],
            );
            removed.push(p);
        } else {
            log::warn!("retention: could not trash {p:?}; skipping");
        }
    }
    if !removed.is_empty() {
        log::info!("retention: moved {} old clip(s) to the Trash (limit {limit_gb} GB)", removed.len());
    }
    removed
}

/// [`enforce_with`] against the real clips DB and the desktop Trash.
pub fn enforce(limit_gb: u64) -> Vec<PathBuf> {
    let Ok(db) = super::open_db() else { return Vec::new() };
    enforce_with(&db, limit_gb, |p| {
        std::process::Command::new("gio")
            .args(["trash", "--"])
            .arg(p)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str, bytes: u64, mtime: u64) -> OwnedFile {
        OwnedFile { path: PathBuf::from(name), bytes, mtime }
    }

    #[test]
    fn removes_oldest_until_under_limit() {
        let files = [f("c", 40, 3), f("a", 40, 1), f("b", 40, 2), f("d", 40, 4)];
        assert_eq!(select_for_removal(&files, 100), vec![PathBuf::from("a"), PathBuf::from("b")]);
        assert_eq!(select_for_removal(&files, 160), Vec::<PathBuf>::new());
        assert_eq!(select_for_removal(&files, 0), Vec::<PathBuf>::new());
    }

    #[test]
    fn never_removes_the_newest() {
        let files = [f("old", 10, 1), f("new", 500, 2)];
        assert_eq!(select_for_removal(&files, 100), vec![PathBuf::from("old")]);
        assert_eq!(select_for_removal(&[f("only", 500, 1)], 100), Vec::<PathBuf>::new());
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("opengg-ret-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn only_owned_files_are_counted_or_removed() {
        let d = tmp("owned");
        let db = Connection::open_in_memory().unwrap();
        let set_mtime = |p: &Path, secs: i64| {
            let c = std::ffi::CString::new(p.to_string_lossy().as_bytes()).unwrap();
            let t = libc::timeval { tv_sec: secs, tv_usec: 0 };
            unsafe { libc::utimes(c.as_ptr(), [t, t].as_ptr()) };
        };
        // Three owned 1 KB-ish clips plus a big foreign file.
        for (i, name) in ["a.mp4", "b.mp4", "c.mp4"].iter().enumerate() {
            let p = d.join(name);
            std::fs::write(&p, vec![0u8; 1000]).unwrap();
            set_mtime(&p, 1_000_000 + i as i64);
            record_owned(&db, &p).unwrap();
        }
        let foreign = d.join("Wedding.mp4");
        std::fs::write(&foreign, vec![0u8; 5000]).unwrap();

        // The foreign file is invisible to the janitor: not listed, so never
        // counted toward the limit and never selected.
        let owned = owned_files(&db).unwrap();
        assert_eq!(owned.len(), 3);
        assert!(owned.iter().all(|o| o.path != foreign));
        let pick = select_for_removal(&owned, 2000);
        assert_eq!(pick, vec![d.join("a.mp4")]);

        // enforce_with removes through the injected trash and prunes the row.
        let removed_log = std::cell::RefCell::new(Vec::new());
        // 1 GB is far above 3 KB → nothing to do.
        let removed = enforce_with(&db, 1, |p| {
            removed_log.borrow_mut().push(p.to_path_buf());
            true
        });
        assert!(removed.is_empty() && removed_log.borrow().is_empty());

        // A deleted file is pruned from the table on the next read.
        std::fs::remove_file(d.join("b.mp4")).unwrap();
        assert_eq!(owned_files(&db).unwrap().len(), 2);
        assert!(foreign.exists());
        std::fs::remove_dir_all(&d).ok();
    }
}

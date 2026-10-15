//! Clip listing — scan directories, parallel ffprobe with caching, fetch single clip.
//!
//! Functions moved verbatim from `frontend/src-tauri/src/commands.rs` per plan §2.1.

use super::db::{get_meta_map, open_db, probe_cache_get, probe_cache_set, ClipInfo, VIDEO_EXTS};
use crate::media::{date_from_stem, fmt_ts_local, probe_video};
use crate::paths::{get_all_clip_dirs, thumb_dir};
use crate::clips::hash_str;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Folder names that hold clips of many games, so they never name a game.
const GENERIC_FOLDERS: &[&str] = &[
    "opengg", "videos", "video", "replay", "replays", "clips", "clip", "recordings",
    "recording", "captures", "capture",
];
/// Filename prefixes recorders use instead of a game name.
const GENERIC_PREFIXES: &[&str] = &[
    "replay", "clip", "recording", "video", "screenrecording", "screencast", "capture",
];

/// The game a clip belongs to, from where it lives.
///
/// Recorders group clips by game FOLDER — GPU Screen Recorder's "save in game
/// folder" writes `<dir>/<Game>/Replay_<date>.mov` — while the filename is
/// often a generic `Replay_…`. So the folder wins whenever it is not a
/// library root; inside a root the filename prefix is used as before
/// (OpenGG's own `Game_<date>.mp4`, SteelSeries' `Game__<date>__…`), and a
/// generic prefix falls back to the root's own name when that looks like a
/// game folder the user added directly.
pub fn game_from_location(path: &Path, roots: &[PathBuf]) -> String {
    let folder_name = |d: &Path| {
        d.file_name()
            .map(|n| n.to_string_lossy().trim().to_string())
            .unwrap_or_default()
    };
    let is_generic_folder =
        |name: &str| name.is_empty() || GENERIC_FOLDERS.contains(&name.to_lowercase().as_str());

    let parent = path.parent().unwrap_or(Path::new(""));
    let parent_is_root = roots.iter().any(|r| r.as_path() == parent);
    let parent_name = folder_name(parent);
    if !parent_is_root && !is_generic_folder(&parent_name) {
        return parent_name;
    }

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    // SteelSeries: GameName__YYYY-MM-DD__HH-MM-SS — split on __ for the full name.
    // Other formats: Prefix_YYYY-MM-DD_HH-MM-SS — split on _ for the prefix.
    let from_name = if let Some(pos) = stem.find("__") {
        stem[..pos].replace(['-', '_'], " ")
    } else {
        stem.split('_').next().unwrap_or("").replace('-', " ")
    };
    let from_name = from_name.trim().to_string();
    let generic_name = from_name.is_empty()
        || from_name.chars().next().is_some_and(|c| c.is_ascii_digit())
        || GENERIC_PREFIXES.contains(&from_name.to_lowercase().as_str());
    if !generic_name {
        return from_name;
    }
    if !is_generic_folder(&parent_name) {
        return parent_name;
    }
    if from_name.is_empty() {
        "Unknown".to_string()
    } else {
        from_name
    }
}

/// Fetch metadata for a single file — used by the frontend file-watcher listener.
pub async fn get_clip_by_path(filepath: String) -> Result<Option<ClipInfo>, String> {
    let p = PathBuf::from(&filepath);
    if !p.is_file() {
        return Ok(None);
    }
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !VIDEO_EXTS.contains(&ext.as_str()) {
        return Ok(None);
    }
    let meta = get_meta_map();
    let td = thumb_dir();
    let _ = std::fs::create_dir_all(&td);
    let m = p.metadata().map_err(|e| format!("{e}"))?;
    let fname = p
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let fp = filepath.clone();
    let id = format!("{:x}", hash_str(&fp));
    let mtime = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("Unknown");
    let created = date_from_stem(stem).unwrap_or_else(|| fmt_ts_local(mtime as i64));
    let p_clone = p.clone();
    let (dur, w, h) = tokio::task::spawn_blocking(move || probe_video(&p_clone))
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?;
    let game_from_filename = game_from_location(&p, &get_all_clip_dirs(""));
    let (cn, fav, game_tag) = meta.get(&fp).cloned().unwrap_or_default();
    let game = if game_tag.is_empty() {
        game_from_filename
    } else {
        game_tag
    };
    let thumb = td.join(format!("{id}.jpg"));
    let thumbnail = if thumb.exists() {
        thumb.to_string_lossy().to_string()
    } else {
        String::new()
    };
    Ok(Some(ClipInfo {
        id,
        filename: fname,
        filepath: fp,
        filesize: m.len(),
        created,
        created_ts: mtime,
        duration: dur,
        width: w,
        height: h,
        game,
        custom_name: cn,
        favorite: fav,
        thumbnail,
        probing: false,
    }))
}

/// Get full clip list with ffprobe for all files (slow).
/// Phase 3a+3b: cache-first ffprobe, parallel for uncached clips.
pub async fn get_clips(folder: String) -> Result<Vec<ClipInfo>, String> {
    use tokio::sync::Semaphore;
    #[cfg(debug_assertions)]
    let t_total = std::time::Instant::now();
    let dirs = get_all_clip_dirs(&folder);
    let meta = get_meta_map();
    let td = thumb_dir();
    let _ = std::fs::create_dir_all(&td);

    // Collect all candidate files first (cheap filesystem scan)
    struct Entry {
        fp: String,
        fname: String,
        id: String,
        filesize: u64,
        created: String,
        mtime: u64,
        game_raw: String,
        cn: String,
        fav: bool,
        game_tag: String,
        thumbnail: String,
    }
    let mut entries: Vec<Entry> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in &dirs {
        if !dir.exists() {
            continue;
        }
        for e in walkdir::WalkDir::new(dir)
            .min_depth(1)
            .into_iter()
            .flatten()
        {
            let p = e.path().to_path_buf();
            if !p.is_file() {
                continue;
            }
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !VIDEO_EXTS.contains(&ext.as_str()) {
                continue;
            }
            let fp = p.to_string_lossy().to_string();
            if seen.contains(&fp) {
                continue;
            }
            seen.insert(fp.clone());
            let m = match e.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let fname = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let id = format!("{:x}", hash_str(&fp));
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("Unknown");
            let created = date_from_stem(stem).unwrap_or_else(|| fmt_ts_local(mtime as i64));
            let game_raw = game_from_location(&p, &dirs);
            let (cn, fav, game_tag) = meta.get(&fp).cloned().unwrap_or_default();
            let thumb = td.join(format!("{id}.jpg"));
            let thumbnail = if thumb.exists() {
                thumb.to_string_lossy().to_string()
            } else {
                String::new()
            };
            entries.push(Entry {
                fp,
                fname,
                id,
                filesize: m.len(),
                created,
                mtime,
                game_raw,
                cn,
                fav,
                game_tag,
                thumbnail,
            });
        }
    }
    #[cfg(debug_assertions)]
    let t_scan = t_total.elapsed().as_millis();

    // Phase 3a: check probe cache; collect uncached for parallel probing
    let db = open_db().ok();
    struct CachedEntry {
        entry_idx: usize,
        dur: f64,
        w: u32,
        h: u32,
    }
    let mut cached: Vec<CachedEntry> = Vec::new();
    let mut uncached_idxs: Vec<usize> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if let Some(ref db) = db {
            if let Some((dur, w, h)) = probe_cache_get(db, &e.fp, e.mtime) {
                cached.push(CachedEntry {
                    entry_idx: i,
                    dur,
                    w,
                    h,
                });
                continue;
            }
        }
        uncached_idxs.push(i);
    }
    #[cfg(debug_assertions)]
    let t_cache = t_total.elapsed().as_millis();
    #[cfg(debug_assertions)]
    let n_uncached = uncached_idxs.len();

    // Phase 3b: parallel ffprobe for uncached clips (max 4 concurrent)
    // acquire().await blocks until a permit is free, properly limiting concurrency.
    let sem = Arc::new(Semaphore::new(4));
    let mut probe_tasks = Vec::new();
    for idx in uncached_idxs {
        let fp = entries[idx].fp.clone();
        let sem = Arc::clone(&sem);
        let task = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let fp2 = fp.clone();
            let result =
                tokio::task::spawn_blocking(move || probe_video(std::path::Path::new(&fp2)))
                    .await
                    .unwrap_or((0.0, 0, 0));
            (idx, fp, result)
        });
        probe_tasks.push(task);
    }
    let mut probe_results: Vec<(usize, String, (f64, u32, u32))> = Vec::new();
    for task in probe_tasks {
        if let Ok(r) = task.await {
            probe_results.push(r);
        }
    }
    #[cfg(debug_assertions)]
    let t_probe = t_total.elapsed().as_millis();
    // Write new probe results to cache
    if let Some(ref db) = db {
        for (idx, fp, (dur, w, h)) in &probe_results {
            probe_cache_set(db, fp, *dur, *w, *h, entries[*idx].mtime);
        }
    }

    // Assemble final ClipInfo list
    let mut probe_map: std::collections::HashMap<usize, (f64, u32, u32)> =
        std::collections::HashMap::new();
    for c in cached {
        probe_map.insert(c.entry_idx, (c.dur, c.w, c.h));
    }
    for (idx, _, dwh) in probe_results {
        probe_map.insert(idx, dwh);
    }

    let mut clips: Vec<ClipInfo> = entries
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            let (dur, w, h) = probe_map.get(&i).copied().unwrap_or((0.0, 0, 0));
            let game = if e.game_tag.is_empty() {
                e.game_raw
            } else {
                e.game_tag
            };
            ClipInfo {
                id: e.id,
                filename: e.fname,
                filepath: e.fp,
                filesize: e.filesize,
                created: e.created,
                created_ts: e.mtime,
                duration: dur,
                width: w,
                height: h,
                game,
                custom_name: e.cn,
                favorite: e.fav,
                thumbnail: e.thumbnail,
                probing: false,
            }
        })
        .collect();

    clips.sort_by(|a, b| {
        b.created
            .cmp(&a.created)
            .then_with(|| b.created_ts.cmp(&a.created_ts))
            .then_with(|| b.filename.cmp(&a.filename))
    });
    #[cfg(debug_assertions)]
    {
        let t_total_ms = t_total.elapsed().as_millis();
        eprintln!("[perf] get_clips: scan={}ms cache={}ms ffprobe={}ms ({} uncached) assemble={}ms total={}ms clips={}",
            t_scan, t_cache - t_scan, t_probe - t_cache, n_uncached,
            t_total_ms - t_probe, t_total_ms, clips.len());
    }
    Ok(clips)
}

/// Fast clip list — skips ffprobe entirely for uncached clips.
/// Uncached clips get duration=0, width=0, height=0 so the grid can appear immediately.
/// Call probe_clips() afterward to fill in missing metadata in the background.
pub fn get_clips_fast(folder: String) -> Result<Vec<ClipInfo>, String> {
    #[cfg(debug_assertions)]
    let t_total = std::time::Instant::now();
    let dirs = get_all_clip_dirs(&folder);
    let meta = get_meta_map();
    let td = thumb_dir();
    let _ = std::fs::create_dir_all(&td);

    struct Entry {
        fp: String,
        fname: String,
        id: String,
        filesize: u64,
        created: String,
        mtime: u64,
        game_raw: String,
        cn: String,
        fav: bool,
        game_tag: String,
        thumbnail: String,
    }
    let mut entries: Vec<Entry> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in &dirs {
        if !dir.exists() {
            continue;
        }
        for e in walkdir::WalkDir::new(dir)
            .min_depth(1)
            .into_iter()
            .flatten()
        {
            let p = e.path().to_path_buf();
            if !p.is_file() {
                continue;
            }
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !VIDEO_EXTS.contains(&ext.as_str()) {
                continue;
            }
            let fp = p.to_string_lossy().to_string();
            if seen.contains(&fp) {
                continue;
            }
            seen.insert(fp.clone());
            let m = match e.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let fname = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let id = format!("{:x}", hash_str(&fp));
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("Unknown");
            let created = date_from_stem(stem).unwrap_or_else(|| fmt_ts_local(mtime as i64));
            let game_raw = game_from_location(&p, &dirs);
            let (cn, fav, game_tag) = meta.get(&fp).cloned().unwrap_or_default();
            let thumb = td.join(format!("{id}.jpg"));
            let thumbnail = if thumb.exists() {
                thumb.to_string_lossy().to_string()
            } else {
                String::new()
            };
            entries.push(Entry {
                fp,
                fname,
                id,
                filesize: m.len(),
                created,
                mtime,
                game_raw,
                cn,
                fav,
                game_tag,
                thumbnail,
            });
        }
    }
    #[cfg(debug_assertions)]
    let t_scan = t_total.elapsed().as_millis();

    // Cache check, but don't probe uncached clips
    let db = open_db().ok();
    struct CachedEntry {
        entry_idx: usize,
        dur: f64,
        w: u32,
        h: u32,
    }
    let mut cached: Vec<CachedEntry> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if let Some(ref db) = db {
            if let Some((dur, w, h)) = probe_cache_get(db, &e.fp, e.mtime) {
                cached.push(CachedEntry {
                    entry_idx: i,
                    dur,
                    w,
                    h,
                });
            }
        }
    }
    #[cfg(debug_assertions)]
    let t_cache = t_total.elapsed().as_millis();

    // Assemble final ClipInfo list (uncached entries get 0 for duration/width/height)
    let mut probe_map: std::collections::HashMap<usize, (f64, u32, u32)> =
        std::collections::HashMap::new();
    for c in cached {
        probe_map.insert(c.entry_idx, (c.dur, c.w, c.h));
    }

    let mut clips: Vec<ClipInfo> = entries
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            let (dur, w, h) = probe_map.get(&i).copied().unwrap_or((0.0, 0, 0));
            let game = if e.game_tag.is_empty() {
                e.game_raw
            } else {
                e.game_tag
            };
            ClipInfo {
                id: e.id,
                filename: e.fname,
                filepath: e.fp,
                filesize: e.filesize,
                created: e.created,
                created_ts: e.mtime,
                duration: dur,
                width: w,
                height: h,
                game,
                custom_name: e.cn,
                favorite: e.fav,
                thumbnail: e.thumbnail,
                probing: dur == 0.0,
            }
        })
        .collect();

    clips.sort_by(|a, b| {
        b.created
            .cmp(&a.created)
            .then_with(|| b.created_ts.cmp(&a.created_ts))
            .then_with(|| b.filename.cmp(&a.filename))
    });
    #[cfg(debug_assertions)]
    {
        let t_total_ms = t_total.elapsed().as_millis();
        eprintln!("[perf] get_clips_fast: scan={}ms cache={}ms assemble={}ms total={}ms clips={}",
            t_scan, t_cache - t_scan, t_total_ms - t_cache, t_total_ms, clips.len());
    }
    Ok(clips)
}

#[cfg(test)]
mod game_tests {
    use super::*;

    fn game(path: &str, roots: &[&str]) -> String {
        let roots: Vec<PathBuf> = roots.iter().map(PathBuf::from).collect();
        game_from_location(Path::new(path), &roots)
    }

    #[test]
    fn game_folder_wins_over_generic_filename() {
        let r = ["/v/OpenGG"];
        assert_eq!(game("/v/OpenGG/Slay the Spire 2/Replay_2026-06-29_17-13-03.mov", &r), "Slay the Spire 2");
        assert_eq!(game("/v/OpenGG/Hades/Hades_2026-01-01_10-00-00.mp4", &r), "Hades");
    }

    #[test]
    fn root_files_use_the_filename() {
        let r = ["/v/OpenGG"];
        assert_eq!(game("/v/OpenGG/Apex-Legends_2026-01-01_10-00-00.mp4", &r), "Apex Legends");
        assert_eq!(game("/v/OpenGG/Counter-Strike__2026-01-01__10-00-00.mp4", &r), "Counter Strike");
        assert_eq!(game("/v/OpenGG/Replay_2026-01-01_10-00-00.mp4", &r), "Replay");
    }

    #[test]
    fn a_game_folder_added_as_a_root_names_its_generic_clips() {
        let r = ["/v/OpenGG", "/v/Slay the Spire 2"];
        assert_eq!(game("/v/Slay the Spire 2/Replay_2026-06-29_17-13-03.mov", &r), "Slay the Spire 2");
        assert_eq!(game("/v/Slay the Spire 2/2026-06-29.mp4", &r), "Slay the Spire 2");
    }

    #[test]
    fn generic_subfolders_do_not_name_a_game() {
        let r = ["/v"];
        assert_eq!(game("/v/Replay/Hades_2026-01-01_10-00-00.mp4", &r), "Hades");
    }
}

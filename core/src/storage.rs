//! Storage stats + thumbnail-cache maintenance (moved verbatim from
//! src-tauri/commands.rs).
//!
//! [`get_storage_info`] is synchronous here; the Tauri host wraps it in
//! `spawn_blocking` (async-runtime concern, kept out of core).

use crate::paths::{shexp, thumb_dir};
use serde::Serialize;
use std::path::PathBuf;

/// Aggregate clip storage figures for the storage settings panel.
#[derive(Serialize)]
pub struct StorageInfo {
    pub clip_count: u64,
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

/// Walk every directory in `clip_directories`, tallying video files and bytes,
/// and report free/total space of the filesystem holding the first that exists.
pub fn get_storage_info(clip_directories: &[String]) -> StorageInfo {
    let mut total_count = 0u64;
    let mut total_used = 0u64;
    let mut first_existing: Option<PathBuf> = None;

    for dir_str in clip_directories {
        let folder = PathBuf::from(shexp(dir_str));
        if !folder.exists() {
            continue;
        }
        if first_existing.is_none() {
            first_existing = Some(folder.clone());
        }
        for e in walkdir::WalkDir::new(&folder)
            .min_depth(1)
            .into_iter()
            .flatten()
        {
            let p = e.path().to_path_buf();
            if !p.is_file() {
                continue;
            }
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if name.ends_with(".mp4")
                || name.ends_with(".mkv")
                || name.ends_with(".webm")
                || name.ends_with(".mov")
            {
                total_count += 1;
                if let Ok(meta) = e.metadata() {
                    total_used += meta.len();
                }
            }
        }
    }

    let fs_root = first_existing.unwrap_or_else(|| PathBuf::from("/"));
    let (total_bytes, free_bytes) = get_fs_stats(&fs_root);

    StorageInfo {
        clip_count: total_count,
        used_bytes: total_used,
        total_bytes,
        free_bytes,
    }
}

/// Delete and recreate the thumbnail cache directory; returns how many files
/// were removed.
pub fn clear_thumbnail_cache() -> Result<u32, String> {
    let td = thumb_dir();
    let mut count = 0u32;
    if td.exists() {
        if let Ok(entries) = std::fs::read_dir(&td) {
            count = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .count() as u32;
        }
        std::fs::remove_dir_all(&td).map_err(|e| format!("remove: {e}"))?;
    }
    std::fs::create_dir_all(&td).map_err(|e| format!("create: {e}"))?;
    Ok(count)
}

#[cfg(unix)]
fn get_fs_stats(path: &std::path::Path) -> (u64, u64) {
    use std::os::unix::ffi::OsStrExt;
    let mut stat: libc_statvfs = unsafe { std::mem::zeroed() };
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap_or_default();
    unsafe {
        if libc_statvfs_call(cpath.as_ptr(), &mut stat) == 0 {
            let bsize = stat.f_frsize;
            return (stat.f_blocks * bsize, stat.f_bfree * bsize);
        }
    }
    (0, 0)
}
#[cfg(not(unix))]
fn get_fs_stats(_path: &std::path::Path) -> (u64, u64) {
    (0, 0)
}

// Thin statvfs wrapper to avoid adding libc as a direct dep.
#[cfg(unix)]
#[repr(C)]
#[allow(non_camel_case_types)]
struct libc_statvfs {
    f_bsize: u64,
    f_frsize: u64,
    f_blocks: u64,
    f_bfree: u64,
    f_bavail: u64,
    f_files: u64,
    f_ffree: u64,
    f_favail: u64,
    f_fsid: u64,
    f_flag: u64,
    f_namemax: u64,
    __spare: [u64; 6],
}
#[cfg(unix)]
extern "C" {
    fn statvfs(path: *const std::ffi::c_char, buf: *mut libc_statvfs) -> std::ffi::c_int;
}
#[cfg(unix)]
unsafe fn libc_statvfs_call(
    path: *const std::ffi::c_char,
    buf: *mut libc_statvfs,
) -> std::ffi::c_int {
    unsafe { statvfs(path, buf) }
}

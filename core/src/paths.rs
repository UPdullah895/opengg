//! Shared filesystem-path helpers (moved verbatim from src-tauri/commands.rs).
//!
//! Clip-directory resolution, `~` expansion, and cache-dir locations used across
//! the clips, media, storage, and export logic. Reads the shared
//! `ui-settings.json` via [`crate::settings::settings_path`].

use crate::settings::settings_path;
use serde_json::Value;
use std::path::PathBuf;

/// Expand a leading `~/` to the user's home directory; otherwise pass through.
pub fn shexp(p: &str) -> String {
    if p.starts_with("~/") {
        if let Some(h) = dirs::home_dir() {
            return p.replacen('~', &h.to_string_lossy(), 1);
        }
    }
    p.into()
}

/// Default clip library location: `<Videos>/OpenGG` (falls back to `~/Videos`).
pub fn default_clips_dir() -> PathBuf {
    dirs::video_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap().join("Videos"))
        .join("OpenGG")
}

/// Thumbnail cache directory: `~/.local/share/opengg/thumbnails`.
pub fn thumb_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("opengg/thumbnails")
}

/// Per-user cache for real per-model device images:
/// `~/.local/share/opengg/device-images`. Nothing populates this yet —
/// Devices roadmap Phase 3 (see [`crate::device_assets`]) ships only bundled
/// generic silhouettes. Defined now so a future sync tier that writes real
/// images here doesn't need to change `device_assets::resolve_device_image`'s
/// resolution order, only add a writer.
pub fn device_images_cache_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("opengg/device-images")
}

/// Resolve the clip directory to use: the explicit `f` if given, else the first
/// entry of `settings.clip_directories`, else [`default_clips_dir`].
pub fn resolve_clips_dir(f: &str) -> PathBuf {
    if !f.is_empty() {
        return PathBuf::from(shexp(f));
    }
    let sp = settings_path();
    if sp.exists() {
        if let Ok(j) = std::fs::read_to_string(&sp) {
            if let Ok(v) = serde_json::from_str::<Value>(&j) {
                if let Some(arr) = v["settings"]["clip_directories"].as_array() {
                    if let Some(first) = arr.first() {
                        if let Some(f) = first.as_str() {
                            return PathBuf::from(shexp(f));
                        }
                    }
                }
            }
        }
    }
    default_clips_dir()
}

/// All directories to scan for clips: every entry from `settings.clip_directories`
/// (de-duplicated), falling back to [`resolve_clips_dir`] with `primary`.
pub fn get_all_clip_dirs(primary: &str) -> Vec<PathBuf> {
    let sp = settings_path();
    if let Ok(j) = std::fs::read_to_string(&sp) {
        if let Ok(v) = serde_json::from_str::<Value>(&j) {
            if let Some(arr) = v["settings"]["clip_directories"].as_array() {
                let dirs: Vec<PathBuf> = arr
                    .iter()
                    .filter_map(|s| s.as_str())
                    .map(|p| PathBuf::from(shexp(p)))
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                if !dirs.is_empty() {
                    return dirs;
                }
            }
        }
    }
    vec![resolve_clips_dir(primary)]
}

/// Waveform peak cache: `~/.local/share/opengg/waveforms`. Holds the JSON
/// peak arrays the clip editor draws in its audio lanes, keyed by clip hash +
/// stream index + peak count, so re-opening a clip never re-decodes its audio.
pub fn waveform_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("opengg/waveforms")
}

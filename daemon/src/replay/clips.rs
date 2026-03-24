//! Clip Management — scanning, metadata, thumbnails, trimming.

use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::process::Command;

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "webm", "avi", "mov", "ts"];

#[derive(Debug, Clone, Serialize)]
pub struct Clip {
    pub id: String,
    pub filename: String,
    pub filepath: String,
    pub duration: f64,
    pub filesize: u64,
    pub game: String,
    pub created_at: String,
    pub thumbnail: String,
    pub width: u32,
    pub height: u32,
}

/// Scan a directory for video clips and extract metadata.
pub async fn scan_clips(dir: &Path) -> Result<Vec<Clip>> {
    let mut clips = Vec::new();

    if !dir.exists() {
        return Ok(clips);
    }

    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| VIDEO_EXTENSIONS.contains(&ext.to_str().unwrap_or("")))
                .unwrap_or(false)
        })
        .collect();

    entries.sort_by_key(|e| std::cmp::Reverse(e.metadata().ok().map(|m| m.modified().ok()).flatten()));

    for entry in entries {
        let path = entry.path();
        let meta = entry.metadata()?;
        let id = format!("{:x}", md5_hash(path.to_str().unwrap_or("")));

        let duration = probe_duration(&path).await.unwrap_or(0.0);
        let (width, height) = probe_dimensions(&path).await.unwrap_or((0, 0));

        let created = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| {
                chrono_format(d.as_secs())
            })
            .unwrap_or_default();

        clips.push(Clip {
            id,
            filename: path.file_name().unwrap_or_default().to_string_lossy().into(),
            filepath: path.to_string_lossy().into(),
            duration,
            filesize: meta.len(),
            game: guess_game_name(&path),
            created_at: created,
            thumbnail: String::new(),
            width,
            height,
        });
    }

    Ok(clips)
}

/// Generate a thumbnail for a video clip.
pub async fn generate_thumbnail(video: &Path, thumb_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(thumb_dir)?;
    let id = format!("{:x}", md5_hash(video.to_str().unwrap_or("")));
    let thumb_path = thumb_dir.join(format!("{id}.jpg"));

    if thumb_path.exists() {
        return Ok(thumb_path);
    }

    Command::new("ffmpeg")
        .args([
            "-y", "-i", video.to_str().unwrap_or(""),
            "-ss", "2",
            "-vframes", "1",
            "-vf", "scale=384:-1",
            "-q:v", "5",
            thumb_path.to_str().unwrap_or(""),
        ])
        .output()
        .await
        .context("ffmpeg not found")?;

    Ok(thumb_path)
}

/// Trim a clip using FFmpeg.
pub async fn trim_clip(
    input: &Path,
    output: &Path,
    start_sec: f64,
    end_sec: f64,
) -> Result<()> {
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i", input.to_str().unwrap_or(""),
            "-ss", &format!("{start_sec:.2}"),
            "-to", &format!("{end_sec:.2}"),
            "-c:v", "libx264",
            "-crf", "18",
            "-preset", "fast",
            "-c:a", "aac",
            "-b:a", "192k",
            output.to_str().unwrap_or(""),
        ])
        .status()
        .await
        .context("ffmpeg not found")?;

    if status.success() {
        tracing::info!("Trimmed clip → {}", output.display());
        Ok(())
    } else {
        anyhow::bail!("FFmpeg trim failed with status {status}")
    }
}

async fn probe_duration(path: &Path) -> Result<f64> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            path.to_str().unwrap_or(""),
        ])
        .output()
        .await?;

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .context("Failed to parse duration")
}

async fn probe_dimensions(path: &Path) -> Result<(u32, u32)> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height",
            "-of", "csv=s=x:p=0",
            path.to_str().unwrap_or(""),
        ])
        .output()
        .await?;

    let text = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = text.trim().split('x').collect();
    if parts.len() == 2 {
        Ok((
            parts[0].parse().unwrap_or(0),
            parts[1].parse().unwrap_or(0),
        ))
    } else {
        Ok((0, 0))
    }
}

fn guess_game_name(path: &Path) -> String {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    // OpenGG names files as "GameName_2025-01-15_12-30-45.mp4"
    stem.split('_').next().unwrap_or("Unknown").replace('-', " ")
}

fn md5_hash(input: &str) -> u64 {
    // Simple hash — not cryptographic, just for IDs
    let mut h: u64 = 5381;
    for b in input.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    h
}

fn chrono_format(unix_secs: u64) -> String {
    // Simple ISO-ish format without pulling in chrono crate
    format!("{}s since epoch", unix_secs)
}

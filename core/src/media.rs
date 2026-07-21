//! Media analysis and processing — ffprobe, thumbnails, waveforms, export.
//!
//! Functions moved verbatim from `frontend/src-tauri/src/commands.rs` per plan §2.1.
//! All code here is Tauri-free; async functions use tokio directly.

use crate::paths::{shexp, thumb_dir};
use crate::clips::{hash_str, open_db, probe_cache_set};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

/// A single audio/video/subtitle stream from ffprobe.
#[derive(Serialize)]
pub struct MediaStream {
    pub index: u32,
    pub codec_type: String, // "video" | "audio" | "subtitle"
    pub codec_name: String, // "h264", "aac", "opus", etc.
    pub channels: u32,      // audio channel count (2=stereo)
    pub sample_rate: String,
    pub language: String,
    pub title: String, // track title if set (e.g. "Game Audio", "Mic")
}

/// Full media file analysis from ffprobe.
#[derive(Serialize)]
pub struct MediaInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub video_codec: String,
    pub streams: Vec<MediaStream>,
    pub video_streams: u32,
    pub audio_streams: u32,
}

/// Analyze a media file via ffprobe — returns resolution, duration, frame rate, codec, and stream list.
pub async fn analyze_media(filepath: String) -> Result<MediaInfo, String> {
    let output = run_command_output_async("ffprobe", &[
        "-v", "quiet",
        "-print_format", "json",
        "-show_format",
        "-show_streams",
        &filepath,
    ]).await?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("parse: {e}"))?;

    let fmt = &json["format"];
    let duration: f64 = fmt["duration"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    let mut streams = Vec::new();
    let mut width = 0u32;
    let mut height = 0u32;
    let mut fps = 0.0f64;
    let mut video_codec = String::new();
    let mut video_count = 0u32;
    let mut audio_count = 0u32;

    if let Some(arr) = json["streams"].as_array() {
        for s in arr {
            let codec_type = s["codec_type"].as_str().unwrap_or("").to_string();
            let codec_name = s["codec_name"].as_str().unwrap_or("").to_string();
            let idx = s["index"].as_u64().unwrap_or(0) as u32;
            let tags = &s["tags"];
            let lang = tags["language"].as_str().unwrap_or("").to_string();
            let title = tags["title"].as_str().unwrap_or("").to_string();

            match codec_type.as_str() {
                "video" => {
                    video_count += 1;
                    if width == 0 {
                        width = s["width"].as_u64().unwrap_or(0) as u32;
                        height = s["height"].as_u64().unwrap_or(0) as u32;
                        video_codec = codec_name.clone();
                        // Parse fps from r_frame_rate "60/1" or "30000/1001"
                        if let Some(rfr) = s["r_frame_rate"].as_str() {
                            let parts: Vec<&str> = rfr.split('/').collect();
                            if parts.len() == 2 {
                                let n: f64 = parts[0].parse().unwrap_or(0.0);
                                let d: f64 = parts[1].parse().unwrap_or(1.0);
                                if d > 0.0 {
                                    fps = n / d;
                                }
                            }
                        }
                    }
                    streams.push(MediaStream {
                        index: idx,
                        codec_type,
                        codec_name,
                        channels: 0,
                        sample_rate: String::new(),
                        language: lang,
                        title,
                    });
                }
                "audio" => {
                    audio_count += 1;
                    let ch = s["channels"].as_u64().unwrap_or(2) as u32;
                    let sr = s["sample_rate"].as_str().unwrap_or("48000").to_string();
                    let track_title = if title.is_empty() {
                        format!("Audio {audio_count}")
                    } else {
                        title
                    };
                    streams.push(MediaStream {
                        index: idx,
                        codec_type,
                        codec_name,
                        channels: ch,
                        sample_rate: sr,
                        language: lang,
                        title: track_title,
                    });
                }
                _ => {}
            }
        }
    }

    Ok(MediaInfo {
        duration,
        width,
        height,
        fps,
        video_codec,
        streams,
        video_streams: video_count,
        audio_streams: audio_count,
    })
}

/// Generate audio waveform peaks data for visualization.
/// Uses ffmpeg to extract PCM samples, then computes peaks.
/// Returns JSON array of peak values (0.0-1.0) for the given audio stream.
pub async fn generate_waveform(
    filepath: String,
    stream_index: u32,
    num_peaks: u32,
) -> Result<Vec<f32>, String> {
    let peaks_count = num_peaks.clamp(100, 2000);

    // Extract raw PCM audio from the specified stream
    let output = run_command_output_async("ffmpeg", &[
        "-i", &filepath,
        "-map", &format!("0:{stream_index}"),
        "-ac", "1",
        "-f", "s16le",
        "-ar", "8000",
        "-",
    ]).await?;

    if !output.status.success() || output.stdout.is_empty() {
        return Ok(vec![0.0; peaks_count as usize]);
    }

    // Parse raw s16le samples
    let samples: Vec<i16> = output
        .stdout
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();

    if samples.is_empty() {
        return Ok(vec![0.0; peaks_count as usize]);
    }

    // Downsample to requested peak count
    let chunk_size = (samples.len() / peaks_count as usize).max(1);
    let peaks: Vec<f32> = (0..peaks_count as usize)
        .map(|i| {
            let start = i * chunk_size;
            let end = (start + chunk_size).min(samples.len());
            let max_abs = samples[start..end]
                .iter()
                .map(|s| s.unsigned_abs() as f32)
                .fold(0.0f32, f32::max);
            (max_abs / 32768.0).min(1.0)
        })
        .collect();

    Ok(peaks)
}

/// Generate a thumbnail image at 480p height from a video clip.
/// Caches results in `~/.local/share/opengg/thumbnails/`.
/// Uses the 10% point in the video, or the provided duration hint to skip ffprobe.
pub async fn generate_thumbnail(filepath: String, duration: Option<f64>) -> Result<String, String> {
    let id = format!("{:x}", hash_str(&filepath));
    let d = thumb_dir();
    let _ = std::fs::create_dir_all(&d);
    let out = d.join(format!("{id}.jpg"));
    if out.exists() {
        return Ok(out.to_string_lossy().to_string());
    }
    #[cfg(debug_assertions)]
    let t_start = std::time::Instant::now();
    // Use caller-provided duration to skip redundant probe_duration ffprobe subprocess
    let dur = duration
        .filter(|&d| d > 0.0)
        .unwrap_or_else(|| probe_duration(&filepath));
    #[cfg(debug_assertions)]
    let t_probe_ms = t_start.elapsed().as_millis();
    let seek = if dur > 1.0 { dur * 0.1 } else { 0.0 };
    // 480p thumbnails: ~853x480 at q:v 3 (~90KB each). Matches SteelSeries quality.
    let r = run_command_output_async("ffmpeg", &[
        "-ss", &format!("{seek:.2}"),
        "-i", &filepath,
        "-vframes", "1",
        "-vf", "scale=-2:480",
        "-q:v", "3",
        "-y", &out.to_string_lossy(),
    ]).await?;
    #[cfg(debug_assertions)]
    {
        let fname = filepath
            .rfind('/')
            .map(|i| &filepath[i + 1..])
            .unwrap_or(&filepath);
        eprintln!(
            "[perf] generate_thumbnail: probe={}ms ffmpeg={}ms total={}ms file={}",
            t_probe_ms,
            t_start.elapsed().as_millis() - t_probe_ms,
            t_start.elapsed().as_millis(),
            fname
        );
    }
    if r.status.success() && out.exists() {
        Ok(out.to_string_lossy().to_string())
    } else {
        let err = format!("ffmpeg: {}", String::from_utf8_lossy(&r.stderr));
        Err(err)
    }
}

/// Phase 3d: Batch thumbnail generation — generates up to 3 concurrently.
/// `durations`: optional per-filepath duration hints. When provided and non-zero,
/// skips the redundant probe_duration ffprobe call for that clip.
pub async fn generate_thumbnails_batch(
    filepaths: Vec<String>,
    durations: Option<Vec<f64>>,
) -> Result<Vec<String>, String> {
    use tokio::sync::Semaphore;
    let sem = Arc::new(Semaphore::new(3));
    let mut tasks = Vec::new();
    for (i, filepath) in filepaths.into_iter().enumerate() {
        let sem = Arc::clone(&sem);
        let provided_dur = durations
            .as_ref()
            .and_then(|d| d.get(i).copied())
            .filter(|&d| d > 0.0);
        let task = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let fp = filepath.clone();
            tokio::task::spawn_blocking(move || {
                let id = format!("{:x}", hash_str(&fp));
                let d = thumb_dir();
                let _ = std::fs::create_dir_all(&d);
                let out = d.join(format!("{id}.jpg"));
                if out.exists() {
                    return out.to_string_lossy().to_string();
                }
                let dur = provided_dur.unwrap_or_else(|| probe_duration(&fp));
                let seek = if dur > 1.0 { dur * 0.1 } else { 0.0 };
                let r = Command::new("ffmpeg")
                    .args([
                        "-ss",
                        &format!("{seek:.2}"),
                        "-i",
                        &fp,
                        "-vframes",
                        "1",
                        "-vf",
                        "scale=-2:480",
                        "-q:v",
                        "3",
                        "-y",
                        &out.to_string_lossy(),
                    ])
                    .output();
                match r {
                    Ok(o) if o.status.success() && out.exists() => {
                        out.to_string_lossy().to_string()
                    }
                    _ => String::new(),
                }
            })
            .await
            .unwrap_or_default()
        });
        tasks.push(task);
    }
    let mut results = Vec::new();
    for task in tasks {
        results.push(task.await.unwrap_or_default());
    }
    Ok(results)
}

/// Take a screenshot at a specific timestamp.
/// `output_dir`: optional override; falls back to `~/Pictures`.
pub async fn take_screenshot(
    filepath: String,
    time_sec: f64,
    output_dir: Option<String>,
) -> Result<String, String> {
    let pics_dir = match output_dir.as_deref().filter(|s| !s.is_empty()) {
        Some(d) => PathBuf::from(shexp(d)),
        None => dirs::picture_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures")),
    };
    let _ = std::fs::create_dir_all(&pics_dir);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let out = pics_dir.join(format!("opengg_screenshot_{ts}.png"));

    let r = run_command_output_async("ffmpeg", &[
        "-ss", &format!("{time_sec:.3}"),
        "-i", &filepath,
        "-vframes", "1",
        "-q:v", "2",
        "-y", &out.to_string_lossy(),
    ]).await?;

    if r.status.success() && out.exists() {
        Ok(out.to_string_lossy().to_string())
    } else {
        Err(format!(
            "Screenshot failed: {}",
            String::from_utf8_lossy(&r.stderr)
        ))
    }
}

/// Parallel ffprobe of multiple files with concurrency limit (max 4).
/// Returns (filepath, duration, width, height) tuples.
pub async fn probe_clips(filepaths: Vec<String>) -> Result<Vec<(String, f64, u32, u32)>, String> {
    use tokio::sync::Semaphore;
    if filepaths.is_empty() {
        return Ok(vec![]);
    }
    #[cfg(debug_assertions)]
    let t_start = std::time::Instant::now();
    let sem = Arc::new(Semaphore::new(4));
    let mut tasks = Vec::new();
    for fp in filepaths {
        let sem = Arc::clone(&sem);
        tasks.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let fp2 = fp.clone();
            let (dur, w, h) =
                tokio::task::spawn_blocking(move || probe_video(std::path::Path::new(&fp2)))
                    .await
                    .unwrap_or((0.0, 0, 0));
            (fp, dur, w, h)
        }));
    }
    let mut results = Vec::new();
    for t in tasks {
        if let Ok(r) = t.await {
            results.push(r);
        }
    }
    // Write to cache
    if let Ok(db) = open_db() {
        for (fp, dur, w, h) in &results {
            let mtime = std::fs::metadata(fp)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            probe_cache_set(&db, fp, *dur, *w, *h, mtime);
        }
    }
    #[cfg(debug_assertions)]
    eprintln!(
        "[perf] probe_clips: {}ms ({} clips)",
        t_start.elapsed().as_millis(),
        results.len()
    );
    Ok(results)
}

/// Calculate export bitrate settings given target file size, duration, and resolution.
pub fn calc_export_settings(
    duration_sec: f64,
    target_mb: f64,
    width: u32,
    height: u32,
) -> Result<String, String> {
    if duration_sec <= 0.0 {
        return Err("Invalid duration".into());
    }
    let audio_kbps = 128.0;
    let total_kbps = target_mb * 8192.0 / duration_sec;
    let video_kbps = (total_kbps - audio_kbps).max(100.0);
    Ok(serde_json::json!({
        "resolution": format!("{width}x{height}"),
        "video_bitrate_kbps": video_kbps as u32,
        "audio_bitrate_kbps": audio_kbps as u32,
        "total_bitrate_kbps": total_kbps as u32,
        "codec": "H.264 (libx264)", "preset": "fast", "passes": 2
    })
    .to_string())
}

// ═══ Private helpers ═══

/// Count actual audio streams in a file via ffprobe
fn count_audio_streams(path: &str) -> u32 {
    get_audio_stream_global_indices(path).len() as u32
}

/// Get the global stream indices of all audio streams.
pub fn get_audio_stream_global_indices(path: &str) -> Vec<u32> {
    if let Ok(o) = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=index",
            "-of",
            "csv=p=0",
            path,
        ])
        .output()
    {
        if o.status.success() {
            return String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|l| l.trim().parse::<u32>().ok())
                .collect();
        }
    }
    vec![0]
}

/// Probe video resolution for normalized overlay sizing.
pub fn probe_resolution(path: &str) -> (u32, u32) {
    if let Ok(o) = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=s=x:p=0",
            path,
        ])
        .output()
    {
        if o.status.success() {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let parts: Vec<&str> = s.split('x').collect();
            if parts.len() == 2 {
                let w = parts[0].parse().unwrap_or(1920);
                let h = parts[1].parse().unwrap_or(1080);
                return (w, h);
            }
        }
    }
    (1920, 1080) // fallback
}

/// Resolve a font by user-chosen name (e.g. "Impact") to a system path.
/// Falls back to the generic best-match font if the requested name is not found.
pub fn find_system_font_by_name(hint: Option<&str>) -> String {
    if let Some(name) = hint {
        let lower = name.to_lowercase();
        let candidates: &[&str] = match lower.as_str() {
            "impact" => &[
                "/usr/share/fonts/truetype/msttcorefonts/Impact.ttf",
                "/usr/share/fonts/truetype/impact.ttf",
                "/usr/share/fonts/impact.ttf",
                "/usr/share/fonts/TTF/Impact.ttf",
            ],
            "tahoma" => &[
                "/usr/share/fonts/truetype/msttcorefonts/Tahoma.ttf",
                "/usr/share/fonts/truetype/tahoma.ttf",
                "/usr/share/fonts/tahoma.ttf",
            ],
            "arial" | "liberation sans" => &[
                "/usr/share/fonts/truetype/msttcorefonts/Arial.ttf",
                "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
                "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
                "/usr/share/fonts/Liberation/LiberationSans-Regular.ttf",
            ],
            "dejavu sans" => &[
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/TTF/DejaVuSans.ttf",
            ],
            _ => &[],
        };
        for p in candidates {
            if Path::new(p).exists() {
                return p.to_string();
            }
        }
        // Unknown name: ask fontconfig
        if let Ok(out) = Command::new("fc-match")
            .args(["--format=%{file}", name])
            .output()
        {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() && Path::new(&p).exists() {
                    return p;
                }
            }
        }
    }
    find_system_font()
}

/// Find a system font that supports Arabic/CJK/Latin characters.
/// Tries common paths on Arch/Ubuntu/Fedora, falls back to fc-match.
pub fn find_system_font() -> String {
    let candidates = [
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/TTF/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ];
    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return path.to_string();
        }
    }
    // Fallback: use fc-match to find any available sans-serif font
    if let Ok(output) = Command::new("fc-match")
        .args(["--format=%{file}", "sans"])
        .output()
    {
        if output.status.success() {
            let p = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !p.is_empty() && std::path::Path::new(&p).exists() {
                return p;
            }
        }
    }
    // Last resort
    "sans".to_string()
}

/// Generate an auto-named output filename with a suffix (e.g., "_export").
pub fn auto_name(input: &str, suffix: &str) -> String {
    let p = Path::new(input);
    let s = p.file_stem().unwrap_or_default().to_string_lossy();
    let e = p.extension().unwrap_or_default().to_string_lossy();
    p.parent()
        .unwrap_or(Path::new("."))
        .join(format!("{s}{suffix}.{e}"))
        .to_string_lossy()
        .into()
}

/// Probe a video file for duration, width, height.
pub fn probe_video(p: &Path) -> (f64, u32, u32) {
    let d = probe_duration(&p.to_string_lossy());
    let dm = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=s=x:p=0",
            &p.to_string_lossy(),
        ])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let ps: Vec<&str> = dm.split('x').collect();
    (
        d,
        ps.first().and_then(|s| s.parse().ok()).unwrap_or(0),
        ps.get(1).and_then(|s| s.parse().ok()).unwrap_or(0),
    )
}

/// Probe a file for duration in seconds.
fn probe_duration(p: &str) -> f64 {
    Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            p,
        ])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(0.0)
}

/// Extract "YYYY-MM-DD HH:MM" from a filename stem.
/// Handles two formats:
///   SteelSeries GG:        GameName__YYYY-MM-DD__HH-MM-SS  (double underscore)
///   gpu-screen-recorder:   Prefix_YYYY-MM-DD_HH-MM-SS      (single underscore)
/// Returns None if neither pattern matches.
pub fn date_from_stem(stem: &str) -> Option<String> {
    let b = stem.as_bytes();
    // SteelSeries: YYYY-MM-DD__HH-MM-SS (20 chars)
    // Pattern positions: DDDD-DD-DD__DD-DD-DD
    if b.len() >= 20 {
        for i in 0..=b.len() - 20 {
            let s = &b[i..i + 20];
            if s[4] == b'-'
                && s[7] == b'-'
                && s[10] == b'_'
                && s[11] == b'_'
                && s[14] == b'-'
                && s[17] == b'-'
                && s[..4].iter().all(u8::is_ascii_digit)
                && s[5..7].iter().all(u8::is_ascii_digit)
                && s[8..10].iter().all(u8::is_ascii_digit)
                && s[12..14].iter().all(u8::is_ascii_digit)
                && s[15..17].iter().all(u8::is_ascii_digit)
                && s[18..20].iter().all(u8::is_ascii_digit)
            {
                let t = std::str::from_utf8(&s[..20]).unwrap();
                return Some(format!("{} {}:{}", &t[..10], &t[12..14], &t[15..17]));
            }
        }
    }
    // gpu-screen-recorder: YYYY-MM-DD_HH-MM-SS (19 chars)
    // Pattern positions: DDDD-DD-DD_DD-DD-DD
    if b.len() >= 19 {
        for i in 0..=b.len() - 19 {
            let s = &b[i..i + 19];
            if s[4] == b'-'
                && s[7] == b'-'
                && s[10] == b'_'
                && s[13] == b'-'
                && s[16] == b'-'
                && s[..4].iter().all(u8::is_ascii_digit)
                && s[5..7].iter().all(u8::is_ascii_digit)
                && s[8..10].iter().all(u8::is_ascii_digit)
                && s[11..13].iter().all(u8::is_ascii_digit)
                && s[14..16].iter().all(u8::is_ascii_digit)
                && s[17..19].iter().all(u8::is_ascii_digit)
            {
                let t = std::str::from_utf8(&s[..19]).unwrap();
                return Some(format!("{} {}:{}", &t[..10], &t[11..13], &t[14..16]));
            }
        }
    }
    None
}

/// Format Unix timestamp as "YYYY-MM-DD HH:MM" using local time (Unix only).
pub fn fmt_ts_local(s: i64) -> String {
    #[cfg(unix)]
    {
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe {
            libc::localtime_r(&s, &mut tm);
        }
        return format!(
            "{}-{:02}-{:02} {:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min
        );
    }
    #[allow(unreachable_code)]
    fmt_ts(s)
}

/// Accurate Unix-timestamp → "YYYY-MM-DD HH:MM" using Howard Hinnant's civil calendar algorithm.
fn fmt_ts(s: i64) -> String {
    let days = s / 86400;
    let rem = s % 86400;
    let (h, m) = (rem / 3600, (rem % 3600) / 60);
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = if mth <= 2 { y + 1 } else { y };
    format!("{yr}-{mth:02}-{d:02} {h:02}:{m:02}")
}

/// Run a command asynchronously, capturing stdout and stderr.
pub async fn run_command_output_async(
    cmd: &str,
    args: &[&str],
) -> Result<std::process::Output, String> {
    let cmd = cmd.to_string();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        Command::new(&cmd)
            .args(&args)
            .output()
            .map_err(|e| format!("{cmd}: {e}"))
    })
    .await
    .map_err(|e| format!("spawn_blocking join: {e}"))?
}

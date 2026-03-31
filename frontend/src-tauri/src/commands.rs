//! Tauri Commands — complete backend for OpenGG.
//!
//! Audio routing ported from the working Python pulsectl code:
//!   pulse.sink_input_move(si.index, sink.index)
//! → pactl move-sink-input <si_index:u32> <sink_index:u32>

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{command, AppHandle, Emitter, Manager};

const AU_PATH: &str = "/org/opengg/Daemon/Audio";
const AU_IFACE: &str = "org.opengg.Daemon.Audio";
const RP_PATH: &str = "/org/opengg/Daemon/Replay";
const RP_IFACE: &str = "org.opengg.Daemon.Replay";

// ═══ Audio ═══
#[command] pub async fn get_channels() -> Result<String, String> { call_dbus("GetChannels", AU_PATH, AU_IFACE, ()).await }
/// Volume control with pactl fallback — controls both sinks and sink-inputs
#[command]
pub async fn set_volume(channel: String, volume: u32) -> Result<(), String> {
    // Try D-Bus first
    if call_dbus_void("SetVolume", AU_PATH, AU_IFACE, (channel.as_str(), volume)).await.is_ok() {
        return Ok(());
    }
    // Direct pactl fallback
    let pct = format!("{volume}%");
    if channel == "Master" {
        run_cmd("pactl", &["set-sink-volume", "@DEFAULT_SINK@", &pct])?;
    } else if channel == "Mic" {
        run_cmd("pactl", &["set-source-volume", "@DEFAULT_SOURCE@", &pct])?;
    } else {
        let sink = format!("OpenGG_{channel}");
        run_cmd("pactl", &["set-sink-volume", &sink, &pct])?;
    }
    Ok(())
}

#[command]
pub async fn set_mute(channel: String, muted: bool) -> Result<(), String> {
    if call_dbus_void("SetMute", AU_PATH, AU_IFACE, (channel.as_str(), muted)).await.is_ok() {
        return Ok(());
    }
    let val = if muted { "1" } else { "0" };
    if channel == "Master" {
        run_cmd("pactl", &["set-sink-mute", "@DEFAULT_SINK@", val])?;
    } else if channel == "Mic" {
        run_cmd("pactl", &["set-source-mute", "@DEFAULT_SOURCE@", val])?;
    } else {
        run_cmd("pactl", &["set-sink-mute", &format!("OpenGG_{channel}"), val])?;
    }
    Ok(())
}

/// Set volume for an individual app (sink-input) by its pactl index
#[command]
pub async fn set_app_volume(app_index: u32, volume: u32) -> Result<(), String> {
    let pct = format!("{volume}%");
    run_cmd("pactl", &["set-sink-input-volume", &app_index.to_string(), &pct])?;
    Ok(())
}
#[command] pub async fn get_apps() -> Result<String, String> {
    match call_dbus("GetApps", AU_PATH, AU_IFACE, ()).await {
        Ok(r) => Ok(r),
        Err(_) => scan_sink_inputs(),
    }
}

// ══════════════════════════════════════════════════════════════════════
//  ★ AUDIO ROUTING — Ported from Python pulsectl (backend.py line 110)
//
//  Python:   pulse.sink_input_move(si.index, sink.index)
//  Rust:     pactl move-sink-input <si_index> <sink_index>
//
//  BOTH arguments must be PulseAudio INTEGER INDICES.
//  Using a PipeWire node ID or a sink NAME string will silently fail.
// ══════════════════════════════════════════════════════════════════════

#[command]
pub async fn route_app(app_id: u32, channel: String) -> Result<(), String> {
    // Try D-Bus daemon first
    if call_dbus_void("RouteApp", AU_PATH, AU_IFACE, (app_id, channel.as_str())).await.is_ok() {
        return Ok(());
    }

    eprintln!("route_app: D-Bus unavailable, using pactl (index={app_id} → {channel})");

    // Get target sink's INTEGER INDEX (not name)
    let sink_idx = if channel == "default" || channel == "Master" {
        get_default_sink_index()?
    } else {
        let name = format!("OpenGG_{channel}");
        ensure_sink_exists(&name, &channel).await?;
        get_sink_index_by_name(&name)?
    };

    // Attempt 1: app_id IS the correct pactl sink-input index
    let r = Command::new("pactl")
        .args(["move-sink-input", &app_id.to_string(), &sink_idx.to_string()])
        .output().map_err(|e| format!("pactl: {e}"))?;

    if r.status.success() {
        eprintln!("route_app: ✓ sink-input {app_id} → sink #{sink_idx} ({channel})");
        return Ok(());
    }

    let err = String::from_utf8_lossy(&r.stderr);
    eprintln!("route_app: attempt 1 failed ({err}), scanning for correct index...");

    // Attempt 2: app_id might be a PipeWire node ID — find the correct pactl index
    if let Ok(si_idx) = find_pactl_si_for_pw_id(app_id) {
        let r2 = Command::new("pactl")
            .args(["move-sink-input", &si_idx.to_string(), &sink_idx.to_string()])
            .output().map_err(|e| format!("pactl: {e}"))?;
        if r2.status.success() {
            eprintln!("route_app: ✓ corrected: PW#{app_id} → pactl si#{si_idx} → sink #{sink_idx}");
            return Ok(());
        }
        return Err(format!("pactl: {}", String::from_utf8_lossy(&r2.stderr)));
    }

    Err(format!("route_app: no matching sink-input for id {app_id}"))
}

/// Get default sink's pactl integer index
fn get_default_sink_index() -> Result<u32, String> {
    let name = run_cmd("pactl", &["get-default-sink"])?;
    get_sink_index_by_name(&name)
}

/// Look up sink's pactl integer index by name — mirrors pulsectl.sink_list()
fn get_sink_index_by_name(sink_name: &str) -> Result<u32, String> {
    let j = run_cmd("pactl", &["-f", "json", "list", "sinks"])?;
    let sinks: Vec<serde_json::Value> = serde_json::from_str(&j).map_err(|e| format!("{e}"))?;
    for s in &sinks {
        if s["name"].as_str() == Some(sink_name) {
            if let Some(idx) = s["index"].as_u64() { return Ok(idx as u32); }
        }
    }
    Err(format!("sink '{sink_name}' not found"))
}

/// Cross-reference PipeWire node ID → pactl sink-input index
fn find_pactl_si_for_pw_id(pw_id: u32) -> Result<u32, String> {
    let j = run_cmd("pactl", &["-f", "json", "list", "sink-inputs"])?;
    let sis: Vec<serde_json::Value> = serde_json::from_str(&j).map_err(|e| format!("{e}"))?;
    let pw_str = pw_id.to_string();
    for si in &sis {
        let idx = si["index"].as_u64().unwrap_or(0) as u32;
        let p = &si["properties"];
        // PipeWire exposes its ID via multiple property keys
        if p["object.serial"].as_str() == Some(&pw_str)
            || p["object.id"].as_str() == Some(&pw_str)
            || p["node.id"].as_str() == Some(&pw_str)
        {
            return Ok(idx);
        }
    }
    Err(format!("no pactl si for PW#{pw_id}"))
}

/// Ensure virtual sink exists (non-blocking)
async fn ensure_sink_exists(name: &str, ch: &str) -> Result<(), String> {
    if let Ok(o) = Command::new("pactl").args(["list", "sinks", "short"]).output() {
        if String::from_utf8_lossy(&o.stdout).contains(name) { return Ok(()); }
    }
    let c = Command::new("pactl").args(["load-module", "module-null-sink",
        &format!("sink_name={name}"),
        &format!("sink_properties=node.description=\"OpenGG - {ch}\" node.nick=\"OpenGG - {ch}\" device.description=\"OpenGG - {ch}\""),
        "channels=2", "channel_map=front-left,front-right"
    ]).output().map_err(|e| format!("{e}"))?;
    if !c.status.success() { return Err(format!("{}", String::from_utf8_lossy(&c.stderr))); }
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    if let Ok(def) = run_cmd("pactl", &["get-default-sink"]) {
        for p in ["FL","FR"] { let _ = Command::new("pw-link").args([&format!("{name}:monitor_{p}"), &format!("{def}:playback_{p}")]).output(); }
    }
    Ok(())
}

/// Scan sink-inputs — mirrors pulsectl.sink_input_list()
/// ★ FIX 2: Filters out streams going to OpenGG virtual sinks' monitors
fn scan_sink_inputs() -> Result<String, String> {
    let j = run_cmd("pactl", &["-f", "json", "list", "sink-inputs"])?;
    let sis: Vec<serde_json::Value> = serde_json::from_str(&j).map_err(|e| format!("{e}"))?;
    let mut apps = Vec::new();
    for si in &sis {
        let idx = si["index"].as_u64().unwrap_or(0) as u32;
        let p = &si["properties"];
        let name = p["application.name"].as_str()
            .or(p["media.name"].as_str())
            .or(p["application.process.binary"].as_str())
            .unwrap_or("Unknown");
        let binary = p["application.process.binary"].as_str().unwrap_or("");

        // ★ FIX 2: Skip internal PipeWire/WirePlumber streams
        let name_lower = name.to_lowercase();
        if name_lower.contains("opengg") || name_lower == "wireplumber"
            || name_lower == "pipewire" || name_lower.contains("peak detect") {
            continue;
        }

        let sink_idx = si["sink"].as_u64().unwrap_or(0) as u32;
        let channel = lookup_sink_channel(sink_idx);

        // Include volume info (0-100) for per-app volume control
        let vol = si["volume"].as_object()
            .and_then(|v| v.values().next())
            .and_then(|ch| ch["value_percent"].as_str())
            .and_then(|s| s.trim_end_matches('%').parse::<u32>().ok())
            .unwrap_or(100);

        apps.push(serde_json::json!({
            "id": idx, "name": name, "binary": binary,
            "channel": channel, "icon": "", "volume": vol
        }));
    }

    // ★ Epic 3: Also scan source-outputs — apps recording from the mic
    // IDs are offset by 90000 to avoid conflicts with sink-input IDs.
    if let Ok(sj) = run_cmd("pactl", &["-f", "json", "list", "source-outputs"]) {
        if let Ok(sos) = serde_json::from_str::<Vec<serde_json::Value>>(&sj) {
            for (idx, so) in sos.iter().enumerate() {
                let p = &so["properties"];
                let name = p["application.name"].as_str()
                    .or(p["media.name"].as_str())
                    .or(p["application.process.binary"].as_str())
                    .unwrap_or("Unknown");
                let binary = p["application.process.binary"].as_str().unwrap_or("");
                let name_lower = name.to_lowercase();
                if name_lower.contains("opengg") || name_lower == "wireplumber"
                    || name_lower == "pipewire" || name_lower.contains("peak detect") {
                    continue;
                }
                let fake_id = 90000u32 + idx as u32;
                apps.push(serde_json::json!({
                    "id": fake_id, "name": name, "binary": binary,
                    "channel": "Mic", "icon": "", "volume": 100
                }));
            }
        }
    }

    Ok(serde_json::to_string(&apps).unwrap_or("[]".into()))
}

fn lookup_sink_channel(sink_idx: u32) -> String {
    if let Ok(j) = run_cmd("pactl", &["-f","json","list","sinks"]) {
        if let Ok(sinks) = serde_json::from_str::<Vec<serde_json::Value>>(&j) {
            for s in &sinks {
                if s["index"].as_u64() == Some(sink_idx as u64) {
                    let n = s["name"].as_str().unwrap_or("");
                    if let Some(ch) = n.strip_prefix("OpenGG_") { return ch.into(); }
                }
            }
        }
    }
    String::new()
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 1: Media Analysis via ffprobe
// ══════════════════════════════════════════════════════════════

#[derive(Serialize)]
pub struct MediaStream {
    pub index: u32,
    pub codec_type: String,  // "video" | "audio" | "subtitle"
    pub codec_name: String,  // "h264", "aac", "opus", etc.
    pub channels: u32,       // audio channel count (2=stereo)
    pub sample_rate: String,
    pub language: String,
    pub title: String,       // track title if set (e.g. "Game Audio", "Mic")
}

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

#[command]
pub async fn analyze_media(filepath: String) -> Result<MediaInfo, String> {
    let output = Command::new("ffprobe")
        .args(["-v", "quiet", "-print_format", "json",
            "-show_format", "-show_streams", &filepath])
        .output()
        .map_err(|e| format!("ffprobe: {e}"))?;

    if !output.status.success() {
        return Err(format!("ffprobe failed: {}", String::from_utf8_lossy(&output.stderr)));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("parse: {e}"))?;

    let fmt = &json["format"];
    let duration: f64 = fmt["duration"].as_str()
        .and_then(|s| s.parse().ok()).unwrap_or(0.0);

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
                                if d > 0.0 { fps = n / d; }
                            }
                        }
                    }
                    streams.push(MediaStream {
                        index: idx, codec_type, codec_name,
                        channels: 0, sample_rate: String::new(), language: lang, title,
                    });
                }
                "audio" => {
                    audio_count += 1;
                    let ch = s["channels"].as_u64().unwrap_or(2) as u32;
                    let sr = s["sample_rate"].as_str().unwrap_or("48000").to_string();
                    let track_title = if title.is_empty() {
                        format!("Audio {audio_count}")
                    } else { title };
                    streams.push(MediaStream {
                        index: idx, codec_type, codec_name,
                        channels: ch, sample_rate: sr, language: lang, title: track_title,
                    });
                }
                _ => {}
            }
        }
    }

    Ok(MediaInfo {
        duration, width, height, fps, video_codec,
        streams, video_streams: video_count, audio_streams: audio_count,
    })
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 4: File Renaming
// ══════════════════════════════════════════════════════════════

#[command]
pub async fn rename_clip(old_path: String, new_name: String) -> Result<String, String> {
    let old = PathBuf::from(&old_path);
    if !old.exists() { return Err(format!("File not found: {old_path}")); }

    let ext = old.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let dir = old.parent().unwrap_or(Path::new("."));
    // Sanitize filename
    // ★ Epic 2: Allow Unicode (Arabic/CJK) — only strip OS-illegal path chars
    let safe_name: String = new_name.chars()
        .filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\0'))
        .collect::<String>().trim().to_string();
    let new_path = dir.join(format!("{safe_name}.{ext}"));

    if new_path.exists() { return Err("A file with that name already exists".into()); }

    std::fs::rename(&old, &new_path).map_err(|e| format!("rename: {e}"))?;

    // Update SQLite metadata
    if let Ok(db) = open_db() {
        let _ = db.execute("UPDATE clip_meta SET filepath=?1 WHERE filepath=?2",
            rusqlite::params![new_path.to_string_lossy().to_string(), old_path]);
        let _ = db.execute("UPDATE trim_state SET filepath=?1 WHERE filepath=?2",
            rusqlite::params![new_path.to_string_lossy().to_string(), old_path]);
    }

    // Rename thumbnail too
    let old_id = format!("{:x}", hash_str(&old_path));
    let new_fp = new_path.to_string_lossy().to_string();
    let new_id = format!("{:x}", hash_str(&new_fp));
    let td = thumb_dir();
    let old_thumb = td.join(format!("{old_id}.jpg"));
    let new_thumb = td.join(format!("{new_id}.jpg"));
    if old_thumb.exists() { let _ = std::fs::rename(&old_thumb, &new_thumb); }

    Ok(new_fp)
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 5: Timeline Export (placeholder — builds ffmpeg args)
// ══════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct TimelineExportClip {
    pub filepath: String,
    pub start: f64,
    pub end: f64,
    pub track: String,
}

#[command]
pub async fn export_timeline(
    clips: Vec<TimelineExportClip>,
    output_dir: String,
    output_name: String,
) -> Result<String, String> {
    if clips.is_empty() { return Err("No clips to export".into()); }

    let dir = if output_dir.is_empty() { default_clips_dir() } else { PathBuf::from(shexp(&output_dir)) };
    let _ = std::fs::create_dir_all(&dir);
    let safe: String = output_name.chars().filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\0')).collect::<String>().trim().to_string();
    let outfile = dir.join(format!("{}.mp4", if safe.is_empty() { "export" } else { &safe }));

    // For single-source trim (most common case), use stream copy
    let video_clips: Vec<&TimelineExportClip> = clips.iter().filter(|c| c.track == "video").collect();
    if video_clips.len() == 1 {
        let vc = video_clips[0];
        let r = Command::new("ffmpeg")
            .args(["-i", &vc.filepath, "-ss", &format!("{:.3}", vc.start),
                "-to", &format!("{:.3}", vc.end), "-c", "copy",
                "-avoid_negative_ts", "make_zero", "-y",
                &outfile.to_string_lossy()])
            .output().map_err(|e| format!("{e}"))?;
        if r.status.success() { return Ok(outfile.to_string_lossy().to_string()); }
        return Err(format!("ffmpeg: {}", String::from_utf8_lossy(&r.stderr)));
    }

    // Multi-clip: use ffmpeg concat demuxer (future — complex filter graph)
    // For now, return a descriptive error
    Err(format!("Multi-clip export ({} clips) coming soon. Use single-clip trim for now.", video_clips.len()))
}

/// Generate audio waveform peaks data for visualization.
/// Uses ffmpeg to extract PCM samples, then computes peaks.
/// Returns JSON array of peak values (0.0-1.0) for the given audio stream.
#[command]
pub async fn generate_waveform(filepath: String, stream_index: u32, num_peaks: u32) -> Result<Vec<f32>, String> {
    let peaks_count = num_peaks.max(100).min(2000);

    // Extract raw PCM audio from the specified stream
    let output = Command::new("ffmpeg")
        .args(["-i", &filepath, "-map", &format!("0:{stream_index}"),
            "-ac", "1", "-f", "s16le", "-ar", "8000", "-"])
        .output()
        .map_err(|e| format!("ffmpeg waveform: {e}"))?;

    if !output.status.success() || output.stdout.is_empty() {
        return Ok(vec![0.0; peaks_count as usize]);
    }

    // Parse raw s16le samples
    let samples: Vec<i16> = output.stdout.chunks_exact(2)
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
            let max_abs = samples[start..end].iter()
                .map(|s| s.unsigned_abs() as f32)
                .fold(0.0f32, f32::max);
            (max_abs / 32768.0).min(1.0)
        })
        .collect();

    Ok(peaks)
}

// ══════════════════════════════════════════════════════════════
//  ★ Epic 3 P3+P4: Export with audio downmix + overlay burn
// ══════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct ExportOverlay {
    pub overlay_type: String,       // "text" | "image" | "gif"
    pub content: String,            // text string or file path
    pub x: f64,                    // percentage 0-100
    pub y: f64,
    pub scale: f64,
    pub start_sec: f64,
    pub dur_sec: f64,
    pub font_name: Option<String>, // e.g. "Impact", "Arial" — resolved to system font path
}

#[derive(Deserialize)]
pub struct ExportAudioTrack {
    pub stream_index: u32,
    pub volume: f64,  // 0.0-1.0
    pub muted: bool,
}

/// Export a clip with strict branching:
///
///  Condition A — No visual overlays → fast stream copy (`-c copy`).
///    Input-side seeking, zero quality loss, no re-encoding.
///
///  Condition B — Has visual overlays → filter_complex + libx264 re-encode.
///    Builds drawtext/overlay filter graph, burns in at CRF 18.
///
/// Stderr is captured to `stderr_log` so error details can be surfaced to the
/// user instead of silently disappearing into the terminal.
#[command]
pub async fn export_clip_with_filters(
    app: AppHandle,
    input_path: String,
    start_sec: f64,
    end_sec: f64,
    audio_tracks: Vec<ExportAudioTrack>,
    overlays: Vec<ExportOverlay>,
    target_mb: f64,
    output_path: String,
) -> Result<String, String> {
    let dur = end_sec - start_sec;
    if dur <= 0.0 { return Err("Invalid trim range".into()); }

    let mut out = if output_path.is_empty() { auto_name(&input_path, "_export") } else { smart_prefix(&output_path) };
    if out == input_path || Path::new(&out) == Path::new(&input_path) {
        out = auto_name(&input_path, "_export");
        eprintln!("export: output collided with input, renamed to {out}");
    }

    // Shared stderr accumulator — lets error path report the actual FFmpeg message.
    let stderr_log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // ─────────────────────────────────────────────────────────────
    // Condition A: NO valid visual overlays → stream copy (fast, lossless)
    // ─────────────────────────────────────────────────────────────
    let has_valid_overlays = overlays.iter().any(|o| match o.overlay_type.as_str() {
        "text"        => !o.content.is_empty(),
        "image" | "gif" => !o.content.is_empty() && Path::new(&o.content).exists(),
        _             => false,
    });

    if !has_valid_overlays {
        // Even with no visual overlays we still need correct audio handling:
        //   • Multiple audio tracks  → downmix to one AAC stream (amix).
        //   • Per-track volume/mute  → apply volume filter even in copy mode.
        //   • Single default track   → copy video + encode audio to AAC.
        // Without explicit -map ffmpeg only picks the "best" audio stream,
        // silently discarding any additional tracks.
        let audio_indices_a = get_audio_stream_global_indices(&input_path);
        let to_rel_a = |global_idx: u32| -> Option<u32> {
            audio_indices_a.iter().position(|&g| g == global_idx).map(|i| i as u32)
        };

        let valid_audio_a: Vec<(&ExportAudioTrack, u32)> = audio_tracks.iter()
            .filter(|t| !t.muted && t.volume > 0.0)
            .filter_map(|t| to_rel_a(t.stream_index).map(|rel| (t, rel)))
            .collect();

        let mut cond_a_fc: Vec<String> = Vec::new();
        let a_map: String = if audio_tracks.is_empty() {
            // No audio track info from frontend: map all source audio as-is.
            "0:a".to_string()
        } else if valid_audio_a.is_empty() {
            // All tracks muted — output silence.
            cond_a_fc.push("anullsrc=r=48000:cl=stereo:d=1[aout]".into());
            "[aout]".to_string()
        } else if valid_audio_a.len() == 1 && (valid_audio_a[0].0.volume - 1.0).abs() < 0.01 {
            // Single track at unity gain → no filter needed, direct map.
            format!("0:a:{}", valid_audio_a[0].1)
        } else {
            // Multiple tracks OR custom volume → build amix with per-track gain.
            let mut mix_ins = Vec::new();
            for (i, (t, rel)) in valid_audio_a.iter().enumerate() {
                let lbl = format!("[ca{i}]");
                cond_a_fc.push(format!("[0:a:{rel}]volume={:.4}{lbl}", t.volume));
                mix_ins.push(lbl);
            }
            let joined = mix_ins.join("");
            cond_a_fc.push(format!(
                "{joined}amix=inputs={}:normalize=0:duration=longest[amix]",
                mix_ins.len()
            ));
            "[amix]".to_string()
        };

        let mut args: Vec<String> = vec![
            "-y".into(),
            "-ss".into(), format!("{start_sec:.3}"),
            "-to".into(), format!("{end_sec:.3}"),
            "-i".into(), input_path.clone(),
        ];
        if !cond_a_fc.is_empty() {
            args.extend(["-filter_complex".into(), cond_a_fc.join(";")]);
        }
        args.extend([
            "-map".into(), "0:v".into(),
            "-map".into(), a_map,
            "-c:v".into(), "copy".into(),
            "-c:a".into(), "aac".into(),
            "-b:a".into(), "192k".into(),
            "-avoid_negative_ts".into(), "make_zero".into(),
            out.clone(),
        ]);
        eprintln!("export (copy+audio): ffmpeg {}", args.join(" "));
        let _ = app.emit("export-progress", serde_json::json!({"percent": 0, "stage": "copying", "speed": ""}));

        let mut child = Command::new("ffmpeg")
            .args(&args)
            .stderr(std::process::Stdio::piped())
            .spawn().map_err(|e| format!("ffmpeg: {e}"))?;

        let log_c = Arc::clone(&stderr_log);
        if let Some(stderr) = child.stderr.take() {
            tokio::task::spawn_blocking(move || {
                use std::io::{BufRead, BufReader};
                for line in BufReader::new(stderr).lines().flatten() {
                    eprintln!("ffmpeg: {line}");
                    log_c.lock().unwrap().push(line);
                }
            });
        }

        { *app.state::<crate::ExportProcess>().child.lock().unwrap() = Some((child, out.clone())); }

        let status = {
            let es = app.state::<crate::ExportProcess>();
            let mut lock = es.child.lock().unwrap();
            if let Some((ref mut c, _)) = *lock { Some(c.wait().map_err(|e| format!("ffmpeg: {e}"))?) } else { None }
        };
        { *app.state::<crate::ExportProcess>().child.lock().unwrap() = None; }
        for i in 0..20 { let _ = std::fs::remove_file(std::env::temp_dir().join(format!("opengg_text_{i}.txt"))); }

        return match status {
            Some(s) if s.success() => {
                let _ = app.emit("export-progress", serde_json::json!({"percent": 100, "stage": "done", "speed": ""}));
                Ok(out)
            }
            Some(_) => {
                std::thread::sleep(std::time::Duration::from_millis(80)); // let stderr thread flush
                let tail = stderr_log.lock().unwrap().iter().rev().take(8).rev().cloned().collect::<Vec<_>>().join("\n");
                let _ = app.emit("export-progress", serde_json::json!({"percent": -1, "stage": "error", "speed": "failed"}));
                Err(format!("FFmpeg stream copy failed.\n\nDetails:\n{tail}"))
            }
            None => Err("Export was cancelled".into()),
        };
    }

    // ─────────────────────────────────────────────────────────────
    // Condition B: Has overlays → filter_complex + libx264 re-encode
    // ─────────────────────────────────────────────────────────────
    let audio_global_indices = get_audio_stream_global_indices(&input_path);
    eprintln!("export (encode): audio streams {:?}", audio_global_indices);

    let to_audio_relative = |global_idx: u32| -> Option<u32> {
        audio_global_indices.iter().position(|&g| g == global_idx).map(|i| i as u32)
    };

    // Input-side seeking (before the main -i so PTS starts at 0)
    let mut inputs: Vec<String> = vec![
        "-ss".into(), format!("{start_sec:.3}"),
        "-to".into(), format!("{end_sec:.3}"),
        "-i".into(), input_path.clone(),
    ];
    let mut filter_parts: Vec<String> = Vec::new();
    let mut input_count = 1u32;
    let mut video_label = "[0:v]".to_string();

    // Audio filter graph
    let valid_audio: Vec<(&ExportAudioTrack, u32)> = audio_tracks.iter()
        .filter(|t| !t.muted && t.volume > 0.0)
        .filter_map(|t| to_audio_relative(t.stream_index).map(|rel| (t, rel)))
        .collect();
    let audio_label = if valid_audio.is_empty() {
        filter_parts.push("anullsrc=r=48000:cl=stereo:d=1[aout]".into());
        "[aout]".to_string()
    } else if valid_audio.len() == 1 {
        let (t, rel) = valid_audio[0];
        filter_parts.push(format!("[0:a:{rel}]volume={:.2}[amix]", t.volume));
        "[amix]".to_string()
    } else {
        let mut mix_ins = Vec::new();
        for (i, (t, rel)) in valid_audio.iter().enumerate() {
            let lbl = format!("[a{i}]");
            filter_parts.push(format!("[0:a:{rel}]volume={:.2}{lbl}", t.volume));
            mix_ins.push(lbl);
        }
        let mix_in = mix_ins.join("");
        filter_parts.push(format!("{mix_in}amix=inputs={}:duration=longest[amix]", valid_audio.len()));
        "[amix]".to_string()
    };

    // Probe resolution for proportional sizing
    let (src_w, src_h) = probe_resolution(&input_path);
    eprintln!("export (encode): source {src_w}x{src_h}");

    // Burn overlays into the video filter chain
    for ov in &overlays {
        let ov_start = (ov.start_sec - start_sec).max(0.0);
        let ov_end   = (ov.start_sec + ov.dur_sec - start_sec).min(dur);
        if ov_end <= ov_start { continue; }
        let enable = format!("between(t\\,{ov_start:.2}\\,{ov_end:.2})");
        match ov.overlay_type.as_str() {
            "text" => {
                let tmp = std::env::temp_dir().join(format!("opengg_text_{}.txt", filter_parts.len()));
                if std::fs::write(&tmp, &ov.content).is_err() { continue; }
                let fs = ((src_h as f64 / 1080.0) * 24.0 * ov.scale / 100.0).max(8.0) as u32;
                // Resolve font: honour user choice, fall back to best system font.
                let font = find_system_font_by_name(ov.font_name.as_deref());
                let x_expr = format!("(W*{}/100-tw/2)", ov.x as u32);
                let y_expr = format!("(H*{}/100-th/2)", ov.y as u32);
                let next = format!("[vov{}]", filter_parts.len());
                // Shadow (shadowx/y + alpha) replaces the old hard-coded black border.
                // Gives a natural depth effect without visual artefacts on bright bg.
                filter_parts.push(format!(
                    "{video_label}drawtext=fontfile='{font}':textfile='{}':fontsize={fs}:fontcolor=white:shadowx=2:shadowy=2:shadowcolor=black@0.65:x={x_expr}:y={y_expr}:enable='{enable}'{next}",
                    tmp.to_string_lossy()
                ));
                video_label = next;
            }
            "image" | "gif" => {
                if !ov.content.is_empty() && Path::new(&ov.content).exists() {
                    let is_gif = ov.overlay_type == "gif" || ov.content.to_lowercase().ends_with(".gif");
                    if is_gif { inputs.push("-ignore_loop".into()); inputs.push("0".into()); }
                    inputs.push("-i".into()); inputs.push(ov.content.clone());
                    let idx = input_count; input_count += 1;
                    // Force even pixel dimensions.
                    // `2*trunc(N/2)` is an FFmpeg expression that rounds N down
                    // to the nearest even integer — libx264 rejects odd dimensions.
                    let scale_w = ((src_w as f64 * ov.scale / 100.0 * 0.3) as u32 / 2 * 2).max(2);
                    let scale_lbl = format!("[sovl{}]", filter_parts.len());
                    // `format=rgba` converts GIF palette / PNG BGRA → RGBA before
                    // scale so the overlay filter always gets a well-defined pixel
                    // format. `-2` on height auto-rounds to even while preserving AR.
                    // DO NOT add a `loop` filter — `-ignore_loop 0` on the input
                    // is sufficient and the extra filter causes a fatal hang.
                    filter_parts.push(format!(
                        "[{idx}:v]format=rgba,scale=w=2*trunc({scale_w}/2):h=-2:flags=lanczos{scale_lbl}"
                    ));
                    let x_expr = format!("(W*{}/100-overlay_w/2)", ov.x as u32);
                    let y_expr = format!("(H*{}/100-overlay_h/2)", ov.y as u32);
                    let next = format!("[vov{}]", filter_parts.len());
                    filter_parts.push(format!(
                        "{video_label}{scale_lbl}overlay=x={x_expr}:y={y_expr}:enable='{enable}':shortest=1{next}"
                    ));
                    video_label = next;
                }
            }
            _ => {}
        }
    }

    // Belt-and-suspenders pixel format fix:
    //   1. format=yuv420p node at the END of the video chain (in-graph conversion).
    //   2. -pix_fmt yuv420p on the output side (encoder-level enforcement).
    // Together they handle every deprecated / unusual input format (yuvj420p,
    // bgra, indexed-color) that would otherwise make libx264 return -22.
    let fmt_label = format!("[vfmt{}]", filter_parts.len());
    filter_parts.push(format!("{video_label}format=yuv420p{fmt_label}"));

    let filter_complex = filter_parts.join(";");

    let mut args: Vec<String> = vec!["-y".into()];
    args.extend(inputs);
    args.extend(["-filter_complex".into(), filter_complex]);
    args.push("-map".into()); args.push(fmt_label);
    args.push("-map".into()); args.push(audio_label);

    if target_mb > 0.0 {
        let video_kbps = ((target_mb * 8192.0 / dur) - 128.0).max(100.0);
        args.extend(["-c:v".into(), "libx264".into(), "-pix_fmt".into(), "yuv420p".into(),
            "-b:v".into(), format!("{}k", video_kbps as u32), "-preset".into(), "fast".into()]);
    } else {
        args.extend(["-c:v".into(), "libx264".into(), "-pix_fmt".into(), "yuv420p".into(),
            "-crf".into(), "18".into(), "-preset".into(), "fast".into()]);
    }
    args.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "128k".into()]);
    args.push(out.clone());

    eprintln!("export (encode): ffmpeg {}", args.join(" "));
    let _ = app.emit("export-progress", serde_json::json!({"percent": 0, "stage": "encoding", "speed": ""}));

    let mut child = Command::new("ffmpeg")
        .args(&args)
        .stderr(std::process::Stdio::piped())
        .spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    if let Some(stderr) = child.stderr.take() {
        let app_c = app.clone();
        let log_c = Arc::clone(&stderr_log);
        tokio::task::spawn_blocking(move || {
            parse_ffmpeg_progress(stderr, dur, 0.0, 100.0, &app_c, Some(log_c));
        });
    }

    { *app.state::<crate::ExportProcess>().child.lock().unwrap() = Some((child, out.clone())); }

    let status = {
        let es = app.state::<crate::ExportProcess>();
        let mut lock = es.child.lock().unwrap();
        if let Some((ref mut c, _)) = *lock { Some(c.wait().map_err(|e| format!("ffmpeg: {e}"))?) } else { None }
    };
    { *app.state::<crate::ExportProcess>().child.lock().unwrap() = None; }
    for i in 0..20 { let _ = std::fs::remove_file(std::env::temp_dir().join(format!("opengg_text_{i}.txt"))); }

    match status {
        Some(s) if s.success() => {
            let _ = app.emit("export-progress", serde_json::json!({"percent": 100, "stage": "done", "speed": ""}));
            Ok(out)
        }
        Some(_) => {
            std::thread::sleep(std::time::Duration::from_millis(80));
            let tail = stderr_log.lock().unwrap().iter().rev().take(8).rev().cloned().collect::<Vec<_>>().join("\n");
            let _ = app.emit("export-progress", serde_json::json!({"percent": -1, "stage": "error", "speed": "failed"}));
            Err(format!("FFmpeg encode failed.\n\nDetails:\n{tail}"))
        }
        None => Err("Export was cancelled".into()),
    }
}

/// ★ Epic 3: Cancel the running FFmpeg export
#[command]
pub async fn cancel_export(app: AppHandle) -> Result<(), String> {
    let export_state = app.state::<crate::ExportProcess>();
    let mut lock = export_state.child.lock().unwrap();
    if let Some((mut child, output_path)) = lock.take() {
        eprintln!("cancel_export: killing FFmpeg process");
        let _ = child.kill();
        let _ = child.wait(); // reap zombie
        // Delete the partial/corrupt output file
        if std::path::Path::new(&output_path).exists() {
            let _ = std::fs::remove_file(&output_path);
            eprintln!("cancel_export: deleted partial file {output_path}");
        }
        let _ = app.emit("export-progress", serde_json::json!({"percent": -1, "stage": "cancelled", "speed": ""}));
        Ok(())
    } else {
        Ok(()) // nothing running
    }
}

// ═══ App Lifecycle ═══

/// ★ Epic 4: Graceful quit — shows window if hidden, then exits
#[command]
pub async fn quit_app(app: AppHandle) -> Result<(), String> {
    eprintln!("OpenGG: quit requested");
    {
        let state = app.state::<crate::ExportProcess>();
        let mut lock = state.child.lock().unwrap();
        if let Some((mut child, path)) = lock.take() {
            let _ = child.kill();
            let _ = child.wait();
            if std::path::Path::new(&path).exists() { let _ = std::fs::remove_file(&path); }
        }
    }
    std::process::exit(0);
}

// ═══ Replay ═══
#[command] pub async fn get_recorder_status() -> Result<String, String> { call_dbus("GetStatus", RP_PATH, RP_IFACE, ()).await.or(Ok("idle".into())) }
#[command] pub async fn start_replay(duration: u32) -> Result<(), String> { call_dbus_void("StartReplay", RP_PATH, RP_IFACE, (duration,)).await }
#[command] pub async fn stop_recorder() -> Result<(), String> { call_dbus_void("Stop", RP_PATH, RP_IFACE, ()).await }
#[command] pub async fn save_replay() -> Result<(), String> { call_dbus_void("SaveReplay", RP_PATH, RP_IFACE, ()).await }

// ═══ SQLite ═══
fn clips_db_path() -> PathBuf { dirs::data_dir().unwrap_or_else(|| PathBuf::from("~/.local/share")).join("opengg/clips.db") }
fn open_db() -> Result<Connection, String> { Connection::open(clips_db_path()).map_err(|e| format!("DB: {e}")) }
pub fn init_clips_db() -> Result<(), String> {
    let p = clips_db_path(); if let Some(d) = p.parent() { std::fs::create_dir_all(d).ok(); }
    open_db()?.execute_batch("CREATE TABLE IF NOT EXISTS clip_meta(filepath TEXT PRIMARY KEY,custom_name TEXT DEFAULT '',favorite INTEGER DEFAULT 0,tags TEXT DEFAULT '',notes TEXT DEFAULT '');
     CREATE TABLE IF NOT EXISTS trim_state(filepath TEXT PRIMARY KEY,trim_start REAL DEFAULT 0,trim_end REAL DEFAULT 0);").map_err(|e| format!("{e}"))
}
fn get_meta_map() -> HashMap<String,(String,bool,String)> {
    let mut m = HashMap::new();
    if let Ok(c) = open_db() {
        let _ = c.execute("ALTER TABLE clip_meta ADD COLUMN game_tag TEXT DEFAULT ''", []);
        if let Ok(mut s) = c.prepare("SELECT filepath,custom_name,favorite,COALESCE(game_tag,'') FROM clip_meta") {
            let _ = s.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,bool>(2)?,r.get::<_,String>(3)?))).map(|rows| { for r in rows.flatten() { m.insert(r.0,(r.1,r.2,r.3)); } });
        }
    }
    m
}

// ═══ Clips ═══
#[derive(Debug,Serialize,Clone)]
pub struct ClipInfo { pub id:String, pub filename:String, pub filepath:String, pub filesize:u64, pub created:String, pub duration:f64, pub width:u32, pub height:u32, pub game:String, pub custom_name:String, pub favorite:bool, pub thumbnail:String }
const VIDEO_EXTS: &[&str] = &["mp4","mkv","webm","avi","mov","ts","flv"];

/// Lightweight clip counter — counts video files without reading metadata.
/// Used by the Home page dashboard so it doesn't trigger full ffprobe scans.
#[command]
pub async fn get_clips_count(folder: String) -> Result<usize, String> {
    let dir = resolve_clips_dir(&folder);
    if !dir.exists() { return Ok(0); }
    let count = std::fs::read_dir(&dir)
        .map_err(|e| format!("{e}"))?
        .flatten()
        .filter(|e| {
            let p = e.path();
            p.is_file() && {
                let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
                VIDEO_EXTS.contains(&ext.as_str())
            }
        })
        .count();
    Ok(count)
}

#[command] pub async fn get_clips(folder: String) -> Result<Vec<ClipInfo>, String> {
    let dirs = get_all_clip_dirs(&folder);
    let meta = get_meta_map(); let td = thumb_dir(); let _ = std::fs::create_dir_all(&td);
    let mut clips = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in &dirs {
        if !dir.exists() { continue; }
        let entries = match std::fs::read_dir(dir) { Ok(r) => r, Err(_) => continue };
        for e in entries.flatten() {
            let p = e.path(); if !p.is_file() { continue; }
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if !VIDEO_EXTS.contains(&ext.as_str()) { continue; }
            let fp = p.to_string_lossy().to_string();
            if seen.contains(&fp) { continue; }
            seen.insert(fp.clone());
            let m = match e.metadata() { Ok(m) => m, Err(_) => continue };
            let fname = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            let id = format!("{:x}", hash_str(&fp));
            let created = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| fmt_ts(d.as_secs() as i64)).unwrap_or_default();
            let (dur,w,h) = probe_video(&p);
            // ★ Epic 2 Task 2: Use file_stem() so extensions are never included in the game name
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("Unknown");
            let game_from_filename = stem.split('_').next().unwrap_or("Unknown").replace('-', " ");
            let (cn,fav,game_tag) = meta.get(&fp).cloned().unwrap_or_default();
            let game = if game_tag.is_empty() { game_from_filename } else { game_tag };
            let thumb = td.join(format!("{id}.jpg"));
            let thumbnail = if thumb.exists() { thumb.to_string_lossy().to_string() } else { String::new() };
            clips.push(ClipInfo{id,filename:fname,filepath:fp,filesize:m.len(),created,duration:dur,width:w,height:h,game,custom_name:cn,favorite:fav,thumbnail});
        }
    }
    clips.sort_by(|a,b| b.created.cmp(&a.created)); Ok(clips)
}

/// Fetch metadata for a single file — used by the frontend file-watcher listener.
#[command]
pub async fn get_clip_by_path(filepath: String) -> Result<Option<ClipInfo>, String> {
    let p = PathBuf::from(&filepath);
    if !p.is_file() { return Ok(None); }
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if !VIDEO_EXTS.contains(&ext.as_str()) { return Ok(None); }
    let meta = get_meta_map();
    let td = thumb_dir(); let _ = std::fs::create_dir_all(&td);
    let m = p.metadata().map_err(|e| format!("{e}"))?;
    let fname = p.file_name().unwrap_or_default().to_string_lossy().to_string();
    let fp = filepath.clone();
    let id = format!("{:x}", hash_str(&fp));
    let created = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| fmt_ts(d.as_secs() as i64)).unwrap_or_default();
    let (dur,w,h) = probe_video(&p);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("Unknown");
    let game_from_filename = stem.split('_').next().unwrap_or("Unknown").replace('-', " ");
    let (cn,fav,game_tag) = meta.get(&fp).cloned().unwrap_or_default();
    let game = if game_tag.is_empty() { game_from_filename } else { game_tag };
    let thumb = td.join(format!("{id}.jpg"));
    let thumbnail = if thumb.exists() { thumb.to_string_lossy().to_string() } else { String::new() };
    Ok(Some(ClipInfo{id,filename:fname,filepath:fp,filesize:m.len(),created,duration:dur,width:w,height:h,game,custom_name:cn,favorite:fav,thumbnail}))
}
#[command] pub async fn generate_thumbnail(filepath: String) -> Result<String, String> {
    let id = format!("{:x}",hash_str(&filepath)); let d = thumb_dir(); let _ = std::fs::create_dir_all(&d);
    let out = d.join(format!("{id}.jpg")); if out.exists() { return Ok(out.to_string_lossy().to_string()); }
    let dur = probe_duration(&filepath); let seek = if dur>1.0{dur*0.1}else{0.0};
    // ★ Epic 3 P2: High-quality thumbnails — 1280px, quality 2
    let r = Command::new("ffmpeg").args([
        "-ss", &format!("{seek:.2}"), "-i", &filepath,
        "-vframes", "1", "-vf", "scale=1280:-1", "-q:v", "2", "-y",
        &out.to_string_lossy()
    ]).output().map_err(|e| format!("{e}"))?;
    if r.status.success() && out.exists() { Ok(out.to_string_lossy().to_string()) }
    else { Err(format!("ffmpeg: {}", String::from_utf8_lossy(&r.stderr))) }
}
#[derive(Deserialize)] pub struct ClipMetaUpdate { pub filepath:String, pub custom_name:Option<String>, pub favorite:Option<bool>, pub game_tag:Option<String>, pub notes:Option<String> }
#[command] pub async fn set_clip_meta(update: ClipMetaUpdate) -> Result<(), String> {
    let db = open_db()?;
    let _ = db.execute("ALTER TABLE clip_meta ADD COLUMN game_tag TEXT DEFAULT ''", []);
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

/// Get full clip metadata including notes (for overlay persistence)
#[command]
pub async fn get_clip_meta(filepath: String) -> Result<String, String> {
    let db = open_db()?;
    let _ = db.execute("ALTER TABLE clip_meta ADD COLUMN game_tag TEXT DEFAULT ''", []);
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

/// Take a screenshot at a specific timestamp.
/// `output_dir`: optional override; falls back to `~/Pictures`.
#[command]
pub async fn take_screenshot(filepath: String, time_sec: f64, output_dir: Option<String>) -> Result<String, String> {
    let pics_dir = match output_dir.as_deref().filter(|s| !s.is_empty()) {
        Some(d) => PathBuf::from(shexp(d)),
        None    => dirs::picture_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures")),
    };
    let _ = std::fs::create_dir_all(&pics_dir);
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let out = pics_dir.join(format!("opengg_screenshot_{ts}.png"));

    let r = Command::new("ffmpeg").args([
        "-ss", &format!("{time_sec:.3}"),
        "-i", &filepath,
        "-vframes", "1",
        "-q:v", "2",
        "-y", &out.to_string_lossy(),
    ]).output().map_err(|e| format!("ffmpeg: {e}"))?;

    if r.status.success() && out.exists() {
        Ok(out.to_string_lossy().to_string())
    } else {
        Err(format!("Screenshot failed: {}", String::from_utf8_lossy(&r.stderr)))
    }
}
#[command] pub async fn delete_clip(filepath: String) -> Result<(), String> { if Path::new(&filepath).exists(){std::fs::remove_file(&filepath).map_err(|e| format!("{e}"))?;} let id=format!("{:x}",hash_str(&filepath)); let t=thumb_dir().join(format!("{id}.jpg")); if t.exists(){let _=std::fs::remove_file(&t);} if let Ok(c)=open_db(){let _=c.execute("DELETE FROM clip_meta WHERE filepath=?1",[&filepath]); let _=c.execute("DELETE FROM trim_state WHERE filepath=?1",[&filepath]);} Ok(()) }
#[command] pub async fn save_trim_state(filepath: String, trim_start: f64, trim_end: f64) -> Result<(), String> { open_db()?.execute("INSERT INTO trim_state(filepath,trim_start,trim_end) VALUES(?1,?2,?3) ON CONFLICT(filepath) DO UPDATE SET trim_start=?2,trim_end=?3", rusqlite::params![filepath,trim_start,trim_end]).map_err(|e| format!("{e}"))?; Ok(()) }
#[derive(Serialize)] pub struct TrimState { pub trim_start: f64, pub trim_end: f64 }
#[command] pub async fn get_trim_state(filepath: String) -> Result<Option<TrimState>, String> { let c=open_db()?; let mut s=c.prepare("SELECT trim_start,trim_end FROM trim_state WHERE filepath=?1").map_err(|e| format!("{e}"))?; match s.query_row([&filepath],|r| Ok(TrimState{trim_start:r.get(0)?,trim_end:r.get(1)?})){Ok(t)=>Ok(Some(t)),Err(rusqlite::Error::QueryReturnedNoRows)=>Ok(None),Err(e)=>Err(format!("{e}"))} }
#[command] pub async fn trim_clip(input_path: String, start_sec: f64, end_sec: f64, output_path: String) -> Result<String, String> { let mut out=if output_path.is_empty(){auto_name(&input_path,"_trim")}else{output_path}; if out==input_path{out=auto_name(&input_path,"_trim")} let r=Command::new("ffmpeg").args(["-i",&input_path,"-ss",&format!("{start_sec:.3}"),"-to",&format!("{end_sec:.3}"),"-c","copy","-avoid_negative_ts","make_zero","-y",&out]).output().map_err(|e| format!("{e}"))?; if r.status.success(){Ok(out)}else{Err(format!("{}",String::from_utf8_lossy(&r.stderr)))} }
/// Export with target size + real-time progress via Tauri events.
/// Parses ffmpeg stderr for "time=HH:MM:SS.xx" to calculate %.
#[command]
pub async fn export_clip_sized(app: AppHandle, input_path: String, start_sec: f64, end_sec: f64, target_mb: f64, output_path: String) -> Result<String, String> {
    let dur = end_sec - start_sec;
    if dur <= 0.0 { return Err("Invalid trim range".into()); }

    let mut out = if output_path.is_empty() { auto_name(&input_path, &format!("_{}mb", target_mb as u32)) } else { smart_prefix(&output_path) };
    if out == input_path { out = auto_name(&input_path, &format!("_{}mb", target_mb as u32)); }

    if target_mb <= 0.0 {
        // Original quality — stream copy (fast, no progress needed)
        let _ = app.emit("export-progress", serde_json::json!({"percent": 50, "stage": "copying"}));
        let result = trim_clip(input_path, start_sec, end_sec, out.clone()).await;
        let _ = app.emit("export-progress", serde_json::json!({"percent": 100, "stage": "done"}));
        return result;
    }

    let audio_kbps: f64 = 128.0;
    let total_kbps = target_mb * 8192.0 / dur;
    let video_kbps = (total_kbps - audio_kbps).max(100.0);
    let vbr = format!("{}k", video_kbps as u32);

    // Pass 1 (analyze)
    let _ = app.emit("export-progress", serde_json::json!({"percent": 0, "stage": "pass1"}));
    let mut child1 = Command::new("ffmpeg")
        .args(["-y", "-i", &input_path, "-ss", &format!("{start_sec:.3}"), "-to", &format!("{end_sec:.3}"),
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-b:v", &vbr, "-preset", "fast", "-pass", "1",
            "-an", "-f", "null", "/dev/null"])
        .stderr(std::process::Stdio::piped())
        .spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    // Stream progress from pass 1 (0-45%)
    if let Some(stderr) = child1.stderr.take() {
        let app_clone = app.clone();
        let dur_clone = dur;
        tokio::task::spawn_blocking(move || {
            parse_ffmpeg_progress(stderr, dur_clone, 0.0, 45.0, &app_clone, None);
        });
    }
    let _status1 = child1.wait().map_err(|e| format!("ffmpeg wait: {e}"))?;

    // Pass 2 (encode)
    let _ = app.emit("export-progress", serde_json::json!({"percent": 45, "stage": "pass2"}));
    let mut child2 = Command::new("ffmpeg")
        .args(["-y", "-i", &input_path, "-ss", &format!("{start_sec:.3}"), "-to", &format!("{end_sec:.3}"),
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-b:v", &vbr, "-preset", "fast", "-pass", "2",
            "-c:a", "aac", "-b:a", "128k", &out])
        .stderr(std::process::Stdio::piped())
        .spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    // Stream progress from pass 2 (45-100%)
    if let Some(stderr) = child2.stderr.take() {
        let app_clone = app.clone();
        let dur_clone = dur;
        tokio::task::spawn_blocking(move || {
            parse_ffmpeg_progress(stderr, dur_clone, 45.0, 100.0, &app_clone, None);
        });
    }
    let status2 = child2.wait().map_err(|e| format!("ffmpeg wait: {e}"))?;

    // Cleanup 2-pass log files
    let _ = std::fs::remove_file("ffmpeg2pass-0.log");
    let _ = std::fs::remove_file("ffmpeg2pass-0.log.mbtree");

    let _ = app.emit("export-progress", serde_json::json!({"percent": 100, "stage": "done"}));

    if status2.success() { Ok(out) }
    else { Err("FFmpeg encoding failed".into()) }
}

/// Parse ffmpeg stderr for "time=HH:MM:SS.xx" and emit progress events.
///
/// ★ THE BUG: FFmpeg writes progress as `\r`-delimited lines (carriage return),
/// NOT `\n` (newline). BufReader::lines() splits on `\n` only, so the reader
/// blocks indefinitely waiting for a newline that never comes.
///
/// FIX: Read byte-by-byte, split on both `\r` and `\n`.
/// Parse FFmpeg progress from stderr, emitting progress events.
/// `log`: optional buffer that collects every stderr line for post-mortem error reporting.
fn parse_ffmpeg_progress(
    stderr: std::process::ChildStderr,
    total_dur: f64,
    pct_start: f64,
    pct_end: f64,
    app: &AppHandle,
    log: Option<Arc<Mutex<Vec<String>>>>,
) {
    use std::io::Read;
    let range = pct_end - pct_start;
    let mut buf = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    let mut reader = std::io::BufReader::new(stderr);

    loop {
        match reader.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {
                let ch = byte[0];
                if ch == b'\r' || ch == b'\n' {
                    if !buf.is_empty() {
                        let line = String::from_utf8_lossy(&buf).to_string();
                        buf.clear();
                        eprintln!("ffmpeg: {line}");
                        if let Some(ref l) = log { l.lock().unwrap().push(line.clone()); }

                        if let Some(pos) = line.find("time=") {
                            let rest = &line[pos + 5..];
                            let ts_end = rest.find(|c: char| c == ' ' || c == '\t')
                                .unwrap_or(rest.len());
                            if let Some(secs) = parse_time_to_secs(&rest[..ts_end]) {
                                let pct = pct_start + (secs / total_dur.max(0.1) * range).min(range);
                                let _ = app.emit("export-progress", serde_json::json!({
                                    "percent": pct.round() as u32,
                                    "stage": "encoding",
                                    "speed": extract_speed(&line),
                                }));
                            }
                        }
                    }
                } else {
                    buf.push(ch);
                }
            }
            Err(_) => break,
        }
    }
}

/// Extract "speed=1.23x" from ffmpeg line
fn extract_speed(line: &str) -> String {
    if let Some(pos) = line.find("speed=") {
        let rest = &line[pos + 6..];
        let end = rest.find(|c: char| c == ' ' || c == '\r' || c == '\n').unwrap_or(rest.len());
        rest[..end].trim().to_string()
    } else {
        String::new()
    }
}

/// Parse "HH:MM:SS.xx" or "MM:SS.xx" to seconds
fn parse_time_to_secs(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        3 => {
            let h: f64 = parts[0].parse().ok()?;
            let m: f64 = parts[1].parse().ok()?;
            let s: f64 = parts[2].parse().ok()?;
            Some(h * 3600.0 + m * 60.0 + s)
        }
        2 => {
            let m: f64 = parts[0].parse().ok()?;
            let s: f64 = parts[1].parse().ok()?;
            Some(m * 60.0 + s)
        }
        _ => parts[0].parse().ok()
    }
}

/// Calculate projected export settings (for UI preview)
#[command]
pub async fn calc_export_settings(duration_sec: f64, target_mb: f64, width: u32, height: u32) -> Result<String, String> {
    if duration_sec <= 0.0 { return Err("Invalid duration".into()); }
    let audio_kbps = 128.0;
    let total_kbps = target_mb * 8192.0 / duration_sec;
    let video_kbps = (total_kbps - audio_kbps).max(100.0);
    Ok(serde_json::json!({
        "resolution": format!("{width}x{height}"),
        "video_bitrate_kbps": video_kbps as u32,
        "audio_bitrate_kbps": audio_kbps as u32,
        "total_bitrate_kbps": total_kbps as u32,
        "codec": "H.264 (libx264)", "preset": "fast", "passes": 2
    }).to_string())
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 5: Export with progress events
// ══════════════════════════════════════════════════════════════

#[derive(Serialize, Clone)]
struct ExportProgress { percent: f64, fps_current: f64, time_processed: String, speed: String }

#[command]
pub async fn export_with_progress(
    app: AppHandle,
    input_path: String, start_sec: f64, end_sec: f64,
    target_mb: f64, output_path: String,
) -> Result<String, String> {
    let dur = end_sec - start_sec;
    if dur <= 0.0 { return Err("Invalid trim range".into()); }

    let out = if output_path.is_empty() { auto_name(&input_path, "_export") } else { smart_prefix(&output_path) };

    // Input-side seeking: -ss/-to BEFORE -i so FFmpeg seeks to the keyframe
    // before start_sec instead of decoding from 0 (fast + correct PTS).
    let mut args: Vec<String> = vec![
        "-y".into(), "-progress".into(), "pipe:1".into(),
        "-ss".into(), format!("{start_sec:.3}"),
        "-to".into(), format!("{end_sec:.3}"),
        "-i".into(), input_path.clone(),
    ];

    if target_mb > 0.0 {
        let vbr = format!("{}k", ((target_mb * 8192.0 / dur) - 128.0).max(100.0) as u32);
        args.extend(["-c:v".into(), "libx264".into(), "-pix_fmt".into(), "yuv420p".into(),
            "-b:v".into(), vbr, "-preset".into(), "fast".into(),
            "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into()]);
    } else {
        // Copy video stream as-is; downmix ALL source audio tracks → one AAC stream.
        // Without explicit mapping, FFmpeg only copies the "best" audio stream.
        let n_audio = (count_audio_streams(&input_path) as usize).max(1);
        if n_audio == 1 {
            args.extend([
                "-map".into(), "0:v".into(),
                "-map".into(), "0:a:0".into(),
                "-c:v".into(), "copy".into(),
                "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(),
                "-avoid_negative_ts".into(), "make_zero".into(),
            ]);
        } else {
            // Build amix filter over all audio streams
            let mix_ins: String = (0..n_audio).map(|i| format!("[0:a:{i}]")).collect();
            let fc = format!("{mix_ins}amix=inputs={n_audio}:normalize=0:duration=longest[amix]");
            args.extend([
                "-filter_complex".into(), fc,
                "-map".into(), "0:v".into(),
                "-map".into(), "[amix]".into(),
                "-c:v".into(), "copy".into(),
                "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(),
                "-avoid_negative_ts".into(), "make_zero".into(),
            ]);
        }
    }
    args.push(out.clone());

    let mut child = Command::new("ffmpeg")
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg: {e}"))?;

    // ★ Read progress from stdout in background
    let handle = app.clone();
    let total_dur = dur;
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(async move {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                // ffmpeg -progress outputs: out_time_ms=12345678
                if let Some(time_us) = line.strip_prefix("out_time_us=") {
                    if let Ok(us) = time_us.parse::<f64>() {
                        let secs = us / 1_000_000.0;
                        let pct = (secs / total_dur * 100.0).min(100.0);
                        let _ = handle.emit("export-progress", ExportProgress {
                            percent: pct,
                            fps_current: 0.0,
                            time_processed: format!("{secs:.1}s"),
                            speed: String::new(),
                        });
                    }
                }
                if line.starts_with("speed=") {
                    let spd = line.strip_prefix("speed=").unwrap_or("").trim().to_string();
                    let _ = handle.emit("export-progress", ExportProgress {
                        percent: -1.0, // -1 = speed update only
                        fps_current: 0.0,
                        time_processed: String::new(),
                        speed: spd,
                    });
                }
            }
        });
    }

    let status = child.wait().map_err(|e| format!("ffmpeg wait: {e}"))?;
    // Send 100% on completion
    let _ = app.emit("export-progress", ExportProgress { percent: 100.0, fps_current: 0.0, time_processed: "done".into(), speed: String::new() });

    if status.success() { Ok(out) } else { Err("FFmpeg export failed".into()) }
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 3: Generate audio waveform peaks via ffmpeg
#[command] pub async fn open_file_location(filepath: String) -> Result<(), String> { Command::new("xdg-open").arg(Path::new(&filepath).parent().unwrap_or(Path::new("/"))).spawn().map_err(|e| format!("{e}"))?; Ok(()) }

// ═══ Recording ═══
#[command] pub async fn start_screen_recording(save_dir: String, fps: u32, quality: String, replay_seconds: u32) -> Result<String, String> { let dir=if save_dir.is_empty(){let d=default_clips_dir(); let _=std::fs::create_dir_all(&d); d}else{let d=PathBuf::from(shexp(&save_dir)); if !d.exists(){std::fs::create_dir_all(&d).map_err(|e| format!("{e}"))?;} d}; let ts=chrono_now(); let outfile=dir.join(format!("opengg_{ts}.mp4")); let target=detect_target(); let qp=match quality.as_str(){"Low"=>"medium","Medium"=>"high","High"=>"very_high","Ultra"=>"ultra",_=>"high"}; let mut args=vec!["-w".into(),target,"-f".into(),fps.to_string(),"-q".into(),qp.into(),"-a".into(),"default_output".into(),"-o".into(),outfile.to_string_lossy().to_string()]; if replay_seconds>0{args.push("-r".into());args.push(replay_seconds.to_string());} Command::new("gpu-screen-recorder").args(&args).spawn().map_err(|e| if e.kind()==std::io::ErrorKind::NotFound{"gpu-screen-recorder not found".into()}else{format!("{e}")})?; Ok(outfile.to_string_lossy().to_string()) }
#[command] pub async fn stop_screen_recording() -> Result<(), String> { let _=Command::new("pkill").args(["-SIGINT","gpu-screen-recorder"]).output(); tokio::time::sleep(std::time::Duration::from_millis(500)).await; Ok(()) }
fn detect_target() -> String { if let Ok(o)=Command::new("sh").args(["-c","xdotool getactivewindow 2>/dev/null"]).output(){let w=String::from_utf8_lossy(&o.stdout).trim().to_string(); if !w.is_empty(){if let Ok(s)=Command::new("xprop").args(["-id",&w,"_NET_WM_STATE"]).output(){if String::from_utf8_lossy(&s.stdout).contains("FULLSCREEN"){return format!("window:{w}");}}}} "screen".into() }
fn chrono_now() -> String { let s=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(); format!("{}-{:02}-{:02}_{:02}-{:02}-{:02}",1970+s/31536000,(s%31536000)/2592000+1,(s%2592000)/86400+1,(s%86400)/3600,(s%3600)/60,s%60) }

// ═══ Theme ═══
fn theme_path() -> PathBuf { dirs::config_dir().unwrap_or_else(||PathBuf::from("~/.config")).join("opengg/theme.json") }
#[command] pub async fn load_theme() -> Result<String, String> { let p=theme_path(); if p.exists(){std::fs::read_to_string(&p).map_err(|e| format!("{e}"))}else{Ok("{\"colors\":{\"--accent\":\"#E94560\"},\"layout\":{\"--clips-grid-cols\":\"4\"}}".into())} }
#[command] pub async fn save_theme(theme_json: String) -> Result<(), String> { let p=theme_path(); if let Some(d)=p.parent(){std::fs::create_dir_all(d).ok();} std::fs::write(&p,&theme_json).map_err(|e| format!("{e}")) }
#[command] pub async fn get_media_server_port(app: AppHandle) -> Result<u16, String> { Ok(app.state::<crate::MediaServerPort>().0) }

// ═══ Audio Devices ═══
#[derive(Debug,Serialize,Deserialize,Clone)] pub struct AudioDevice{pub name:String,pub description:String,pub device_type:String,pub is_default:bool}
#[command] pub async fn get_audio_devices() -> Result<Vec<AudioDevice>, String> { let mut d=Vec::new(); let ds=run_cmd("pactl",&["get-default-sink"]).unwrap_or_default(); let dr=run_cmd("pactl",&["get-default-source"]).unwrap_or_default(); if let Ok(o)=run_cmd("pactl",&["-f","json","list","sinks"]){if let Ok(v)=serde_json::from_str::<serde_json::Value>(&o){for s in v.as_array().unwrap_or(&vec![]){let n=s["name"].as_str().unwrap_or("").to_string(); if n.starts_with("OpenGG_"){continue;} d.push(AudioDevice{is_default:n==ds,description:s["description"].as_str().unwrap_or(&n).into(),name:n,device_type:"sink".into()});}}} if let Ok(o)=run_cmd("pactl",&["-f","json","list","sources"]){if let Ok(v)=serde_json::from_str::<serde_json::Value>(&o){for s in v.as_array().unwrap_or(&vec![]){let n=s["name"].as_str().unwrap_or("").to_string(); if n.contains(".monitor"){continue;} d.push(AudioDevice{is_default:n==dr,description:s["description"].as_str().unwrap_or(&n).into(),name:n,device_type:"source".into()});}}} Ok(d) }
// ══════════════════════════════════════════════════════════════
//  ★ EPIC 1 FIX: PipeWire routing leak — audio duplication bug
//
//  Previous bug: the old code only unlinked from the system-default
//  sink. If the channel had been routed to a DIFFERENT device earlier,
//  that link was never destroyed → audio played from two outputs.
//
//  Fix: query ALL current sinks and run `pw-link -d` for every one.
//  pw-link -d is a no-op when no link exists, so it's safe to spam.
// ══════════════════════════════════════════════════════════════

/// Destroy every existing pw-link from `{sink_name}:monitor_FL/FR` to
/// any physical sink that is currently listed by PulseAudio.
fn unlink_virtual_sink_from_all(sink_name: &str) {
    let json = match run_cmd("pactl", &["-f", "json", "list", "sinks"]) {
        Ok(j) => j,
        Err(_) => return,
    };
    let sinks: Vec<serde_json::Value> = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => return,
    };
    for s in &sinks {
        if let Some(target) = s["name"].as_str() {
            for p in ["FL", "FR"] {
                // pw-link -d silently exits 0 when the link doesn't exist
                Command::new("pw-link")
                    .args(["-d",
                        &format!("{sink_name}:monitor_{p}"),
                        &format!("{target}:playback_{p}"),
                    ])
                    .output()
                    .ok();
            }
        }
    }
    eprintln!("set_channel_device: unlinked {sink_name} from {} sinks", sinks.len());
}

#[command]
pub async fn set_channel_device(channel: String, device_name: String) -> Result<(), String> {
    if channel == "Mic" {
        let _ = Command::new("pactl").args(["set-default-source", &device_name]).output();
        return Ok(());
    }
    if channel == "Master" {
        let _ = Command::new("pactl").args(["set-default-sink", &device_name]).output();
        return Ok(());
    }

    let sink = format!("OpenGG_{channel}");

    // ★ Step 1: tear down ALL existing links for this virtual sink.
    //   This is the core fix for the routing leak / audio duplication bug.
    unlink_virtual_sink_from_all(&sink);

    // ★ Step 2: create fresh links to the newly selected device.
    for p in ["FL", "FR"] {
        let r = Command::new("pw-link")
            .args([
                &format!("{sink}:monitor_{p}"),
                &format!("{device_name}:playback_{p}"),
            ])
            .output();
        if let Ok(o) = r {
            if o.status.success() {
                eprintln!("set_channel_device: linked {sink}:monitor_{p} → {device_name}:playback_{p}");
            } else {
                eprintln!("set_channel_device: pw-link failed: {}", String::from_utf8_lossy(&o.stderr).trim());
            }
        }
    }
    Ok(())
}

// ═══ VU ═══
#[derive(Serialize, Clone)] struct VuLevels { channels: HashMap<String, f32> }

/// Real-time per-channel VU meters via native libpulse.
///
/// One `pa_simple` connection per channel reads S16LE PCM from the channel's
/// monitor source.  Each reader runs in `spawn_blocking` so the async runtime
/// is never stalled.  Stopping is cooperative: set the AtomicBool to false and
/// each thread exits on its next 32 ms read; the `Simple` handle is dropped at
/// thread exit, calling `pa_simple_free()` automatically (RAII).
///
/// Using native libpulse completely eliminates:
///  • Process spawns (old pw-cat approach spawned 6 OS processes per stream)
///  • Mic fallback bug (pw-cat silently routed unknown targets to the mic)
///  • SIGTERM dance on shutdown
#[command]
pub async fn start_vu_stream(app: AppHandle) -> Result<(), String> {
    use libpulse_binding::sample::{Format, Spec};
    use libpulse_binding::stream::Direction;
    use libpulse_simple_binding::Simple;

    let st = app.state::<crate::VuState>();
    if st.0.load(Ordering::Relaxed) { return Ok(()); }
    st.0.store(true, Ordering::Relaxed);
    let running = st.0.clone();
    let handle  = app.clone();

    // Resolve default sink/source once — not inside the hot path.
    let master_monitor = run_cmd("pactl", &["get-default-sink"])
        .map(|s| format!("{s}.monitor"))
        .unwrap_or_default();
    let mic_source = run_cmd("pactl", &["get-default-source"]).unwrap_or_default();

    // Guard: only connect to sources that currently exist.
    let known_sources: std::collections::HashSet<String> = {
        let mut set = std::collections::HashSet::new();
        if let Ok(json) = run_cmd("pactl", &["-f", "json", "list", "sources"]) {
            if let Ok(sources) = serde_json::from_str::<Vec<serde_json::Value>>(&json) {
                for s in &sources {
                    if let Some(name) = s["name"].as_str() { set.insert(name.to_string()); }
                }
            }
        }
        set
    };
    eprintln!("start_vu_stream: {} PA sources: {:?}", known_sources.len(), known_sources);

    let levels: Arc<Mutex<HashMap<String, f32>>> = Arc::new(Mutex::new(HashMap::new()));

    let channel_targets: Vec<(&'static str, String)> = vec![
        ("Master", master_monitor),
        ("Game",   "OpenGG_Game.monitor".into()),
        ("Chat",   "OpenGG_Chat.monitor".into()),
        ("Media",  "OpenGG_Media.monitor".into()),
        ("Aux",    "OpenGG_Aux.monitor".into()),
        ("Mic",    mic_source),
    ];

    let spec = Spec { format: Format::S16le, rate: 8000, channels: 1 };

    for (name, target) in channel_targets {
        if target.is_empty() || !known_sources.contains(target.as_str()) {
            eprintln!("start_vu_stream: {name} → '{target}' not in PA sources, level=0");
            levels.lock().unwrap().insert(name.to_string(), 0.0);
            continue;
        }

        let levels_clone  = Arc::clone(&levels);
        let running_clone = running.clone();

        tokio::task::spawn_blocking(move || {
            // Create the PA simple connection inside spawn_blocking — Simple is !Send.
            let pa = match Simple::new(
                None, "OpenGG VU", Direction::Record,
                Some(target.as_str()), name,
                &spec, None, None,
            ) {
                Ok(p)  => p,
                Err(e) => {
                    eprintln!("libpulse: {name} → failed to open '{target}': {e:?}");
                    return;
                }
            };
            eprintln!("start_vu_stream: {name} → libpulse connected to '{target}'");

            // 512 bytes = 256 i16 samples = 32 ms at 8 kHz mono — fast enough to
            // check `running` frequently while producing smooth meter values.
            let mut buf = vec![0u8; 512];
            let mut prev = 0.0f32;

            while running_clone.load(Ordering::Relaxed) {
                if pa.read(&mut buf).is_err() { break; }

                // RMS for perceptual loudness (vs peak which looks jittery).
                let sum_sq: f32 = buf.chunks_exact(2)
                    .map(|c| { let s = i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0; s * s })
                    .sum();
                let rms = (sum_sq / (buf.len() / 2) as f32).sqrt().min(1.0);

                // Fast attack, slow decay for natural VU ballistics.
                let smoothed = if rms > prev { rms * 0.9 + prev * 0.1 }
                                         else { rms * 0.3 + prev * 0.7 };
                prev = smoothed;
                levels_clone.lock().unwrap().insert(name.to_string(), smoothed);
            }
            // `pa` is dropped here → pa_simple_free() called automatically (RAII).
        });
    }

    // Emitter: publishes the shared map at ~30 fps.
    tokio::spawn(async move {
        while running.load(Ordering::Relaxed) {
            let snapshot = levels.lock().unwrap().clone();
            let _ = handle.emit("vu-levels", VuLevels { channels: snapshot });
            tokio::time::sleep(std::time::Duration::from_millis(16)).await; // ~60 Hz
        }
    });

    Ok(())
}

/// Stops the VU stream. Reader threads exit cooperatively within one read
/// period (~32 ms) and free their PA connections via RAII.
#[command]
pub async fn stop_vu_stream(app: AppHandle) -> Result<(), String> {
    app.state::<crate::VuState>().0.store(false, Ordering::Relaxed);
    Ok(())
}

// ═══ Settings ═══
fn settings_path() -> PathBuf { dirs::config_dir().unwrap_or_else(||PathBuf::from("~/.config")).join("opengg/ui-settings.json") }
#[command] pub async fn save_ui_settings(settings_json: String) -> Result<(), String> { let p=settings_path(); if let Some(d)=p.parent(){std::fs::create_dir_all(d).ok();} std::fs::write(&p,&settings_json).map_err(|e| format!("{e}")) }
#[command] pub async fn load_ui_settings() -> Result<String, String> { let p=settings_path(); if p.exists(){std::fs::read_to_string(&p).map_err(|e| format!("{e}"))}else{Ok("null".into())} }

/// Opens the user-facing locales directory in the system file manager.
/// Creates `~/.config/opengg/locales/` if it does not yet exist.
/// If `en.json` is absent, writes the bundled English template so translators
/// immediately have a complete base file to duplicate and translate.
#[command]
pub async fn open_locales_folder() -> Result<String, String> {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("opengg/locales");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create dir: {e}"))?;

    // Embed the shipped English locale at compile time — always stays in sync
    // with the app's built-in translations.
    const EN_TEMPLATE: &str = include_str!("../../src/locales/en.json");
    let template_path = dir.join("en.json");
    if !template_path.exists() {
        std::fs::write(&template_path, EN_TEMPLATE)
            .map_err(|e| format!("write en.json template: {e}"))?;
    }

    let path_str = dir.to_string_lossy().to_string();
    Command::new("xdg-open")
        .arg(&dir)
        .spawn()
        .map_err(|e| format!("xdg-open: {e}"))?;
    Ok(path_str)
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 3: Modular Plugin / Extensions System
// ══════════════════════════════════════════════════════════════

/// Developer guide written to the plugins folder on first open.
const EXTENSIONS_GUIDE: &str = r#"# How to Create an OpenGG Extension

## Directory Structure

Place your extension in its own subdirectory inside this `plugins/` folder:

```
plugins/
  my-extension/
    manifest.json      ← required
    README.md          ← optional
```

## manifest.json Schema

```json
{
  "id":          "my-extension",
  "name":        "My Extension",
  "description": "A one-line description shown in Settings → Extensions.",
  "version":     "1.0.0",
  "author":      "Your Name",
  "hooks": {
    "sidebar_tab": false,
    "export_filter": false,
    "settings_section": false
  }
}
```

### Fields

| Field         | Required | Description |
|---------------|----------|-------------|
| `id`          | ✓        | Unique kebab-case identifier (a-z, 0-9, hyphen). |
| `name`        | ✓        | Human-readable display name. |
| `description` | ✗        | Short description (≤ 120 chars). |
| `version`     | ✗        | SemVer string e.g. `"1.0.0"`. |
| `hooks`       | ✗        | Future hook declarations (currently informational). |

## Future Hook Points (Roadmap)

- **sidebar_tab** — Inject a custom Vue component tab into the Advanced Editor sidebar.
- **export_filter** — Register an FFmpeg filter that runs after the main video pipeline.
- **settings_section** — Add a custom card to the Settings page.

Once the hook API is stable this guide will be updated with working examples.
"#;

fn plugins_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("opengg/plugins")
}

#[derive(Serialize)]
pub struct ExtensionInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub path: String,
}

/// Creates `~/.local/share/opengg/plugins/` if needed, writes the developer
/// guide on the first visit, then opens the folder in the file manager.
#[command]
pub async fn open_extensions_folder() -> Result<String, String> {
    let dir = plugins_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create dir: {e}"))?;

    let guide = dir.join("HOW_TO_CREATE_EXTENSIONS.md");
    if !guide.exists() {
        std::fs::write(&guide, EXTENSIONS_GUIDE)
            .map_err(|e| format!("write guide: {e}"))?;
    }

    let path_str = dir.to_string_lossy().to_string();
    Command::new("xdg-open")
        .arg(&dir)
        .spawn()
        .map_err(|e| format!("xdg-open: {e}"))?;
    Ok(path_str)
}

/// Scans `~/.local/share/opengg/plugins/` for subdirectories containing a
/// `manifest.json`. Returns the parsed metadata for each valid extension.
#[command]
pub async fn scan_extensions() -> Result<Vec<ExtensionInfo>, String> {
    let dir = plugins_dir();
    if !dir.exists() { return Ok(vec![]); }

    let mut exts = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("read dir: {e}"))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }

        let manifest_path = path.join("manifest.json");
        if !manifest_path.exists() { continue; }

        let raw = match std::fs::read_to_string(&manifest_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let v: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue, // skip malformed manifests silently
        };

        let id   = v["id"].as_str().unwrap_or("").to_string();
        let name = v["name"].as_str().unwrap_or(&id).to_string();
        if id.is_empty() || name.is_empty() { continue; }

        exts.push(ExtensionInfo {
            id,
            name,
            description: v["description"].as_str().unwrap_or("").to_string(),
            version:     v["version"].as_str().unwrap_or("0.0.0").to_string(),
            path:        path.to_string_lossy().to_string(),
        });
    }

    exts.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(exts)
}

/// Reads every `*.json` file in `~/.config/opengg/locales/` and returns their
/// raw content. The frontend parses each file, extracts `_meta.{name,dir}`,
/// and registers the locale dynamically via `i18n.global.setLocaleMessage`.
#[derive(Serialize)]
pub struct UserLocale {
    pub code: String,
    pub json_content: String,
}

#[command]
pub async fn list_user_locales() -> Result<Vec<UserLocale>, String> {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("opengg/locales");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut locales = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("read dir: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let code = match path.file_stem().and_then(|s| s.to_str()) {
            Some(c) if !c.is_empty() => c.to_string(),
            _ => continue,
        };
        if let Ok(content) = std::fs::read_to_string(&path) {
            locales.push(UserLocale { code, json_content: content });
        }
    }
    Ok(locales)
}

#[derive(Serialize)]
pub struct StorageInfo {
    pub clip_count: u64,
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

/// Returns disk usage for the clips folder plus filesystem free/total space.
#[command]
pub async fn get_storage_info(clip_directories: Vec<String>) -> Result<StorageInfo, String> {
    use std::fs;
    let mut total_count = 0u64;
    let mut total_used = 0u64;
    let mut first_existing: Option<PathBuf> = None;

    for dir_str in &clip_directories {
        let folder = PathBuf::from(dir_str);
        if !folder.exists() { continue; }
        if first_existing.is_none() { first_existing = Some(folder.clone()); }
        if let Ok(entries) = fs::read_dir(&folder) {
            for e in entries.flatten() {
                if let Ok(meta) = e.metadata() {
                    if meta.is_file() {
                        let name = e.file_name().to_string_lossy().to_lowercase();
                        if name.ends_with(".mp4") || name.ends_with(".mkv") || name.ends_with(".webm") || name.ends_with(".mov") {
                            total_count += 1;
                            total_used += meta.len();
                        }
                    }
                }
            }
        }
    }

    let fs_root = first_existing.unwrap_or_else(|| PathBuf::from("/"));
    let (total_bytes, free_bytes) = get_fs_stats(&fs_root);

    Ok(StorageInfo { clip_count: total_count, used_bytes: total_used, total_bytes, free_bytes })
}

#[cfg(unix)]
fn get_fs_stats(path: &PathBuf) -> (u64, u64) {
    use std::os::unix::ffi::OsStrExt;
    let mut stat: libc_statvfs = unsafe { std::mem::zeroed() };
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap_or_default();
    unsafe {
        if libc_statvfs_call(cpath.as_ptr(), &mut stat) == 0 {
            let bsize = stat.f_frsize as u64;
            return (stat.f_blocks * bsize, stat.f_bfree * bsize);
        }
    }
    (0, 0)
}
#[cfg(not(unix))]
fn get_fs_stats(_path: &PathBuf) -> (u64, u64) { (0, 0) }

// Thin statvfs wrapper to avoid adding libc as a direct dep.
#[cfg(unix)]
#[repr(C)]
struct libc_statvfs {
    f_bsize: u64, f_frsize: u64, f_blocks: u64, f_bfree: u64, f_bavail: u64,
    f_files: u64, f_ffree: u64, f_favail: u64, f_fsid: u64, f_flag: u64,
    f_namemax: u64, __spare: [u64; 6],
}
#[cfg(unix)]
extern "C" { fn statvfs(path: *const std::ffi::c_char, buf: *mut libc_statvfs) -> std::ffi::c_int; }
#[cfg(unix)]
unsafe fn libc_statvfs_call(path: *const std::ffi::c_char, buf: *mut libc_statvfs) -> std::ffi::c_int {
    unsafe { statvfs(path, buf) }
}

// ═══ Helpers ═══
fn thumb_dir() -> PathBuf { dirs::data_dir().unwrap_or_else(||PathBuf::from("~/.local/share")).join("opengg/thumbnails") }

/// Count actual audio streams in a file via ffprobe
fn count_audio_streams(path: &str) -> u32 {
    get_audio_stream_global_indices(path).len() as u32
}

/// Get the global stream indices of all audio streams.
fn get_audio_stream_global_indices(path: &str) -> Vec<u32> {
    if let Ok(o) = Command::new("ffprobe")
        .args(["-v", "quiet", "-select_streams", "a", "-show_entries", "stream=index", "-of", "csv=p=0", path])
        .output() {
        if o.status.success() {
            return String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|l| l.trim().parse::<u32>().ok())
                .collect();
        }
    }
    vec![0]
}

/// ★ Epic 5: Probe video resolution for normalized overlay sizing
fn probe_resolution(path: &str) -> (u32, u32) {
    if let Ok(o) = Command::new("ffprobe")
        .args(["-v", "quiet", "-select_streams", "v:0",
            "-show_entries", "stream=width,height", "-of", "csv=s=x:p=0", path])
        .output() {
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
fn find_system_font_by_name(hint: Option<&str>) -> String {
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
            if Path::new(p).exists() { return p.to_string(); }
        }
        // Unknown name: ask fontconfig
        if let Ok(out) = Command::new("fc-match")
            .args(["--format=%{file}", name])
            .output()
        {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() && Path::new(&p).exists() { return p; }
            }
        }
    }
    find_system_font()
}

/// Find a system font that supports Arabic/CJK/Latin characters.
/// Tries common paths on Arch/Ubuntu/Fedora, falls back to fc-match.
fn find_system_font() -> String {
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
        if std::path::Path::new(path).exists() { return path.to_string(); }
    }
    // Fallback: use fc-match to find any available sans-serif font
    if let Ok(output) = Command::new("fc-match").args(["--format=%{file}", "sans"]).output() {
        if output.status.success() {
            let p = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !p.is_empty() && std::path::Path::new(&p).exists() { return p; }
        }
    }
    // Last resort
    "sans".to_string()
}

/// Clear the thumbnail cache directory
#[command]
pub async fn clear_thumbnail_cache() -> Result<u32, String> {
    let td = thumb_dir();
    let mut count = 0u32;
    if td.exists() {
        // Count files before deleting
        if let Ok(entries) = std::fs::read_dir(&td) {
            count = entries.filter_map(|e| e.ok()).filter(|e| e.path().is_file()).count() as u32;
        }
        std::fs::remove_dir_all(&td).map_err(|e| format!("remove: {e}"))?;
    }
    std::fs::create_dir_all(&td).map_err(|e| format!("create: {e}"))?;
    Ok(count)
}
fn resolve_clips_dir(f:&str) -> PathBuf { if !f.is_empty(){return PathBuf::from(shexp(f));} let sp=settings_path(); if sp.exists(){if let Ok(j)=std::fs::read_to_string(&sp){if let Ok(v)=serde_json::from_str::<serde_json::Value>(&j){ if let Some(arr)=v["settings"]["clip_directories"].as_array(){if let Some(first)=arr.first(){if let Some(f)=first.as_str(){return PathBuf::from(shexp(f));}}}}}} default_clips_dir() }
pub fn default_clips_dir() -> PathBuf { dirs::video_dir().unwrap_or_else(||dirs::home_dir().unwrap().join("Videos")).join("OpenGG") }
pub fn shexp(p:&str) -> String { if p.starts_with("~/"){if let Some(h)=dirs::home_dir(){return p.replacen("~",&h.to_string_lossy(),1);}} p.into() }
pub fn settings_path_pub() -> PathBuf { settings_path() }

/// Returns all directories to scan for clips: all entries from `clip_directories` in settings.
fn get_all_clip_dirs(primary: &str) -> Vec<PathBuf> {
    let sp = settings_path();
    if let Ok(j) = std::fs::read_to_string(&sp) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&j) {
            if let Some(arr) = v["settings"]["clip_directories"].as_array() {
                let dirs: Vec<PathBuf> = arr.iter()
                    .filter_map(|s| s.as_str())
                    .map(|p| PathBuf::from(shexp(p)))
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                if !dirs.is_empty() { return dirs; }
            }
        }
    }
    vec![resolve_clips_dir(primary)]
}
fn auto_name(input:&str,suffix:&str) -> String { let p=Path::new(input); let s=p.file_stem().unwrap_or_default().to_string_lossy(); let e=p.extension().unwrap_or_default().to_string_lossy(); p.parent().unwrap_or(Path::new(".")).join(format!("{s}{suffix}.{e}")).to_string_lossy().into() }

/// Prepend "OpenGG_" to the filename if it doesn't already start with it.
fn smart_prefix(path: &str) -> String {
    let p = Path::new(path);
    let stem = p.file_stem().unwrap_or_default().to_string_lossy();
    if stem.starts_with("OpenGG_") { return path.to_string(); }
    let ext = p.extension().unwrap_or_default().to_string_lossy();
    let new_name = if ext.is_empty() { format!("OpenGG_{stem}") } else { format!("OpenGG_{stem}.{ext}") };
    p.parent().unwrap_or(Path::new(".")).join(new_name).to_string_lossy().into()
}

#[tauri::command]
pub async fn register_global_shortcuts(
    app: tauri::AppHandle,
    save_replay: String,
    toggle_recording: String,
    screenshot: String,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    let combos: &[(&str, &str)] = &[
        (&save_replay, "save_replay"),
        (&toggle_recording, "toggle_recording"),
        (&screenshot, "screenshot"),
    ];
    for (combo, action) in combos {
        if combo.is_empty() { continue; }
        let shortcut: tauri_plugin_global_shortcut::Shortcut = combo.parse()
            .map_err(|_| format!("Invalid shortcut: '{combo}'"))?;
        let a = action.to_string();
        let app2 = app.clone();
        gs.on_shortcut(shortcut, move |_app, _sc, event| {
            if event.state() == ShortcutState::Pressed {
                let _ = app2.emit(&format!("global-shortcut-{a}"), ());
            }
        }).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn probe_video(p:&Path) -> (f64,u32,u32) { let d=probe_duration(&p.to_string_lossy()); let dm=Command::new("ffprobe").args(["-v","quiet","-select_streams","v:0","-show_entries","stream=width,height","-of","csv=s=x:p=0",&p.to_string_lossy()]).output().ok().map(|o|String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default(); let ps:Vec<&str>=dm.split('x').collect(); (d,ps.first().and_then(|s|s.parse().ok()).unwrap_or(0),ps.get(1).and_then(|s|s.parse().ok()).unwrap_or(0)) }
fn probe_duration(p:&str) -> f64 { Command::new("ffprobe").args(["-v","quiet","-show_entries","format=duration","-of","default=noprint_wrappers=1:nokey=1",p]).output().ok().and_then(|o|String::from_utf8_lossy(&o.stdout).trim().parse().ok()).unwrap_or(0.0) }
fn hash_str(s:&str) -> u64 { let mut h:u64=5381; for b in s.bytes(){h=h.wrapping_mul(33).wrapping_add(b as u64);} h }
/// Accurate Unix-timestamp → "YYYY-MM-DD HH:MM" using Howard Hinnant's civil calendar algorithm.
fn fmt_ts(s: i64) -> String {
    let days    = s / 86400;
    let rem     = s % 86400;
    let (h, m)  = (rem / 3600, (rem % 3600) / 60);
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y   = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp  = (5 * doy + 2) / 153;
    let d   = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr  = if mth <= 2 { y + 1 } else { y };
    format!("{yr}-{mth:02}-{d:02} {h:02}:{m:02}")
}
fn get_vol(n:&str) -> Option<f32> { let o=Command::new("pactl").args(["get-sink-volume",n]).output().ok()?; for p in String::from_utf8_lossy(&o.stdout).split('/'){let s=p.trim();if s.ends_with('%'){if let Ok(v)=s.trim_end_matches('%').trim().parse::<f32>(){return Some(v/100.0);}}} None }
// ══════════════════════════════════════════════════════════════
//  ★ EPIC 2: Crash-log directory opener
// ══════════════════════════════════════════════════════════════

/// Returns the directory where opengg_crash.log lives.
pub fn crash_log_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("opengg")
}

/// Opens the OS file manager at the crash-log directory so the user can
/// retrieve logs for bug reports.
#[command]
pub async fn open_crash_logs_folder() -> Result<(), String> {
    let dir = crash_log_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{e}"))?;
    Command::new("xdg-open").arg(&dir).spawn().map_err(|e| format!("{e}"))?;
    Ok(())
}

// ══════════════════════════════════════════════════════════════
//  ★ Virtual Audio Onboarding / Factory Reset
// ══════════════════════════════════════════════════════════════

const VIRTUAL_CHANNELS: &[&str] = &["Game", "Chat", "Media", "Aux"];

/// Returns true if all 4 OpenGG virtual sinks are present in PipeWire.
#[command]
pub async fn check_virtual_audio_status() -> Result<bool, String> {
    let out = run_cmd("pactl", &["list", "sinks", "short"]).unwrap_or_default();
    let all_present = VIRTUAL_CHANNELS.iter().all(|ch| out.contains(&format!("OpenGG_{ch}")));
    Ok(all_present)
}

/// Create all OpenGG virtual null sinks via pactl (idempotent — skips existing).
#[command]
pub async fn create_virtual_audio() -> Result<(), String> {
    let existing = run_cmd("pactl", &["list", "sinks", "short"]).unwrap_or_default();
    for ch in VIRTUAL_CHANNELS {
        let sink_name = format!("OpenGG_{ch}");
        if existing.contains(&sink_name) { continue; }
        run_cmd("pactl", &[
            "load-module", "module-null-sink",
            &format!("sink_name={sink_name}"),
            &format!("sink_properties=node.description=\"OpenGG - {ch}\" node.nick=\"OpenGG - {ch}\" device.description=\"OpenGG - {ch}\""),
            "channels=2", "channel_map=front-left,front-right",
        ])?;
    }
    log::info!("Virtual audio sinks created");
    Ok(())
}

/// Unload all OpenGG virtual sinks and restart PipeWire + WirePlumber.
#[command]
pub async fn remove_virtual_audio() -> Result<(), String> {
    let modules_json = run_cmd("pactl", &["-f", "json", "list", "modules"]).unwrap_or_default();
    if let Ok(mods) = serde_json::from_str::<Vec<serde_json::Value>>(&modules_json) {
        for m in &mods {
            if m["name"].as_str() != Some("module-null-sink") { continue; }
            let args = m["argument"].as_str().unwrap_or("");
            if !args.contains("OpenGG_") { continue; }
            if let Some(idx) = m["index"].as_u64() {
                let _ = run_cmd("pactl", &["unload-module", &idx.to_string()]);
            }
        }
    }
    let _ = run_cmd("systemctl", &["--user", "restart", "pipewire"]);
    let _ = run_cmd("systemctl", &["--user", "restart", "wireplumber"]);
    log::info!("Virtual audio removed and PipeWire restarted");
    Ok(())
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 3: Startup audio hydration
//
//  Reads the saved per-channel device map from ui-settings.json
//  and re-applies pw-link connections so virtual sinks stay
//  routed to the correct physical output after a restart.
// ══════════════════════════════════════════════════════════════

#[command]
pub fn hydrate_audio_routing() {
    let settings_path = dirs::config_dir()
        .unwrap_or_default()
        .join("opengg/ui-settings.json");

    let json = match std::fs::read_to_string(&settings_path) {
        Ok(j) => j,
        Err(_) => return, // no saved settings yet — nothing to hydrate
    };
    let v: serde_json::Value = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => return,
    };
    let devices = match v["mixer"]["devices"].as_object() {
        Some(d) => d.clone(),
        None => return,
    };

    for (channel, device_val) in &devices {
        let device = match device_val.as_str() {
            Some(d) if !d.is_empty() => d.to_string(),
            _ => continue,
        };

        if channel == "Mic" {
            Command::new("pactl")
                .args(["set-default-source", &device])
                .output()
                .ok();
            eprintln!("hydrate: Mic source → {device}");
        } else if channel == "Master" {
            // Setting default sink is intentionally skipped — it changes the
            // system-wide default and surprises the user.  The frontend will
            // call set_channel_device if the user actively changes it.
        } else {
            // Virtual sink (Game/Chat/Media/Aux): disconnect old links then
            // reconnect to the saved physical device.
            let sink = format!("OpenGG_{channel}");
            if let Ok(def) = run_cmd("pactl", &["get-default-sink"]) {
                for p in ["FL", "FR"] {
                    Command::new("pw-link")
                        .args(["-d", &format!("{sink}:monitor_{p}"), &format!("{def}:playback_{p}")])
                        .output()
                        .ok();
                }
            }
            for p in ["FL", "FR"] {
                Command::new("pw-link")
                    .args([&format!("{sink}:monitor_{p}"), &format!("{device}:playback_{p}")])
                    .output()
                    .ok();
            }
            eprintln!("hydrate: {channel} → {device}");
        }
    }
}

// ══════════════════════════════════════════════════════════════
//  ★ EPIC 4: Background-daemon control commands
// ══════════════════════════════════════════════════════════════

/// Updates the in-process RunInBackground flag. Called from the frontend
/// whenever the "Keep running in background when closed" toggle changes.
#[command]
pub async fn set_run_in_background(app: AppHandle, val: bool) -> Result<(), String> {
    app.state::<crate::RunInBackground>()
        .0
        .store(val, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// Returns true if the XDG autostart entry for OpenGG exists.
#[command]
pub async fn get_autostart() -> Result<bool, String> {
    let desktop = dirs::home_dir()
        .ok_or_else(|| "no home dir".to_string())?
        .join(".config/autostart/opengg.desktop");
    Ok(desktop.exists())
}

/// Creates or removes the XDG autostart `.desktop` entry.
#[command]
pub async fn set_autostart(enable: bool) -> Result<(), String> {
    let dir = dirs::home_dir()
        .ok_or_else(|| "no home dir".to_string())?
        .join(".config/autostart");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{e}"))?;
    let desktop = dir.join("opengg.desktop");

    if enable {
        let exe = std::env::current_exe().map_err(|e| format!("{e}"))?;
        let content = format!(
            "[Desktop Entry]\nType=Application\nName=OpenGG\nExec={}\nHidden=false\nNoDisplay=false\nX-GNOME-Autostart-enabled=true\n",
            exe.display()
        );
        std::fs::write(&desktop, content).map_err(|e| format!("{e}"))?;
    } else if desktop.exists() {
        std::fs::remove_file(&desktop).map_err(|e| format!("{e}"))?;
    }
    Ok(())
}

// ══════════════════════════════════════════════════════════════
//  ★ GPU Screen Recorder integration
// ══════════════════════════════════════════════════════════════

use crate::GsrProcess;

/// Start gpu-screen-recorder in replay-buffer mode.
/// `output_dir`: where clips are saved; `replay_secs`: buffer length; `fps`, `quality`: encode settings.
#[command]
pub fn start_gsr_replay(
    app: AppHandle,
    output_dir: String,
    replay_secs: u32,
    fps: u32,
    quality: String,
) -> Result<(), String> {
    let state = app.state::<GsrProcess>();
    let mut lock = state.0.lock().unwrap();
    if lock.is_some() {
        return Err("gpu-screen-recorder is already running".into());
    }
    let crf = match quality.as_str() {
        "Low"    => "35",
        "Medium" => "28",
        _        => "23", // High
    };
    // Use the "focused" window as capture target; falls back to entire screen if not available.
    let child = std::process::Command::new("gpu-screen-recorder")
        .args([
            "-w", "focused",
            "-f", &fps.to_string(),
            "-r", &replay_secs.to_string(),
            "-c", "mp4",
            "-o", &output_dir,
            "-q", crf,
        ])
        .spawn()
        .map_err(|e| format!("Failed to start gpu-screen-recorder: {e}"))?;
    log::info!("GSR started (pid {})", child.id());
    *lock = Some(child);
    Ok(())
}

/// Save the current replay buffer by sending SIGUSR1 to the GSR process.
#[command]
pub fn save_gsr_replay(app: AppHandle) -> Result<(), String> {
    let state = app.state::<GsrProcess>();
    let lock = state.0.lock().unwrap();
    match &*lock {
        Some(child) => {
            let pid = child.id();
            #[cfg(unix)]
            unsafe { libc::kill(pid as libc::pid_t, libc::SIGUSR1); }
            log::info!("GSR SIGUSR1 → pid {pid}");
            Ok(())
        }
        None => Err("gpu-screen-recorder is not running".into()),
    }
}

/// Stop the GSR process with SIGTERM.
#[command]
pub fn stop_gsr_replay(app: AppHandle) -> Result<(), String> {
    let state = app.state::<GsrProcess>();
    let mut lock = state.0.lock().unwrap();
    if let Some(mut child) = lock.take() {
        #[cfg(unix)]
        unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM); }
        let _ = child.wait();
        log::info!("GSR stopped");
    }
    Ok(())
}

/// Returns true if the GSR process is currently running.
#[command]
pub fn is_gsr_running(app: AppHandle) -> bool {
    let state = app.state::<GsrProcess>();
    let mut lock = state.0.lock().unwrap();
    match lock.as_mut() {
        Some(child) => match child.try_wait() {
            Ok(None) => true,    // still running
            _        => { lock.take(); false } // exited — clean up
        },
        None => false,
    }
}

/// Returns the title of the currently focused window (uses xdotool).
#[command]
pub fn get_active_window_title() -> String {
    std::process::Command::new("xdotool")
        .args(["getactivewindow", "getwindowname"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

// rng() removed — VU meters now use real PipeWire peak data
async fn call_dbus<R:serde::de::DeserializeOwned+zbus::zvariant::Type>(m:&str,p:&str,i:&str,a:impl serde::Serialize+zbus::zvariant::Type)->Result<R,String>{let c=zbus::Connection::session().await.map_err(|e|format!("{e}"))?;let r:R=c.call_method(Some("org.opengg.Daemon"),p,Some(i),m,&a).await.map_err(|e|format!("{m}:{e}"))?.body().deserialize().map_err(|e|format!("{m}:{e}"))?;Ok(r)}
async fn call_dbus_void(m:&str,p:&str,i:&str,a:impl serde::Serialize+zbus::zvariant::Type)->Result<(),String>{let c=zbus::Connection::session().await.map_err(|e|format!("{e}"))?;c.call_method(Some("org.opengg.Daemon"),p,Some(i),m,&a).await.map_err(|e|format!("{m}:{e}"))?;Ok(())}
fn run_cmd(c:&str,a:&[&str])->Result<String,String>{let o=Command::new(c).args(a).output().map_err(|e|format!("{c}:{e}"))?;if o.status.success(){Ok(String::from_utf8_lossy(&o.stdout).trim().into())}else{Err(String::from_utf8_lossy(&o.stderr).trim().into())}}

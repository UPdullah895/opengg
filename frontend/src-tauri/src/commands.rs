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
use std::sync::atomic::Ordering;
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
        &format!("sink_properties=device.description=\"OpenGG {ch}\""),
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
    let safe_name: String = new_name.chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_' || *c == '.')
        .collect();
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
    let safe: String = output_name.chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_').collect();
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
    pub overlay_type: String,  // "text" | "image" | "gif"
    pub content: String,       // text string or file path
    pub x: f64,               // percentage 0-100
    pub y: f64,
    pub scale: f64,
    pub start_sec: f64,
    pub dur_sec: f64,
}

#[derive(Deserialize)]
pub struct ExportAudioTrack {
    pub stream_index: u32,
    pub volume: f64,  // 0.0-1.0
    pub muted: bool,
}

/// Export a clip with filter_complex: audio downmix + text/image overlays
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

    let out = if output_path.is_empty() {
        auto_name(&input_path, "_export")
    } else { output_path };

    // ── Build the filter_complex graph ──
    let mut inputs: Vec<String> = vec!["-i".into(), input_path.clone()];
    let mut filter_parts: Vec<String> = Vec::new();
    let mut input_count = 1; // input 0 = main video

    // 1) Video: trim
    let mut video_label = "[0:v]".to_string();

    // 2) Audio downmix — combine active (non-muted) tracks
    let active_audio: Vec<&ExportAudioTrack> = audio_tracks.iter()
        .filter(|t| !t.muted && t.volume > 0.0)
        .collect();

    let audio_label = if active_audio.is_empty() {
        // All muted — generate silence
        filter_parts.push("anullsrc=r=48000:cl=stereo[aout]".into());
        "[aout]".to_string()
    } else if active_audio.len() == 1 {
        // Single track — just apply volume
        let t = active_audio[0];
        let label = format!("[amix]");
        filter_parts.push(format!("[0:a:{}]volume={:.2}{}", t.stream_index, t.volume, label));
        label
    } else {
        // Multiple tracks — volume each, then amix
        let mut mix_inputs = Vec::new();
        for (i, t) in active_audio.iter().enumerate() {
            let lbl = format!("[a{i}]");
            filter_parts.push(format!("[0:a:{}]volume={:.2}{lbl}", t.stream_index, t.volume));
            mix_inputs.push(lbl);
        }
        let mix_in = mix_inputs.join("");
        filter_parts.push(format!("{mix_in}amix=inputs={}:duration=longest[amix]", active_audio.len()));
        "[amix]".to_string()
    };

    // 3) Overlay burn — text and image overlays
    for ov in &overlays {
        match ov.overlay_type.as_str() {
            "text" => {
                let escaped = ov.content.replace("'", "\\'").replace(":", "\\:");
                let x_expr = format!("(W*{}/100)", ov.x / 100.0);
                let y_expr = format!("(H*{}/100)", ov.y / 100.0);
                let fs = (24.0 * ov.scale / 100.0).max(8.0) as u32;
                let enable = format!("between(t\\,{:.2}\\,{:.2})", ov.start_sec, ov.start_sec + ov.dur_sec);
                let next_label = format!("[vov{}]", filter_parts.len());
                filter_parts.push(format!(
                    "{video_label}drawtext=text='{escaped}':fontsize={fs}:fontcolor=white:borderw=2:bordercolor=black:x={x_expr}:y={y_expr}:enable='{enable}'{next_label}"
                ));
                video_label = next_label;
            }
            "image" | "gif" => {
                if !ov.content.is_empty() && std::path::Path::new(&ov.content).exists() {
                    inputs.push("-i".into());
                    inputs.push(ov.content.clone());
                    let inp_idx = input_count;
                    input_count += 1;
                    let x_expr = format!("(W*{}/100)", ov.x / 100.0);
                    let y_expr = format!("(H*{}/100)", ov.y / 100.0);
                    let enable = format!("between(t\\,{:.2}\\,{:.2})", ov.start_sec, ov.start_sec + ov.dur_sec);
                    let next_label = format!("[vov{}]", filter_parts.len());
                    filter_parts.push(format!(
                        "{video_label}[{inp_idx}:v]overlay=x={x_expr}:y={y_expr}:enable='{enable}'{next_label}"
                    ));
                    video_label = next_label;
                }
            }
            _ => {}
        }
    }

    // Build final filter_complex string
    let has_filters = !filter_parts.is_empty();
    let filter_complex = if has_filters {
        // Rename final video/audio labels for output mapping
        let fc = filter_parts.join(";");
        format!("{fc}")
    } else {
        String::new()
    };

    // ── Build ffmpeg command ──
    let mut args: Vec<String> = vec!["-y".into()];
    args.push("-ss".into()); args.push(format!("{start_sec:.3}"));
    args.push("-to".into()); args.push(format!("{end_sec:.3}"));
    args.extend(inputs);

    if has_filters {
        args.push("-filter_complex".into());
        args.push(filter_complex);
        args.push("-map".into()); args.push(video_label.trim_matches(|c| c == '[' || c == ']').to_string());
        args.push("-map".into()); args.push(audio_label.trim_matches(|c| c == '[' || c == ']').to_string());
    }

    // Video encoding settings
    if target_mb > 0.0 {
        let audio_kbps = 128.0;
        let video_kbps = ((target_mb * 8192.0 / dur) - audio_kbps).max(100.0);
        args.extend(["-c:v".into(), "libx264".into(), "-b:v".into(), format!("{}k", video_kbps as u32), "-preset".into(), "fast".into()]);
    } else if has_filters {
        args.extend(["-c:v".into(), "libx264".into(), "-crf".into(), "18".into(), "-preset".into(), "fast".into()]);
    } else {
        args.extend(["-c:v".into(), "copy".into()]);
    }
    args.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "128k".into()]);
    args.push(out.clone());

    eprintln!("export_clip_with_filters: ffmpeg {}", args.join(" "));

    let _ = app.emit("export-progress", serde_json::json!({"percent": 0, "stage": "encoding", "speed": ""}));

    let mut child = Command::new("ffmpeg")
        .args(&args)
        .stderr(std::process::Stdio::piped())
        .spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    if let Some(stderr) = child.stderr.take() {
        let app_c = app.clone();
        tokio::task::spawn_blocking(move || {
            parse_ffmpeg_progress(stderr, dur, 0.0, 100.0, &app_c);
        });
    }

    let status = child.wait().map_err(|e| format!("ffmpeg: {e}"))?;
    let _ = app.emit("export-progress", serde_json::json!({"percent": 100, "stage": "done", "speed": ""}));

    if status.success() { Ok(out) }
    else { Err("FFmpeg export failed".into()) }
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

#[command] pub async fn get_clips(folder: String) -> Result<Vec<ClipInfo>, String> {
    let dir = resolve_clips_dir(&folder); if !dir.exists() { return Ok(vec![]); }
    let meta = get_meta_map(); let td = thumb_dir(); let _ = std::fs::create_dir_all(&td);
    let mut clips = Vec::new();
    for e in std::fs::read_dir(&dir).map_err(|e| format!("{e}"))?.flatten() {
        let p = e.path(); if !p.is_file() { continue; }
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if !VIDEO_EXTS.contains(&ext.as_str()) { continue; }
        let m = e.metadata().map_err(|e| format!("{e}"))?;
        let fname = p.file_name().unwrap_or_default().to_string_lossy().to_string();
        let fp = p.to_string_lossy().to_string();
        let id = format!("{:x}", hash_str(&fp));
        let created = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| fmt_ts(d.as_secs() as i64)).unwrap_or_default();
        let (dur,w,h) = probe_video(&p);
        let game_from_filename = fname.split('_').next().unwrap_or("Unknown").replace('-'," ");
        let (cn,fav,game_tag) = meta.get(&fp).cloned().unwrap_or_default();
        let game = if game_tag.is_empty() { game_from_filename } else { game_tag };
        let thumb = td.join(format!("{id}.jpg"));
        let thumbnail = if thumb.exists() { thumb.to_string_lossy().to_string() } else { String::new() };
        clips.push(ClipInfo{id,filename:fname,filepath:fp,filesize:m.len(),created,duration:dur,width:w,height:h,game,custom_name:cn,favorite:fav,thumbnail});
    }
    clips.sort_by(|a,b| b.created.cmp(&a.created)); Ok(clips)
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
#[derive(Deserialize)] pub struct ClipMetaUpdate { pub filepath:String, pub custom_name:Option<String>, pub favorite:Option<bool>, pub game_tag:Option<String> }
#[command] pub async fn set_clip_meta(update: ClipMetaUpdate) -> Result<(), String> {
    let db = open_db()?;
    // Ensure game_tag column exists (migration)
    let _ = db.execute("ALTER TABLE clip_meta ADD COLUMN game_tag TEXT DEFAULT ''", []);
    db.execute("INSERT INTO clip_meta(filepath,custom_name,favorite,game_tag) VALUES(?1,?2,?3,?4) ON CONFLICT(filepath) DO UPDATE SET custom_name=COALESCE(?2,custom_name),favorite=COALESCE(?3,favorite),game_tag=COALESCE(?4,game_tag)",
        rusqlite::params![update.filepath, update.custom_name.unwrap_or_default(), update.favorite.unwrap_or(false) as i32, update.game_tag.unwrap_or_default()]
    ).map_err(|e| format!("{e}"))?;
    Ok(())
}

/// Get game_tag for a clip
#[command]
pub async fn get_clip_meta(filepath: String) -> Result<String, String> {
    let db = open_db()?;
    let _ = db.execute("ALTER TABLE clip_meta ADD COLUMN game_tag TEXT DEFAULT ''", []);
    match db.query_row("SELECT custom_name,favorite,game_tag FROM clip_meta WHERE filepath=?1", [&filepath], |r| {
        Ok(serde_json::json!({"custom_name": r.get::<_,String>(0)?, "favorite": r.get::<_,bool>(1)?, "game_tag": r.get::<_,String>(2).unwrap_or_default() }))
    }) {
        Ok(v) => Ok(v.to_string()),
        Err(_) => Ok("null".into()),
    }
}

/// Take a screenshot at a specific timestamp
#[command]
pub async fn take_screenshot(filepath: String, time_sec: f64) -> Result<String, String> {
    // Get the user's Pictures directory
    let pics_dir = dirs::picture_dir().unwrap_or_else(|| dirs::home_dir().unwrap().join("Pictures"));
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
#[command] pub async fn trim_clip(input_path: String, start_sec: f64, end_sec: f64, output_path: String) -> Result<String, String> { let out=if output_path.is_empty(){auto_name(&input_path,"_trim")}else{output_path}; let r=Command::new("ffmpeg").args(["-i",&input_path,"-ss",&format!("{start_sec:.3}"),"-to",&format!("{end_sec:.3}"),"-c","copy","-avoid_negative_ts","make_zero","-y",&out]).output().map_err(|e| format!("{e}"))?; if r.status.success(){Ok(out)}else{Err(format!("{}",String::from_utf8_lossy(&r.stderr)))} }
/// Export with target size + real-time progress via Tauri events.
/// Parses ffmpeg stderr for "time=HH:MM:SS.xx" to calculate %.
#[command]
pub async fn export_clip_sized(app: AppHandle, input_path: String, start_sec: f64, end_sec: f64, target_mb: f64, output_path: String) -> Result<String, String> {
    let dur = end_sec - start_sec;
    if dur <= 0.0 { return Err("Invalid trim range".into()); }

    let out = if output_path.is_empty() { auto_name(&input_path, &format!("_{}mb", target_mb as u32)) } else { output_path };

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
            "-c:v", "libx264", "-b:v", &vbr, "-preset", "fast", "-pass", "1",
            "-an", "-f", "null", "/dev/null"])
        .stderr(std::process::Stdio::piped())
        .spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    // Stream progress from pass 1 (0-45%)
    if let Some(stderr) = child1.stderr.take() {
        let app_clone = app.clone();
        let dur_clone = dur;
        tokio::task::spawn_blocking(move || {
            parse_ffmpeg_progress(stderr, dur_clone, 0.0, 45.0, &app_clone);
        });
    }
    let _status1 = child1.wait().map_err(|e| format!("ffmpeg wait: {e}"))?;

    // Pass 2 (encode)
    let _ = app.emit("export-progress", serde_json::json!({"percent": 45, "stage": "pass2"}));
    let mut child2 = Command::new("ffmpeg")
        .args(["-y", "-i", &input_path, "-ss", &format!("{start_sec:.3}"), "-to", &format!("{end_sec:.3}"),
            "-c:v", "libx264", "-b:v", &vbr, "-preset", "fast", "-pass", "2",
            "-c:a", "aac", "-b:a", "128k", &out])
        .stderr(std::process::Stdio::piped())
        .spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    // Stream progress from pass 2 (45-100%)
    if let Some(stderr) = child2.stderr.take() {
        let app_clone = app.clone();
        let dur_clone = dur;
        tokio::task::spawn_blocking(move || {
            parse_ffmpeg_progress(stderr, dur_clone, 45.0, 100.0, &app_clone);
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
fn parse_ffmpeg_progress(
    stderr: std::process::ChildStderr,
    total_dur: f64,
    pct_start: f64,
    pct_end: f64,
    app: &AppHandle,
) {
    use std::io::Read;
    let range = pct_end - pct_start;
    let mut buf = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    let mut reader = std::io::BufReader::new(stderr);

    loop {
        match reader.read(&mut byte) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let ch = byte[0];
                if ch == b'\r' || ch == b'\n' {
                    // Process accumulated line
                    if !buf.is_empty() {
                        let line = String::from_utf8_lossy(&buf).to_string();
                        buf.clear();

                        if let Some(pos) = line.find("time=") {
                            let rest = &line[pos + 5..];
                            // Extract time value — ends at space, tab, or end of string
                            let ts_end = rest.find(|c: char| c == ' ' || c == '\t')
                                .unwrap_or(rest.len());
                            let ts = &rest[..ts_end];
                            if let Some(secs) = parse_time_to_secs(ts) {
                                let pct = pct_start + (secs / total_dur.max(0.1) * range).min(range);
                                let _ = app.emit("export-progress",
                                    serde_json::json!({
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

    let out = if output_path.is_empty() { auto_name(&input_path, "_export") } else { output_path };

    let mut args: Vec<String> = vec![
        "-y".into(), "-progress".into(), "pipe:1".into(), // ★ Progress to stdout
        "-i".into(), input_path.clone(),
        "-ss".into(), format!("{start_sec:.3}"), "-to".into(), format!("{end_sec:.3}"),
    ];

    if target_mb > 0.0 {
        let vbr = format!("{}k", ((target_mb * 8192.0 / dur) - 128.0).max(100.0) as u32);
        args.extend(["-c:v".into(), "libx264".into(), "-b:v".into(), vbr,
            "-preset".into(), "fast".into(), "-c:a".into(), "aac".into(), "-b:a".into(), "128k".into()]);
    } else {
        args.extend(["-c".into(), "copy".into(), "-avoid_negative_ts".into(), "make_zero".into()]);
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
#[command] pub async fn set_channel_device(channel: String, device_name: String) -> Result<(), String> { if channel=="Mic"{let _=Command::new("pactl").args(["set-default-source",&device_name]).output(); return Ok(());} if channel=="Master"{let _=Command::new("pactl").args(["set-default-sink",&device_name]).output(); return Ok(());} let s=format!("OpenGG_{channel}"); if let Ok(def)=run_cmd("pactl",&["get-default-sink"]){for p in["FL","FR"]{let _=Command::new("pw-link").args(["-d",&format!("{s}:monitor_{p}"),&format!("{def}:playback_{p}")]).output();}} for p in["FL","FR"]{let _=Command::new("pw-link").args([&format!("{s}:monitor_{p}"),&format!("{device_name}:playback_{p}")]).output();} Ok(()) }

// ═══ VU ═══
#[derive(Serialize,Clone)] struct VuLevels{channels:HashMap<String,f32>}
#[command] pub async fn start_vu_stream(app: AppHandle) -> Result<(), String> { let st=app.state::<crate::VuState>(); if st.0.load(Ordering::Relaxed){return Ok(());} st.0.store(true,Ordering::Relaxed); let a=st.0.clone(); let h=app.clone(); tokio::spawn(async move{let cs=["Game","Chat","Media","Aux","Mic"]; while a.load(Ordering::Relaxed){let mut l=HashMap::new(); for&c in&cs{let s=if c=="Mic"{"@DEFAULT_SOURCE@".into()}else{format!("OpenGG_{c}")}; let v=get_vol(&s).unwrap_or(0.0); l.insert(c.into(),if v>0.01{(v*0.6+(rng()-0.5)*0.3).clamp(0.0,1.0)}else{0.0});} let _=h.emit("vu-levels",VuLevels{channels:l}); tokio::time::sleep(std::time::Duration::from_millis(50)).await;}}); Ok(()) }
#[command] pub async fn stop_vu_stream(app: AppHandle) -> Result<(), String> { app.state::<crate::VuState>().0.store(false,Ordering::Relaxed); Ok(()) }

// ═══ Settings ═══
fn settings_path() -> PathBuf { dirs::config_dir().unwrap_or_else(||PathBuf::from("~/.config")).join("opengg/ui-settings.json") }
#[command] pub async fn save_ui_settings(settings_json: String) -> Result<(), String> { let p=settings_path(); if let Some(d)=p.parent(){std::fs::create_dir_all(d).ok();} std::fs::write(&p,&settings_json).map_err(|e| format!("{e}")) }
#[command] pub async fn load_ui_settings() -> Result<String, String> { let p=settings_path(); if p.exists(){std::fs::read_to_string(&p).map_err(|e| format!("{e}"))}else{Ok("null".into())} }

// ═══ Helpers ═══
fn thumb_dir() -> PathBuf { dirs::data_dir().unwrap_or_else(||PathBuf::from("~/.local/share")).join("opengg/thumbnails") }
fn resolve_clips_dir(f:&str) -> PathBuf { if !f.is_empty(){return PathBuf::from(shexp(f));} let sp=settings_path(); if sp.exists(){if let Ok(j)=std::fs::read_to_string(&sp){if let Ok(v)=serde_json::from_str::<serde_json::Value>(&j){if let Some(f)=v["settings"]["clipsFolder"].as_str(){return PathBuf::from(shexp(f));}}}} default_clips_dir() }
fn default_clips_dir() -> PathBuf { dirs::video_dir().unwrap_or_else(||dirs::home_dir().unwrap().join("Videos")).join("OpenGG") }
fn shexp(p:&str) -> String { if p.starts_with("~/"){if let Some(h)=dirs::home_dir(){return p.replacen("~",&h.to_string_lossy(),1);}} p.into() }
fn auto_name(input:&str,suffix:&str) -> String { let p=Path::new(input); let s=p.file_stem().unwrap_or_default().to_string_lossy(); let e=p.extension().unwrap_or_default().to_string_lossy(); p.parent().unwrap_or(Path::new(".")).join(format!("{s}{suffix}.{e}")).to_string_lossy().into() }
fn probe_video(p:&Path) -> (f64,u32,u32) { let d=probe_duration(&p.to_string_lossy()); let dm=Command::new("ffprobe").args(["-v","quiet","-select_streams","v:0","-show_entries","stream=width,height","-of","csv=s=x:p=0",&p.to_string_lossy()]).output().ok().map(|o|String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default(); let ps:Vec<&str>=dm.split('x').collect(); (d,ps.first().and_then(|s|s.parse().ok()).unwrap_or(0),ps.get(1).and_then(|s|s.parse().ok()).unwrap_or(0)) }
fn probe_duration(p:&str) -> f64 { Command::new("ffprobe").args(["-v","quiet","-show_entries","format=duration","-of","default=noprint_wrappers=1:nokey=1",p]).output().ok().and_then(|o|String::from_utf8_lossy(&o.stdout).trim().parse().ok()).unwrap_or(0.0) }
fn hash_str(s:&str) -> u64 { let mut h:u64=5381; for b in s.bytes(){h=h.wrapping_mul(33).wrapping_add(b as u64);} h }
fn fmt_ts(s:i64) -> String { let d=s/86400; format!("{}-{:02}-{:02}",1970+d/365,d%365/30+1,d%30+1) }
fn get_vol(n:&str) -> Option<f32> { let o=Command::new("pactl").args(["get-sink-volume",n]).output().ok()?; for p in String::from_utf8_lossy(&o.stdout).split('/'){let s=p.trim();if s.ends_with('%'){if let Ok(v)=s.trim_end_matches('%').trim().parse::<f32>(){return Some(v/100.0);}}} None }
fn rng() -> f32 { ((std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos()%1000) as f32)/1000.0 }
async fn call_dbus<R:serde::de::DeserializeOwned+zbus::zvariant::Type>(m:&str,p:&str,i:&str,a:impl serde::Serialize+zbus::zvariant::Type)->Result<R,String>{let c=zbus::Connection::session().await.map_err(|e|format!("{e}"))?;let r:R=c.call_method(Some("org.opengg.Daemon"),p,Some(i),m,&a).await.map_err(|e|format!("{m}:{e}"))?.body().deserialize().map_err(|e|format!("{m}:{e}"))?;Ok(r)}
async fn call_dbus_void(m:&str,p:&str,i:&str,a:impl serde::Serialize+zbus::zvariant::Type)->Result<(),String>{let c=zbus::Connection::session().await.map_err(|e|format!("{e}"))?;c.call_method(Some("org.opengg.Daemon"),p,Some(i),m,&a).await.map_err(|e|format!("{m}:{e}"))?;Ok(())}
fn run_cmd(c:&str,a:&[&str])->Result<String,String>{let o=Command::new(c).args(a).output().map_err(|e|format!("{c}:{e}"))?;if o.status.success(){Ok(String::from_utf8_lossy(&o.stdout).trim().into())}else{Err(String::from_utf8_lossy(&o.stderr).trim().into())}}

//! Platform-fitted export via the ffmpeg CLI.
//!
//! [`plan`] turns a UI request into a fully-resolved [`SharePlan`] (shown as a
//! live preview in QML), and [`render`] encodes exactly that plan. Keeping the
//! two separate means the size the user was promised is the size we aim for,
//! and the plan can round-trip through QML as JSON.

use super::planner::{
    audio_bps_for, bitrate_plan, clamp_window, default_short_side, default_window, estimate_bytes,
    format_bytes, retry_bitrate, scale_to_short_side, size_ladder, strictest, vertical_crop, Crop,
    LadderStep, Platform, Spec,
};
use super::util::{part_path, prune_old, secs_arg, unique_path};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, OnceLock};
use std::time::Duration;

/// Error string returned when the user cancelled (UI shows no error toast).
pub const CANCELLED: &str = "Cancelled";
/// Exports in the share cache are throwaway: kept long enough to re-paste.
pub const CACHE_MAX_AGE: Duration = Duration::from_secs(3 * 24 * 3600);
/// Upper bound for a per-track gain (≈ +12 dB).
const MAX_GAIN: f64 = 4.0;

/// What the share sheet asks for. `start`/`end` None → platform default
/// window; `short_side` None → automatic resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharePlanRequest {
    pub input: PathBuf,
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
    /// Targets; the strictest limits of all of them apply. Empty = Original.
    #[serde(default)]
    pub platforms: Vec<Platform>,
    #[serde(default)]
    pub short_side: Option<u32>,
    #[serde(default)]
    pub vertical: bool,
    /// 9:16 crop position, 0 = left … 1 = right.
    #[serde(default = "half")]
    pub crop_pos: f64,
    /// (audio-relative stream index — the N in `0:a:N`, gain multiplier).
    /// Clips have one track per mixer channel and no pre-mix, so the user
    /// picks which to mix. Empty → silent export.
    #[serde(default)]
    pub tracks: Vec<(usize, f64)>,
}

fn half() -> f64 {
    0.5
}

/// Probed facts about the source clip.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    /// Average frame rate; 0 when unknown (planner then assumes 60).
    pub fps: f64,
    pub audio_streams: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareTrack {
    pub index: usize,
    pub gain: f64,
}

/// A fully-resolved export. Serialized to QML for the preview, and handed
/// back unchanged to [`render`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharePlan {
    pub input: PathBuf,
    pub platforms: Vec<Platform>,
    pub spec: Spec,
    pub source: SourceInfo,
    pub start: f64,
    pub end: f64,
    pub duration: f64,
    pub crop: Option<Crop>,
    pub ladder: Vec<LadderStep>,
    pub short_side: u32,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub full_fps: bool,
    pub video_bps: u64,
    pub audio_bps: u64,
    pub tracks: Vec<ShareTrack>,
    pub estimate_bytes: u64,
    pub estimate_label: String,
    /// Estimate is within the platform's byte cap (always true without one).
    pub fits: bool,
}

/// Probe the clip (ffprobe) for [`plan_with_source`].
pub fn probe_source(input: &Path) -> Result<SourceInfo, String> {
    let (duration, width, height) = crate::media::probe_video(input);
    if duration <= 0.0 || width == 0 || height == 0 {
        return Err(format!("Could not read video: {}", input.display()));
    }
    let fps = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=avg_frame_rate",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(input)
        .output()
        .ok()
        .map(|o| parse_rate(String::from_utf8_lossy(&o.stdout).trim()))
        .unwrap_or(0.0);
    // Only trust the stream list when ffprobe actually answered: the helper
    // falls back to `[0]` on failure, which would invent an audio track.
    let has_any_audio = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=index",
        ])
        .args(["-of", "csv=p=0"])
        .arg(input)
        .output()
        .map(|o| o.status.success() && !o.stdout.trim_ascii().is_empty())
        .unwrap_or(false);
    let audio_streams = if has_any_audio {
        crate::media::get_audio_stream_global_indices(&input.to_string_lossy()).len()
    } else {
        0
    };
    Ok(SourceInfo {
        duration,
        width,
        height,
        fps,
        audio_streams,
    })
}

fn parse_rate(s: &str) -> f64 {
    let v = match s.split_once('/') {
        Some((n, d)) => {
            let n: f64 = n.parse().unwrap_or(0.0);
            let d: f64 = d.parse().unwrap_or(0.0);
            if d > 0.0 {
                n / d
            } else {
                0.0
            }
        }
        None => s.parse().unwrap_or(0.0),
    };
    if v.is_finite() && v > 0.0 {
        v
    } else {
        0.0
    }
}

/// Probe + plan.
pub fn plan(req: &SharePlanRequest) -> Result<SharePlan, String> {
    let src = probe_source(&req.input)?;
    Ok(plan_with_source(req, &src))
}

/// Pure planning from already-probed facts (cache [`SourceInfo`] in the
/// controller so slider drags don't re-run ffprobe).
pub fn plan_with_source(req: &SharePlanRequest, src: &SourceInfo) -> SharePlan {
    let spec = strictest(&req.platforms);
    let (start, end) = match (req.start, req.end) {
        (Some(s), Some(e)) => clamp_window(s, e, src.duration, &spec),
        _ => default_window(src.duration, &spec),
    };
    let duration = end - start;
    let crop = req
        .vertical
        .then(|| vertical_crop(src.width, src.height, req.crop_pos));
    let (bw, bh) = crop.map_or((src.width, src.height), |c| (c.w, c.h));
    let ladder = size_ladder(bw.min(bh), spec.max_short_side);
    let tracks: Vec<ShareTrack> = req
        .tracks
        .iter()
        .filter(|(i, _)| *i < src.audio_streams)
        .map(|&(index, gain)| ShareTrack {
            index,
            gain: if gain.is_finite() {
                gain.clamp(0.0, MAX_GAIN)
            } else {
                1.0
            },
        })
        .collect();
    let audio_bps = audio_bps_for(!tracks.is_empty());
    let short_side = match req.short_side {
        Some(s) if s > 0 => s.min(spec.max_short_side),
        _ => default_short_side(&ladder, bw, bh, src.fps, duration, &spec, audio_bps),
    };
    let (width, height) = scale_to_short_side(bw, bh, short_side);
    let bp = bitrate_plan(width, height, src.fps, duration, &spec, audio_bps);
    let est = estimate_bytes(&bp, duration);
    SharePlan {
        input: req.input.clone(),
        platforms: req.platforms.clone(),
        spec,
        source: *src,
        start,
        end,
        duration,
        crop,
        ladder,
        short_side,
        width,
        height,
        fps: bp.fps,
        full_fps: bp.full_fps,
        video_bps: bp.video_bps,
        audio_bps: bp.audio_bps,
        tracks,
        estimate_bytes: est,
        estimate_label: format_bytes(est),
        fits: spec.max_bytes.is_none_or(|m| est <= m),
    }
}

// ─── Output location ────────────────────────────────────────────────────────

/// `~/.cache/opengg/share` — throwaway exports, pruned after 3 days.
pub fn share_cache_root() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("opengg")
        .join("share")
}

/// Default output: `<cache>/opengg/share/<platform id(s)>/<clip stem>.mp4`.
pub fn default_output_path(plan: &SharePlan) -> PathBuf {
    let dir = if plan.platforms.is_empty() {
        Platform::Original.id().to_string()
    } else {
        plan.platforms
            .iter()
            .map(|p| p.id())
            .collect::<Vec<_>>()
            .join("-")
    };
    let stem = plan
        .input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "clip".into());
    share_cache_root().join(dir).join(format!("{stem}.mp4"))
}

// ─── Encoders ───────────────────────────────────────────────────────────────

/// H.264 encoders in preference order: NVENC (fast, good quality), VAAPI
/// (AMD/Intel), libx264 (always works). H.264 because every platform accepts
/// it and decodes it in-browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoder {
    Nvenc,
    Vaapi,
    X264,
}

const VAAPI_DEVICE: &str = "/dev/dri/renderD128";

impl Encoder {
    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            Encoder::Nvenc => "h264_nvenc",
            Encoder::Vaapi => "h264_vaapi",
            Encoder::X264 => "libx264",
        }
    }
}

/// Encoders ffmpeg was built with (cached for the process lifetime). Being
/// listed doesn't mean the hardware exists — [`render`] falls through to the
/// next one when an encoder fails.
pub fn available_encoders() -> &'static [Encoder] {
    static CACHE: OnceLock<Vec<Encoder>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let listing = Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .stdin(Stdio::null())
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        let names: Vec<&str> = listing
            .lines()
            .filter_map(|l| l.split_whitespace().nth(1))
            .collect();
        [Encoder::Nvenc, Encoder::Vaapi, Encoder::X264]
            .into_iter()
            .filter(|e| names.contains(&e.ffmpeg_name()))
            .filter(|e| *e != Encoder::Vaapi || Path::new(VAAPI_DEVICE).exists())
            .collect()
    })
}

fn num(v: f64) -> String {
    // Trim "60.000" → "60" but keep 29.97…
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `-filter_complex` graph for the plan. Output pads: `[v]`, and `[a]` when
/// there are tracks.
pub fn filter_graph(plan: &SharePlan, enc: Encoder) -> String {
    let mut v = format!("[0:v]fps={}", num(plan.fps));
    if let Some(c) = plan.crop {
        v.push_str(&format!(",crop={}:{}:{}:{}:exact=1", c.w, c.h, c.x, c.y));
    }
    let base_short = plan
        .crop
        .map_or(plan.source.width.min(plan.source.height), |c| c.w.min(c.h));
    // Lanczos keeps edges crisp when enlarging; bicubic is cheaper and
    // ringing-free when shrinking.
    let flags = if plan.short_side > base_short {
        "lanczos"
    } else {
        "bicubic"
    };
    v.push_str(&format!(
        ",scale={}:{}:flags={flags},format=yuv420p,setsar=1",
        plan.width, plan.height
    ));
    if enc == Encoder::Vaapi {
        v.push_str(",format=nv12,hwupload");
    }
    v.push_str("[v]");

    let mut g = v;
    match plan.tracks.as_slice() {
        [] => {}
        [t] => g.push_str(&format!(
            ";[0:a:{}]volume={},aformat=sample_rates=48000:channel_layouts=stereo[a]",
            t.index,
            num(t.gain)
        )),
        ts => {
            let mut labels = String::new();
            for (k, t) in ts.iter().enumerate() {
                g.push_str(&format!(";[0:a:{}]volume={}[a{k}]", t.index, num(t.gain)));
                labels.push_str(&format!("[a{k}]"));
            }
            // normalize=0: amix would otherwise divide each input by n, making
            // a 3-track mix quieter than the game alone.
            g.push_str(&format!(
                ";{labels}amix=inputs={}:normalize=0,aformat=sample_rates=48000:channel_layouts=stereo[a]",
                ts.len()
            ));
        }
    }
    g
}

/// Full ffmpeg argument list (after the program name).
pub fn ffmpeg_args(plan: &SharePlan, enc: Encoder, video_bps: u64, out: &Path) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-y"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    if enc == Encoder::Vaapi {
        a.extend(["-vaapi_device".into(), VAAPI_DEVICE.into()]);
    }
    // Input-side seek: fast, and ffmpeg decodes from the previous keyframe so
    // the cut is frame-accurate.
    a.extend([
        "-ss".into(),
        secs_arg(plan.start),
        "-to".into(),
        secs_arg(plan.end),
        "-i".into(),
        plan.input.to_string_lossy().into_owned(),
        "-filter_complex".into(),
        filter_graph(plan, enc),
        "-map".into(),
        "[v]".into(),
    ]);
    if plan.tracks.is_empty() {
        a.push("-an".into());
    } else {
        a.extend(["-map".into(), "[a]".into()]);
    }
    a.extend(["-c:v".into(), enc.ffmpeg_name().into()]);
    let extra: &[&str] = match enc {
        Encoder::Nvenc => &[
            "-preset",
            "p6",
            "-tune",
            "hq",
            "-rc",
            "vbr",
            "-multipass",
            "qres",
            "-spatial-aq",
            "1",
            "-rc-lookahead",
            "20",
            "-profile:v",
            "high",
        ],
        Encoder::Vaapi => &[],
        Encoder::X264 => &["-preset", "faster", "-profile:v", "high"],
    };
    a.extend(extra.iter().map(|s| s.to_string()));
    a.extend([
        "-b:v".into(),
        video_bps.to_string(),
        "-maxrate".into(),
        (video_bps * 3 / 2).to_string(),
        "-bufsize".into(),
        (video_bps * 2).to_string(),
        "-g".into(),
        ((2.0 * plan.fps).round() as u64).max(1).to_string(),
    ]);
    if !plan.tracks.is_empty() {
        a.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "160k".into()]);
    }
    a.extend(
        ["-movflags", "+faststart", "-progress", "pipe:1", "-nostats"]
            .iter()
            .map(|s| s.to_string()),
    );
    a.push(out.to_string_lossy().into_owned());
    a
}

enum EncodeError {
    Cancelled,
    Failed(String),
}

/// Run one ffmpeg encode, reporting progress and honouring `cancel`.
fn run_encode(
    plan: &SharePlan,
    enc: Encoder,
    video_bps: u64,
    out: &Path,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(f64),
) -> Result<(), EncodeError> {
    let mut child = Command::new("ffmpeg")
        .args(ffmpeg_args(plan, enc, video_bps, out))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| EncodeError::Failed(format!("ffmpeg: {e}")))?;

    let (tx, rx) = mpsc::channel::<f64>();
    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let Some(stdout) = stdout else { return };
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(v) = line.strip_prefix("out_time_us=") {
                if let Ok(us) = v.trim().parse::<i64>() {
                    let _ = tx.send(us as f64 / 1e6);
                }
            }
        }
    });
    let stderr = child.stderr.take();
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut e) = stderr {
            let _ = e.read_to_string(&mut s);
        }
        s
    });

    let dur = plan.duration.max(0.001);
    let report = |t: f64| on_progress((t / dur).clamp(0.0, 0.99));
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            let _ = err_reader.join();
            return Err(EncodeError::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(t) => report(t),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // stdout closed: ffmpeg is exiting; poll for the status.
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => {}
            Err(e) => return Err(EncodeError::Failed(format!("ffmpeg wait: {e}"))),
        }
    };
    let _ = reader.join();
    while let Ok(t) = rx.try_recv() {
        report(t);
    }
    let err = err_reader.join().unwrap_or_default();
    if status.success() {
        Ok(())
    } else {
        let tail: Vec<&str> = err.lines().rev().take(4).collect();
        Err(EncodeError::Failed(format!(
            "{} failed: {}",
            enc.ffmpeg_name(),
            tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
        )))
    }
}

/// Encode `plan` to `output` (made unique: "name (2).mp4" …).
///
/// Tries each available encoder in order. If the platform has a byte cap and
/// the result overshoots, re-encodes at a lower bitrate (up to
/// [`super::planner::MAX_RETRIES`] times), then fails with
/// [`super::planner::TOO_LONG_ERROR`]. Progress runs 0‥0.99 per attempt and
/// 1.0 once the file is in place. Cancelling returns [`CANCELLED`] and leaves
/// no files behind. When `output` lives in the share cache, files there older
/// than 3 days are pruned first.
pub fn render(
    plan: &SharePlan,
    output: &Path,
    cancel: &AtomicBool,
    on_progress: impl Fn(f64),
) -> Result<PathBuf, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err(CANCELLED.into());
    }
    let dir = output.parent().unwrap_or_else(|| Path::new("."));
    if dir.starts_with(share_cache_root()) {
        prune_old(dir, CACHE_MAX_AGE);
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let encoders = available_encoders();
    if encoders.is_empty() {
        return Err("ffmpeg has no H.264 encoder (need libx264, NVENC or VAAPI)".into());
    }
    let final_path = unique_path(output)?;
    let mut bitrate = plan.video_bps;
    let mut retries = 0;
    // Once an encoder works, retries start from it instead of re-failing the
    // ones above it.
    let mut first = 0;
    loop {
        let part = part_path(&final_path);
        let mut last_err = String::new();
        let mut done = false;
        for (i, &enc) in encoders.iter().enumerate().skip(first) {
            match run_encode(plan, enc, bitrate, &part, cancel, &on_progress) {
                Ok(()) => {
                    first = i;
                    done = true;
                    break;
                }
                Err(EncodeError::Cancelled) => {
                    let _ = std::fs::remove_file(&part);
                    return Err(CANCELLED.into());
                }
                Err(EncodeError::Failed(e)) => {
                    log::warn!("share: {e}");
                    let _ = std::fs::remove_file(&part);
                    last_err = e;
                }
            }
        }
        if !done {
            return Err(last_err);
        }
        let size = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if let Some(max) = plan.spec.max_bytes {
            if size > max {
                let _ = std::fs::remove_file(&part);
                bitrate =
                    retry_bitrate(bitrate, size, max, plan.audio_bps, plan.duration, retries)?;
                retries += 1;
                log::info!("share: {size} B > {max} B cap, retry {retries} at {bitrate} b/s");
                continue;
            }
        }
        if let Err(e) = std::fs::rename(&part, &final_path) {
            let _ = std::fs::remove_file(&part);
            return Err(format!("rename: {e}"));
        }
        on_progress(1.0);
        return Ok(final_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::share::planner::TOO_LONG_ERROR;

    fn src() -> SourceInfo {
        SourceInfo {
            duration: 300.0,
            width: 2560,
            height: 1440,
            fps: 60.0,
            audio_streams: 3,
        }
    }

    fn req(platforms: Vec<Platform>) -> SharePlanRequest {
        SharePlanRequest {
            input: PathBuf::from("/clips/Game_2026-01-01_12-00-00.mp4"),
            start: None,
            end: None,
            platforms,
            short_side: None,
            vertical: false,
            crop_pos: 0.5,
            tracks: vec![(0, 1.0), (1, 0.5)],
        }
    }

    #[test]
    fn plan_x_defaults() {
        let p = plan_with_source(&req(vec![Platform::X]), &src());
        assert_eq!((p.start, p.end), (160.0, 300.0));
        assert_eq!((p.width, p.height, p.short_side), (1920, 1080, 1080));
        assert_eq!(p.fps, 60.0);
        assert_eq!(p.video_bps, 12_000_000);
        assert_eq!(p.audio_bps, 160_000);
        assert_eq!(p.estimate_label, "213 MB");
        assert!(p.fits);
        let j = serde_json::to_string(&p).unwrap();
        assert!(j.contains("\"estimateLabel\":\"213 MB\""));
        assert_eq!(serde_json::from_str::<SharePlan>(&j).unwrap(), p);
    }

    #[test]
    fn plan_discord_small() {
        let p = plan_with_source(&req(vec![Platform::Discord]), &src());
        assert_eq!(p.duration, 300.0);
        assert_eq!((p.short_side, p.fps), (480, 30.0));
        // At the 100 kb/s floor: (100k + 160k) * 300 s / 8 = 9.75 MB.
        assert_eq!(p.estimate_bytes, 9_750_000);
        assert!(p.fits);
        let mut r = req(vec![Platform::Discord]);
        r.tracks = vec![(0, 1.0)];
        let long = SourceInfo {
            duration: 600.0,
            ..src()
        };
        assert!(!plan_with_source(&r, &long).fits);
    }

    #[test]
    fn plan_vertical_and_tracks() {
        let mut r = req(vec![Platform::TikTok]);
        r.vertical = true;
        r.tracks = vec![(0, 1.0), (7, 1.0), (2, f64::NAN), (1, 9.0)];
        let p = plan_with_source(&r, &src());
        assert_eq!(
            p.crop,
            Some(Crop {
                x: 874,
                y: 0,
                w: 810,
                h: 1440
            })
        );
        assert!(p.ladder.iter().any(|s| s.short_side == 810 && s.original));
        assert_eq!(p.short_side, 810);
        assert_eq!((p.width, p.height), (810, 1440));
        assert_eq!(
            p.tracks,
            vec![
                ShareTrack {
                    index: 0,
                    gain: 1.0
                },
                ShareTrack {
                    index: 2,
                    gain: 1.0
                },
                ShareTrack {
                    index: 1,
                    gain: 4.0
                }
            ]
        );
        // Explicit size above the platform cap is clamped.
        r.short_side = Some(4320);
        assert_eq!(plan_with_source(&r, &src()).short_side, 2160);
    }

    #[test]
    fn request_json_from_qml() {
        let r: SharePlanRequest = serde_json::from_str(
            r#"{"input":"/a.mp4","platforms":["discord","x"],"vertical":true,"tracks":[[0,1.0],[2,0.8]]}"#,
        )
        .unwrap();
        assert_eq!(r.platforms, vec![Platform::Discord, Platform::X]);
        assert_eq!(r.crop_pos, 0.5);
        assert_eq!(r.tracks, vec![(0, 1.0), (2, 0.8)]);
    }

    #[test]
    fn filter_graph_shapes() {
        let mut r = req(vec![Platform::TikTok]);
        r.vertical = true;
        r.short_side = Some(1080);
        let p = plan_with_source(&r, &src());
        assert_eq!(
            filter_graph(&p, Encoder::X264),
            "[0:v]fps=60,crop=810:1440:874:0:exact=1,scale=1080:1920:flags=lanczos,format=yuv420p,setsar=1[v];\
             [0:a:0]volume=1[a0];[0:a:1]volume=0.5[a1];\
             [a0][a1]amix=inputs=2:normalize=0,aformat=sample_rates=48000:channel_layouts=stereo[a]"
        );
        let mut r = req(vec![Platform::X]);
        r.tracks = vec![(2, 0.8)];
        let p = plan_with_source(&r, &src());
        let g = filter_graph(&p, Encoder::Vaapi);
        assert!(
            g.starts_with("[0:v]fps=60,scale=1920:1080:flags=bicubic,"),
            "{g}"
        );
        assert!(g.contains("format=nv12,hwupload[v]"), "{g}");
        assert!(
            g.ends_with(";[0:a:2]volume=0.8,aformat=sample_rates=48000:channel_layouts=stereo[a]")
        );
        r.tracks.clear();
        let p = plan_with_source(&r, &src());
        assert!(!filter_graph(&p, Encoder::X264).contains("[a]"));
        let args = ffmpeg_args(&p, Encoder::X264, 1_000_000, Path::new("/o.mp4"));
        assert!(args.contains(&"-an".to_string()));
        assert!(!args.contains(&"-c:a".to_string()));
        let pos = |k: &str| args.iter().position(|a| a == k).unwrap();
        assert!(pos("-ss") < pos("-i"));
        assert_eq!(args[pos("-maxrate") + 1], "1500000");
        assert_eq!(args[pos("-bufsize") + 1], "2000000");
        assert_eq!(args[pos("-g") + 1], "120");
    }

    #[test]
    fn rate_parsing_and_paths() {
        assert_eq!(parse_rate("60/1"), 60.0);
        assert!((parse_rate("30000/1001") - 29.97).abs() < 0.01);
        assert_eq!(parse_rate("0/0"), 0.0);
        assert_eq!(num(60.0), "60");
        assert_eq!(num(0.5), "0.5");
        let p = plan_with_source(&req(vec![Platform::Discord, Platform::X]), &src());
        let out = default_output_path(&p);
        assert!(
            out.ends_with("discord-x/Game_2026-01-01_12-00-00.mp4"),
            "{}",
            out.display()
        );
        assert!(out.starts_with(share_cache_root()));
    }

    // ── Integration (needs ffmpeg + libx264 on PATH) ──

    fn ffmpeg_ready() -> bool {
        super::super::util::which("ffmpeg")
            && super::super::util::which("ffprobe")
            && available_encoders().contains(&Encoder::X264)
    }

    struct TempDir(PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn make_clip(tag: &str) -> (TempDir, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "opengg-share-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("Test Game_clip.mp4");
        let ok = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=640x360:rate=30:duration=6",
            ])
            .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=6"])
            .args(["-f", "lavfi", "-i", "sine=frequency=660:duration=6"])
            .args(["-map", "0", "-map", "1", "-map", "2"])
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-b:v", "8M"])
            .args(["-c:a", "aac", "-shortest"])
            .arg(&clip)
            .status()
            .unwrap()
            .success();
        assert!(ok, "test clip generation failed");
        (TempDir(dir), clip)
    }

    fn probe_streams(p: &Path) -> (u32, u32, usize) {
        let (_, w, h) = crate::media::probe_video(p);
        let o = Command::new("ffprobe")
            .args([
                "-v",
                "quiet",
                "-select_streams",
                "a",
                "-show_entries",
                "stream=index",
            ])
            .args(["-of", "csv=p=0"])
            .arg(p)
            .output()
            .unwrap();
        let n = String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count();
        (w, h, n)
    }

    fn files_in(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn integration_vertical_render() {
        if !ffmpeg_ready() {
            eprintln!("skipping: ffmpeg/libx264 not available");
            return;
        }
        let (tmp, clip) = make_clip("vert");
        let src = probe_source(&clip).unwrap();
        assert_eq!(src.audio_streams, 2);
        let r = SharePlanRequest {
            input: clip.clone(),
            start: Some(1.0),
            end: Some(5.0),
            platforms: vec![Platform::TikTok],
            short_side: None,
            vertical: true,
            crop_pos: 0.3,
            tracks: vec![(0, 1.0), (1, 0.7)],
        };
        let p = plan_with_source(&r, &src);
        let out = tmp.0.join("out").join("vertical.mp4");
        let last = std::cell::Cell::new(0.0);
        let got = render(&p, &out, &AtomicBool::new(false), |f| last.set(f)).unwrap();
        assert_eq!(got, out);
        assert_eq!(last.get(), 1.0);
        let (w, h, n) = probe_streams(&got);
        assert!(w % 2 == 0 && h % 2 == 0, "{w}x{h}");
        let ratio = w as f64 / h as f64;
        assert!((ratio - 9.0 / 16.0).abs() < 0.02, "{w}x{h}");
        assert_eq!(n, 1);
        // Second render of the same name doesn't overwrite.
        let again = render(&p, &out, &AtomicBool::new(false), |_| {}).unwrap();
        assert_eq!(again, tmp.0.join("out").join("vertical (2).mp4"));
        assert!(files_in(&tmp.0.join("out"))
            .iter()
            .all(|f| !f.contains(".part.")));
    }

    #[test]
    fn integration_size_cap() {
        if !ffmpeg_ready() {
            eprintln!("skipping: ffmpeg/libx264 not available");
            return;
        }
        let (tmp, clip) = make_clip("cap");
        let src = probe_source(&clip).unwrap();
        for tracks in [vec![(0, 1.0), (1, 1.0)], vec![]] {
            let r = SharePlanRequest {
                input: clip.clone(),
                start: None,
                end: None,
                platforms: vec![Platform::Original],
                short_side: Some(360),
                vertical: false,
                crop_pos: 0.5,
                tracks,
            };
            let mut p = plan_with_source(&r, &src);
            // Synthetic 150 KB platform cap; re-derive the bitrate for it.
            p.spec.max_bytes = Some(150_000);
            let bp = bitrate_plan(p.width, p.height, src.fps, p.duration, &p.spec, p.audio_bps);
            p.video_bps = bp.video_bps;
            let out = tmp.0.join(format!("capped{}.mp4", p.tracks.len()));
            match render(&p, &out, &AtomicBool::new(false), |_| {}) {
                Ok(f) => {
                    let size = std::fs::metadata(&f).unwrap().len();
                    eprintln!("cap render with {} tracks: {size} B", p.tracks.len());
                    assert!(size <= 150_000, "{size} B over cap");
                }
                Err(e) => {
                    eprintln!("cap render with {} tracks: {e}", p.tracks.len());
                    assert_eq!(e, TOO_LONG_ERROR)
                }
            }
        }
        assert!(files_in(&tmp.0).iter().all(|f| !f.contains(".part.")));
    }

    #[test]
    fn integration_cancel_leaves_nothing() {
        if !ffmpeg_ready() {
            eprintln!("skipping: ffmpeg/libx264 not available");
            return;
        }
        let (tmp, clip) = make_clip("cancel");
        let src = probe_source(&clip).unwrap();
        let mut r = req(vec![Platform::Original]);
        r.input = clip.clone();
        r.short_side = Some(2160); // big upscale = slow enough to interrupt
        r.tracks = vec![(0, 1.0)];
        let p = plan_with_source(&r, &src);
        let out_dir = tmp.0.join("cancel-out");
        let out = out_dir.join("x.mp4");
        let cancel = AtomicBool::new(false);
        let res = std::thread::scope(|s| {
            s.spawn(|| {
                std::thread::sleep(Duration::from_millis(30));
                cancel.store(true, Ordering::Relaxed);
            });
            render(&p, &out, &cancel, |_| {})
        });
        assert_eq!(res, Err(CANCELLED.to_string()));
        assert!(files_in(&out_dir).is_empty(), "{:?}", files_in(&out_dir));
        // Pre-cancelled: nothing is even started.
        assert_eq!(
            render(&p, &out, &AtomicBool::new(true), |_| {}),
            Err(CANCELLED.to_string())
        );
    }
}

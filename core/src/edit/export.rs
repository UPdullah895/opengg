//! Render an [`EditProject`] to a new file with the ffmpeg CLI.
//!
//! Design notes (the WHY):
//! - **Blocking, cancellable, progress via callback.** The caller (a Qt
//!   controller) runs this on a worker thread; it polls `cancel` every 100 ms
//!   and kills ffmpeg, so a "Cancel" click takes effect promptly.
//! - **Input seek.** Clips can be long replays while the edit uses a few
//!   seconds near the end. `-ss` before `-i` seeks the demuxer so ffmpeg
//!   doesn't decode everything before the first kept part; we seek 2 s early
//!   (keyframe slack) and pass that point to the graph as `time_origin`
//!   because input seeking rebases timestamps to 0.
//! - **Atomic output.** We render to a hidden `.<stem>.<rand>.part.mp4` next
//!   to the destination and rename only on success, so the clip library
//!   watcher never sees (and thumbnails) a half-written file, and a failed or
//!   cancelled export leaves nothing behind.
//! - **Encoder fallback.** `Auto` tries NVENC, then VAAPI, then libx264. An
//!   encoder can be compiled in yet unusable (no NVIDIA GPU, no render node),
//!   which only shows when ffmpeg runs — so each failure falls through to the
//!   next, and the final error lists every attempt.
//! - **`nice -n 10`.** Exports often run while a game is still being played;
//!   lowering priority keeps the game's frame pacing smooth.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, OnceLock};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::graph::{self, num, GraphOptions};
use super::project::EditProject;
use crate::subprocess;

/// Video encoder choice. Stream copy is deliberately absent: any edit
/// (cuts at non-keyframes, speed, looks, levels) needs a re-encode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Encoder {
    /// Software H.264 at the given CRF (lower = better; ~18–23 is sensible).
    X264 { crf: u8 },
    /// NVIDIA `h264_nvenc`.
    Nvenc,
    /// VA-API `h264_vaapi` on `/dev/dri/renderD128`.
    Vaapi,
    /// Hardware first, software fallback.
    Auto,
}

/// Everything about an export that isn't the edit itself.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportOptions {
    /// Destination file (overwritten on success).
    pub output: PathBuf,
    pub encoder: Encoder,
    /// Add a pre-mixed "Mix" audio track first (only with ≥ 2 tracks).
    pub mix_track: bool,
    /// AAC bitrate per audio track, kbit/s.
    pub audio_bitrate_kbps: u32,
}

/// Seek this many seconds before the first kept frame (keyframe slack).
const SEEK_PAD: f64 = 2.0;
/// Extra input read past the last kept frame, so the trim end is never short.
const TAIL_PAD: f64 = 1.0;
const VAAPI_DEVICE: &str = "/dev/dri/renderD128";
/// CRF used for the libx264 leg of `Auto`.
const AUTO_X264_CRF: u8 = 20;

/// What we learn about the input before building the graph.
#[derive(Debug, Clone, PartialEq)]
struct Probe {
    fps: f64,
    /// One entry per audio stream; `None` when the stream has no title.
    titles: Vec<Option<String>>,
}

fn probe(input: &Path) -> Result<Probe, String> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,r_frame_rate,avg_frame_rate:stream_tags=title,name,handler_name",
            "-of",
            "json",
        ])
        .arg(input)
        .output()
        .map_err(|e| format!("ffprobe: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    parse_probe(&String::from_utf8_lossy(&out.stdout))
}

/// Parse a `num/den` rate; rejects 0, garbage and absurd values (VFR
/// recordings sometimes report a timebase like `1000/1` as `r_frame_rate`).
fn parse_rate(s: &str) -> Option<f64> {
    let (a, b) = s.split_once('/').unwrap_or((s, "1"));
    let v = a.trim().parse::<f64>().ok()? / b.trim().parse::<f64>().ok()?;
    (v.is_finite() && v > 0.0 && v <= 240.0).then_some(v)
}

fn parse_probe(json: &str) -> Result<Probe, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("ffprobe output: {e}"))?;
    let streams = v["streams"].as_array().cloned().unwrap_or_default();
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .ok_or("input has no video stream")?;
    let rate = |k: &str| video[k].as_str().and_then(parse_rate);
    let fps = rate("r_frame_rate")
        .or_else(|| rate("avg_frame_rate"))
        .unwrap_or(60.0);
    let titles = streams
        .iter()
        .filter(|s| s["codec_type"] == "audio")
        .map(|s| track_title(&s["tags"]))
        .collect();
    Ok(Probe { fps, titles })
}

/// A track's display name from its ffprobe `tags`. MP4 keeps per-track names
/// in `name` (gpu-screen-recorder) or `handler_name` (ffmpeg's mov muxer drops
/// `title` but writes `handler_name`), so all three are checked — the same
/// order `media::analyze_media_sync` uses. "SoundHandler" is ffmpeg's generic
/// default, not a name.
fn track_title(tags: &serde_json::Value) -> Option<String> {
    ["title", "name", "handler_name"]
        .iter()
        .filter_map(|k| tags[*k].as_str())
        .map(str::trim)
        .find(|t| !t.is_empty() && *t != "SoundHandler")
        .map(str::to_string)
}

/// Encoders compiled into the local ffmpeg (cached: the answer can't change
/// while we run, and the probe costs a process spawn).
fn available_encoders() -> &'static [String] {
    static CACHE: OnceLock<Vec<String>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
            .map(|o| parse_encoders(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or_default()
    })
}

/// Lines look like ` V....D h264_nvenc  NVIDIA NVENC ...`; take the name
/// after a 6-char capability column.
fn parse_encoders(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let caps = it.next()?;
            let name = it.next()?;
            // Skip the legend (" V..... = Video") and the "------" rule.
            (caps.len() == 6
                && name != "="
                && caps.chars().next().is_some_and(|c| "VAS".contains(c)))
            .then(|| name.to_string())
        })
        .collect()
}

/// A concrete encoder attempt.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Attempt {
    Nvenc,
    Vaapi,
    X264(u8),
}

impl Attempt {
    fn codec(self) -> &'static str {
        match self {
            Attempt::Nvenc => "h264_nvenc",
            Attempt::Vaapi => "h264_vaapi",
            Attempt::X264(_) => "libx264",
        }
    }

    /// Options placed before `-i` (global/hwdevice options).
    fn pre_input_args(self) -> Vec<String> {
        match self {
            // Also becomes the filter device, which `hwupload` needs.
            Attempt::Vaapi => vec!["-vaapi_device".into(), VAAPI_DEVICE.into()],
            _ => vec![],
        }
    }

    fn codec_args(self) -> Vec<String> {
        let v: Vec<&str> = match self {
            Attempt::Nvenc => vec![
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p5",
                "-rc",
                "vbr",
                "-cq",
                "21",
                "-b:v",
                "0",
            ],
            Attempt::Vaapi => vec!["-c:v", "h264_vaapi"],
            Attempt::X264(_) => vec!["-c:v", "libx264", "-preset", "veryfast", "-crf"],
        };
        let mut v: Vec<String> = v.into_iter().map(String::from).collect();
        if let Attempt::X264(crf) = self {
            v.push(crf.to_string());
            v.extend(["-pix_fmt".to_string(), "yuv420p".to_string()]);
        }
        v
    }
}

/// Expand the user's choice into the ordered attempts to make. `Auto` skips
/// encoders this ffmpeg build doesn't list; an explicit choice is always
/// tried so the user sees ffmpeg's own error if it's missing.
fn plan(enc: Encoder, listed: &[String]) -> Vec<Attempt> {
    match enc {
        Encoder::X264 { crf } => vec![Attempt::X264(crf.min(51))],
        Encoder::Nvenc => vec![Attempt::Nvenc],
        Encoder::Vaapi => vec![Attempt::Vaapi],
        Encoder::Auto => {
            let mut v: Vec<Attempt> = [Attempt::Nvenc, Attempt::Vaapi]
                .into_iter()
                .filter(|a| listed.iter().any(|e| e == a.codec()))
                .collect();
            v.push(Attempt::X264(AUTO_X264_CRF));
            v
        }
    }
}

/// Hidden, unique sibling of `output` to render into.
fn part_path(output: &Path) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mixed = nanos
        ^ (u64::from(std::process::id()) << 32)
        ^ SEQ
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let dir = output.parent().unwrap_or_else(|| Path::new("."));
    let stem = output
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "export".into());
    dir.join(format!(".{stem}.{mixed:016x}.part.mp4"))
}

/// Parse one `-progress` line into output microseconds. Both keys carry
/// microseconds in current ffmpeg (`out_time_ms` is a historical misnomer).
fn parse_progress_line(line: &str) -> Option<f64> {
    let (k, v) = line.trim().split_once('=')?;
    if k != "out_time_us" && k != "out_time_ms" {
        return None;
    }
    let us = v.trim().parse::<i64>().ok()?;
    Some(us as f64 / 1_000_000.0)
}

enum RunError {
    Cancelled,
    Failed(String),
}

/// Build the full ffmpeg argument list (after the program name).
#[allow(clippy::too_many_arguments)] // all plain data, one call site + tests
fn ffmpeg_args(
    input: &Path,
    part: &Path,
    attempt: Attempt,
    project: &EditProject,
    probe: &Probe,
    mix_track: bool,
    audio_bitrate_kbps: u32,
) -> Vec<String> {
    let from = project
        .parts
        .iter()
        .map(|p| p.start)
        .fold(f64::INFINITY, f64::min);
    let to = project
        .parts
        .iter()
        .map(|p| p.end)
        .fold(f64::NEG_INFINITY, f64::max);
    let seek = (from - SEEK_PAD).max(0.0);
    let read = to - seek + TAIL_PAD;
    let tracks = probe.titles.len();
    let g = graph::build(
        project,
        probe.fps,
        tracks,
        seek,
        GraphOptions {
            mix_track,
            hw_upload: attempt == Attempt::Vaapi,
        },
    );

    let mut a: Vec<String> = vec!["-hide_banner".into(), "-nostdin".into(), "-y".into()];
    a.extend(attempt.pre_input_args());
    a.extend(["-ss".into(), num(seek), "-t".into(), num(read), "-i".into()]);
    a.push(input.to_string_lossy().into_owned());
    a.extend(["-filter_complex".into(), g.filter_complex.clone()]);
    a.extend(["-map".into(), g.video_out.clone()]);
    for l in &g.audio_outs {
        a.extend(["-map".into(), l.clone()]);
    }
    a.extend(attempt.codec_args());
    if !g.audio_outs.is_empty() {
        a.extend([
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            format!("{}k", audio_bitrate_kbps.max(32)),
            "-ar".into(),
            "48000".into(),
        ]);
        let has_mix = g.audio_outs.first().is_some_and(|l| l == "[amix]");
        let mut titles: Vec<String> = Vec::new();
        if has_mix {
            titles.push("Mix".into());
        }
        for (t, title) in probe.titles.iter().enumerate() {
            titles.push(title.clone().unwrap_or_else(|| format!("Track {}", t + 1)));
        }
        // `title` for containers/tools that read it; `handler_name` because
        // that is what the MP4 muxer actually persists per track.
        for (i, title) in titles.iter().enumerate() {
            a.extend([format!("-metadata:s:a:{i}"), format!("title={title}")]);
            a.extend([
                format!("-metadata:s:a:{i}"),
                format!("handler_name={title}"),
            ]);
        }
    }
    a.extend([
        "-movflags".into(),
        "+faststart".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-nostats".into(),
    ]);
    a.push(part.to_string_lossy().into_owned());
    a
}

/// Run one ffmpeg attempt to completion, cancellation or failure.
fn run_ffmpeg(
    args: &[String],
    total: f64,
    cancel: &AtomicBool,
    on_progress: &impl Fn(f64),
    last_pct: &mut i64,
) -> Result<(), RunError> {
    let mut cmd = if subprocess::is_available("nice") {
        let mut c = Command::new("nice");
        c.args(["-n", "10", "ffmpeg"]);
        c
    } else {
        Command::new("ffmpeg")
    };
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    log::debug!("edit export: ffmpeg {}", args.join(" "));
    let mut child = cmd
        .spawn()
        .map_err(|e| RunError::Failed(format!("spawn ffmpeg: {e}")))?;

    // Progress lines → channel, so callbacks run on *this* thread (the
    // callback needn't be Send) and the loop can still poll `cancel`.
    let (tx, rx) = mpsc::channel::<f64>();
    let stdout = child.stdout.take();
    let out_thread = thread::spawn(move || {
        if let Some(s) = stdout {
            for line in BufReader::new(s).lines().map_while(Result::ok) {
                if let Some(t) = parse_progress_line(&line) {
                    let _ = tx.send(t);
                }
            }
        }
    });
    // Drain stderr concurrently (a full pipe would stall ffmpeg); keep the
    // tail for the error message.
    let stderr = child.stderr.take();
    let err_thread = thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut s) = stderr {
            let _ = s.read_to_end(&mut buf);
        }
        let text = String::from_utf8_lossy(&buf).into_owned();
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        lines[lines.len().saturating_sub(6)..].join("\n")
    });

    let mut report = |t: f64| {
        let done = if total > 0.0 {
            (t / total).clamp(0.0, 0.99)
        } else {
            0.0
        };
        let pct = (done * 100.0).floor() as i64;
        if pct != *last_pct {
            *last_pct = pct;
            on_progress(done);
        }
    };

    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = out_thread.join();
            let _ = err_thread.join();
            return Err(RunError::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(t) => report(t),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // stdout closed: ffmpeg is exiting. Still honour cancel above
                // on the next turn, but don't busy-spin.
                match child.try_wait() {
                    Ok(Some(s)) => break s,
                    Ok(None) => thread::sleep(Duration::from_millis(100)),
                    Err(e) => return Err(RunError::Failed(format!("ffmpeg wait: {e}"))),
                }
            }
        }
    };
    let _ = out_thread.join();
    let tail = err_thread.join().unwrap_or_default();
    if status.success() {
        Ok(())
    } else {
        Err(RunError::Failed(if tail.is_empty() {
            format!("ffmpeg exited with {status}")
        } else {
            tail
        }))
    }
}

/// Export `project` (an edit of `input`) to `opts.output`.
///
/// Blocks until done. `on_progress` receives `[0, 0.99]` while encoding
/// (only when the whole percent changes) and `1.0` once the file is in
/// place. Setting `cancel` aborts within ~100 ms and returns
/// `Err("cancelled")`. On any error no output or temp file is left behind.
pub fn export(
    input: &Path,
    project: &EditProject,
    opts: &ExportOptions,
    cancel: &AtomicBool,
    on_progress: impl Fn(f64),
) -> Result<PathBuf, String> {
    if project.parts.is_empty() {
        return Err("nothing to export: the edit has no parts".into());
    }
    let total = project.output_duration();
    if total.is_nan() || total <= 0.0 {
        return Err("nothing to export: the edit is empty".into());
    }
    if opts.output == input {
        return Err("output must differ from the input clip".into());
    }
    let probe = probe(input)?;
    let attempts = plan(opts.encoder, available_encoders());
    let mut failures = Vec::new();
    let mut last_pct = -1;

    for attempt in attempts {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let part = part_path(&opts.output);
        let args = ffmpeg_args(
            input,
            &part,
            attempt,
            project,
            &probe,
            opts.mix_track,
            opts.audio_bitrate_kbps,
        );
        match run_ffmpeg(&args, total, cancel, &on_progress, &mut last_pct) {
            Ok(()) => {
                if let Err(e) = std::fs::rename(&part, &opts.output) {
                    let _ = std::fs::remove_file(&part);
                    return Err(format!("move export into place: {e}"));
                }
                on_progress(1.0);
                return Ok(opts.output.clone());
            }
            Err(RunError::Cancelled) => {
                let _ = std::fs::remove_file(&part);
                return Err("cancelled".into());
            }
            Err(RunError::Failed(msg)) => {
                let _ = std::fs::remove_file(&part);
                log::warn!("edit export: {} failed: {msg}", attempt.codec());
                failures.push(format!("{}: {msg}", attempt.codec()));
            }
        }
    }
    Err(format!("export failed\n{}", failures.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::project::Part;

    #[test]
    fn rates() {
        assert_eq!(parse_rate("60/1"), Some(60.0));
        assert!((parse_rate("60000/1001").unwrap() - 59.94).abs() < 0.01);
        assert_eq!(parse_rate("0/0"), None);
        assert_eq!(parse_rate("1000/1"), None);
    }

    #[test]
    fn probe_json() {
        let j = r#"{"streams":[
            {"codec_type":"video","r_frame_rate":"1000/1","avg_frame_rate":"30/1"},
            {"codec_type":"audio","tags":{"title":"Game"}},
            {"codec_type":"audio","tags":{"handler_name":"SoundHandler"}},
            {"codec_type":"audio","tags":{"name":"Mic","handler_name":"SoundHandler"}},
            {"codec_type":"audio","tags":{"handler_name":"Chat"}}]}"#;
        let p = parse_probe(j).unwrap();
        assert_eq!(p.fps, 30.0);
        assert_eq!(
            p.titles,
            vec![
                Some("Game".into()),
                None,
                Some("Mic".into()),
                Some("Chat".into())
            ]
        );
        assert!(parse_probe(r#"{"streams":[]}"#).is_err());
    }

    #[test]
    fn encoders_list() {
        let t = "Encoders:\n V..... = Video\n ------\n V....D libx264   libx264 H.264\n V....D h264_vaapi  H.264\n A....D aac  AAC\n";
        let e = parse_encoders(t);
        assert!(e.contains(&"libx264".to_string()));
        assert!(e.contains(&"h264_vaapi".to_string()));
        assert!(e.contains(&"aac".to_string()));
        assert_eq!(e.len(), 3);
        let listed = vec!["h264_vaapi".to_string(), "libx264".to_string()];
        assert_eq!(
            plan(Encoder::Auto, &listed),
            vec![Attempt::Vaapi, Attempt::X264(AUTO_X264_CRF)]
        );
        assert_eq!(plan(Encoder::Nvenc, &[]), vec![Attempt::Nvenc]);
    }

    #[test]
    fn progress_lines() {
        assert_eq!(parse_progress_line("out_time_us=1500000"), Some(1.5));
        assert_eq!(parse_progress_line("out_time_ms=2000000"), Some(2.0));
        assert_eq!(parse_progress_line("out_time=00:00:01.5"), None);
        assert_eq!(parse_progress_line("out_time_us=N/A"), None);
    }

    #[test]
    fn part_paths_are_hidden_and_unique() {
        let a = part_path(Path::new("/x/clip.mp4"));
        let b = part_path(Path::new("/x/clip.mp4"));
        assert_ne!(a, b);
        let name = a.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with(".clip.") && name.ends_with(".part.mp4"));
        assert_eq!(name.len(), ".clip.".len() + 16 + ".part.mp4".len());
    }

    #[test]
    fn args_seek_and_titles() {
        let mut p = EditProject::new(60.0, 2);
        p.parts = vec![Part::new(30.0, 35.0), Part::new(40.0, 50.0)];
        let probe = Probe {
            fps: 60.0,
            titles: vec![Some("Game".into()), None],
        };
        let a = ffmpeg_args(
            Path::new("in.mp4"),
            Path::new(".o.part.mp4"),
            Attempt::X264(20),
            &p,
            &probe,
            true,
            192,
        );
        let s = a.join(" ");
        assert!(s.contains("-ss 28 -t 23 -i in.mp4"), "{s}");
        assert!(s.contains("trim=start=2:end=7"));
        assert!(s.contains("-map [vout] -map [amix] -map [a0] -map [a1]"));
        assert!(s.contains(
            "-metadata:s:a:0 title=Mix -metadata:s:a:0 handler_name=Mix -metadata:s:a:1 title=Game"
        ));
        assert!(s.contains("-metadata:s:a:2 title=Track 2"));
        assert!(s.contains("-c:v libx264 -preset veryfast -crf 20 -pix_fmt yuv420p"));
        assert!(s.contains("-b:a 192k"));
        assert!(s.ends_with("-progress pipe:1 -nostats .o.part.mp4"));
        let v = ffmpeg_args(
            Path::new("in.mp4"),
            Path::new("o"),
            Attempt::Vaapi,
            &p,
            &probe,
            false,
            192,
        )
        .join(" ");
        assert!(v.starts_with("-hide_banner -nostdin -y -vaapi_device /dev/dri/renderD128 -ss"));
        assert!(v.contains("format=nv12,hwupload[vout]"));
    }

    // ── Integration (needs ffmpeg; skipped otherwise) ──

    fn have_ffmpeg() -> bool {
        subprocess::is_available("ffmpeg") && subprocess::is_available("ffprobe")
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "opengg-edit-{tag}-{}-{:x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn make_clip(dir: &Path) -> PathBuf {
        let p = dir.join("src.mp4");
        let st = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=30:duration=4",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=4",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=660:duration=4",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=880:duration=4",
                "-map",
                "0",
                "-map",
                "1",
                "-map",
                "2",
                "-map",
                "3",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-metadata:s:a:0",
                "handler_name=Game",
                "-metadata:s:a:1",
                "handler_name=Chat",
                "-metadata:s:a:2",
                "handler_name=Mic",
            ])
            .arg(&p)
            .status()
            .unwrap();
        assert!(st.success(), "failed to generate test clip");
        p
    }

    fn streams(p: &Path) -> Vec<(String, Option<String>)> {
        let o = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type:stream_tags=title,name,handler_name",
                "-of",
                "json",
            ])
            .arg(p)
            .output()
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        v["streams"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                (
                    s["codec_type"].as_str().unwrap_or("").to_string(),
                    track_title(&s["tags"]),
                )
            })
            .collect()
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".part.mp4"))
            .collect()
    }

    fn edit() -> EditProject {
        let mut p = EditProject::new(4.0, 3);
        p.parts = vec![Part::new(0.0, 1.5), Part::new(2.0, 4.0)];
        p.parts[1].speed = 2.0;
        p.set_track_muted(1, true).unwrap()
    }

    #[test]
    fn integration_export_tracks_and_duration() {
        if !have_ffmpeg() {
            eprintln!("skipping: ffmpeg not on PATH");
            return;
        }
        let dir = temp_dir("export");
        let src = make_clip(&dir);
        let project = edit();
        let expected = project.output_duration(); // 1.5 + 1.0
        for mix in [false, true] {
            let out = dir.join(format!("out-{mix}.mp4"));
            let opts = ExportOptions {
                output: out.clone(),
                encoder: Encoder::X264 { crf: 28 },
                mix_track: mix,
                audio_bitrate_kbps: 96,
            };
            let seen = std::cell::RefCell::new(Vec::new());
            let res = export(&src, &project, &opts, &AtomicBool::new(false), |f| {
                seen.borrow_mut().push(f)
            });
            assert_eq!(res.as_deref(), Ok(out.as_path()), "{res:?}");
            assert!(out.exists());
            let s = streams(&out);
            let audio: Vec<_> = s.iter().filter(|(t, _)| t == "audio").collect();
            assert_eq!(s.iter().filter(|(t, _)| t == "video").count(), 1);
            if mix {
                assert_eq!(audio.len(), 4);
                assert_eq!(audio[0].1.as_deref(), Some("Mix"));
                assert_eq!(audio[2].1.as_deref(), Some("Chat"));
            } else {
                assert_eq!(audio.len(), 3);
                assert_eq!(audio[0].1.as_deref(), Some("Game"));
                assert_eq!(audio[2].1.as_deref(), Some("Mic"));
            }
            let d = crate::media::probe_duration(&out.to_string_lossy());
            assert!((d - expected).abs() <= 0.15, "duration {d} vs {expected}");
            assert_eq!(seen.borrow().last().copied(), Some(1.0));
        }
        // Seeked input (time_origin > 0): late window only.
        let late = EditProject::new(4.0, 3).set_trim(2.5, 4.0).unwrap();
        let out = dir.join("late.mp4");
        let opts = ExportOptions {
            output: out.clone(),
            encoder: Encoder::X264 { crf: 28 },
            mix_track: false,
            audio_bitrate_kbps: 96,
        };
        export(&src, &late, &opts, &AtomicBool::new(false), |_| {}).unwrap();
        let d = crate::media::probe_duration(&out.to_string_lossy());
        assert!((d - 1.5).abs() <= 0.15, "duration {d}");
        assert!(leftovers(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn integration_cancel_leaves_nothing() {
        if !have_ffmpeg() {
            eprintln!("skipping: ffmpeg not on PATH");
            return;
        }
        let dir = temp_dir("cancel");
        let src = make_clip(&dir);
        let out = dir.join("out.mp4");
        let opts = ExportOptions {
            output: out.clone(),
            encoder: Encoder::X264 { crf: 28 },
            mix_track: true,
            audio_bitrate_kbps: 96,
        };
        // Cancelled before start.
        let cancel = AtomicBool::new(true);
        let res = export(&src, &edit(), &opts, &cancel, |_| {});
        assert_eq!(res, Err("cancelled".to_string()));
        assert!(!out.exists());
        assert!(leftovers(&dir).is_empty());
        // Cancelled mid-run: flip the flag from the first progress report,
        // which exercises the kill + part-file cleanup path.
        let cancel = AtomicBool::new(false);
        let res = export(&src, &edit(), &opts, &cancel, |_| {
            cancel.store(true, Ordering::Relaxed)
        });
        assert_eq!(res, Err("cancelled".to_string()));
        assert!(!out.exists());
        assert!(leftovers(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

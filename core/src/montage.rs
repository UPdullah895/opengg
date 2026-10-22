//! Montage builder — stitch several clips into one highlight reel.
//!
//! The SteelSeries Moments equivalent: pick clips, order them, choose a
//! transition, optionally lay a music bed underneath, export one video. It is
//! a single ffmpeg `filter_complex` run, split into two halves so the hard
//! part is testable without ffmpeg:
//!
//! - [`plan`] is pure: given a [`MontageSpec`] and the probed facts about each
//!   source ([`SourceInfo`]), it produces the full ffmpeg argument vector and
//!   the expected output duration. Every filtergraph decision lives here.
//! - [`build_montage`] probes the sources, calls [`plan`], and runs ffmpeg on
//!   the caller's thread with progress parsed from `-progress pipe:1` and a
//!   cancel flag the caller can raise.
//!
//! Choices that differ from GG on purpose:
//! - Each clip's **saved trim window** is honoured (the editor's in/out
//!   points), so trimming once is enough — GG makes you trim again inside the
//!   montage timeline.
//! - Captures carry one audio stream per source (Game / Chat / Mic …, see
//!   `gsr::remux_with_track_titles`), so a clip's tracks are **summed** into
//!   one stereo bed; a clip with no audio gets matching silence so the joins
//!   (`concat` / `acrossfade`) always see the same stream layout.
//! - Music can be **ducked** under the game audio with a sidechain
//!   compressor instead of a flat volume that either drowns the callouts or
//!   disappears in quiet stretches.
//! - A **vertical 9:16** layout with a blurred fill makes a Shorts/TikTok cut
//!   without a second editor.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::subprocess::command;

/// Shortest clip segment the planner accepts, in seconds. Anything shorter is
/// almost always a stale trim window and would make a transition impossible.
pub const MIN_SEGMENT_SECS: f64 = 0.5;

/// How clips join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Transition {
    /// Hard cut (`concat`): no overlap, total length is the plain sum.
    Cut,
    /// Cross-dissolve (`xfade=fade` + `acrossfade`).
    #[default]
    Crossfade,
    /// Dip through black (`xfade=fadeblack` + `acrossfade`).
    Fadeblack,
}

/// How a source that does not match the output aspect is fitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Scale to fit and pad with black bars.
    #[default]
    Letterbox,
    /// Scale to fit over a blurred, cropped copy of the same frame — the
    /// usual look for a 16:9 game capture in a 9:16 frame.
    Blur,
}

/// One clip in the montage, in playback order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MontageClip {
    pub path: String,
    /// Segment start in seconds. `None` = the saved trim start, else 0.
    #[serde(default)]
    pub start: Option<f64>,
    /// Segment end in seconds. `None` = the saved trim end, else the clip end.
    #[serde(default)]
    pub end: Option<f64>,
}

/// Optional music bed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MusicBed {
    pub path: String,
    /// Linear gain applied to the music, 0.0..=1.0.
    #[serde(default = "default_music_volume")]
    pub volume: f64,
    /// Duck the music whenever the clips' own audio is loud.
    #[serde(default = "default_true")]
    pub duck: bool,
}

fn default_music_volume() -> f64 {
    0.35
}
fn default_true() -> bool {
    true
}
fn default_transition_secs() -> f64 {
    0.6
}
fn default_fps() -> u32 {
    60
}

/// Everything the user chose in the montage dialog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MontageSpec {
    pub clips: Vec<MontageClip>,
    #[serde(default)]
    pub transition: Transition,
    /// Overlap length for `crossfade` / `fadeblack`; ignored for `cut`.
    #[serde(default = "default_transition_secs")]
    pub transition_secs: f64,
    /// Output frame size. `0` for either = the first clip's size.
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub fit: Fit,
    #[serde(default = "default_fps")]
    pub fps: u32,
    /// Fade the whole reel in from and out to black.
    #[serde(default = "default_true")]
    pub fade_in_out: bool,
    #[serde(default)]
    pub music: Option<MusicBed>,
    /// `h264` (default) or `h265` — the montage is always an MP4.
    #[serde(default)]
    pub codec: String,
    /// Absolute output path; empty = auto-named next to the first clip.
    #[serde(default)]
    pub output: String,
}

/// What [`plan`] needs to know about one source, gathered by probing.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub audio_streams: u32,
    /// Saved trim window from the clips DB, if any.
    pub trim: Option<(f64, f64)>,
}

/// The resolved segment for one clip after trims and clamping.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Segment {
    pub path: String,
    pub start: f64,
    pub duration: f64,
}

/// A ready-to-run ffmpeg invocation.
#[derive(Debug, Clone, PartialEq)]
pub struct MontagePlan {
    pub segments: Vec<Segment>,
    pub width: u32,
    pub height: u32,
    /// Transition length actually used (clamped; 0 for cuts).
    pub transition_secs: f64,
    /// Expected output length in seconds.
    pub total_secs: f64,
    pub output: String,
    pub args: Vec<String>,
}

/// Output length for segment lengths joined with overlap `t`.
pub fn total_duration(durations: &[f64], t: f64) -> f64 {
    let sum: f64 = durations.iter().sum();
    let joins = durations.len().saturating_sub(1) as f64;
    (sum - joins * t).max(0.0)
}

/// Round down to an even number — every yuv420p encoder needs even sides.
fn even(v: u32) -> u32 {
    v & !1
}

/// Seconds formatted the way ffmpeg likes them in filter arguments.
fn secs(v: f64) -> String {
    format!("{v:.3}")
}

/// Resolve each clip's segment from its explicit window, else its saved trim,
/// else the whole clip — clamped to the real duration.
fn resolve_segment(clip: &MontageClip, info: &SourceInfo) -> Result<Segment, String> {
    let (trim_start, trim_end) = info.trim.unwrap_or((0.0, info.duration));
    let start = clip.start.unwrap_or(trim_start).clamp(0.0, info.duration);
    let mut end = clip.end.unwrap_or(trim_end);
    if end <= 0.0 || end > info.duration {
        end = info.duration;
    }
    let duration = end - start;
    if duration < MIN_SEGMENT_SECS {
        return Err(format!(
            "{} is too short to use ({duration:.2}s after trimming)",
            Path::new(&clip.path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| clip.path.clone())
        ));
    }
    Ok(Segment {
        path: clip.path.clone(),
        start,
        duration,
    })
}

/// Default output path: `<first clip dir>/Montage_<unix time>.mp4`.
pub fn default_output(first_clip: &str) -> String {
    let dir = Path::new(first_clip)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    dir.join(format!("Montage_{stamp}.mp4"))
        .to_string_lossy()
        .into()
}

/// Build the ffmpeg invocation for `spec`. `sources[i]` describes
/// `spec.clips[i]`. Pure: touches neither the filesystem nor ffmpeg.
pub fn plan(spec: &MontageSpec, sources: &[SourceInfo]) -> Result<MontagePlan, String> {
    if spec.clips.is_empty() {
        return Err("Pick at least one clip".into());
    }
    if spec.clips.len() != sources.len() {
        return Err("internal: clip/probe count mismatch".into());
    }
    if let Some(bad) = sources.iter().zip(&spec.clips).find(|(s, _)| s.duration <= 0.0) {
        return Err(format!("Could not read {}", bad.1.path));
    }

    let segments = spec
        .clips
        .iter()
        .zip(sources)
        .map(|(c, s)| resolve_segment(c, s))
        .collect::<Result<Vec<_>, _>>()?;
    let durations: Vec<f64> = segments.iter().map(|s| s.duration).collect();

    // A transition cannot be longer than half the shortest segment, or two
    // neighbouring overlaps would eat the clip between them entirely.
    let shortest = durations.iter().cloned().fold(f64::INFINITY, f64::min);
    let t = if spec.transition == Transition::Cut || segments.len() < 2 {
        0.0
    } else {
        spec.transition_secs.clamp(0.0, shortest / 2.0)
    };
    // A zero-length xfade is a cut; route it through concat, which is cheaper.
    let transition = if t <= 0.0 { Transition::Cut } else { spec.transition };

    let (mut w, mut h) = (spec.width, spec.height);
    if w == 0 || h == 0 {
        w = sources[0].width;
        h = sources[0].height;
    }
    if w < 2 || h < 2 {
        return Err("Could not determine the output resolution".into());
    }
    let (w, h) = (even(w), even(h));
    let fps = spec.fps.clamp(1, 240);
    let total = total_duration(&durations, t);

    let output = if spec.output.trim().is_empty() {
        default_output(&segments[0].path)
    } else {
        spec.output.trim().to_string()
    };
    if segments.iter().any(|s| s.path == output) {
        return Err("The montage would overwrite one of its own clips".into());
    }

    let mut args: Vec<String> = ["-y", "-hide_banner", "-loglevel", "error", "-nostats"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    for s in &segments {
        // Input-side seek: fast, and the decoder starts at the in-point so
        // every filter below sees the segment starting at t=0.
        args.extend([
            "-ss".into(),
            secs(s.start),
            "-t".into(),
            secs(s.duration),
            "-i".into(),
            s.path.clone(),
        ]);
    }
    let music_input = segments.len();
    if let Some(m) = &spec.music {
        // Loop a short track so it always covers the whole reel; the
        // filtergraph trims it back to `total`.
        args.extend([
            "-stream_loop".into(),
            "-1".into(),
            "-i".into(),
            m.path.clone(),
        ]);
    }

    let mut g: Vec<String> = Vec::new();
    let afmt = "aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo";

    for (i, (seg, src)) in segments.iter().zip(sources).enumerate() {
        let d = secs(seg.duration);
        // ── video: normalise size, SAR, rate, pixel format and timebase so
        // xfade/concat accept every pair.
        let tail = format!("setsar=1,fps={fps},format=yuv420p,settb=AVTB,setpts=PTS-STARTPTS[v{i}]");
        match spec.fit {
            Fit::Letterbox => g.push(format!(
                "[{i}:v:0]scale={w}:{h}:force_original_aspect_ratio=decrease,\
                 pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black,{tail}"
            )),
            Fit::Blur => {
                g.push(format!("[{i}:v:0]split=2[bg{i}][fg{i}]"));
                g.push(format!(
                    "[bg{i}]scale={w}:{h}:force_original_aspect_ratio=increase,\
                     crop={w}:{h},boxblur=luma_radius=24:luma_power=2[bgb{i}]"
                ));
                g.push(format!(
                    "[fg{i}]scale={w}:{h}:force_original_aspect_ratio=decrease[fgs{i}]"
                ));
                g.push(format!(
                    "[bgb{i}][fgs{i}]overlay=(W-w)/2:(H-h)/2,{tail}"
                ));
            }
        }
        // ── audio: one stereo bed per clip, exactly `d` long. apad+atrim
        // pins the length even when a track ends early, which acrossfade and
        // concat both need to stay in sync with the video.
        let pin = format!("{afmt},apad,atrim=duration={d},asetpts=PTS-STARTPTS[a{i}]");
        match src.audio_streams {
            0 => g.push(format!("anullsrc=r=48000:cl=stereo,{pin}")),
            1 => g.push(format!("[{i}:a:0]{pin}")),
            n => {
                let ins: String = (0..n).map(|k| format!("[{i}:a:{k}]")).collect();
                // normalize=0: the tracks were separate sources of one live
                // mix, so sum them rather than dividing each by n.
                g.push(format!(
                    "{ins}amix=inputs={n}:duration=longest:normalize=0,{pin}"
                ));
            }
        }
    }

    // ── joins
    let n = segments.len();
    let (mut vlast, mut alast) = ("v0".to_string(), "a0".to_string());
    if n > 1 {
        match transition {
            Transition::Cut => {
                let ins: String = (0..n).map(|i| format!("[v{i}][a{i}]")).collect();
                g.push(format!("{ins}concat=n={n}:v=1:a=1[vj][aj]"));
                vlast = "vj".into();
                alast = "aj".into();
            }
            Transition::Crossfade | Transition::Fadeblack => {
                let kind = if transition == Transition::Fadeblack {
                    "fadeblack"
                } else {
                    "fade"
                };
                let mut offset = 0.0;
                for i in 1..n {
                    offset += durations[i - 1] - t;
                    let (vo, ao) = (format!("vx{i}"), format!("ax{i}"));
                    g.push(format!(
                        "[{vlast}][v{i}]xfade=transition={kind}:duration={}:offset={}[{vo}]",
                        secs(t),
                        secs(offset)
                    ));
                    g.push(format!("[{alast}][a{i}]acrossfade=d={}:c1=tri:c2=tri[{ao}]", secs(t)));
                    vlast = vo;
                    alast = ao;
                }
            }
        }
    }

    // ── whole-reel fade in/out
    let mut vchain = format!("[{vlast}]");
    let mut achain = format!("[{alast}]");
    let fade = (total / 4.0).min(0.5);
    if spec.fade_in_out && fade > 0.05 {
        let st = secs(total - fade);
        vchain.push_str(&format!(
            "fade=t=in:d={f},fade=t=out:st={st}:d={f}",
            f = secs(fade)
        ));
        achain.push_str(&format!(
            "afade=t=in:d={f},afade=t=out:st={st}:d={f}",
            f = secs(fade)
        ));
    } else {
        vchain.push_str("null");
        achain.push_str("anull");
    }
    g.push(format!("{vchain}[vout]"));

    // ── music bed
    match &spec.music {
        None => g.push(format!("{achain}[aout]")),
        Some(m) => {
            let vol = m.volume.clamp(0.0, 1.0);
            let mfade = (total / 4.0).min(2.0);
            g.push(format!(
                "[{music_input}:a:0]{afmt},volume={vol:.3},atrim=duration={},\
                 asetpts=PTS-STARTPTS,afade=t=out:st={}:d={}[mus]",
                secs(total),
                secs(total - mfade),
                secs(mfade)
            ));
            if m.duck {
                g.push(format!("{achain}[clipa]"));
                g.push("[clipa]asplit=2[clipmix][clipkey]".into());
                g.push(
                    "[mus][clipkey]sidechaincompress=threshold=0.04:ratio=8:\
                     attack=15:release=450:makeup=1[ducked]"
                        .into(),
                );
                g.push("[clipmix][ducked]amix=inputs=2:duration=first:normalize=0,alimiter=limit=0.95[aout]".into());
            } else {
                g.push(format!("{achain}[clipa]"));
                g.push("[clipa][mus]amix=inputs=2:duration=first:normalize=0,alimiter=limit=0.95[aout]".into());
            }
        }
    }

    let vcodec = match spec.codec.as_str() {
        "h265" | "libx265" => "libx265",
        _ => "libx264",
    };
    args.extend(["-filter_complex".into(), g.join(";")]);
    args.extend(
        [
            "-map", "[vout]", "-map", "[aout]",
            "-c:v", vcodec, "-preset", "fast", "-crf", "20", "-pix_fmt", "yuv420p",
            "-c:a", "aac", "-b:a", "192k",
            "-movflags", "+faststart",
            "-progress", "pipe:1",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    if vcodec == "libx265" {
        // Apple players refuse HEVC tagged hev1 in MP4.
        args.extend(["-tag:v".into(), "hvc1".into()]);
    }
    args.extend(["-t".into(), secs(total), output.clone()]);

    Ok(MontagePlan {
        segments,
        width: w,
        height: h,
        transition_secs: t,
        total_secs: total,
        output,
        args,
    })
}

/// Probe one clip for everything [`plan`] needs.
pub fn probe_source(path: &str) -> SourceInfo {
    let (duration, width, height) = crate::media::probe_video(Path::new(path));
    let audio_streams = crate::media::count_audio_streams(path);
    let trim = crate::clips::get_trim_state(path)
        .ok()
        .flatten()
        .filter(|t| t.trim_end > t.trim_start)
        .map(|t| (t.trim_start, t.trim_end));
    SourceInfo {
        duration,
        width,
        height,
        audio_streams,
        trim,
    }
}

/// Probe every clip and plan the run. Blocking (one ffprobe pair per clip).
pub fn prepare(spec: &MontageSpec) -> Result<MontagePlan, String> {
    let sources: Vec<SourceInfo> = spec.clips.iter().map(|c| probe_source(&c.path)).collect();
    plan(spec, &sources)
}

/// Pull the encoded position out of one `-progress` line, in seconds.
/// ffmpeg reports `out_time_us` (and, despite the name, `out_time_ms` is also
/// microseconds), so both are accepted.
pub fn parse_progress_line(line: &str) -> Option<f64> {
    let (key, val) = line.trim().split_once('=')?;
    match key {
        "out_time_us" | "out_time_ms" => val.parse::<f64>().ok().map(|us| us / 1_000_000.0),
        _ => None,
    }
}

/// Run a planned montage. Blocks until ffmpeg exits — call it from a worker
/// thread. `on_progress(percent)` fires as encoding advances; raising
/// `cancel` kills ffmpeg and removes the partial file.
pub fn run_plan<F: Fn(f64)>(
    plan: &MontagePlan,
    cancel: Arc<AtomicBool>,
    on_progress: F,
) -> Result<String, String> {
    if let Some(dir) = Path::new(&plan.output).parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
    }

    let mut child = command("ffmpeg")
        .args(&plan.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg: {e}"))?;

    // stdout carries -progress key=value lines; stderr only errors
    // (-loglevel error), kept for the failure message.
    let stdout = child.stdout.take().ok_or("ffmpeg: no stdout")?;
    let latest = Arc::new(Mutex::new(0.0_f64));
    let latest_w = Arc::clone(&latest);
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(t) = parse_progress_line(&line) {
                if let Ok(mut l) = latest_w.lock() {
                    *l = t;
                }
            }
        }
    });
    let mut stderr = child.stderr.take().ok_or("ffmpeg: no stderr")?;
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });

    let total = plan.total_secs.max(0.001);
    let mut last_pct = -1.0;
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            let _ = err_reader.join();
            let _ = std::fs::remove_file(&plan.output);
            return Err("Cancelled".into());
        }
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => {}
            Err(e) => return Err(format!("ffmpeg wait: {e}")),
        }
        let pos = latest.lock().map(|l| *l).unwrap_or(0.0);
        // Capped below 100 until ffmpeg has actually exited and the moov
        // atom (faststart) is written.
        let pct = (pos / total * 100.0).clamp(0.0, 99.0);
        if (pct - last_pct).abs() >= 0.5 {
            last_pct = pct;
            on_progress(pct);
        }
        std::thread::sleep(Duration::from_millis(150));
    };
    let _ = reader.join();
    let err = err_reader.join().unwrap_or_default();

    if status.success() {
        on_progress(100.0);
        Ok(plan.output.clone())
    } else {
        let _ = std::fs::remove_file(&plan.output);
        let tail: Vec<&str> = err.lines().rev().take(4).collect();
        let msg = tail.into_iter().rev().collect::<Vec<_>>().join(" | ");
        Err(if msg.is_empty() {
            "FFmpeg montage render failed".into()
        } else {
            format!("FFmpeg montage render failed: {msg}")
        })
    }
}

/// Probe, plan and render in one call.
pub fn build_montage<F: Fn(f64)>(
    spec: &MontageSpec,
    cancel: Arc<AtomicBool>,
    on_progress: F,
) -> Result<String, String> {
    let p = prepare(spec)?;
    run_plan(&p, cancel, on_progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(duration: f64, audio: u32) -> SourceInfo {
        SourceInfo {
            duration,
            width: 1920,
            height: 1080,
            audio_streams: audio,
            trim: None,
        }
    }

    fn clip(p: &str) -> MontageClip {
        MontageClip {
            path: p.into(),
            start: None,
            end: None,
        }
    }

    fn spec(clips: &[&str]) -> MontageSpec {
        MontageSpec {
            clips: clips.iter().map(|p| clip(p)).collect(),
            transition: Transition::Crossfade,
            transition_secs: 1.0,
            width: 0,
            height: 0,
            fit: Fit::Letterbox,
            fps: 60,
            fade_in_out: false,
            music: None,
            codec: String::new(),
            output: "/out/m.mp4".into(),
        }
    }

    fn graph(p: &MontagePlan) -> &str {
        let i = p.args.iter().position(|a| a == "-filter_complex").unwrap();
        &p.args[i + 1]
    }

    #[test]
    fn total_duration_subtracts_overlaps() {
        assert_eq!(total_duration(&[10.0, 10.0, 10.0], 1.0), 28.0);
        assert_eq!(total_duration(&[10.0], 1.0), 10.0);
        assert_eq!(total_duration(&[], 1.0), 0.0);
    }

    #[test]
    fn crossfade_offsets_accumulate() {
        let p = plan(&spec(&["/a.mp4", "/b.mp4", "/c.mp4"]), &[src(10.0, 1), src(8.0, 1), src(6.0, 1)]).unwrap();
        let g = graph(&p);
        assert!(g.contains("xfade=transition=fade:duration=1.000:offset=9.000[vx1]"), "{g}");
        assert!(g.contains("[vx1][v2]xfade=transition=fade:duration=1.000:offset=16.000[vx2]"), "{g}");
        assert!(g.contains("[ax1][a2]acrossfade=d=1.000"), "{g}");
        assert_eq!(p.total_secs, 22.0);
        assert_eq!(p.args.last().unwrap(), "/out/m.mp4");
    }

    #[test]
    fn cut_uses_concat_and_full_length() {
        let mut s = spec(&["/a.mp4", "/b.mp4"]);
        s.transition = Transition::Cut;
        let p = plan(&s, &[src(5.0, 1), src(5.0, 1)]).unwrap();
        assert!(graph(&p).contains("[v0][a0][v1][a1]concat=n=2:v=1:a=1[vj][aj]"));
        assert_eq!(p.total_secs, 10.0);
        assert_eq!(p.transition_secs, 0.0);
    }

    #[test]
    fn transition_clamped_to_half_shortest_segment() {
        let mut s = spec(&["/a.mp4", "/b.mp4"]);
        s.transition_secs = 5.0;
        let p = plan(&s, &[src(10.0, 1), src(2.0, 1)]).unwrap();
        assert_eq!(p.transition_secs, 1.0);
        assert_eq!(p.total_secs, 11.0);
    }

    #[test]
    fn saved_trim_is_honoured_and_explicit_window_wins() {
        let mut s = spec(&["/a.mp4", "/b.mp4"]);
        s.clips[1].start = Some(1.0);
        s.clips[1].end = Some(3.0);
        let mut a = src(30.0, 1);
        a.trim = Some((12.0, 20.0));
        let mut b = src(30.0, 1);
        b.trim = Some((5.0, 25.0));
        let p = plan(&s, &[a, b]).unwrap();
        assert_eq!(p.segments[0].start, 12.0);
        assert_eq!(p.segments[0].duration, 8.0);
        assert_eq!(p.segments[1].start, 1.0);
        assert_eq!(p.segments[1].duration, 2.0);
        let i = p.args.iter().position(|a| a == "/a.mp4").unwrap();
        assert_eq!(&p.args[i - 5..i], ["-ss", "12.000", "-t", "8.000", "-i"]);
    }

    #[test]
    fn audio_tracks_are_summed_or_synthesised() {
        let p = plan(&spec(&["/a.mp4", "/b.mp4", "/c.mp4"]), &[src(5.0, 3), src(5.0, 0), src(5.0, 1)]).unwrap();
        let g = graph(&p);
        assert!(g.contains("[0:a:0][0:a:1][0:a:2]amix=inputs=3:duration=longest:normalize=0"), "{g}");
        assert!(g.contains("anullsrc=r=48000:cl=stereo,"), "{g}");
        assert!(g.contains("atrim=duration=5.000"), "{g}");
        assert!(g.contains("[2:a:0]aformat="), "{g}");
    }

    #[test]
    fn music_is_looped_trimmed_and_ducked() {
        let mut s = spec(&["/a.mp4", "/b.mp4"]);
        s.music = Some(MusicBed {
            path: "/song.ogg".into(),
            volume: 0.5,
            duck: true,
        });
        let p = plan(&s, &[src(10.0, 1), src(10.0, 1)]).unwrap();
        let i = p.args.iter().position(|a| a == "/song.ogg").unwrap();
        assert_eq!(&p.args[i - 3..i], ["-stream_loop", "-1", "-i"]);
        let g = graph(&p);
        assert!(g.contains("[2:a:0]"), "{g}");
        assert!(g.contains("volume=0.500,atrim=duration=19.000"), "{g}");
        assert!(g.contains("sidechaincompress"), "{g}");

        s.music.as_mut().unwrap().duck = false;
        let p = plan(&s, &[src(10.0, 1), src(10.0, 1)]).unwrap();
        assert!(!graph(&p).contains("sidechaincompress"));
        assert!(graph(&p).contains("[clipa][mus]amix"));
    }

    #[test]
    fn vertical_blur_layout_and_even_sizes() {
        let mut s = spec(&["/a.mp4"]);
        s.width = 1081;
        s.height = 1921;
        s.fit = Fit::Blur;
        let p = plan(&s, &[src(5.0, 1)]).unwrap();
        assert_eq!((p.width, p.height), (1080, 1920));
        let g = graph(&p);
        assert!(g.contains("split=2[bg0][fg0]"), "{g}");
        assert!(g.contains("crop=1080:1920"), "{g}");
    }

    #[test]
    fn source_size_is_default_and_fades_are_applied() {
        let mut s = spec(&["/a.mp4"]);
        s.fade_in_out = true;
        let p = plan(&s, &[src(8.0, 1)]).unwrap();
        assert_eq!((p.width, p.height), (1920, 1080));
        let g = graph(&p);
        assert!(g.contains("fade=t=out:st=7.500:d=0.500"), "{g}");
        assert!(g.contains("afade=t=in:d=0.500"), "{g}");
    }

    #[test]
    fn rejects_bad_input() {
        assert!(plan(&spec(&[]), &[]).is_err());
        assert!(plan(&spec(&["/a.mp4"]), &[src(0.0, 1)]).is_err());
        let mut short = src(10.0, 1);
        short.trim = Some((5.0, 5.2));
        assert!(plan(&spec(&["/a.mp4"]), &[short]).is_err());
        let mut s = spec(&["/out/m.mp4"]);
        s.output = "/out/m.mp4".into();
        assert!(plan(&s, &[src(5.0, 1)]).is_err());
    }

    #[test]
    fn h265_gets_hvc1_tag() {
        let mut s = spec(&["/a.mp4"]);
        s.codec = "h265".into();
        let p = plan(&s, &[src(5.0, 1)]).unwrap();
        assert!(p.args.windows(2).any(|w| w == ["-c:v", "libx265"]));
        assert!(p.args.windows(2).any(|w| w == ["-tag:v", "hvc1"]));
    }

    #[test]
    fn spec_deserialises_with_defaults() {
        let s: MontageSpec =
            serde_json::from_str(r#"{"clips":[{"path":"/a.mp4"}],"transition":"fadeblack"}"#).unwrap();
        assert_eq!(s.transition, Transition::Fadeblack);
        assert_eq!(s.fps, 60);
        assert!(s.fade_in_out);
        assert_eq!(s.transition_secs, 0.6);
    }

    #[test]
    fn progress_lines_parse() {
        assert_eq!(parse_progress_line("out_time_us=2500000"), Some(2.5));
        assert_eq!(parse_progress_line("out_time_ms=1000000"), Some(1.0));
        assert_eq!(parse_progress_line("out_time_us=N/A"), None);
        assert_eq!(parse_progress_line("progress=continue"), None);
    }

    /// End-to-end render against real ffmpeg, skipped where it is absent.
    /// Covers what string checks cannot: that ffmpeg accepts the graph.
    #[test]
    fn renders_with_real_ffmpeg() {
        if !crate::subprocess::is_available("ffmpeg") || !crate::subprocess::is_available("ffprobe") {
            eprintln!("ffmpeg/ffprobe missing — skipping render test");
            return;
        }
        let dir = std::env::temp_dir().join(format!("opengg-montage-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mk = |name: &str, extra: &[&str]| {
            let p = dir.join(name);
            let mut a: Vec<&str> = vec!["-y", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=s=320x180:r=30:d=2"];
            a.extend_from_slice(extra);
            a.extend_from_slice(&["-c:v", "libx264", "-pix_fmt", "yuv420p", "-shortest"]);
            let ps = p.to_string_lossy().to_string();
            let mut cmd = command("ffmpeg");
            cmd.args(&a).arg(&ps);
            assert!(cmd.status().unwrap().success());
            ps
        };
        // Two-track clip, silent clip, one-track clip of a different size.
        let a = mk("a.mp4", &["-f", "lavfi", "-i", "sine=f=440:d=2", "-f", "lavfi", "-i", "sine=f=660:d=2", "-map", "0:v", "-map", "1:a", "-map", "2:a", "-c:a", "aac"]);
        let b = mk("b.mp4", &[]);
        let c = {
            let p = dir.join("c.mp4");
            let ps = p.to_string_lossy().to_string();
            assert!(command("ffmpeg")
                .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=s=200x200:r=25:d=2", "-f", "lavfi", "-i", "sine=d=2", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", &ps])
                .status()
                .unwrap()
                .success());
            ps
        };
        let music = {
            let p = dir.join("m.wav");
            let ps = p.to_string_lossy().to_string();
            assert!(command("ffmpeg")
                .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=f=220:d=1", &ps])
                .status()
                .unwrap()
                .success());
            ps
        };
        let out = dir.join("out.mp4").to_string_lossy().to_string();
        let spec = MontageSpec {
            clips: vec![clip(&a), clip(&b), clip(&c)],
            transition: Transition::Crossfade,
            transition_secs: 0.5,
            width: 320,
            height: 180,
            fit: Fit::Blur,
            fps: 30,
            fade_in_out: true,
            music: Some(MusicBed { path: music, volume: 0.4, duck: true }),
            codec: "h264".into(),
            output: out.clone(),
        };
        let sources: Vec<SourceInfo> = spec
            .clips
            .iter()
            .map(|c| {
                let (duration, width, height) = crate::media::probe_video(Path::new(&c.path));
                SourceInfo {
                    duration,
                    width,
                    height,
                    audio_streams: crate::media::count_audio_streams(&c.path),
                    trim: None,
                }
            })
            .collect();
        assert_eq!(sources[0].audio_streams, 2);
        assert_eq!(sources[1].audio_streams, 0);
        let p = plan(&spec, &sources).unwrap();
        let progress = Mutex::new(Vec::new());
        let res = run_plan(&p, Arc::new(AtomicBool::new(false)), |pct| progress.lock().unwrap().push(pct));
        assert_eq!(res, Ok(out.clone()));
        assert_eq!(progress.lock().unwrap().last().copied(), Some(100.0));
        let (dur, w, h) = crate::media::probe_video(Path::new(&out));
        assert_eq!((w, h), (320, 180));
        assert!((dur - p.total_secs).abs() < 0.2, "dur {dur} vs {}", p.total_secs);
        assert_eq!(crate::media::count_audio_streams(&out), 1);

        // Cancel before start: ffmpeg is killed and no partial file stays.
        let res = run_plan(&p, Arc::new(AtomicBool::new(true)), |_| {});
        assert_eq!(res, Err("Cancelled".into()));
        assert!(!Path::new(&out).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

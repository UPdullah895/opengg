//! The editor's export dialog, expressed as an edit.
//!
//! The current editor UI has one trim window plus per-track preview
//! volumes and mutes. [`SimpleEdit`] carries exactly that, plus a colour
//! look and a clip speed, and [`export_simple`] turns it into an
//! [`EditProject`] and renders it — so what the user hears in the editor is
//! what the file contains, instead of the mix silently being dropped at
//! export time.
//!
//! Size targets and non-H.264 codecs are handled by rendering the edit to a
//! high-quality intermediate first and then running the existing sized /
//! codec export on that, so every combination of options works.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::Deserialize;

use super::export::{export, Encoder, ExportOptions};
use super::looks::Look;
use super::project::EditProject;

/// Options from the export dialog. Missing fields mean "unchanged".
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct SimpleEdit {
    /// Look id (`"vivid"`, …); empty or unknown = none.
    pub look: String,
    /// Clip speed; 0 is read as 1.
    pub speed: f64,
    /// Apply `levels` / `muted` to the export.
    pub apply_mix: bool,
    /// Per-track gain, by audio-track index (0..=2).
    pub levels: Vec<f64>,
    /// Per-track mute, by audio-track index.
    pub muted: Vec<bool>,
    /// Add a pre-mixed "Mix" track first (players that only play the first
    /// track then hear everything).
    pub mix_track: bool,
}

impl SimpleEdit {
    fn look(&self) -> Look {
        Look::from_id(&self.look).unwrap_or(Look::None)
    }

    fn speed(&self) -> f64 {
        if self.speed > 0.0 {
            self.speed
        } else {
            1.0
        }
    }

    /// True when this changes anything a stream copy could not reproduce.
    pub fn is_active(&self) -> bool {
        self.look() != Look::None
            || (self.speed() - 1.0).abs() > 1e-9
            || self.mix_track
            || (self.apply_mix
                && (self.levels.iter().any(|v| (v - 1.0).abs() > 1e-9) || self.muted.iter().any(|m| *m)))
    }

    /// The project these options describe for a clip of `duration` seconds
    /// with `tracks` audio tracks, trimmed to `[start, end]`.
    pub fn project(&self, duration: f64, tracks: usize, start: f64, end: f64) -> EditProject {
        let mut p = EditProject::new(duration, tracks);
        if let Some(t) = p.set_trim(start, end) {
            p = t;
        }
        let all: Vec<usize> = (0..p.parts.len()).collect();
        if let Some(s) = p.set_part_speed(&all, self.speed()) {
            p = s;
        }
        if self.apply_mix {
            for (t, v) in self.levels.iter().enumerate().take(tracks) {
                if let Some(n) = p.set_track_level(t, *v) {
                    p = n;
                }
            }
            for (t, m) in self.muted.iter().enumerate().take(tracks) {
                if let Some(n) = p.set_track_muted(t, *m) {
                    p = n;
                }
            }
        }
        if let Some(n) = p.set_look(self.look(), 1.0) {
            p = n;
        }
        p
    }

    /// One-line description for the dialog's summary.
    pub fn describe(&self) -> String {
        let mut bits = Vec::new();
        if self.look() != Look::None {
            bits.push(format!("look {}", self.look().id()));
        }
        if (self.speed() - 1.0).abs() > 1e-9 {
            bits.push(format!("{}× speed", super::graph::num(self.speed())));
        }
        if self.apply_mix {
            bits.push("track mix applied".into());
        }
        if self.mix_track {
            bits.push("+ Mix track".into());
        }
        bits.join(" • ")
    }
}

/// Render `input` trimmed to `[start, end]` with `edit` applied to `out`.
///
/// `target_mb` > 0 and/or a `codec` other than H.264 run a second pass via
/// [`crate::media::export_clip_sized`] on an intermediate. `on_progress`
/// gets 0..=100 and a stage name, like the other editor exports.
#[allow(clippy::too_many_arguments)]
pub fn export_simple(
    input: &Path,
    start: f64,
    end: f64,
    edit: &SimpleEdit,
    target_mb: f64,
    codec: &str,
    out: &Path,
    cancel: &AtomicBool,
    on_progress: impl Fn(f64, &str),
) -> Result<PathBuf, String> {
    let fp = input.to_string_lossy().to_string();
    let duration = crate::media::probe_duration(&fp);
    if duration <= 0.0 {
        return Err("Could not read the clip's duration".into());
    }
    let tracks = crate::media::analyze_media_sync(&fp).map(|i| i.audio_streams as usize).unwrap_or(0);
    let project = edit.project(duration, tracks, start, end);

    let direct = target_mb <= 0.0 && matches!(codec, "h264" | "copy" | "");
    if direct {
        let opts = ExportOptions {
            output: out.to_path_buf(),
            encoder: Encoder::Auto,
            mix_track: edit.mix_track,
            audio_bitrate_kbps: 192,
        };
        return export(input, &project, &opts, cancel, |f| on_progress(f * 100.0, "encoding"));
    }

    // Two steps: a near-lossless intermediate in the output folder (hidden,
    // so the library ignores it), then the sized / codec export from it.
    let dir = out.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let tmp = dir.join(format!(".opengg-edit-{}.mp4", std::process::id()));
    let opts = ExportOptions {
        output: tmp.clone(),
        encoder: Encoder::X264 { crf: 14 },
        mix_track: edit.mix_track,
        audio_bitrate_kbps: 192,
    };
    export(input, &project, &opts, cancel, |f| on_progress(f * 50.0, "effects"))?;
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        let _ = std::fs::remove_file(&tmp);
        return Err("cancelled".into());
    }
    let out_s = out.to_string_lossy().to_string();
    let res = crate::media::export_clip_sized(
        &tmp.to_string_lossy(),
        0.0,
        project.output_duration(),
        target_mb,
        &out_s,
        if codec == "copy" { "h264" } else { codec },
        |pct, stage| on_progress(50.0 + pct / 2.0, stage),
    );
    let _ = std::fs::remove_file(&tmp);
    // The sized pass cannot be interrupted mid-run; a cancel that arrived
    // during it still must not leave a file the user walked away from.
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        if let Ok(p) = &res {
            let _ = std::fs::remove_file(p);
        }
        return Err("cancelled".into());
    }
    res.map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_by_default() {
        let e = SimpleEdit::default();
        assert!(!e.is_active());
        assert!(!SimpleEdit { apply_mix: true, levels: vec![1.0, 1.0], ..Default::default() }.is_active());
        assert!(SimpleEdit { look: "vivid".into(), ..Default::default() }.is_active());
        assert!(SimpleEdit { speed: 2.0, ..Default::default() }.is_active());
        assert!(SimpleEdit { apply_mix: true, muted: vec![false, true], ..Default::default() }.is_active());
        // Levels only count when the user chose to apply them.
        assert!(!SimpleEdit { levels: vec![0.5], ..Default::default() }.is_active());
    }

    #[test]
    fn project_from_options() {
        let e = SimpleEdit {
            look: "mono".into(),
            speed: 2.0,
            apply_mix: true,
            levels: vec![0.5, 1.0, 9.0],
            muted: vec![false, true],
            mix_track: true,
        };
        let p = e.project(30.0, 2, 10.0, 20.0);
        assert_eq!(p.parts.len(), 1);
        assert!((p.parts[0].start - 10.0).abs() < 1e-9 && (p.parts[0].end - 20.0).abs() < 1e-9);
        assert!((p.output_duration() - 5.0).abs() < 1e-9);
        assert_eq!(p.look, Look::Mono);
        assert!((p.track_levels[0] - 0.5).abs() < 1e-9);
        // Extra entries beyond the clip's tracks are ignored.
        assert_eq!(p.track_levels.len(), 2);
        assert_eq!(p.track_muted, vec![false, true]);
        assert_eq!(e.describe(), "look mono • 2× speed • track mix applied • + Mix track");
    }

    fn have_ffmpeg() -> bool {
        std::process::Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
    }

    #[test]
    fn exports_with_effects_and_size_target() {
        if !have_ffmpeg() {
            return;
        }
        let d = std::env::temp_dir().join(format!("opengg-simple-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let clip = d.join("in.mp4");
        let ok = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y",
                "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=30",
                "-f", "lavfi", "-i", "sine=frequency=300",
                "-f", "lavfi", "-i", "sine=frequency=600",
                "-map", "0", "-map", "1", "-map", "2", "-t", "4",
                "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"])
            .arg(&clip)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let cancel = AtomicBool::new(false);
        let edit = SimpleEdit { look: "vivid".into(), speed: 2.0, ..Default::default() };

        let direct = d.join("direct.mp4");
        export_simple(&clip, 0.0, 4.0, &edit, 0.0, "h264", &direct, &cancel, |_, _| {}).unwrap();
        let dur = crate::media::probe_duration(&direct.to_string_lossy());
        assert!((dur - 2.0).abs() < 0.2, "2× speed of 4 s should be ~2 s, got {dur}");

        let sized = d.join("sized.mp4");
        export_simple(&clip, 0.0, 4.0, &edit, 1.0, "h264", &sized, &cancel, |_, _| {}).unwrap();
        assert!(std::fs::metadata(&sized).unwrap().len() <= 1024 * 1024);
        assert_eq!(crate::media::count_audio_streams(&sized.to_string_lossy()), 2);
        // No intermediate or part files left behind.
        let leftovers: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        std::fs::remove_dir_all(&d).ok();
    }
}

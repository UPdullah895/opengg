//! EditorController — trim-only clip editor (plan §Phase 5, "trim-only first"
//! fallback). Backs `TrimEditor.qml`: probes a clip's duration, loads/saves
//! its persisted trim window (`opengg_core::clips::{get,save}_trim_state`),
//! and runs a lossless ffmpeg stream-copy export via
//! `opengg_core::media::trim_clip` on a background thread with progress
//! reported back through qproperties.
//!
//! Multi-track audio preview/mixing and the full `AdvancedEditor.vue` mode
//! are deliberately out of scope here — Qt Multimedia's GStreamer backend
//! only decodes a clip's first audio track (PoC-confirmed), so true
//! multi-track playback needs a hand-built GStreamer pipeline, not a QML
//! port. This controller only ever plays/exports the single embedded track.

#[cxx_qt::bridge]
pub mod qobject {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(f64, duration)]
        #[qproperty(f64, trim_start, cxx_name = "trimStart")]
        #[qproperty(f64, trim_end, cxx_name = "trimEnd")]
        #[qproperty(bool, export_running, cxx_name = "exportRunning")]
        #[qproperty(f64, export_progress, cxx_name = "exportProgress")]
        #[qproperty(QString, export_stage, cxx_name = "exportStage")]
        #[qproperty(QString, export_error, cxx_name = "exportError")]
        #[qproperty(QString, export_result, cxx_name = "exportResult")]
        /// Full ffprobe analysis of the loaded clip as JSON (duration, width,
        /// height, fps, video_codec, streams[], video_streams, audio_streams).
        /// Backs the editor's INFO panel and its per-audio-track timeline
        /// lanes; empty until the background probe started by `loadClip`
        /// finishes.
        #[qproperty(QString, media_info_json, cxx_name = "mediaInfoJson")]
        /// Path of the most recent `grabFrame` result (empty until one lands).
        #[qproperty(QString, screenshot_path, cxx_name = "screenshotPath")]
        type EditorController = super::EditorControllerRust;

        /// Probe the clip's duration and load its saved trim window (if any,
        /// else the full clip). Resets export state.
        #[qinvokable]
        #[cxx_name = "loadClip"]
        fn load_clip(self: Pin<&mut Self>, filepath: QString);

        /// Persist the current trim window for this clip.
        #[qinvokable]
        #[cxx_name = "saveTrim"]
        fn save_trim(self: Pin<&mut Self>, filepath: QString, start: f64, end: f64);

        /// Run the lossless trim export on a background thread.
        #[qinvokable]
        #[cxx_name = "exportTrim"]
        fn export_trim(self: Pin<&mut Self>, filepath: QString, start: f64, end: f64);

        /// Grab the frame at `time_sec` to the screenshot directory. Threaded
        /// (ffmpeg subprocess); the resulting path lands in `screenshotPath`.
        #[qinvokable]
        #[cxx_name = "grabFrame"]
        fn grab_frame(self: Pin<&mut Self>, filepath: QString, time_sec: f64);

        /// Set the clip's game tag, preserving its other metadata.
        #[qinvokable]
        #[cxx_name = "setGameTag"]
        fn set_game_tag(self: Pin<&mut Self>, filepath: QString, game: QString);

        /// Full export with the editor dialog's options. `target_mb` of 0 means
        /// "original size"; `codec` is one of h264/h265/vp9/av1/copy. A 0 target
        /// with `copy` is the lossless stream-copy path (every audio track
        /// preserved); anything else re-encodes.
        #[qinvokable]
        #[cxx_name = "exportClip"]
        fn export_clip(
            self: Pin<&mut Self>,
            filepath: QString,
            start: f64,
            end: f64,
            target_mb: f64,
            output_path: QString,
            codec: QString,
        );

        /// Default export directory (the first configured clip directory).
        #[qinvokable]
        #[cxx_name = "defaultExportDir"]
        fn default_export_dir(self: &Self) -> QString;

        /// Human-readable summary of what the chosen options will produce —
        /// backs the dialog's grey preview line.
        #[qinvokable]
        #[cxx_name = "exportSummary"]
        fn export_summary(
            self: &Self,
            duration: f64,
            target_mb: f64,
            codec: QString,
        ) -> QString;
    }

    impl cxx_qt::Threading for EditorController {}
}

use core::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct EditorControllerRust {
    duration: f64,
    trim_start: f64,
    trim_end: f64,
    export_running: bool,
    export_progress: f64,
    export_stage: QString,
    export_error: QString,
    export_result: QString,
    media_info_json: QString,
    screenshot_path: QString,
}

impl qobject::EditorController {
    pub fn load_clip(mut self: Pin<&mut Self>, filepath: QString) {
        let fp = filepath.to_string();
        let dur = opengg_core::media::probe_duration(&fp);
        self.as_mut().set_duration(dur);
        self.as_mut().set_export_running(false);
        self.as_mut().set_export_progress(0.0);
        self.as_mut().set_export_stage(QString::default());
        self.as_mut().set_export_error(QString::default());
        self.as_mut().set_export_result(QString::default());

        match opengg_core::clips::get_trim_state(&fp) {
            Ok(Some(t)) => {
                self.as_mut().set_trim_start(t.trim_start);
                self.as_mut().set_trim_end(t.trim_end.min(dur).max(t.trim_start));
            }
            _ => {
                self.as_mut().set_trim_start(0.0);
                self.as_mut().set_trim_end(dur);
            }
        }

        // Full analysis (codec/fps/streams) is a second ffprobe, so it runs off
        // the Qt thread — `duration` above already came from the cheap probe,
        // letting the timeline lay out before this lands.
        self.as_mut().set_media_info_json(QString::default());
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let json = opengg_core::media::analyze_media_sync(&fp)
                .ok()
                .and_then(|info| serde_json::to_string(&info).ok())
                .unwrap_or_default();
            let _ = qt_thread.queue(move |mut c| {
                c.as_mut().set_media_info_json(QString::from(&json));
            });
        });
    }

    pub fn save_trim(self: Pin<&mut Self>, filepath: QString, start: f64, end: f64) {
        if let Err(e) = opengg_core::clips::save_trim_state(&filepath.to_string(), start, end) {
            eprintln!("EditorController::save_trim: {e}");
        }
    }

    pub fn grab_frame(mut self: Pin<&mut Self>, filepath: QString, time_sec: f64) {
        self.as_mut().set_screenshot_path(QString::default());
        let fp = filepath.to_string();
        // Honour the configured screenshot directory, same as the recorder's
        // own hotkey grab; falls back to XDG Pictures inside core.
        let dir = opengg_core::settings::load_ui_settings()
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .and_then(|v| {
                v.get("settings")?
                    .get("screenshotDirs")?
                    .get(0)?
                    .as_str()
                    .map(str::to_string)
            })
            .unwrap_or_default();
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = opengg_core::media::take_screenshot_sync(
                &fp,
                time_sec,
                if dir.is_empty() { None } else { Some(dir.as_str()) },
            );
            let path = match result {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("EditorController::grab_frame: {e}");
                    String::new()
                }
            };
            let _ = qt_thread.queue(move |mut c| {
                c.as_mut().set_screenshot_path(QString::from(&path));
            });
        });
    }

    pub fn set_game_tag(self: Pin<&mut Self>, filepath: QString, game: QString) {
        let fp = filepath.to_string();
        // set_clip_meta writes `favorite` unconditionally, so it has to be
        // re-sent or tagging a clip would silently un-favourite it — the same
        // trap ClipsController::set_custom_name documents.
        let favorite = opengg_core::clips::get_clip_meta(&fp)
            .ok()
            .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
            .and_then(|v| v.get("favorite").and_then(|f| f.as_bool()));
        let update = opengg_core::clips::ClipMetaUpdate {
            filepath: fp,
            custom_name: None,
            favorite,
            game_tag: Some(game.to_string()),
            notes: None,
        };
        if let Err(e) = opengg_core::clips::set_clip_meta(update) {
            eprintln!("EditorController::set_game_tag: {e}");
        }
    }

    pub fn default_export_dir(&self) -> QString {
        let dir = opengg_core::settings::load_ui_settings()
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .and_then(|v| {
                v.get("settings")?
                    .get("clip_directories")?
                    .get(0)?
                    .as_str()
                    .map(str::to_string)
            })
            .unwrap_or_else(|| {
                opengg_core::paths::default_clips_dir()
                    .to_string_lossy()
                    .to_string()
            });
        // Settings store this with a leading ~; the dialog concatenates it
        // straight into an ffmpeg output path, which does no shell expansion.
        QString::from(&opengg_core::paths::shexp(&dir))
    }

    pub fn export_summary(&self, duration: f64, target_mb: f64, codec: QString) -> QString {
        let codec = codec.to_string();
        let enc = opengg_core::media::video_encoder_for(&codec);
        let res = serde_json::from_str::<serde_json::Value>(&self.media_info_json.to_string())
            .ok()
            .and_then(|v| {
                let w = v.get("width")?.as_u64()?;
                let h = v.get("height")?.as_u64()?;
                Some(format!("{w}x{h}"))
            })
            .unwrap_or_else(|| "—".into());

        let label = match enc {
            "libx265" => "H.265",
            "libvpx-vp9" => "VP9",
            "libsvtav1" => "AV1",
            "copy" => "H.264",
            _ => "H.264",
        };

        let text = if target_mb <= 0.0 && enc == "copy" {
            format!("Stream copy • {label} • {res}")
        } else if target_mb <= 0.0 {
            format!("Re-encode CRF 20 • {label} • {res}")
        } else if duration > 0.0 {
            let total_kbps = target_mb * 8192.0 / duration;
            let video_kbps = (total_kbps - 128.0).max(100.0) as u32;
            format!("2-pass • {label} • {res} • {video_kbps} kbps video + 128 kbps audio")
        } else {
            format!("2-pass • {label} • {res}")
        };
        QString::from(&text)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn export_clip(
        mut self: Pin<&mut Self>,
        filepath: QString,
        start: f64,
        end: f64,
        target_mb: f64,
        output_path: QString,
        codec: QString,
    ) {
        self.as_mut().set_export_running(true);
        self.as_mut().set_export_progress(0.0);
        self.as_mut().set_export_stage(QString::from("starting"));
        self.as_mut().set_export_error(QString::default());
        self.as_mut().set_export_result(QString::default());

        let fp = filepath.to_string();
        let out = output_path.to_string();
        let codec = codec.to_string();
        let qt_thread = self.qt_thread();
        let progress_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = opengg_core::media::export_clip_sized(
                &fp,
                start,
                end,
                target_mb,
                &out,
                &codec,
                move |pct, stage| {
                    let stage = stage.to_string();
                    let _ = progress_thread.queue(move |mut c| {
                        c.as_mut().set_export_progress(pct);
                        c.as_mut().set_export_stage(QString::from(&stage));
                    });
                },
            );
            let _ = qt_thread.queue(move |mut c| {
                c.as_mut().set_export_running(false);
                match result {
                    Ok(o) => c.as_mut().set_export_result(QString::from(&o)),
                    Err(e) => c.as_mut().set_export_error(QString::from(&e)),
                }
            });
        });
    }

    pub fn export_trim(mut self: Pin<&mut Self>, filepath: QString, start: f64, end: f64) {
        self.as_mut().set_export_running(true);
        self.as_mut().set_export_progress(0.0);
        self.as_mut().set_export_stage(QString::from("copying"));
        self.as_mut().set_export_error(QString::default());
        self.as_mut().set_export_result(QString::default());

        let fp = filepath.to_string();
        let qt_thread = self.qt_thread();
        let progress_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = opengg_core::media::trim_clip(&fp, start, end, "", move |pct, stage| {
                let stage = stage.to_string();
                let _ = progress_thread.queue(move |mut c| {
                    c.as_mut().set_export_progress(pct);
                    c.as_mut().set_export_stage(QString::from(&stage));
                });
            });
            let _ = qt_thread.queue(move |mut c| {
                c.as_mut().set_export_running(false);
                match result {
                    Ok(out) => c.as_mut().set_export_result(QString::from(&out)),
                    Err(e) => c.as_mut().set_export_error(QString::from(&e)),
                }
            });
        });
    }
}

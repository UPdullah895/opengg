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

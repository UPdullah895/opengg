//! MontageController — backs `MontageDialog.qml`. Thin glue over
//! `opengg_core::montage`: the dialog hands over its choices as one JSON
//! spec, the controller probes + plans + renders on a worker thread and
//! reports progress through qproperties, the same shape `EditorController`
//! uses for single-clip exports.

#[cxx_qt::bridge]
pub mod qobject {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, running)]
        #[qproperty(f64, progress)]
        /// "probing" while ffprobe runs, then "rendering".
        #[qproperty(QString, stage)]
        #[qproperty(QString, error)]
        /// Output path of the last successful render.
        #[qproperty(QString, result)]
        /// `[{path, duration, start, end, audioTracks}]` for the clips last
        /// passed to `probeClips` — the dialog's per-row lengths and the
        /// running total. Empty until the background probe lands.
        #[qproperty(QString, clips_json, cxx_name = "clipsJson")]
        /// Last file chosen by `pickMusic` (empty until one is picked).
        #[qproperty(QString, picked_music, cxx_name = "pickedMusic")]
        type MontageController = super::MontageControllerRust;

        /// Probe each clip's length, saved trim and audio track count off
        /// the Qt thread; the answer lands in `clipsJson`.
        #[qinvokable]
        #[cxx_name = "probeClips"]
        fn probe_clips(self: Pin<&mut Self>, filepaths: &QStringList);

        /// Render a montage from a JSON `MontageSpec` (see core::montage).
        #[qinvokable]
        fn build(self: Pin<&mut Self>, spec_json: &QString);

        /// Stop a running render; the partial file is removed.
        #[qinvokable]
        fn cancel(self: Pin<&mut Self>);

        /// Forget the last result/error so a reopened dialog starts clean.
        #[qinvokable]
        fn reset(self: Pin<&mut Self>);

        /// Open the desktop's file picker for a music track; the choice
        /// lands in `pickedMusic`. Returns false when no native picker exists
        /// and the dialog must rely on its editable path field instead.
        #[qinvokable]
        #[cxx_name = "pickMusic"]
        fn pick_music(self: Pin<&mut Self>) -> bool;

        /// Suggested output path for a montage of these clips.
        #[qinvokable]
        #[cxx_name = "defaultOutput"]
        fn default_output(self: &Self, first_clip: &QString) -> QString;
    }

    impl cxx_qt::Threading for MontageController {}
}

use core::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use opengg_core::montage;

#[derive(Default)]
pub struct MontageControllerRust {
    running: bool,
    progress: f64,
    stage: QString,
    error: QString,
    result: QString,
    clips_json: QString,
    picked_music: QString,
    cancel: Arc<AtomicBool>,
    /// Bumped per `probeClips` call so a slow probe of an older selection
    /// cannot overwrite the answer for the current one.
    probe_generation: u64,
}

impl qobject::MontageController {
    pub fn probe_clips(mut self: Pin<&mut Self>, filepaths: &QStringList) {
        let paths: Vec<String> = filepaths.iter().map(|p| p.to_string()).collect();
        let generation = {
            let mut r = self.as_mut().rust_mut();
            r.probe_generation += 1;
            r.probe_generation
        };
        self.as_mut().set_clips_json(QString::default());
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let rows: Vec<serde_json::Value> = paths
                .iter()
                .map(|p| {
                    let s = montage::probe_source(p);
                    let (start, end) = s.trim.unwrap_or((0.0, s.duration));
                    serde_json::json!({
                        "path": p,
                        "duration": s.duration,
                        "start": start,
                        "end": end.min(s.duration),
                        "width": s.width,
                        "height": s.height,
                        "audioTracks": s.audio_streams,
                    })
                })
                .collect();
            let json = serde_json::to_string(&rows).unwrap_or_default();
            let _ = qt_thread.queue(move |mut c| {
                if c.rust().probe_generation == generation {
                    c.as_mut().set_clips_json(QString::from(&json));
                }
            });
        });
    }

    pub fn build(mut self: Pin<&mut Self>, spec_json: &QString) {
        if *self.running() {
            return;
        }
        let spec: montage::MontageSpec = match serde_json::from_str(&spec_json.to_string()) {
            Ok(s) => s,
            Err(e) => {
                self.as_mut().set_error(QString::from(&format!("Invalid montage settings: {e}")));
                return;
            }
        };

        let cancel = Arc::new(AtomicBool::new(false));
        self.as_mut().rust_mut().cancel = Arc::clone(&cancel);
        self.as_mut().set_running(true);
        self.as_mut().set_progress(0.0);
        self.as_mut().set_stage(QString::from("probing"));
        self.as_mut().set_error(QString::default());
        self.as_mut().set_result(QString::default());

        let qt_thread = self.qt_thread();
        let progress_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = montage::prepare(&spec).and_then(|plan| {
                let stage_thread = progress_thread.clone();
                let _ = stage_thread.queue(|mut c| c.as_mut().set_stage(QString::from("rendering")));
                montage::run_plan(&plan, cancel, move |pct| {
                    let _ = progress_thread.queue(move |mut c| c.as_mut().set_progress(pct));
                })
            });
            let _ = qt_thread.queue(move |mut c| {
                c.as_mut().set_running(false);
                c.as_mut().set_stage(QString::default());
                match result {
                    Ok(out) => c.as_mut().set_result(QString::from(&out)),
                    Err(e) => c.as_mut().set_error(QString::from(&e)),
                }
            });
        });
    }

    pub fn cancel(self: Pin<&mut Self>) {
        self.rust().cancel.store(true, Ordering::Relaxed);
    }

    pub fn reset(mut self: Pin<&mut Self>) {
        if *self.running() {
            return;
        }
        self.as_mut().set_progress(0.0);
        self.as_mut().set_error(QString::default());
        self.as_mut().set_result(QString::default());
    }

    pub fn pick_music(self: Pin<&mut Self>) -> bool {
        if opengg_core::dialogs::native_picker().is_none() {
            return false;
        }
        let home = std::env::var("HOME").unwrap_or_default();
        let music = format!("{home}/Music");
        let start = if std::path::Path::new(&music).is_dir() { music } else { home };
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let picked = opengg_core::dialogs::pick_file(
                "Choose montage music",
                &start,
                "Audio",
                &["*.mp3", "*.ogg", "*.opus", "*.flac", "*.wav", "*.m4a", "*.aac"],
            );
            match picked {
                Ok(Some(path)) => {
                    let _ = qt_thread.queue(move |mut c| c.as_mut().set_picked_music(QString::from(&path)));
                }
                Ok(None) => {}
                Err(e) => eprintln!("MontageController::pick_music: {e}"),
            }
        });
        true
    }

    pub fn default_output(&self, first_clip: &QString) -> QString {
        QString::from(&montage::default_output(&first_clip.to_string()))
    }
}

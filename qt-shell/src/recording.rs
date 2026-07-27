//! RecordingController — a QML singleton exposing GSR (gpu-screen-recorder)
//! replay-buffer control from `opengg_core::gsr`.
//!
//! Boundary (plan §2.2/§2.3): all subprocess management lives in
//! `opengg_core::gsr`; this is presentation glue that reads the same
//! `~/.config/opengg/ui-settings.json` the Tauri frontend already writes
//! (via `opengg_core::settings::load_ui_settings`) to build GSR spawn params,
//! mirroring `gsrParams()` in `frontend/src/components/RecordingDropdown.vue`
//! field-for-field so both UIs agree on what "start" means.

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
        #[qproperty(bool, running)]
        #[qproperty(QString, status_text, cxx_name = "statusText")]
        #[qproperty(QString, error)]
        type RecordingController = super::RecordingControllerRust;

        /// Poll the current GSR process state (also clears a stale crash).
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>);

        /// Start the replay buffer using the shared ui-settings.json GSR config.
        #[qinvokable]
        fn start(self: Pin<&mut Self>);

        /// Stop the replay buffer.
        #[qinvokable]
        fn stop(self: Pin<&mut Self>);

        /// Flush the replay buffer to a saved clip.
        #[qinvokable]
        fn save(self: Pin<&mut Self>);
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct RecordingControllerRust {
    running: bool,
    status_text: QString,
    error: QString,
}

struct GsrParams {
    output_dir: String,
    replay_secs: u32,
    fps: u32,
    quality: String,
    bitrate_kbps: Option<u32>,
    monitor_target: String,
    audio_sources: Vec<String>,
}

/// Build GSR spawn params from the shared settings file, with the same
/// fallbacks as `gsrParams()` in RecordingDropdown.vue.
fn load_params() -> GsrParams {
    let raw = opengg_core::settings::load_ui_settings().unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    let s = &v["settings"];

    let output_dir = s["clip_directories"][0]
        .as_str()
        .filter(|d| !d.is_empty())
        .unwrap_or("~/Videos/OpenGG")
        .to_string();
    let replay_secs = s["gsrReplaySecs"].as_u64().unwrap_or(30) as u32;
    let fps = s["gsrFps"].as_u64().unwrap_or(60) as u32;
    let quality = s["gsrQuality"].as_str().unwrap_or("cbr").to_string();
    let bitrate_kbps = if quality == "cbr" {
        s["gsrCbrBitrate"].as_u64().map(|b| b as u32)
    } else {
        None
    };
    let monitor_target = s["gsrMonitorTarget"]
        .as_str()
        .filter(|t| !t.is_empty())
        .unwrap_or("screen")
        .to_string();
    let audio_sources = s["captureTracks"]
        .as_array()
        .map(|tracks| {
            tracks
                .iter()
                .filter_map(|t| t["source"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    GsrParams {
        output_dir,
        replay_secs,
        fps,
        quality,
        bitrate_kbps,
        monitor_target,
        audio_sources,
    }
}

impl qobject::RecordingController {
    fn apply_status(mut self: Pin<&mut Self>) {
        match opengg_core::gsr::is_gsr_running() {
            Ok(running) => {
                self.as_mut().set_running(running);
                let secs = if running {
                    opengg_core::gsr::get_recorder_status()
                } else {
                    "idle".to_string()
                };
                let text = if let Some(s) = secs.strip_prefix("replay:") {
                    format!("Recording ({s}s buffer)")
                } else {
                    "Idle".to_string()
                };
                self.as_mut().set_status_text(QString::from(&text));
                self.as_mut().set_error(QString::default());
            }
            Err(crash) => {
                self.as_mut().set_running(false);
                self.as_mut().set_status_text(QString::from("Crashed"));
                self.as_mut().set_error(QString::from(&crash.stderr_tail));
            }
        }
    }

    pub fn refresh(self: Pin<&mut Self>) {
        self.apply_status();
    }

    pub fn start(mut self: Pin<&mut Self>) {
        let p = load_params();
        if let Err(e) = opengg_core::gsr::start_gsr_replay(
            p.output_dir,
            p.replay_secs,
            p.fps,
            p.quality,
            p.bitrate_kbps,
            p.monitor_target,
            p.audio_sources,
        ) {
            eprintln!("RecordingController::start: {e}");
            self.as_mut().set_error(QString::from(&e));
        }
        self.apply_status();
    }

    pub fn stop(mut self: Pin<&mut Self>) {
        if let Err(e) = opengg_core::gsr::stop_gsr_replay() {
            eprintln!("RecordingController::stop: {e}");
            self.as_mut().set_error(QString::from(&e));
        }
        self.apply_status();
    }

    pub fn save(mut self: Pin<&mut Self>) {
        match opengg_core::gsr::save_gsr_replay(false) {
            Ok(_) => self.as_mut().set_error(QString::default()),
            Err(e) => {
                eprintln!("RecordingController::save: {e}");
                self.as_mut().set_error(QString::from(&e));
            }
        }
        self.apply_status();
    }
}

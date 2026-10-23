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
        #[qproperty(QString, monitors_json, cxx_name = "monitorsJson")]
        #[qproperty(QString, audio_sinks_json, cxx_name = "audioSinksJson")]
        #[qproperty(QString, capture_sources_json, cxx_name = "captureSourcesJson")]
        #[qproperty(QString, session_type, cxx_name = "sessionType")]
        #[qproperty(QString, diagnostics_json, cxx_name = "diagnosticsJson")]
        #[qproperty(bool, diagnostics_running, cxx_name = "diagnosticsRunning")]
        /// True while a replay save is being written (runs off the UI thread).
        #[qproperty(bool, saving)]
        /// Outcome of the latest save as JSON: `{ok, reason, message,
        /// game, filename, filepath, sizeMb, lowDiskFreeBytes, seq}`.
        /// `reason` is a stable id ("off", "starting", "busy", "timeout",
        /// "empty", "diskFull", "error") for a translated message; `seq`
        /// increments every save so QML sees repeats of the same outcome.
        #[qproperty(QString, last_save_json, cxx_name = "lastSaveJson")]
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

        /// Save for an auto-clip moment (`HotkeyController.lastMoment`) and
        /// name the clip after it. A buffer that is off or starting is not
        /// an error here: auto-clip fires whether or not the user is
        /// recording, so that case stays silent.
        #[qinvokable]
        #[cxx_name = "saveMoment"]
        fn save_moment(self: Pin<&mut Self>, moment_json: &QString);

        /// Honour `gsrAutoStart` at app launch: start the buffer if the
        /// recorder is enabled, waiting 15 s first when the machine booted
        /// less than two minutes ago (GPU stack and portals still settling).
        #[qinvokable]
        #[cxx_name = "autoStart"]
        fn auto_start(self: Pin<&mut Self>);

        /// Periodic health check (QML ticks it every few seconds): restarts a
        /// crashed recorder or one left stale by a suspend, per
        /// `gsrAutoRestart`, within a retry budget.
        /// `auto_restart` is the `gsrAutoRestart` setting, passed from
        /// QML's cached settings so the tick never touches the disk.
        #[qinvokable]
        fn supervise(self: Pin<&mut Self>, auto_restart: bool);

        /// Take a desktop screenshot (Screenshot hotkey) into the first
        /// screenshot directory, named after the game in front. Threaded.
        #[qinvokable]
        fn screenshot(self: Pin<&mut Self>);

        /// Stop (if running) and restart the replay buffer with the current
        /// ui-settings.json GSR config — called after a GSR setting changes.
        #[qinvokable]
        fn restart(self: Pin<&mut Self>);

        /// Re-enumerate monitors/audio sinks/capture sources/session type for
        /// the Capture & Sound settings panel's live dropdowns.
        #[qinvokable]
        #[cxx_name = "refreshDevices"]
        fn refresh_devices(self: Pin<&mut Self>);

        /// Run the GSR pre-flight diagnostic suite (spawns a real ~1.3s test
        /// capture) on a background thread, then publish `diagnosticsJson`.
        #[qinvokable]
        #[cxx_name = "runDiagnostics"]
        fn run_diagnostics(self: Pin<&mut Self>, audio_sources_json: QString, monitor_target: QString);
    }

    impl cxx_qt::Threading for RecordingController {}
}

use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct RecordingControllerRust {
    running: bool,
    status_text: QString,
    error: QString,
    monitors_json: QString,
    audio_sinks_json: QString,
    capture_sources_json: QString,
    session_type: QString,
    diagnostics_json: QString,
    diagnostics_running: bool,
    saving: bool,
    last_save_json: QString,
    save_seq: u64,
    /// The user wants the buffer on (they or auto-start started it, and
    /// nobody stopped it) — what the supervisor restores after a crash.
    wanted: bool,
    supervisor: opengg_core::supervisor::Supervisor,
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
        {
            let mut r = self.as_mut().rust_mut();
            r.wanted = true;
            r.supervisor.reset();
        }
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
        self.as_mut().rust_mut().wanted = false;
        if let Err(e) = opengg_core::gsr::stop_gsr_replay() {
            eprintln!("RecordingController::stop: {e}");
            self.as_mut().set_error(QString::from(&e));
        }
        self.apply_status();
    }

    pub fn save(self: Pin<&mut Self>) {
        self.save_with(None);
    }

    pub fn save_moment(self: Pin<&mut Self>, moment_json: &QString) {
        self.save_with(Some(moment_json.to_string()));
    }

    fn save_with(mut self: Pin<&mut Self>, moment: Option<String>) {
        // A save waits for gpu-screen-recorder to finish writing (seconds on
        // a long buffer) and remuxes the file, so it must not run on the UI
        // thread. The core gate refuses overlapping saves on its own; this
        // flag only stops QML from queueing threads while one is running.
        if *self.saving() {
            return;
        }
        self.as_mut().set_saving(true);
        let opts = load_save_options();
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let outcome = opengg_core::gsr::save_gsr_replay(false, &opts);
            // Name it before anything shows it, so the library and the toast
            // both read the moment's name rather than the timestamp.
            let name = match (&outcome, &moment) {
                (Ok(r), Some(m)) => opengg_core::autoclip::tag_clip(&r.filepath, m)
                    .map_err(|e| eprintln!("RecordingController::save_moment: {e}"))
                    .ok(),
                _ => None,
            };
            let json = match &outcome {
                Ok(r) => serde_json::json!({
                    "ok": true,
                    "reason": "",
                    "message": "",
                    "game": r.game_title,
                    "filename": r.filename,
                    "name": name.clone().unwrap_or_default(),
                    "moment": moment.is_some(),
                    "filepath": r.filepath,
                    "sizeMb": r.filesize_mb,
                    "lowDiskFreeBytes": r.low_disk_free_bytes,
                }),
                Err(f) => serde_json::json!({
                    "ok": false,
                    "reason": f.id(),
                    "message": f.to_string(),
                }),
            };
            // Own thread: the low-disk toast waits for the "saved" one, and
            // that wait must not hold up `saving` going false.
            let quiet = moment.is_some()
                && matches!(&outcome, Err(opengg_core::gsr::SaveFailure::Refused(_)));
            if !quiet {
                std::thread::spawn(move || notify_save_outcome(&outcome, name.as_deref()));
            }
            let _ = qt_thread.queue(move |mut c| {
                if quiet {
                    c.as_mut().set_saving(false);
                    return;
                }
                let seq = c.as_ref().rust().save_seq + 1;
                c.as_mut().rust_mut().save_seq = seq;
                let mut json = json;
                json["seq"] = serde_json::json!(seq);
                let failed = !json["ok"].as_bool().unwrap_or(false);
                c.as_mut().set_last_save_json(QString::from(&json.to_string()));
                if failed {
                    let msg = json["message"].as_str().unwrap_or("").to_string();
                    c.as_mut().set_error(QString::from(&msg));
                } else {
                    c.as_mut().set_error(QString::default());
                }
                c.as_mut().set_saving(false);
                c.apply_status();
            });
        });
    }

    pub fn auto_start(self: Pin<&mut Self>) {
        let raw = opengg_core::settings::load_ui_settings().unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
        let s = &v["settings"];
        let enabled = s["gsrEnabled"].as_bool().unwrap_or(false);
        let auto = s["gsrAutoStart"].as_bool().unwrap_or(false);
        if !enabled || !auto || *self.running() {
            return;
        }
        let delay = opengg_core::supervisor::uptime()
            .map(opengg_core::supervisor::auto_start_delay)
            .unwrap_or_default();
        if delay.is_zero() {
            self.start();
            return;
        }
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let _ = qt_thread.queue(|mut c| {
                if !*c.running() {
                    c.as_mut().start();
                }
            });
        });
    }

    pub fn supervise(mut self: Pin<&mut Self>, auto_restart: bool) {
        use opengg_core::supervisor::{Action, Observation};
        // `is_gsr_running` hands over a crash exactly once; keep its stderr
        // tail so the reason still reaches the UI when no restart follows.
        let (alive, crash_tail) = match opengg_core::gsr::is_gsr_running() {
            Ok(a) => (a, None),
            Err(c) => (false, Some(c.stderr_tail)),
        };
        if let Some(tail) = &crash_tail {
            self.as_mut().set_error(QString::from(tail));
        }
        let obs = Observation {
            wanted: self.as_ref().rust().wanted,
            alive,
            auto_restart,
            suspended_total: opengg_core::supervisor::suspended_total(),
        };
        let action = self.as_mut().rust_mut().supervisor.tick(std::time::Instant::now(), obs);
        match action {
            Action::Nothing => {
                // Keep the running flag honest when the process died
                // without anyone asking (no auto-restart configured).
                if *self.running() != alive {
                    if alive {
                        self.apply_status();
                    } else {
                        self.as_mut().set_running(false);
                        let text = if crash_tail.is_some() { "Crashed" } else { "Idle" };
                        self.as_mut().set_status_text(QString::from(text));
                    }
                }
            }
            Action::Restart => {
                eprintln!("RecordingController: supervisor restarting gpu-screen-recorder");
                let p = load_params();
                if let Err(e) = opengg_core::gsr::restart_gsr_replay(
                    p.output_dir,
                    p.replay_secs,
                    p.fps,
                    p.quality,
                    p.bitrate_kbps,
                    p.monitor_target,
                    p.audio_sources,
                ) {
                    self.as_mut().set_error(QString::from(&e));
                }
                self.apply_status();
            }
            Action::GiveUp => {
                let msg = "The recorder keeps stopping — automatic restarts are paused. Check Capture & Sound diagnostics.";
                opengg_core::notify::show_warning("Replay buffer stopped", msg);
                self.as_mut().set_error(QString::from(msg));
                self.apply_status();
            }
        }
    }

    pub fn screenshot(self: Pin<&mut Self>) {
        let raw = opengg_core::settings::load_ui_settings().unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
        let s = &v["settings"];
        let dir = s["screenshotDirs"][0]
            .as_str()
            .or_else(|| s["clip_directories"][0].as_str())
            .filter(|d| !d.is_empty())
            .unwrap_or("~/Videos/OpenGG")
            .to_string();
        let per_game = s["clipsPerGameFolder"].as_bool().unwrap_or(false);
        let notify = s["enableClipNotifications"].as_bool().unwrap_or(true);
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let game = opengg_core::foreground::current_game_name();
            let root = std::path::PathBuf::from(opengg_core::paths::shexp(&dir));
            let res = opengg_core::desktop_shot::take(&root, &game, per_game);
            match &res {
                Ok(p) if notify => opengg_core::notify::show_info(
                    "Screenshot saved",
                    &p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                ),
                Ok(_) => {}
                Err(e) => opengg_core::notify::show_warning("Screenshot not saved", e),
            }
            if let Err(e) = res {
                let _ = qt_thread.queue(move |mut c| c.as_mut().set_error(QString::from(&e)));
            }
        });
    }

    pub fn restart(mut self: Pin<&mut Self>) {
        {
            let mut r = self.as_mut().rust_mut();
            r.wanted = true;
            r.supervisor.reset();
        }
        let p = load_params();
        if let Err(e) = opengg_core::gsr::restart_gsr_replay(
            p.output_dir,
            p.replay_secs,
            p.fps,
            p.quality,
            p.bitrate_kbps,
            p.monitor_target,
            p.audio_sources,
        ) {
            eprintln!("RecordingController::restart: {e}");
            self.as_mut().set_error(QString::from(&e));
        }
        self.apply_status();
    }

    pub fn refresh_devices(mut self: Pin<&mut Self>) {
        let monitors = opengg_core::gsr::list_monitors();
        let sinks = opengg_core::audio::list_audio_sinks().unwrap_or_default();
        let sources = opengg_core::audio::list_capture_sources().unwrap_or_default();
        let session = opengg_core::audio::get_session_type();

        self.as_mut().set_monitors_json(QString::from(
            &serde_json::to_string(&monitors).unwrap_or_else(|_| "[]".into()),
        ));
        self.as_mut().set_audio_sinks_json(QString::from(
            &serde_json::to_string(&sinks).unwrap_or_else(|_| "[]".into()),
        ));
        self.as_mut().set_capture_sources_json(QString::from(
            &serde_json::to_string(&sources).unwrap_or_else(|_| "[]".into()),
        ));
        self.as_mut().set_session_type(QString::from(&session));
    }

    pub fn run_diagnostics(
        mut self: Pin<&mut Self>,
        audio_sources_json: QString,
        monitor_target: QString,
    ) {
        self.as_mut().set_diagnostics_running(true);
        let audio_sources: Vec<String> =
            serde_json::from_str(&audio_sources_json.to_string()).unwrap_or_default();
        let monitor_target = monitor_target.to_string();
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = opengg_core::gsr::gsr_diagnostics(audio_sources, monitor_target);
            let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".into());
            let _ = qt_thread.queue(move |mut controller| {
                controller.as_mut().set_diagnostics_json(QString::from(&json));
                controller.as_mut().set_diagnostics_running(false);
            });
        });
    }
}

/// Filing rules for saved clips, from the Storage settings panel.
fn load_save_options() -> opengg_core::gsr::SaveOptions {
    let raw = opengg_core::settings::load_ui_settings().unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    let s = &v["settings"];
    opengg_core::gsr::SaveOptions {
        per_game_folder: s["clipsPerGameFolder"].as_bool().unwrap_or(false),
        max_folder_gb: s["maxClipFolderGb"].as_u64().unwrap_or(0),
    }
}

/// Desktop notification for a save, success or not. A refused or failed
/// save used to be silent — the user only found out when the clip was
/// missing — so failures always notify (they are rare and actionable),
/// even with clip notifications turned off. A "busy" refusal stays quiet:
/// the save already in flight will report.
/// `name` is the auto-clip name, shown in place of the file name.
fn notify_save_outcome(
    outcome: &Result<opengg_core::gsr::SaveReplayResult, opengg_core::gsr::SaveFailure>,
    name: Option<&str>,
) {
    let raw = opengg_core::settings::load_ui_settings().unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    let s = &v["settings"];

    let enabled = s["enableClipNotifications"].as_bool().unwrap_or(true);
    let mode = s["notificationStyle"].as_str().unwrap_or("auto");
    let duration_secs = s["notificationDuration"].as_u64();

    match outcome {
        Ok(r) => {
            opengg_core::notify::show_clip_notification(
                &r.game_title,
                name.unwrap_or(&r.filename),
                r.filesize_mb,
                true,
                enabled,
                mode,
                duration_secs,
            );
            if let Some(free) = r.low_disk_free_bytes {
                // After the "saved" toast, so both get read.
                std::thread::sleep(std::time::Duration::from_millis(3500));
                opengg_core::notify::show_warning(
                    "Low disk space",
                    &format!("{:.1} GB left for clips", free as f64 / 1e9),
                );
            }
        }
        Err(f) if f.id() == "busy" => {}
        Err(f) => opengg_core::notify::show_warning("Clip not saved", &f.to_string()),
    }
}

//! SystemController — thin QML-invokable facade over `opengg_core::system`
//! (XDG autostart entry, crash-log folder, clipboard, and the About panel's
//! dependency/distro/device-access probes). Probe results are exposed as
//! JSON-string qproperties (`depsJson`/`distroJson`/`accessJson`), mirroring
//! `SettingsController.settingsJson` — QML calls `refresh()` once then
//! `JSON.parse(...)` each property rather than getting bespoke typed fields
//! per probe.

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
        #[qproperty(QString, deps_json, cxx_name = "depsJson")]
        #[qproperty(QString, distro_json, cxx_name = "distroJson")]
        #[qproperty(QString, access_json, cxx_name = "accessJson")]
        /// Outcome of the last `copyDiagnostics`: "" while idle, "working",
        /// "copied", or an error message.
        #[qproperty(QString, diagnostics_status, cxx_name = "diagnosticsStatus")]
        type SystemController = super::SystemControllerRust;

        /// True if the XDG autostart `.desktop` entry exists.
        #[qinvokable]
        #[cxx_name = "getAutostart"]
        fn get_autostart(self: &Self) -> bool;

        /// Create/remove the XDG autostart entry.
        #[qinvokable]
        #[cxx_name = "setAutostart"]
        fn set_autostart(self: &Self, enable: bool);

        /// Open the crash-log directory in the system file manager.
        #[qinvokable]
        #[cxx_name = "openCrashLogsFolder"]
        fn open_crash_logs_folder(self: &Self);

        /// Build the diagnostics report (versions, tools, access, daemon
        /// reachability, latest crash report, log tail) off the UI thread
        /// and put it on the clipboard for a bug report. Nothing is sent
        /// anywhere; the user pastes it where they choose.
        #[qinvokable]
        #[cxx_name = "copyDiagnostics"]
        fn copy_diagnostics(self: Pin<&mut Self>);

        /// Re-run the dependency/distro/device-access probes and refresh
        /// `depsJson`/`distroJson`/`accessJson`.
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>);

        /// Open the folder containing `path` in the file manager.
        #[qinvokable]
        #[cxx_name = "revealInFolder"]
        fn reveal_in_folder(self: &Self, path: &QString);

        /// Copy text to the clipboard.
        #[qinvokable]
        #[cxx_name = "writeClipboard"]
        fn write_clipboard(self: &Self, text: QString);

        /// The qt-shell crate's build-time version (Settings → About).
        #[qinvokable]
        #[cxx_name = "appVersion"]
        fn app_version(self: &Self) -> QString;
    }

    impl cxx_qt::Threading for SystemController {}
}

use core::pin::Pin;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct SystemControllerRust {
    deps_json: QString,
    distro_json: QString,
    access_json: QString,
    diagnostics_status: QString,
}

impl qobject::SystemController {
    pub fn get_autostart(&self) -> bool {
        opengg_core::system::get_autostart().unwrap_or(false)
    }

    pub fn set_autostart(&self, enable: bool) {
        if let Err(e) = opengg_core::system::set_autostart(enable) {
            eprintln!("SystemController::set_autostart: {e}");
        }
    }

    pub fn copy_diagnostics(mut self: Pin<&mut Self>) {
        use cxx_qt::Threading;
        self.as_mut().set_diagnostics_status(QString::from("working"));
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let report = opengg_core::diagnostics::report();
            let status = match opengg_core::system::write_clipboard(&report) {
                Ok(()) => "copied".to_string(),
                Err(e) => e,
            };
            let _ = qt_thread.queue(move |mut c| c.as_mut().set_diagnostics_status(QString::from(&status)));
        });
    }

    pub fn open_crash_logs_folder(&self) {
        if let Err(e) = opengg_core::system::open_crash_logs_folder() {
            eprintln!("SystemController::open_crash_logs_folder: {e}");
        }
    }

    pub fn refresh(mut self: Pin<&mut Self>) {
        let deps = opengg_core::system::get_dependency_status();
        let distro = opengg_core::system::get_distro_info();
        let access = opengg_core::system::get_device_access_status();

        let deps_str = serde_json::to_string(&deps).unwrap_or_else(|_| "[]".into());
        let distro_str = serde_json::to_string(&distro).unwrap_or_else(|_| "{}".into());
        let access_str = serde_json::to_string(&access).unwrap_or_else(|_| "{}".into());

        self.as_mut().set_deps_json(QString::from(&deps_str));
        self.as_mut().set_distro_json(QString::from(&distro_str));
        self.as_mut().set_access_json(QString::from(&access_str));
    }

    pub fn reveal_in_folder(&self, path: &QString) {
        if let Err(e) = opengg_core::system::reveal_in_folder(&path.to_string()) {
            eprintln!("revealInFolder: {e}");
        }
    }

    pub fn write_clipboard(&self, text: QString) {
        if let Err(e) = opengg_core::system::write_clipboard(&text.to_string()) {
            eprintln!("SystemController::write_clipboard: {e}");
        }
    }

    pub fn app_version(&self) -> QString {
        QString::from(env!("CARGO_PKG_VERSION"))
    }
}

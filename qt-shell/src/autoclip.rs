//! AutoClipController — backs the Auto-clip card in Settings → Capture &
//! Sound. Thin glue over `opengg_core::autoclip`: whether CS2 has our
//! integration cfg, installing or removing it, and the daemon listener's
//! status (is the game actually talking to us?). The moments themselves
//! arrive through `HotkeyController` and are saved by `RecordingController`.

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
        /// `{gameFound, installed, listening, port, error, lastPayloadAt,
        /// moments, lastMoment, now}`. `error` is the daemon's; `now` is
        /// this machine's clock, so QML can age `lastPayloadAt` without
        /// trusting its own `Date`. Empty until the first `refresh`.
        #[qproperty(QString, status_json, cxx_name = "statusJson")]
        /// True while an install or uninstall runs.
        #[qproperty(bool, busy)]
        /// The last install/uninstall failure, or empty.
        #[qproperty(QString, error)]
        type AutoClipController = super::AutoClipControllerRust;

        /// Re-read the install state and the daemon's status, off the Qt
        /// thread.
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>);

        /// Write the Game State Integration cfg into every CS2 install.
        #[qinvokable]
        #[cxx_name = "installCs2"]
        fn install_cs2(self: Pin<&mut Self>);

        /// Remove it again.
        #[qinvokable]
        #[cxx_name = "uninstallCs2"]
        fn uninstall_cs2(self: Pin<&mut Self>);
    }

    impl cxx_qt::Threading for AutoClipController {}
}

use core::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::QString;
use opengg_core::autoclip;

#[derive(Default)]
pub struct AutoClipControllerRust {
    status_json: QString,
    busy: bool,
    error: QString,
}

fn status() -> String {
    let install = autoclip::cs2_install_state();
    let mut v: serde_json::Value = match autoclip::daemon_status() {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| serde_json::json!({})),
        // No daemon: show it as not listening, with the reason.
        Err(e) => serde_json::json!({ "listening": false, "error": e }),
    };
    v["gameFound"] = install.game_found.into();
    v["installed"] = install.installed.into();
    v["now"] = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .into();
    v.to_string()
}

impl qobject::AutoClipController {
    pub fn refresh(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let json = status();
            let _ = qt_thread.queue(move |mut c| c.as_mut().set_status_json(QString::from(&json)));
        });
    }

    pub fn install_cs2(self: Pin<&mut Self>) {
        self.run(|| autoclip::install_cs2().map(|_| ()));
    }

    pub fn uninstall_cs2(self: Pin<&mut Self>) {
        self.run(autoclip::uninstall_cs2);
    }

    fn run(mut self: Pin<&mut Self>, f: impl FnOnce() -> Result<(), String> + Send + 'static) {
        if *self.busy() {
            return;
        }
        self.as_mut().set_busy(true);
        self.as_mut().set_error(QString::default());
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = f();
            let json = status();
            let _ = qt_thread.queue(move |mut c| {
                c.as_mut().set_busy(false);
                if let Err(e) = result {
                    c.as_mut().set_error(QString::from(&e));
                }
                c.as_mut().set_status_json(QString::from(&json));
            });
        });
    }
}

//! HotkeyController — global shortcuts, over `opengg_core::hotkeys`.
//!
//! The daemon does the evdev listening (it is the part that works without
//! window focus on both X11 and Wayland); this controller pushes the user's
//! bindings down to it and turns each press back into something QML can act
//! on. The actions themselves run in this process because the
//! `gpu-screen-recorder` child does — see `core/src/hotkeys.rs`.

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
        /// The most recent action name — "saveReplay", "toggleRecording" or
        /// "screenshot", matching the keys in `ui-settings.json`'s
        /// `shortcuts`.
        #[qproperty(QString, last_action, cxx_name = "lastAction")]
        /// Bumped on every press. QML dispatches on *this* rather than on
        /// `lastAction`, because pressing the same hotkey twice leaves
        /// `lastAction` unchanged and would emit no change signal.
        #[qproperty(i32, press_count, cxx_name = "pressCount")]
        /// Whether the daemon accepted our bindings. False means the keys
        /// will not fire, and `error` says why.
        #[qproperty(bool, active)]
        #[qproperty(QString, error)]
        type HotkeyController = super::HotkeyControllerRust;

        /// Begin receiving presses. Idempotent — a second call is ignored,
        /// so QML may call it from `Component.onCompleted` without guarding.
        #[qinvokable]
        fn start(self: Pin<&mut Self>);

        /// Send the current bindings to the daemon's listener. Call whenever
        /// Settings → Shortcuts changes; without it the daemon keeps using
        /// whatever is in its own `daemon.toml`.
        #[qinvokable]
        #[cxx_name = "pushBindings"]
        fn push_bindings(
            self: Pin<&mut Self>,
            save_replay: &QString,
            toggle_recording: &QString,
            screenshot: &QString,
        );
    }

    impl cxx_qt::Threading for HotkeyController {}
}

use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt::Threading;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct HotkeyControllerRust {
    last_action: QString,
    press_count: i32,
    active: bool,
    error: QString,
    listening: bool,
}

impl qobject::HotkeyController {
    pub fn start(mut self: Pin<&mut Self>) {
        if self.as_ref().rust().listening {
            return;
        }
        self.as_mut().rust_mut().listening = true;

        let qt_thread = self.qt_thread();
        std::thread::Builder::new()
            .name("hotkey-signals".into())
            .spawn(move || {
                let result = opengg_core::hotkeys::listen(|action| {
                    let _ = qt_thread.queue(move |mut c| {
                        let n = *c.as_ref().press_count() + 1;
                        c.as_mut().set_last_action(QString::from(&action));
                        c.as_mut().set_press_count(n);
                    });
                });
                if let Err(e) = result {
                    eprintln!("HotkeyController: signal listener stopped — {e}");
                }
            })
            .ok();
    }

    pub fn push_bindings(
        mut self: Pin<&mut Self>,
        save_replay: &QString,
        toggle_recording: &QString,
        screenshot: &QString,
    ) {
        let (s, t, p) = (
            save_replay.to_string(),
            toggle_recording.to_string(),
            screenshot.to_string(),
        );
        // The daemon auto-activates, so this can block briefly on first call.
        let qt_thread = self.as_mut().qt_thread();
        std::thread::spawn(move || {
            let outcome = opengg_core::hotkeys::push_bindings(&s, &t, &p);
            let _ = qt_thread.queue(move |mut c| match outcome {
                Ok(()) => {
                    c.as_mut().set_active(true);
                    c.as_mut().set_error(QString::default());
                }
                Err(e) => {
                    eprintln!("HotkeyController::push_bindings: {e}");
                    c.as_mut().set_active(false);
                    c.as_mut().set_error(QString::from(&e));
                }
            });
        });
    }
}

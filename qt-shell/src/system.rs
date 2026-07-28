//! SystemController — thin QML-invokable facade over `opengg_core::system`
//! (XDG autostart entry + crash-log folder). No properties: these are
//! one-shot actions/queries, not observable state.

#[cxx_qt::bridge]
pub mod qobject {
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type SystemController = super::SystemControllerRust;

        /// True if the XDG autostart `.desktop` entry exists.
        #[qinvokable]
        fn get_autostart(self: &Self) -> bool;

        /// Create/remove the XDG autostart entry.
        #[qinvokable]
        fn set_autostart(self: &Self, enable: bool);

        /// Open the crash-log directory in the system file manager.
        #[qinvokable]
        fn open_crash_logs_folder(self: &Self);
    }
}

#[derive(Default)]
pub struct SystemControllerRust;

impl qobject::SystemController {
    pub fn get_autostart(&self) -> bool {
        opengg_core::system::get_autostart().unwrap_or(false)
    }

    pub fn set_autostart(&self, enable: bool) {
        if let Err(e) = opengg_core::system::set_autostart(enable) {
            eprintln!("SystemController::set_autostart: {e}");
        }
    }

    pub fn open_crash_logs_folder(&self) {
        if let Err(e) = opengg_core::system::open_crash_logs_folder() {
            eprintln!("SystemController::open_crash_logs_folder: {e}");
        }
    }
}

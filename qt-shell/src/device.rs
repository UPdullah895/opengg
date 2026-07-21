//! DeviceController — a QML singleton exposing live device data from the openggd
//! daemon (`org.opengg.Daemon.Device`). Read-only for now (does not write to the
//! user's real hardware); setters land with the full Devices UI in a later phase.
//!
//! The daemon D-Bus client lives in `opengg_core::device` (plan §2.2/§2.3); this
//! controller is pure presentation glue over `opengg_core::device::get_devices()`.

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
        #[qproperty(QString, devices_json, cxx_name = "devicesJson")]
        #[qproperty(bool, connected)]
        type DeviceController = super::DeviceControllerRust;

        /// Fetch the current device list from the daemon.
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>);
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct DeviceControllerRust {
    devices_json: QString,
    connected: bool,
}

impl qobject::DeviceController {
    pub fn refresh(mut self: Pin<&mut Self>) {
        match opengg_core::device::get_devices() {
            Ok(j) => {
                self.as_mut().set_devices_json(QString::from(&j));
                self.as_mut().set_connected(true);
            }
            Err(_) => self.as_mut().set_connected(false),
        }
    }
}

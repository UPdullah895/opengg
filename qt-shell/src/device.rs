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

    impl cxx_qt::Threading for DeviceController {}
}

use core::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct DeviceControllerRust {
    devices_json: QString,
    connected: bool,
}

impl qobject::DeviceController {
    /// Reload the device list.
    ///
    /// DevicesPage polls this on a 3s repeating Timer, and `get_devices` is a
    /// blocking D-Bus round trip, so running it on the Qt thread stalled the
    /// render loop every 3 seconds for as long as the daemon took to answer.
    /// The I/O runs on a worker; only the result comes back to the Qt thread.
    pub fn refresh(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let devices = opengg_core::device::get_devices();
            let _ = qt_thread.queue(move |mut controller| match devices {
                Ok(j) => {
                    controller.as_mut().set_devices_json(QString::from(&j));
                    controller.as_mut().set_connected(true);
                }
                Err(_) => controller.as_mut().set_connected(false),
            });
        });
    }
}

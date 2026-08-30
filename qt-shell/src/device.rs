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
        // Set on a failed setDpi/setPollingRate write, cleared on the next
        // successful one. `lastErrorDeviceId` lets a specific device card
        // show the error next to the control that caused it rather than as
        // an ambiguous page-wide banner — two devices' cards can otherwise
        // both be mid-write at once.
        #[qproperty(QString, last_error, cxx_name = "lastError")]
        #[qproperty(QString, last_error_device_id, cxx_name = "lastErrorDeviceId")]
        type DeviceController = super::DeviceControllerRust;

        /// Fetch the current device list from the daemon.
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>);

        /// Set a mouse's DPI (must be one of that device's `dpiOptions`) and
        /// refresh. Runs off the Qt thread — a D-Bus round trip to ratbagd.
        #[qinvokable]
        #[cxx_name = "setDpi"]
        fn set_dpi(self: Pin<&mut Self>, device_id: &QString, dpi: i32);

        /// Set a mouse's polling rate in Hz (must be one of that device's
        /// `pollingRateOptions`) and refresh.
        #[qinvokable]
        #[cxx_name = "setPollingRate"]
        fn set_polling_rate(self: Pin<&mut Self>, device_id: &QString, rate: i32);
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
    last_error: QString,
    last_error_device_id: QString,
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

    /// Set a mouse's DPI and refresh. Always refreshes afterward — on
    /// success so the UI reflects what the device actually reports back
    /// (not just what we asked for), and on failure so a stale slider
    /// position doesn't linger looking like it "took."
    pub fn set_dpi(self: Pin<&mut Self>, device_id: &QString, dpi: i32) {
        let device_id = device_id.to_string();
        let dpi = dpi.max(0) as u32;
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = opengg_core::device::set_mouse_dpi(device_id.clone(), dpi);
            let _ = qt_thread.queue(move |mut controller| {
                match result {
                    Ok(()) => {
                        controller.as_mut().set_last_error(QString::from(""));
                        controller.as_mut().set_last_error_device_id(QString::from(""));
                    }
                    Err(e) => {
                        eprintln!("setDpi: {e}");
                        controller.as_mut().set_last_error(QString::from(&e));
                        controller
                            .as_mut()
                            .set_last_error_device_id(QString::from(&device_id));
                    }
                }
                controller.as_mut().refresh();
            });
        });
    }

    /// Set a mouse's polling rate and refresh. Same always-refresh reasoning
    /// as `set_dpi`.
    pub fn set_polling_rate(self: Pin<&mut Self>, device_id: &QString, rate: i32) {
        let device_id = device_id.to_string();
        let rate = rate.max(0) as u32;
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = opengg_core::device::set_mouse_polling_rate(device_id.clone(), rate);
            let _ = qt_thread.queue(move |mut controller| {
                match result {
                    Ok(()) => {
                        controller.as_mut().set_last_error(QString::from(""));
                        controller.as_mut().set_last_error_device_id(QString::from(""));
                    }
                    Err(e) => {
                        eprintln!("setPollingRate: {e}");
                        controller.as_mut().set_last_error(QString::from(&e));
                        controller
                            .as_mut()
                            .set_last_error_device_id(QString::from(&device_id));
                    }
                }
                controller.as_mut().refresh();
            });
        });
    }
}

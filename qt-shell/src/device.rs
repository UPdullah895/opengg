//! DeviceController — a QML singleton exposing live device data from the openggd
//! daemon (`org.opengg.Daemon.Device`). Read-only for now (does not write to the
//! user's real hardware); setters land with the full Devices UI in a later phase.
//!
//! Boundary note: thin D-Bus client only; moves to opengg-core with the other
//! daemon clients during the core-extraction task (TODO(core), plan §2.2/§2.3).

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

const DEST: &str = "org.opengg.Daemon";
const PATH: &str = "/org/opengg/Daemon/Device";
const IFACE: &str = "org.opengg.Daemon.Device";

pub struct DeviceControllerRust {
    devices_json: QString,
    connected: bool,
    conn: Option<zbus::blocking::Connection>,
}

impl Default for DeviceControllerRust {
    fn default() -> Self {
        Self {
            devices_json: QString::default(),
            connected: false,
            conn: zbus::blocking::Connection::session().ok(),
        }
    }
}

impl qobject::DeviceController {
    pub fn refresh(mut self: Pin<&mut Self>) {
        let json = self.conn.as_ref().and_then(|c| fetch_devices(c).ok());
        match json {
            Some(j) => {
                self.as_mut().set_devices_json(QString::from(&j));
                self.as_mut().set_connected(true);
            }
            None => self.as_mut().set_connected(false),
        }
    }
}

fn fetch_devices(c: &zbus::blocking::Connection) -> zbus::Result<String> {
    let reply = c.call_method(Some(DEST), PATH, Some(IFACE), "GetDevices", &())?;
    let json: String = reply.body().deserialize()?;
    Ok(json)
}

//! Blocking D-Bus client for the OpenGG daemon (`org.opengg.Daemon`).
//!
//! A blocking facade over zbus's async API (project decision: blocking client
//! in core), so the synchronous cxx-qt shell can call it directly. zbus 6
//! dropped its own blocking API; each call here is one `zbus::block_on`, and
//! between calls zbus keeps the cached connection serviced on a helper thread.
//! Never call these from inside another `block_on` — zbus panics on nesting.

use serde::de::DeserializeOwned;
use std::sync::OnceLock;
use zbus::Type;

pub const AU_PATH: &str = "/org/opengg/Daemon/Audio";
pub const AU_IFACE: &str = "org.opengg.Daemon.Audio";
pub const RP_PATH: &str = "/org/opengg/Daemon/Replay";
pub const RP_IFACE: &str = "org.opengg.Daemon.Replay";
pub const DV_PATH: &str = "/org/opengg/Daemon/Device";
pub const DV_IFACE: &str = "org.opengg.Daemon.Device";
pub const EX_PATH: &str = "/org/opengg/Daemon/Extensions";
pub const EX_IFACE: &str = "org.opengg.Daemon.Extensions";

// Cached blocking D-Bus session connection to avoid reconnecting on every call.
static DBUS_SESSION: OnceLock<zbus::Connection> = OnceLock::new();

fn session() -> Result<&'static zbus::Connection, String> {
    if let Some(c) = DBUS_SESSION.get() {
        return Ok(c);
    }
    let c = zbus::block_on(zbus::Connection::session()).map_err(|e| format!("{e}"))?;
    let _ = DBUS_SESSION.set(c);
    Ok(DBUS_SESSION.get().unwrap())
}

pub fn call_dbus<R: DeserializeOwned + Type>(
    m: &str,
    p: &str,
    i: &str,
    a: impl serde::Serialize + Type,
) -> Result<R, String> {
    let c = session()?;
    let reply = zbus::block_on(c.call_method(Some("org.opengg.Daemon"), p, Some(i), m, &a))
        .map_err(|e| format!("{m}:{e}"))?;
    let r: R = reply.body().deserialize().map_err(|e| format!("{m}:{e}"))?;
    Ok(r)
}

pub fn call_dbus_void(
    m: &str,
    p: &str,
    i: &str,
    a: impl serde::Serialize + Type,
) -> Result<(), String> {
    let c = session()?;
    match zbus::block_on(c.call_method(Some("org.opengg.Daemon"), p, Some(i), m, &a)) {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("call_dbus: method '{m}' on {p}.{i} failed — {e}");
            Err(format!("D-Bus method {m}: {e}"))
        }
    }
}

//! How a device is physically attached — Devices regression pass.
//!
//! **ratbagd has no connection-type property.** Introspecting
//! `org.freedesktop.ratbag1.Device` gives exactly `DeviceType`,
//! `FirmwareVersion`, `Model`, `Name` and `Profiles` — nothing about how the
//! thing is plugged in. So this is derived from the kernel's own USB/HID
//! topology instead, which is real data rather than a guess, and in
//! particular is *not* string-matching the word "Wireless" out of a marketing
//! name (a name says what the product supports, not how it is attached right
//! now — the very same G502 reports "LIGHTSPEED Wireless Gaming Mouse" while
//! sitting on a charging cable).
//!
//! The signal: a USB HID device's nearest USB *device* ancestor is the thing
//! that actually enumerated on the bus.
//! - Plugged in directly, a mouse enumerates as itself, so that ancestor
//!   carries the mouse's own product id → **wired**.
//! - Talking through a wireless receiver, the *receiver* is what enumerated,
//!   so the ancestor carries the receiver's product id while the HID device
//!   underneath reports the mouse's own (different) wireless product id →
//!   **wireless**.
//!
//! Bluetooth is read straight off ratbagd's `Model` string, whose first
//! field is the bus (`"usb:046d:c08d:0"` / `"bluetooth:..."`).
//!
//! A hub in the path doesn't confuse this: a hub is not the HID interface's
//! parent *device*, the mouse still is.

use std::path::{Path, PathBuf};

/// How a device is attached, as far as kernel topology can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    Wired,
    Wireless,
    Bluetooth,
}

impl ConnectionType {
    /// The lowercase tag put on the wire, for the UI's connection badge.
    pub fn as_tag(self) -> &'static str {
        match self {
            Self::Wired => "wired",
            Self::Wireless => "wireless",
            Self::Bluetooth => "bluetooth",
        }
    }
}

/// The bus field of a ratbagd `Model` string (`"<bus>:<vid>:<pid>:<ver>"`).
/// Returns `None` for anything not shaped like that, rather than guessing.
fn model_bus(model: &str) -> Option<&str> {
    let bus = model.split(':').next()?;
    (!bus.is_empty()).then_some(bus)
}

/// The pure decision, split out from the sysfs walk below so it can actually
/// be tested — same split `identity_overrides.rs` uses between its pure
/// mutation methods and its untested `load`/`save`.
///
/// `usb_parent_pid` is the product id of the nearest USB device ancestor, or
/// `None` when the walk couldn't find one (in which case a USB device is
/// reported as wired: it is on the USB bus and nothing indicates a receiver,
/// and inventing "wireless" from missing data would be worse than the plain
/// reading).
fn classify(model: &str, device_pid: u16, usb_parent_pid: Option<u16>) -> Option<ConnectionType> {
    match model_bus(model)? {
        "bluetooth" => Some(ConnectionType::Bluetooth),
        "usb" => Some(match usb_parent_pid {
            // Something other than this device enumerated on the bus for it
            // — i.e. a receiver is relaying, so the link to the device itself
            // is wireless.
            Some(parent) if parent != device_pid => ConnectionType::Wireless,
            _ => ConnectionType::Wired,
        }),
        _ => None,
    }
}

/// Read a `0x`-less lowercase hex id file (`idProduct`) from a sysfs dir.
fn read_hex_id(dir: &Path, file: &str) -> Option<u16> {
    let raw = std::fs::read_to_string(dir.join(file)).ok()?;
    u16::from_str_radix(raw.trim(), 16).ok()
}

/// Walk up from a hidraw node to the nearest USB device directory (the first
/// ancestor carrying both `idVendor` and `idProduct`) and return its product
/// id.
fn usb_parent_pid_of(sysname: &str) -> Option<u16> {
    // `/sys/class/hidraw/<sysname>/device` is the HID device; its ancestors
    // run up through the USB interface to the USB device that enumerated.
    let start = std::fs::canonicalize(format!("/sys/class/hidraw/{sysname}/device")).ok()?;

    let mut cur: Option<&Path> = Some(start.as_path());
    while let Some(dir) = cur {
        if dir.join("idVendor").is_file() {
            if let Some(pid) = read_hex_id(dir, "idProduct") {
                return Some(pid);
            }
        }
        // Stop at the sysfs root rather than walking off the top.
        if dir == Path::new("/sys") || dir == Path::new("/") {
            break;
        }
        cur = dir.parent();
    }
    None
}

/// Determine how the device behind `sysname` (a hidraw node name such as
/// `"hidraw11"`, which is also the last path segment of ratbagd's own device
/// object path) is attached. `None` when it can't be told — callers must
/// treat that as "don't show a badge", never as a default of either kind.
pub fn detect(sysname: &str, model: &str, device_pid: u16) -> Option<ConnectionType> {
    let parent = usb_parent_pid_of(sysname);
    let resolved = PathBuf::from(format!("/sys/class/hidraw/{sysname}"));
    if !resolved.exists() && model_bus(model) != Some("bluetooth") {
        // No sysfs node to reason about and not obviously Bluetooth — say
        // nothing rather than assert a connection type from thin air.
        return None;
    }
    classify(model, device_pid, parent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_is_read_from_the_model_bus_field() {
        assert_eq!(
            classify("bluetooth:046d:b02d:0", 0xb02d, None),
            Some(ConnectionType::Bluetooth)
        );
    }

    #[test]
    fn a_device_that_enumerated_as_itself_is_wired() {
        // Live case on the machine this was written against: the G502's HID
        // node hangs off a USB device whose idProduct is the mouse's own.
        assert_eq!(
            classify("usb:046d:c08d:0", 0xc08d, Some(0xc08d)),
            Some(ConnectionType::Wired)
        );
    }

    #[test]
    fn a_device_behind_a_differently_identified_receiver_is_wireless() {
        // The receiver (c547) is what enumerated; the mouse reports its own
        // wireless product id (407f) underneath it.
        assert_eq!(
            classify("usb:046d:407f:0", 0x407f, Some(0xc547)),
            Some(ConnectionType::Wireless)
        );
    }

    #[test]
    fn a_usb_device_with_no_findable_parent_reads_as_wired_not_wireless() {
        // Missing topology must not be reported as the more specific claim.
        assert_eq!(
            classify("usb:046d:c08d:0", 0xc08d, None),
            Some(ConnectionType::Wired)
        );
    }

    #[test]
    fn an_unrecognized_bus_yields_nothing_rather_than_a_guess() {
        assert_eq!(classify("i2c:0001:0002:0", 0x0002, None), None);
        assert_eq!(classify("", 0x0002, None), None);
    }

    #[test]
    fn tags_are_the_lowercase_strings_the_ui_matches_on() {
        assert_eq!(ConnectionType::Wired.as_tag(), "wired");
        assert_eq!(ConnectionType::Wireless.as_tag(), "wireless");
        assert_eq!(ConnectionType::Bluetooth.as_tag(), "bluetooth");
    }
}

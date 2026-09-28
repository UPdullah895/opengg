use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Mouse,
    Headset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqMeta {
    pub bands: u32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub model: String,
    pub device_type: DeviceType,
    pub vid: u16,
    pub pid: u16,
    /// When this card represents a cross-transport merge of two or more
    /// ratbagd device objects that are believed to be the same physical
    /// mouse (see `ratbag::merge_key_slug` / `IdentityOverrides`), this
    /// lists every member link as a `"{vid:04x}:{pid:04x}"` string. `None`
    /// (or omitted) for an ordinary single-link device. `vid`/`pid` above
    /// always describe the currently "primary" (live, if any) link.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_ids: Option<Vec<String>>,
    /// How the device is physically attached ("wired" / "wireless" /
    /// "bluetooth"), derived from kernel USB/HID topology in
    /// `device::connection` — ratbagd itself reports no such property.
    /// `None` when it genuinely could not be determined; the UI shows no
    /// badge in that case rather than defaulting to either claim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
    // Mouse-only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polling_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi_options: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polling_rate_options: Option<Vec<u32>>,
    /// Number of physical buttons on the active profile (Devices Phase 5's
    /// hotspot editor needs this to cap how many hotspots can be placed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub button_count: Option<u32>,
    // Headset-only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_level: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_charging: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidetone: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chatmix: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_presets: Option<HashMap<String, Vec<f32>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_meta: Option<EqMeta>,
}

// ── Device-ID vocabulary ──────────────────────────────────────────────────
//
// A device id is `"{prefix}{vid:04x}:{pid:04x}"`, e.g. `"headset:1038:2202"`.
// Lower-case hex, zero padded, on BOTH sides.
//
// The formatter and the parser live together here on purpose. They used to
// sit in different modules — `headset.rs` built ids with `{}` (decimal)
// while `dbus.rs` read them back with `from_str_radix(.., 16)`. Nothing
// caught it because each half was self-consistent: the id for 0x1038:0x2202
// was written "headset:4152:8706" and read back as 0x4152:0x8706, so every
// headset command was addressed to a device that does not exist. Keeping
// the pair adjacent, with a round-trip test, is what stops that recurring.

/// Format a VID/PID pair as the canonical `"vvvv:pppp"` body of a device id.
pub fn format_vid_pid(vid: u16, pid: u16) -> String {
    format!("{vid:04x}:{pid:04x}")
}

/// Build a full device id for `prefix` (which must include its trailing `:`,
/// e.g. `"headset:"`).
pub fn format_device_id(prefix: &str, vid: u16, pid: u16) -> String {
    format!("{prefix}{}", format_vid_pid(vid, pid))
}

/// Inverse of [`format_device_id`]. `None` if the prefix or the hex pair
/// does not match.
pub fn parse_device_id(device_id: &str, prefix: &str) -> Option<(u16, u16)> {
    let body = device_id.strip_prefix(prefix)?;
    let (vid_str, pid_str) = body.split_once(':')?;
    let vid = u16::from_str_radix(vid_str, 16).ok()?;
    let pid = u16::from_str_radix(pid_str, 16).ok()?;
    Some((vid, pid))
}

#[cfg(test)]
mod id_tests {
    use super::*;

    /// The regression that mattered: build an id, parse it back, get the
    /// same numbers. The decimal-vs-hex mismatch this guards against broke
    /// every headset control in the app.
    #[test]
    fn device_ids_round_trip() {
        // Arctis Nova 7 and a Logitech mouse — both halves of the range
        // where decimal and hex disagree.
        for (vid, pid) in [(0x1038u16, 0x2202u16), (0x046d, 0xc08d), (0, 0), (0xffff, 0xffff)] {
            let id = format_device_id("headset:", vid, pid);
            assert_eq!(parse_device_id(&id, "headset:"), Some((vid, pid)), "id was {id}");
        }
    }

    /// The exact shape headsetcontrol's `--device` flag expects.
    #[test]
    fn format_is_zero_padded_lowercase_hex() {
        assert_eq!(format_device_id("headset:", 0x1038, 0x2202), "headset:1038:2202");
        assert_eq!(format_vid_pid(0x046d, 0x0a5c), "046d:0a5c");
    }

    /// A decimal-formatted id must NOT silently parse into different
    /// numbers — that silence is what hid the original bug.
    #[test]
    fn decimal_ids_do_not_round_trip() {
        let wrong = format!("headset:{}:{}", 0x1038, 0x2202); // "headset:4152:8706"
        assert_ne!(parse_device_id(&wrong, "headset:"), Some((0x1038, 0x2202)));
    }

    #[test]
    fn rejects_malformed() {
        assert_eq!(parse_device_id("headset:nope", "headset:"), None);
        assert_eq!(parse_device_id("ratbag:1038:2202", "headset:"), None);
        assert_eq!(parse_device_id("headset:zzzz:2202", "headset:"), None);
    }
}

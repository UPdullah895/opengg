//! ratbagd D-Bus integration for mouse management (org.freedesktop.ratbag1).

use anyhow::{Context, Result};
use zbus::{proxy, zvariant, Connection};

use super::identity_overrides::IdentityOverrides;
use super::types::{DeviceInfo, DeviceType};

// ── D-Bus proxy definitions ──────────────────────────────────────────────────

#[proxy(
    interface = "org.freedesktop.ratbag1.Manager",
    default_service = "org.freedesktop.ratbag1",
    default_path = "/org/freedesktop/ratbag1"
)]
trait Manager {
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.ratbag1.Device",
    default_service = "org.freedesktop.ratbag1"
)]
trait Device {
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn model(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn profiles(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;

    // NOTE: libratbag's own dbus.rst documents `Commit() → ()` (no return
    // value), but this installed ratbagd (0.18-1) actually replies with a
    // `u` — confirmed live via `busctl --system introspect
    // .../device/hidraw0`, which shows `.Commit  method  -  u  -`. A `()`
    // return type here made zbus fail deserializing every successful
    // Commit() reply with "Signature mismatch: got `u`, expected ``",
    // silently breaking every write that got far enough to call it (i.e.
    // once the separate `Resolution` variant-wrapping bug was fixed, this
    // became the next thing blocking a real write). The docs' own wording
    // ("this call always succeeds ... errors surface via the Resync
    // signal") suggests this return value isn't a success/failure code to
    // act on — it's logged at debug level and otherwise ignored.
    fn commit(&self) -> zbus::Result<u32>;
}

#[proxy(
    interface = "org.freedesktop.ratbag1.Profile",
    default_service = "org.freedesktop.ratbag1"
)]
trait Profile {
    #[zbus(property)]
    fn resolutions(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;

    #[zbus(property)]
    fn is_active(&self) -> zbus::Result<bool>;

    // NOTE: ReportRate/ReportRates live on **Profile**, not Resolution — see
    // libratbag's own dbus.rst (`org.freedesktop.ratbag1.Profile.ReportRate`,
    // type `u`), confirmed live against this machine's ratbagd
    // (`busctl --system call ... /profile/hidraw0/p0 ... GetAll` shows
    // `"ReportRate" u 1000` and `"ReportRates" au 4 125 250 500 1000`, and
    // the *Resolution* object's own GetAll has neither key at all). The
    // previous code queried both from the Resolution proxy, which doesn't
    // have them — SetPollingRate/report_rate reads have never worked for
    // any device, on any machine, regardless of connection mode.
    #[zbus(property)]
    fn report_rate(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn set_report_rate(&self, value: u32) -> zbus::Result<()>;

    #[zbus(property)]
    fn report_rates(&self) -> zbus::Result<Vec<u32>>;

    /// The active profile's buttons, in physical order. Used both to report
    /// a device's button count (Devices Phase 5's hotspot editor needs to
    /// know how many hotspots a device can have) and to reach each button's
    /// own object for reading/writing its `Mapping`.
    #[zbus(property)]
    fn buttons(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.ratbag1.Button",
    default_service = "org.freedesktop.ratbag1"
)]
trait Button {
    /// 0-based physical button index. NOT the same numbering as a `BUTTON`
    /// action's target value below (that's 1-based) — confirmed live via
    /// `ratbagctl info`, which prints "Button: 0 is mapped to 'button 1'"
    /// for an unmodified device: button *index* 0 defaults to *action*
    /// target 1 (its own physical click).
    #[zbus(property)]
    fn index(&self) -> zbus::Result<u32>;

    /// Which `ActionType` values (see `ButtonActionType`) this specific
    /// button supports writing. Confirmed live to vary is possible in
    /// principle (libratbag's own model allows a per-button capability
    /// mask) even though every button on the G502 tested against this
    /// session reports the same set `[0,1,2,3,4]`.
    #[zbus(property)]
    fn action_types(&self) -> zbus::Result<Vec<u32>>;

    /// The button's current mapping: `(action_type, payload)`. Confirmed
    /// live this is a single-nested variant (`busctl` shows `(uv) 1 u 1`,
    /// i.e. type=1/BUTTON, payload=`u 1`) — unlike `Resolution`, there is
    /// no *extra* outer variant wrap to unwrap here.
    #[zbus(property)]
    fn mapping(&self) -> zbus::Result<(u32, zbus::zvariant::OwnedValue)>;
}

#[proxy(
    interface = "org.freedesktop.ratbag1.Resolution",
    default_service = "org.freedesktop.ratbag1"
)]
trait Resolution {
    // NOTE: `Resolution` (the DPI value) is declared `:type: v` in
    // libratbag's own dbus.rst — its value is itself a nested variant,
    // holding either a plain `u` (both axes) or a `(uu)` pair (separate x/y),
    // depending on the device/profile. Confirmed live:
    // `busctl --system call ... Properties Get ss ... Resolution Resolution`
    // returns `v v u 400` — a variant containing a variant. A typed
    // `(u32, u32)` getter (the previous code) can't deserialize that at all,
    // so it silently failed on every device — DPI never actually populated.
    // Declared here as `OwnedValue` so `extract_dpi_x`/`set_resolution_value`
    // can branch on the real shape instead of assuming one.
    #[zbus(property)]
    fn resolution(&self) -> zbus::Result<zbus::zvariant::OwnedValue>;

    #[zbus(property)]
    fn resolutions(&self) -> zbus::Result<Vec<u32>>;

    #[zbus(property)]
    fn is_active(&self) -> zbus::Result<bool>;
}

// ── Button action mapping ───────────────────────────────────────────────────
//
// Investigated live against this machine's ratbagd (0.18-1) rather than
// assumed: there is no `SetActionSpecial`/`SetActionButton`/`SetActionKey`/
// `SetActionMacro`/`SetActionNone` method anywhere on the `Button` interface
// (`busctl introspect .../button/hidraw13/p1/b0` lists only the `ActionTypes`
// and `Mapping` properties, both above). Every action type is written
// through the single `Mapping` property, shaped `(uv)`: a `u` selecting
// which of the 5 action types is active, plus a `v` holding that type's own
// payload. `libratbag`'s own `ratbagctl` (`/usr/bin/ratbagctl`, installed
// alongside this ratbagd) is the ground truth for both the enum values and
// the payload shape per type — cross-referenced here, not guessed:
//
//   NONE    = 0, payload `u` (always 0) — disables the button.
//   BUTTON  = 1, payload `u`  — a **1-based** physical button number, e.g.
//             mapping button index 5 to action target `1` makes it act as
//             button 1 (left click). Do not confuse with `Button.Index`
//             (0-based) above.
//   SPECIAL = 2, payload `u`  — one of the `ActionSpecial` sentinel values
//             (large constants, `(1 << 30) + N`), see `SPECIAL_ACTIONS`.
//   KEY     = 3, payload `u`  — a raw Linux evdev keycode (`KEY_*` from
//             `linux/input-event-codes.h`), see `KEY_ACTIONS`.
//   MACRO   = 4, payload `a(uu)` — a sequence of (press/release/wait, value)
//             steps. **Deliberately not implemented for writing or decoding
//             in this phase** — a macro sequence editor is a materially
//             bigger UI than "pick one action from a list," out of scope for
//             this pass. A button currently holding a macro is reported back
//             as `{"type":"macro"}` with no further detail (still lets the
//             UI show *that* a button is bound to something, just not
//             what) rather than silently misreporting it as `none`.
//
// `ActionType`/`ActionSpecial` numeric values and the `KEY_*` codes below are
// Linux kernel/libratbag protocol constants, not creative expression — the
// same category as HTTP status codes — copied from this machine's own
// `/usr/bin/ratbagctl` (`ActionType`/`ActionSpecial` enums) and
// `/usr/include/linux/input-event-codes.h` (`KEY_*` `#define`s) to guarantee
// they match what this ratbagd build actually expects on the wire.

const ACTION_TYPE_NONE: u32 = 0;
const ACTION_TYPE_BUTTON: u32 = 1;
const ACTION_TYPE_SPECIAL: u32 = 2;
const ACTION_TYPE_KEY: u32 = 3;
const ACTION_TYPE_MACRO: u32 = 4;

/// `(machine name, ratbagd ActionSpecial value, human label)` — copied
/// verbatim from `/usr/bin/ratbagctl`'s `ActionSpecial` enum and
/// `SPECIAL_DESCRIPTION` table (libratbag 0.18-1). Machine names are this
/// daemon's own wire vocabulary (snake_case), not libratbag's.
const SPECIAL_ACTIONS: &[(&str, u32, &str)] = &[
    // ActionSpecial.UNKNOWN itself (`1 << 30`, no `+ N` offset) is a real,
    // documented enum member — not a catch-all placeholder we're inventing.
    // Found live: this session's own connected G502 has a button (index 8)
    // whose factory-default Mapping is genuinely this value, which the
    // table originally omitted, making `decode_button_action` report it as
    // a made-up `"unknown_1073741824"` name instead of the real one.
    ("unknown", 1 << 30, "Unknown"),
    ("doubleclick", (1 << 30) + 1, "Doubleclick"),
    ("wheel_left", (1 << 30) + 2, "Wheel Left"),
    ("wheel_right", (1 << 30) + 3, "Wheel Right"),
    ("wheel_up", (1 << 30) + 4, "Wheel Up"),
    ("wheel_down", (1 << 30) + 5, "Wheel Down"),
    ("ratchet_mode_switch", (1 << 30) + 6, "Ratchet Mode"),
    ("resolution_cycle_up", (1 << 30) + 7, "Cycle DPI Up"),
    ("resolution_cycle_down", (1 << 30) + 8, "Cycle DPI Down"),
    ("resolution_up", (1 << 30) + 9, "DPI Up"),
    ("resolution_down", (1 << 30) + 10, "DPI Down"),
    ("resolution_alternate", (1 << 30) + 11, "DPI Switch (hold)"),
    ("resolution_default", (1 << 30) + 12, "Default DPI"),
    ("profile_cycle_up", (1 << 30) + 13, "Cycle Profile Up"),
    ("profile_cycle_down", (1 << 30) + 14, "Cycle Profile Down"),
    ("profile_up", (1 << 30) + 15, "Profile Up"),
    ("profile_down", (1 << 30) + 16, "Profile Down"),
    ("second_mode", (1 << 30) + 17, "Second Mode"),
    ("battery_level", (1 << 30) + 18, "Battery Level"),
];

/// `(machine name, Linux evdev keycode, human label)` — a deliberately
/// modest subset of `/usr/include/linux/input-event-codes.h`'s `KEY_*`
/// constants (letters, digits, function keys, common modifiers/navigation),
/// not the full evdev keycode space. Good enough for the common gaming-mouse
/// rebind cases (a side button pressing "E", a number key, a modifier) —
/// no numpad-specific, media, or international keys. Extend this table
/// (never invent numbers — copy them from the same header) if a real need
/// for one shows up.
const KEY_ACTIONS: &[(&str, u32, &str)] = &[
    ("esc", 1, "Esc"),
    ("1", 2, "1"),
    ("2", 3, "2"),
    ("3", 4, "3"),
    ("4", 5, "4"),
    ("5", 6, "5"),
    ("6", 7, "6"),
    ("7", 8, "7"),
    ("8", 9, "8"),
    ("9", 10, "9"),
    ("0", 11, "0"),
    ("backspace", 14, "Backspace"),
    ("tab", 15, "Tab"),
    ("q", 16, "Q"),
    ("w", 17, "W"),
    ("e", 18, "E"),
    ("r", 19, "R"),
    ("t", 20, "T"),
    ("y", 21, "Y"),
    ("u", 22, "U"),
    ("i", 23, "I"),
    ("o", 24, "O"),
    ("p", 25, "P"),
    ("enter", 28, "Enter"),
    ("leftctrl", 29, "Left Ctrl"),
    ("a", 30, "A"),
    ("s", 31, "S"),
    ("d", 32, "D"),
    ("f", 33, "F"),
    ("g", 34, "G"),
    ("h", 35, "H"),
    ("j", 36, "J"),
    ("k", 37, "K"),
    ("l", 38, "L"),
    ("leftshift", 42, "Left Shift"),
    ("z", 44, "Z"),
    ("x", 45, "X"),
    ("c", 46, "C"),
    ("v", 47, "V"),
    ("b", 48, "B"),
    ("n", 49, "N"),
    ("m", 50, "M"),
    ("rightshift", 54, "Right Shift"),
    ("leftalt", 56, "Left Alt"),
    ("space", 57, "Space"),
    ("capslock", 58, "Caps Lock"),
    ("f1", 59, "F1"),
    ("f2", 60, "F2"),
    ("f3", 61, "F3"),
    ("f4", 62, "F4"),
    ("f5", 63, "F5"),
    ("f6", 64, "F6"),
    ("f7", 65, "F7"),
    ("f8", 66, "F8"),
    ("f9", 67, "F9"),
    ("f10", 68, "F10"),
    ("rightctrl", 97, "Right Ctrl"),
    ("rightalt", 100, "Right Alt"),
    ("home", 102, "Home"),
    ("up", 103, "Up"),
    ("pageup", 104, "Page Up"),
    ("left", 105, "Left"),
    ("right", 106, "Right"),
    ("end", 107, "End"),
    ("down", 108, "Down"),
    ("pagedown", 109, "Page Down"),
    ("insert", 110, "Insert"),
    ("delete", 111, "Delete"),
    ("f11", 87, "F11"),
    ("f12", 88, "F12"),
    ("leftmeta", 125, "Left Super"),
    ("rightmeta", 126, "Right Super"),
];

/// A button action as the daemon's JSON wire format represents it — what
/// `GetButtonMappings` reports per button and what `SetButtonAction` accepts.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ButtonAction {
    None,
    /// `target` is the 1-based physical button number (see the module doc
    /// comment above) — NOT a 0-based button index.
    Button { target: u32 },
    Special { name: String },
    Key { name: String },
    /// Reported when the device's active mapping is a macro this phase
    /// doesn't decode — never accepted by `SetButtonAction`.
    Macro,
    /// Reported for an action type this daemon doesn't recognize at all
    /// (future libratbag versions may add more) — never accepted by
    /// `SetButtonAction`.
    Unknown,
}

/// Decode a raw `(action_type, payload)` `Mapping` value into a `ButtonAction`.
fn decode_button_action(action_type: u32, payload: &zvariant::Value) -> ButtonAction {
    let as_u32 = || match unwrap_nested_variant(payload) {
        zvariant::Value::U32(v) => Some(*v),
        _ => None,
    };
    match action_type {
        ACTION_TYPE_NONE => ButtonAction::None,
        ACTION_TYPE_BUTTON => match as_u32() {
            Some(target) => ButtonAction::Button { target },
            None => ButtonAction::Unknown,
        },
        ACTION_TYPE_SPECIAL => match as_u32() {
            Some(v) => match SPECIAL_ACTIONS.iter().find(|(_, val, _)| *val == v) {
                Some((name, _, _)) => ButtonAction::Special { name: name.to_string() },
                // A real ActionSpecial value ratbagd sent us that isn't in
                // our (deliberately non-exhaustive) table — surface the raw
                // number rather than silently dropping it.
                None => ButtonAction::Special { name: format!("unknown_{v}") },
            },
            None => ButtonAction::Unknown,
        },
        ACTION_TYPE_KEY => match as_u32() {
            Some(v) => match KEY_ACTIONS.iter().find(|(_, code, _)| *code == v) {
                Some((name, _, _)) => ButtonAction::Key { name: name.to_string() },
                None => ButtonAction::Key { name: format!("unknown_{v}") },
            },
            None => ButtonAction::Unknown,
        },
        ACTION_TYPE_MACRO => ButtonAction::Macro,
        _ => ButtonAction::Unknown,
    }
}

/// Encode a `ButtonAction` into the `(action_type, payload)` pair to write
/// to `Mapping`. Returns the numeric `ActionType` (to check against the
/// button's advertised `ActionTypes` before writing) and the payload value.
fn encode_button_action(action: &ButtonAction) -> Result<(u32, zvariant::Value<'static>)> {
    match action {
        ButtonAction::None => Ok((ACTION_TYPE_NONE, zvariant::Value::U32(0))),
        ButtonAction::Button { target } => {
            Ok((ACTION_TYPE_BUTTON, zvariant::Value::U32(*target)))
        }
        ButtonAction::Special { name } => SPECIAL_ACTIONS
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, v, _)| (ACTION_TYPE_SPECIAL, zvariant::Value::U32(*v)))
            .ok_or_else(|| anyhow::anyhow!("unknown special action \"{name}\"")),
        ButtonAction::Key { name } => KEY_ACTIONS
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, code, _)| (ACTION_TYPE_KEY, zvariant::Value::U32(*code)))
            .ok_or_else(|| anyhow::anyhow!("unknown key \"{name}\"")),
        ButtonAction::Macro | ButtonAction::Unknown => {
            anyhow::bail!("this action type cannot be written by OpenGG yet")
        }
    }
}

/// One button's full state, as `GetButtonMappings` reports it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ButtonMapping {
    /// 0-based, matches `Button.Index` — this is what `SetButtonAction`
    /// expects as its `button_index` argument, NOT the 1-based `target` a
    /// `Button` action's own payload uses.
    pub index: u32,
    /// Which `ButtonAction` variant *tags* (`"none"`/`"button"`/`"special"`/
    /// `"key"`/`"macro"`) this specific button can be set to — always a
    /// subset of what `GetButtonActionCatalog` offers overall.
    pub supported_action_types: Vec<String>,
    pub action: ButtonAction,
}

fn action_type_tag(action_type: u32) -> Option<&'static str> {
    match action_type {
        ACTION_TYPE_NONE => Some("none"),
        ACTION_TYPE_BUTTON => Some("button"),
        ACTION_TYPE_SPECIAL => Some("special"),
        ACTION_TYPE_KEY => Some("key"),
        ACTION_TYPE_MACRO => Some("macro"),
        _ => None,
    }
}

/// Static JSON catalog of every action the UI can offer — the `special` and
/// `key` tables above, shaped for a flat picker list. Device-independent
/// (no D-Bus round trip needed), but lives here rather than in
/// `opengg-core` because the machine names it hands out must exactly match
/// what `encode_button_action` accepts.
pub fn button_action_catalog_json() -> String {
    let specials: Vec<_> = SPECIAL_ACTIONS
        .iter()
        .map(|(name, _, label)| serde_json::json!({"name": name, "label": label}))
        .collect();
    let keys: Vec<_> = KEY_ACTIONS
        .iter()
        .map(|(name, _, label)| serde_json::json!({"name": name, "label": label}))
        .collect();
    serde_json::json!({"special": specials, "key": keys}).to_string()
}

/// Unwrap the one extra variant layer ratbagd's `Resolution` property adds on
/// top of the real payload (see the `Resolution` proxy's doc comment above).
fn unwrap_nested_variant<'a>(value: &'a zvariant::Value<'a>) -> &'a zvariant::Value<'a> {
    match value {
        zvariant::Value::Value(boxed) => boxed.as_ref(),
        other => other,
    }
}

/// Extract the x-axis DPI from a `Resolution` property value, regardless of
/// whether it's currently shaped as a plain `u` or a `(uu)` pair.
fn extract_dpi_x(value: &zvariant::Value) -> Option<u32> {
    match unwrap_nested_variant(value) {
        zvariant::Value::U32(x) => Some(*x),
        zvariant::Value::Structure(s) => match s.fields().first() {
            Some(zvariant::Value::U32(x)) => Some(*x),
            _ => None,
        },
        _ => None,
    }
}

/// Build the value to write back to a `Resolution` property for a new DPI,
/// preserving whatever shape (`u` vs `(uu)`) it's currently using — libratbag
/// explicitly documents that changing shape is invalid ("assigning a single
/// u to a resolution object previously exporting (uu) is invalid").
fn build_resolution_value(current: &zvariant::Value, dpi: u32) -> zvariant::Value<'static> {
    let new_inner = match unwrap_nested_variant(current) {
        zvariant::Value::Structure(_) => zvariant::Value::new((dpi, dpi)),
        _ => zvariant::Value::new(dpi),
    };
    zvariant::Value::Value(Box::new(new_inner))
}

// ── Cross-transport identity ────────────────────────────────────────────────
//
// ratbagd's D-Bus API exposes no persistent per-unit serial — `Model` is only
// "usb:VVVV:PPPP:version", identical for every unit of the same mouse model
// (confirmed against a live ratbagd instance: a Logitech G502 LIGHTSPEED
// reports "usb:046d:c08d:0" over its wireless receiver and "usb:046d:407f:0"
// when wired, as two entirely separate ratbagd device objects with no shared
// property at all). So a single physical mouse can legitimately show up as
// two distinct (vid, pid) pairs depending on connection mode, and both can be
// present in ratbagd's device list *simultaneously* — this isn't a
// stale/replaced-link case we can resolve by timing.
//
// The fallback here is a name-similarity heuristic: same vendor ID, and the
// device names match after stripping known connection-mode/marketing tokens.
// It is deliberately conservative — see `merge_key_slug` — and is always
// overridable by an explicit user decision recorded in
// `IdentityOverrides` (`MergeMouseDevices` / `SplitMouseDevice` on the D-Bus
// interface), which always wins over the heuristic for that specific pair.

/// Marketing/connection-mode tokens stripped before comparing model names.
/// Deliberately excludes anything that could denote a genuinely different
/// hardware revision (e.g. "hero", "se", "plus") — stripping those would
/// risk silently merging two different real devices, which is a worse
/// failure mode than leaving a duplicate card on screen.
const NAME_MERGE_STOPWORDS: &[&str] = &[
    "wireless",
    "lightspeed",
    "bluetooth",
    "receiver",
    "dongle",
    "gaming",
    "mouse",
    "keyboard",
    "headset",
    "usb",
    "rf",
    "2.4ghz",
    "2.4g",
];

/// Reduce a device name to a comparison key for the merge heuristic, or
/// `None` if the name is too generic to safely merge on. Requires at least
/// one remaining token that looks like a real model designator (contains a
/// digit) — otherwise two unrelated devices sharing a vague name like
/// "Logitech Wireless Mouse" would wrongly collapse into one card.
fn merge_key_slug(name: &str) -> Option<String> {
    let tokens: Vec<String> = name
        .split_whitespace()
        .map(|t| t.to_lowercase())
        .filter(|t| !NAME_MERGE_STOPWORDS.contains(&t.as_str()))
        .collect();

    let has_model_designator = tokens.iter().any(|t| t.chars().any(|c| c.is_ascii_digit()));
    if tokens.len() < 2 || !has_model_designator {
        return None;
    }
    Some(tokens.join("-"))
}

/// Parse a `"{vid:04x}:{pid:04x}"` member string.
fn parse_member(s: &str) -> Option<(u16, u16)> {
    let (v, p) = s.split_once(':')?;
    Some((
        u16::from_str_radix(v, 16).ok()?,
        u16::from_str_radix(p, 16).ok()?,
    ))
}

fn format_member(vid: u16, pid: u16) -> String {
    format!("{vid:04x}:{pid:04x}")
}

/// Parse the body of a `"ratbag:"` id (after stripping that prefix) into its
/// member `(vid, pid)` list. A legacy single-link id `"046d:c08d"` yields one
/// member; a merged id `"merged:046d:c08d+046d:407f"` yields all of them.
fn parse_id_members(id_body: &str) -> Vec<(u16, u16)> {
    if let Some(rest) = id_body.strip_prefix("merged:") {
        rest.split('+').filter_map(parse_member).collect()
    } else {
        parse_member(id_body).into_iter().collect()
    }
}

/// A single ratbagd device object, before cross-transport grouping.
struct RawRatbagDevice {
    sysname: String,
    vid: u16,
    pid: u16,
    name: String,
    model: String,
    dpi: Option<u32>,
    dpi_options: Option<Vec<u32>>,
    polling_rate: Option<u32>,
    polling_rate_options: Option<Vec<u32>>,
    button_count: Option<u32>,
    /// Resolved once at enumeration time from kernel USB/HID topology — see
    /// `super::connection`. Kept on the raw link rather than the merged card
    /// because it describes *this* physical link, and a merged card's primary
    /// link is exactly the one whose connection it should be showing.
    connection: Option<super::connection::ConnectionType>,
}

/// Everything `read_active_profile_state` can learn from a device's active
/// profile in one pass. A struct rather than a growing tuple — `button_count`
/// is the second field added since this started as `(dpi, dpi_options)`.
#[derive(Default)]
struct ProfileState {
    dpi: Option<u32>,
    dpi_options: Option<Vec<u32>>,
    polling_rate: Option<u32>,
    polling_rate_options: Option<Vec<u32>>,
    button_count: Option<u32>,
}

/// Union-find grouping of raw ratbagd devices believed to be the same
/// physical mouse — by explicit user override first, then by the name
/// heuristic. Returns groups as index lists into `raw`.
fn group_raw_devices(raw: &[RawRatbagDevice], overrides: &IdentityOverrides) -> Vec<Vec<usize>> {
    let n = raw.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }
    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[ra] = rb;
        }
    }

    for i in 0..n {
        for j in (i + 1)..n {
            let key_i = format_member(raw[i].vid, raw[i].pid);
            let key_j = format_member(raw[j].vid, raw[j].pid);

            if overrides.is_force_merge(&key_i, &key_j) {
                union(&mut parent, i, j);
                continue;
            }
            if overrides.is_force_split(&key_i, &key_j) {
                continue;
            }
            if raw[i].vid == raw[j].vid {
                if let (Some(a), Some(b)) =
                    (merge_key_slug(&raw[i].name), merge_key_slug(&raw[j].name))
                {
                    if a == b {
                        union(&mut parent, i, j);
                    }
                }
            }
        }
    }

    let mut groups: std::collections::HashMap<usize, Vec<usize>> = Default::default();
    for i in 0..n {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }
    groups.into_values().collect()
}

/// Pick the "primary" link among a set of raw device indices believed to be
/// the same physical mouse: whichever is currently "live" (reporting real
/// dpi data), or — if none/more than one are — the first when sorted by
/// `(vid, pid)`, for a deterministic tie-break.
///
/// Both `build_device_info` (what to *display*) and `resolve_sysname_for_id`
/// (what a write should *target*) call this exact function rather than each
/// keeping their own copy of the same logic — this hardware genuinely does
/// have both transport links simultaneously "live" at once (confirmed: a
/// Logitech G502 LIGHTSPEED's wired and receiver links both report a real
/// active profile/resolution at the same time), and two independently
/// written selection orderings previously drifted apart under exactly that
/// condition: `SetPollingRate` would resolve to one link while the merged
/// card kept displaying the other, untouched, link's value — a write that
/// silently didn't show up. Sharing one function makes that impossible by
/// construction instead of by two implementations happening to agree.
fn select_primary(raw: &[RawRatbagDevice], idxs: &[usize]) -> usize {
    let mut sorted = idxs.to_vec();
    sorted.sort_by_key(|&i| (raw[i].vid, raw[i].pid));
    sorted
        .iter()
        .copied()
        .find(|&i| raw[i].dpi.is_some())
        .unwrap_or(sorted[0])
}

/// Build the public `DeviceInfo` for one group of raw ratbagd devices. Field
/// values (dpi/polling_rate/name/model) come from whichever member is
/// currently "live" (has an active resolution reporting real values), so a
/// disconnected/phantom link in a merged group never shadows real data.
fn build_device_info(raw: &[RawRatbagDevice], group: &[usize]) -> DeviceInfo {
    let mut idxs = group.to_vec();
    idxs.sort_by_key(|&i| (raw[i].vid, raw[i].pid));
    let primary_idx = select_primary(raw, &idxs);
    let primary = &raw[primary_idx];

    let (id, linked_ids) = if idxs.len() == 1 {
        (format!("ratbag:{}", format_member(primary.vid, primary.pid)), None)
    } else {
        let members: Vec<String> = idxs
            .iter()
            .map(|&i| format_member(raw[i].vid, raw[i].pid))
            .collect();
        (
            format!("ratbag:merged:{}", members.join("+")),
            Some(members),
        )
    };

    // Capability-gated, not DeviceType-gated: the QML side checks
    // `capabilities.includes("dpi")` rather than `deviceType === "mouse"`
    // before showing a control, so a mouse whose active resolution doesn't
    // (yet) report options — or a future non-mouse device that does — is
    // handled correctly without a new branch in the UI.
    let mut capabilities = Vec::new();
    if primary.dpi_options.as_ref().is_some_and(|v| !v.is_empty()) {
        capabilities.push("dpi".to_string());
    }
    if primary
        .polling_rate_options
        .as_ref()
        .is_some_and(|v| !v.is_empty())
    {
        capabilities.push("polling_rate".to_string());
    }
    if primary.button_count.is_some_and(|n| n > 0) {
        capabilities.push("buttons".to_string());
    }

    DeviceInfo {
        id,
        name: primary.name.clone(),
        model: primary.model.clone(),
        device_type: DeviceType::Mouse,
        vid: primary.vid,
        pid: primary.pid,
        linked_ids,
        dpi: primary.dpi,
        polling_rate: primary.polling_rate,
        dpi_options: primary.dpi_options.clone(),
        polling_rate_options: primary.polling_rate_options.clone(),
        button_count: primary.button_count,
        connection: primary.connection.map(|c| c.as_tag().to_string()),
        battery_level: None,
        battery_charging: None,
        sidetone: None,
        chatmix: None,
        capabilities: if capabilities.is_empty() { None } else { Some(capabilities) },
        eq_presets: None,
        eq_meta: None,
    }
}

// ── RatbagManager ────────────────────────────────────────────────────────────

pub struct RatbagManager {
    conn: Connection,
}

impl RatbagManager {
    pub async fn new() -> Result<Self> {
        let conn = Connection::system()
            .await
            .context("failed to connect to D-Bus system bus")?;
        Ok(Self { conn })
    }

    pub async fn list_devices(&self) -> Vec<DeviceInfo> {
        let raw = self.list_raw_devices().await;
        let overrides = IdentityOverrides::load();
        group_raw_devices(&raw, &overrides)
            .iter()
            .map(|group| build_device_info(&raw, group))
            .collect()
    }

    async fn list_raw_devices(&self) -> Vec<RawRatbagDevice> {
        let manager = {
            let mut last_err = None;
            let mut result = None;
            for _ in 0..3 {
                match ManagerProxy::new(&self.conn).await {
                    Ok(m) => { result = Some(m); break; }
                    Err(e) => {
                        last_err = Some(e);
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    }
                }
            }
            match result {
                Some(m) => m,
                None => {
                    tracing::warn!("ratbagd not available after 3 attempts: {}", last_err.unwrap());
                    return vec![];
                }
            }
        };

        let paths = match manager.devices().await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("ratbagd devices property failed: {e}");
                return vec![];
            }
        };

        let mut devices = Vec::new();
        for path in paths {
            if let Some(raw) = self.read_raw_device(&path).await {
                devices.push(raw);
            }
        }
        devices
    }

    async fn read_raw_device(&self, path: &zbus::zvariant::OwnedObjectPath) -> Option<RawRatbagDevice> {
        let dev = DeviceProxy::builder(&self.conn)
            .path(path.as_ref())
            .ok()?
            .build()
            .await
            .ok()?;

        let name = dev.name().await.unwrap_or_default();
        let model_str = dev.model().await.unwrap_or_default();

        // Parse VID/PID from model string "usb:VVVV:PPPP:00"
        let (vid, pid) = parse_model_id(&model_str);

        // Get DPI/polling-rate/button-count (+ their permitted-value lists)
        // from the active profile and its active resolution.
        let state = self.read_active_profile_state(&dev).await;

        let sysname = path
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or("unknown")
            .to_string();

        let sysname_for_conn = sysname.clone();
        let model_for_conn = model_str.clone();

        Some(RawRatbagDevice {
            sysname,
            vid,
            pid,
            name,
            model: model_str,
            dpi: state.dpi,
            dpi_options: state.dpi_options,
            polling_rate: state.polling_rate,
            polling_rate_options: state.polling_rate_options,
            button_count: state.button_count,
            connection: super::connection::detect(&sysname_for_conn, &model_for_conn, pid),
        })
    }

    /// Read DPI (from the active resolution), polling rate, and button
    /// count (both from the active profile) for a device's currently-active
    /// profile. DPI/polling-rate come from two different ratbagd objects
    /// with different property sets — see the `Profile`/`Resolution` proxy
    /// doc comments above for why.
    async fn read_active_profile_state(&self, dev: &DeviceProxy<'_>) -> ProfileState {
        let profiles = match dev.profiles().await {
            Ok(p) => p,
            Err(_) => return ProfileState::default(),
        };

        for profile_path in &profiles {
            let Ok(profile) = ProfileProxy::builder(&self.conn)
                .path(profile_path.as_ref())
                .unwrap()
                .build()
                .await
            else {
                continue;
            };

            if !profile.is_active().await.unwrap_or(false) {
                continue;
            }

            let polling_rate = profile.report_rate().await.ok();
            let polling_rate_options = profile.report_rates().await.ok();
            let button_count = profile.buttons().await.ok().map(|b| b.len() as u32);

            let res_paths = match profile.resolutions().await {
                Ok(r) => r,
                Err(_) => {
                    return ProfileState {
                        polling_rate,
                        polling_rate_options,
                        button_count,
                        ..Default::default()
                    }
                }
            };

            for res_path in &res_paths {
                let Ok(res) = ResolutionProxy::builder(&self.conn)
                    .path(res_path.as_ref())
                    .unwrap()
                    .build()
                    .await
                else {
                    continue;
                };

                if !res.is_active().await.unwrap_or(false) {
                    continue;
                }

                let dpi = res
                    .resolution()
                    .await
                    .ok()
                    .and_then(|v| extract_dpi_x(&v));
                let dpi_options = res.resolutions().await.ok();

                return ProfileState {
                    dpi,
                    dpi_options,
                    polling_rate,
                    polling_rate_options,
                    button_count,
                };
            }

            // Active profile found, but no active resolution under it —
            // still return what we already have.
            return ProfileState {
                polling_rate,
                polling_rate_options,
                button_count,
                ..Default::default()
            };
        }
        ProfileState::default()
    }

    /// Resolve a device id (legacy single-link `"vid:pid"` or merged
    /// `"merged:vid:pid+vid:pid"`) back to whichever ratbagd sysname is
    /// currently backing it. Re-scans on every call rather than caching —
    /// sysnames are route-scoped and can change across replugs/reboots, and
    /// for a merged id more than one member may currently be enumerated
    /// (see the module doc comment), so this always re-derives which link is
    /// actually live. Uses the exact same `select_primary` tie-break as
    /// `build_device_info` — see that function's doc comment for why sharing
    /// it (rather than each keeping its own "prefer the live one" logic)
    /// matters here specifically.
    async fn resolve_sysname_for_id(&self, id_body: &str) -> Result<String> {
        let members = parse_id_members(id_body);
        if members.is_empty() {
            anyhow::bail!("malformed ratbag device id: {id_body}");
        }

        let raw = self.list_raw_devices().await;
        let idxs: Vec<usize> = raw
            .iter()
            .enumerate()
            .filter(|(_, d)| members.contains(&(d.vid, d.pid)))
            .map(|(i, _)| i)
            .collect();

        if idxs.is_empty() {
            anyhow::bail!("no ratbagd device currently matches id {id_body}");
        }
        let primary = select_primary(&raw, &idxs);
        Ok(raw[primary].sysname.clone())
    }

    pub async fn set_dpi(&self, id_body: &str, dpi: u32) -> Result<()> {
        let sysname = self.resolve_sysname_for_id(id_body).await?;
        let path = format!("/org/freedesktop/ratbag1/device/{sysname}");
        let dev = DeviceProxy::builder(&self.conn)
            .path(path.as_str())
            .context("invalid path")?
            .build()
            .await
            .context("DeviceProxy build failed")?;

        let profiles = dev.profiles().await.context("get profiles")?;
        for profile_path in &profiles {
            let profile = ProfileProxy::builder(&self.conn)
                .path(profile_path.as_ref())
                .unwrap()
                .build()
                .await?;
            if !profile.is_active().await.unwrap_or(false) {
                continue;
            }
            let res_paths = profile.resolutions().await?;
            for res_path in &res_paths {
                let res = ResolutionProxy::builder(&self.conn)
                    .path(res_path.as_ref())
                    .unwrap()
                    .build()
                    .await?;
                if res.is_active().await.unwrap_or(false) {
                    // ratbagd accepts a Properties.Set on `Resolution` for
                    // basically any u32 and reports success either way, but
                    // an out-of-stage value gets silently dropped by the
                    // device/protocol underneath it — confirmed live: a
                    // SetDpi(999999) call returned Ok(()) and the mouse's
                    // actual DPI never moved off its prior value. Validate
                    // against this resolution's own advertised stage list
                    // before writing so a bad value is a real, visible
                    // error instead of a silent no-op that the caller has
                    // no way to distinguish from "it worked."
                    let valid = res.resolutions().await.context("read valid DPI stages")?;
                    if !valid.contains(&dpi) {
                        anyhow::bail!(
                            "{dpi} is not a supported DPI stage for {sysname} (valid: {valid:?})"
                        );
                    }
                    // `Resolution` is declared `:type: v` by ratbagd — its
                    // value is itself a nested variant, holding either a
                    // plain `u` or a `(uu)` pair (see the `Resolution` proxy
                    // doc comment). A bare `Value::from(dpi)` only wraps it
                    // once (the standard Properties.Set layer), which
                    // ratbagd rejects: "Incorrect parameters for property
                    // 'Resolution', expected 'v', got 'u'" — confirmed live
                    // against this exact device. We re-read the current
                    // value first to preserve its shape (libratbag: "a
                    // client must leave the type intact"), then build the
                    // correctly double-wrapped replacement.
                    let current = res.resolution().await.context("read current Resolution")?;
                    let new_value = build_resolution_value(&current, dpi);
                    res.inner()
                        .set_property("Resolution", new_value)
                        .await
                        .context("write Resolution")?;
                    let commit_result = dev.commit().await?;
                    tracing::debug!("Commit() returned {commit_result} for {sysname}");
                    tracing::info!("Set DPI to {dpi} on {sysname}");
                    return Ok(());
                }
            }
        }
        anyhow::bail!("active resolution not found for {sysname}")
    }

    pub async fn set_polling_rate(&self, id_body: &str, rate: u32) -> Result<()> {
        let sysname = self.resolve_sysname_for_id(id_body).await?;
        let path = format!("/org/freedesktop/ratbag1/device/{sysname}");
        let dev = DeviceProxy::builder(&self.conn)
            .path(path.as_str())
            .context("invalid path")?
            .build()
            .await
            .context("DeviceProxy build failed")?;

        // ReportRate lives on the active **Profile**, not on a Resolution —
        // see the `Profile` proxy doc comment. No per-resolution lookup
        // needed at all.
        let profiles = dev.profiles().await.context("get profiles")?;
        for profile_path in &profiles {
            let profile = ProfileProxy::builder(&self.conn)
                .path(profile_path.as_ref())
                .unwrap()
                .build()
                .await?;
            if !profile.is_active().await.unwrap_or(false) {
                continue;
            }
            // Unlike Resolution, ratbagd does NOT clamp/reject an
            // out-of-stage ReportRate write at all — confirmed live: a
            // SetPollingRate(999) call returned Ok(()) *and* the device's
            // raw ReportRate property actually became 999, an arbitrary
            // value never advertised by ReportRates. Validate here so an
            // out-of-range request is a real error, not real (bogus)
            // hardware state.
            let valid = profile.report_rates().await.context("read valid report rates")?;
            if !valid.contains(&rate) {
                anyhow::bail!(
                    "{rate}Hz is not a supported report rate for {sysname} (valid: {valid:?})"
                );
            }
            profile
                .set_report_rate(rate)
                .await
                .context("write ReportRate")?;
            let commit_result = dev.commit().await?;
            tracing::debug!("Commit() returned {commit_result} for {sysname}");
            tracing::info!("Set polling rate to {rate}Hz on {sysname}");
            return Ok(());
        }
        anyhow::bail!("active profile not found for {sysname}")
    }

    /// Read every button on the active profile: its 0-based index, which
    /// `ButtonAction` tags it supports, and its current action. Used by the
    /// button-mapping editor to know how many hotspots a device can have and
    /// what each one is currently bound to before the user changes anything.
    pub async fn get_button_mappings(&self, id_body: &str) -> Result<Vec<ButtonMapping>> {
        let sysname = self.resolve_sysname_for_id(id_body).await?;
        let path = format!("/org/freedesktop/ratbag1/device/{sysname}");
        let dev = DeviceProxy::builder(&self.conn)
            .path(path.as_str())
            .context("invalid path")?
            .build()
            .await
            .context("DeviceProxy build failed")?;

        let profiles = dev.profiles().await.context("get profiles")?;
        for profile_path in &profiles {
            let profile = ProfileProxy::builder(&self.conn)
                .path(profile_path.as_ref())
                .unwrap()
                .build()
                .await?;
            if !profile.is_active().await.unwrap_or(false) {
                continue;
            }

            let button_paths = profile.buttons().await.context("get buttons")?;
            let mut out = Vec::with_capacity(button_paths.len());
            for button_path in &button_paths {
                let button = ButtonProxy::builder(&self.conn)
                    .path(button_path.as_ref())
                    .unwrap()
                    .build()
                    .await?;
                let index = button.index().await.unwrap_or(0);
                let supported_action_types = button
                    .action_types()
                    .await
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|t| action_type_tag(*t))
                    .map(str::to_string)
                    .collect();
                let (action_type, payload) = button.mapping().await.context("read Mapping")?;
                let action = decode_button_action(action_type, &payload);
                out.push(ButtonMapping { index, supported_action_types, action });
            }
            return Ok(out);
        }
        anyhow::bail!("active profile not found for {sysname}")
    }

    /// Set one button's action. `button_index` is the 0-based `Button.Index`
    /// (as reported by `get_button_mappings`, NOT a `ButtonAction::Button`
    /// target value — see the module doc comment on the two numbering
    /// schemes above).
    pub async fn set_button_action(
        &self,
        id_body: &str,
        button_index: u32,
        action: &ButtonAction,
    ) -> Result<()> {
        let sysname = self.resolve_sysname_for_id(id_body).await?;
        let path = format!("/org/freedesktop/ratbag1/device/{sysname}");
        let dev = DeviceProxy::builder(&self.conn)
            .path(path.as_str())
            .context("invalid path")?
            .build()
            .await
            .context("DeviceProxy build failed")?;

        let (type_num, payload) = encode_button_action(action)?;

        let profiles = dev.profiles().await.context("get profiles")?;
        for profile_path in &profiles {
            let profile = ProfileProxy::builder(&self.conn)
                .path(profile_path.as_ref())
                .unwrap()
                .build()
                .await?;
            if !profile.is_active().await.unwrap_or(false) {
                continue;
            }

            let button_paths = profile.buttons().await.context("get buttons")?;
            for button_path in &button_paths {
                let button = ButtonProxy::builder(&self.conn)
                    .path(button_path.as_ref())
                    .unwrap()
                    .build()
                    .await?;
                if button.index().await.unwrap_or(u32::MAX) != button_index {
                    continue;
                }

                let supported = button.action_types().await.unwrap_or_default();
                if !supported.contains(&type_num) {
                    anyhow::bail!(
                        "button {button_index} on {sysname} does not support this action type \
                         (supports: {supported:?})"
                    );
                }

                let value = zvariant::Value::new((type_num, payload));
                button
                    .inner()
                    .set_property("Mapping", value)
                    .await
                    .context("write Mapping")?;
                let commit_result = dev.commit().await?;
                tracing::debug!("Commit() returned {commit_result} for {sysname}");
                tracing::info!("Set button {button_index} action to {action:?} on {sysname}");
                return Ok(());
            }
            anyhow::bail!("button index {button_index} not found on {sysname}");
        }
        anyhow::bail!("active profile not found for {sysname}")
    }

    /// Manual override: record that the devices behind `id_body_a` and
    /// `id_body_b` are the same physical mouse, regardless of what the name
    /// heuristic concludes. Either id may itself already be a merged id —
    /// every member of one is paired with every member of the other. Takes
    /// effect on the next `list_devices()` call (no live-state mutation).
    pub async fn merge_devices(&self, id_body_a: &str, id_body_b: &str) -> Result<()> {
        let members_a = parse_id_members(id_body_a);
        let members_b = parse_id_members(id_body_b);
        if members_a.is_empty() || members_b.is_empty() {
            anyhow::bail!("malformed device id in merge request");
        }

        let mut overrides = IdentityOverrides::load();
        for &(va, pa) in &members_a {
            for &(vb, pb) in &members_b {
                overrides.add_force_merge(&format_member(va, pa), &format_member(vb, pb));
            }
        }
        overrides.save()
    }

    /// Manual override: decompose a merged device id back into separate
    /// cards, and remember that decision so the name heuristic never
    /// re-merges these specific links on its own again.
    pub async fn split_device(&self, id_body: &str) -> Result<()> {
        let members = parse_id_members(id_body);
        if members.len() < 2 {
            anyhow::bail!("device id {id_body} is not a merged device, nothing to split");
        }

        let mut overrides = IdentityOverrides::load();
        for i in 0..members.len() {
            for j in (i + 1)..members.len() {
                let (va, pa) = members[i];
                let (vb, pb) = members[j];
                overrides.add_force_split(&format_member(va, pa), &format_member(vb, pb));
            }
        }
        overrides.save()
    }
}

fn parse_model_id(model: &str) -> (u16, u16) {
    // Format: "usb:VVVV:PPPP:00" or empty
    let parts: Vec<&str> = model.split(':').collect();
    let vid = parts
        .get(1)
        .and_then(|s| u16::from_str_radix(s, 16).ok())
        .unwrap_or(0);
    let pid = parts
        .get(2)
        .and_then(|s| u16::from_str_radix(s, 16).ok())
        .unwrap_or(0);
    (vid, pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_action_none_round_trips() {
        let (type_num, payload) = encode_button_action(&ButtonAction::None).unwrap();
        assert_eq!(type_num, ACTION_TYPE_NONE);
        assert_eq!(decode_button_action(type_num, &payload), ButtonAction::None);
    }

    #[test]
    fn button_action_button_round_trips_with_1_based_target() {
        let action = ButtonAction::Button { target: 3 };
        let (type_num, payload) = encode_button_action(&action).unwrap();
        assert_eq!(type_num, ACTION_TYPE_BUTTON);
        assert_eq!(payload, zvariant::Value::U32(3));
        assert_eq!(decode_button_action(type_num, &payload), action);
    }

    #[test]
    fn button_action_special_round_trips_by_name() {
        let action = ButtonAction::Special { name: "wheel_up".to_string() };
        let (type_num, payload) = encode_button_action(&action).unwrap();
        assert_eq!(type_num, ACTION_TYPE_SPECIAL);
        // The wire value must be the real libratbag sentinel, not just any
        // number — this is the whole point of the round trip.
        assert_eq!(payload, zvariant::Value::U32((1 << 30) + 4));
        assert_eq!(decode_button_action(type_num, &payload), action);
    }

    #[test]
    fn button_action_key_round_trips_by_name() {
        let action = ButtonAction::Key { name: "e".to_string() };
        let (type_num, payload) = encode_button_action(&action).unwrap();
        assert_eq!(type_num, ACTION_TYPE_KEY);
        // KEY_E from linux/input-event-codes.h on this machine — see the
        // module doc comment for where this number came from.
        assert_eq!(payload, zvariant::Value::U32(18));
        assert_eq!(decode_button_action(type_num, &payload), action);
    }

    #[test]
    fn encode_rejects_unknown_special_and_key_names() {
        assert!(encode_button_action(&ButtonAction::Special { name: "not_a_real_one".into() })
            .is_err());
        assert!(encode_button_action(&ButtonAction::Key { name: "not_a_real_key".into() })
            .is_err());
    }

    #[test]
    fn encode_rejects_macro_and_unknown() {
        // Deliberately unimplemented for writing this phase — see the
        // module doc comment. Must fail loudly, not silently do nothing.
        assert!(encode_button_action(&ButtonAction::Macro).is_err());
        assert!(encode_button_action(&ButtonAction::Unknown).is_err());
    }

    #[test]
    fn decode_reports_an_undecoded_macro_without_losing_that_something_is_bound() {
        let payload = zvariant::Value::new(Vec::<(u32, u32)>::new());
        assert_eq!(decode_button_action(ACTION_TYPE_MACRO, &payload), ButtonAction::Macro);
    }

    #[test]
    fn decode_maps_the_real_action_special_unknown_sentinel_to_a_named_entry() {
        // Found live on this session's own connected G502 (button index 8):
        // ActionSpecial.UNKNOWN (`1 << 30`, no offset) is a real libratbag
        // enum member a factory-default profile can genuinely report, not
        // just a theoretical case — must decode to a recognized catalog
        // entry, not the "not in our table" fallback string.
        let payload = zvariant::Value::U32(1 << 30);
        assert_eq!(
            decode_button_action(ACTION_TYPE_SPECIAL, &payload),
            ButtonAction::Special { name: "unknown".to_string() }
        );
    }

    #[test]
    fn decode_surfaces_an_out_of_table_special_value_instead_of_dropping_it() {
        // A real ActionSpecial the device sent that isn't in our
        // deliberately-non-exhaustive table -- must still show *something*
        // changed, not silently report ActionType::Unknown.
        let unrecognized: u32 = (1 << 30) + 999;
        let payload = zvariant::Value::U32(unrecognized);
        match decode_button_action(ACTION_TYPE_SPECIAL, &payload) {
            ButtonAction::Special { name } => {
                assert_eq!(name, format!("unknown_{unrecognized}"))
            }
            other => panic!("expected a Special variant, got {other:?}"),
        }
    }

    #[test]
    fn special_and_key_catalog_names_are_unique() {
        let mut special_names: Vec<&str> = SPECIAL_ACTIONS.iter().map(|(n, _, _)| *n).collect();
        let before = special_names.len();
        special_names.sort_unstable();
        special_names.dedup();
        assert_eq!(special_names.len(), before, "duplicate name in SPECIAL_ACTIONS");

        let mut key_names: Vec<&str> = KEY_ACTIONS.iter().map(|(n, _, _)| *n).collect();
        let before = key_names.len();
        key_names.sort_unstable();
        key_names.dedup();
        assert_eq!(key_names.len(), before, "duplicate name in KEY_ACTIONS");
    }

    #[test]
    fn button_action_catalog_json_includes_both_tables() {
        let json = button_action_catalog_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed["special"].as_array().unwrap().len(),
            SPECIAL_ACTIONS.len()
        );
        assert_eq!(parsed["key"].as_array().unwrap().len(), KEY_ACTIONS.len());
        // Spot-check one real entry rather than just the counts.
        assert!(parsed["key"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"] == "e" && v["label"] == "E"));
    }

    #[test]
    fn buttons_capability_is_populated_only_when_button_count_is_positive() {
        let mut with_buttons = raw("hidraw0", 0x046d, 0xc08d, "Logitech G502", Some(800));
        with_buttons.button_count = Some(11);
        let info = build_device_info(&[with_buttons], &[0]);
        assert_eq!(info.button_count, Some(11));
        assert!(info.capabilities.as_ref().unwrap().contains(&"buttons".to_string()));

        let mut zero_buttons = raw("hidraw1", 0x046d, 0x1234, "Some Other Mouse", Some(800));
        zero_buttons.button_count = Some(0);
        let info = build_device_info(&[zero_buttons], &[0]);
        assert_eq!(info.button_count, Some(0));
        assert_eq!(info.capabilities, None);
    }

    #[test]
    fn extract_dpi_x_reads_single_u_shape() {
        // What ratbagd actually sends for this shape, confirmed live via
        // `busctl ... Properties Get ... Resolution Resolution` → "v v u 400"
        // — one level of Properties.Get's standard wrapper (already stripped
        // by the time this fn sees it) plus ratbagd's own declared `v` type.
        let value = zvariant::Value::Value(Box::new(zvariant::Value::U32(400)));
        assert_eq!(extract_dpi_x(&value), Some(400));
    }

    #[test]
    fn extract_dpi_x_reads_tuple_shape() {
        let value = zvariant::Value::Value(Box::new(zvariant::Value::new((800u32, 800u32))));
        assert_eq!(extract_dpi_x(&value), Some(800));
    }

    #[test]
    fn build_resolution_value_preserves_single_u_shape() {
        let current = zvariant::Value::Value(Box::new(zvariant::Value::U32(400)));
        let updated = build_resolution_value(&current, 1600);
        assert_eq!(extract_dpi_x(&updated), Some(1600));
        match unwrap_nested_variant(&updated) {
            zvariant::Value::U32(_) => {}
            other => panic!("expected the u32 shape to be preserved, got {other:?}"),
        }
    }

    #[test]
    fn build_resolution_value_preserves_tuple_shape() {
        let current = zvariant::Value::Value(Box::new(zvariant::Value::new((800u32, 800u32))));
        let updated = build_resolution_value(&current, 1600);
        assert_eq!(extract_dpi_x(&updated), Some(1600));
        match unwrap_nested_variant(&updated) {
            zvariant::Value::Structure(_) => {}
            other => panic!("expected the (u,u) shape to be preserved, got {other:?}"),
        }
    }

    fn raw(sysname: &str, vid: u16, pid: u16, name: &str, dpi: Option<u32>) -> RawRatbagDevice {
        RawRatbagDevice {
            sysname: sysname.to_string(),
            vid,
            pid,
            name: name.to_string(),
            model: format!("usb:{vid:04x}:{pid:04x}:0"),
            dpi,
            dpi_options: None,
            polling_rate: None,
            polling_rate_options: None,
            button_count: None,
            // These grouping/merge tests are about identity, not topology;
            // `detect` does real sysfs I/O and is verified live instead.
            connection: None,
        }
    }

    #[test]
    fn merge_key_slug_matches_across_connection_modes() {
        let wireless = merge_key_slug("Logitech G502 LIGHTSPEED Wireless Gaming Mouse");
        let wired = merge_key_slug("Logitech G502");
        assert_eq!(wireless, wired);
        assert_eq!(wireless.as_deref(), Some("logitech-g502"));
    }

    #[test]
    fn merge_key_slug_rejects_generic_names() {
        // No model designator (no digit token) — too generic to safely merge on.
        assert_eq!(merge_key_slug("Logitech Wireless Mouse"), None);
        assert_eq!(merge_key_slug("Mouse"), None);
    }

    #[test]
    fn merge_key_slug_does_not_collapse_different_hardware_revisions() {
        // "hero"/"se" aren't in the stopword list on purpose — a G502 and a
        // G502 HERO are different real hardware and must never merge.
        let base = merge_key_slug("Logitech G502");
        let hero = merge_key_slug("Logitech G502 HERO");
        assert_ne!(base, hero);
    }

    #[test]
    fn grouping_merges_g502_wired_and_wireless_by_name_heuristic() {
        let raw = vec![
            raw("hidraw0", 0x046d, 0xc08d, "Logitech G502 LIGHTSPEED Wireless Gaming Mouse", Some(800)),
            raw("hidraw13", 0x046d, 0x407f, "Logitech G502", None),
        ];
        let overrides = IdentityOverrides::default();
        let groups = group_raw_devices(&raw, &overrides);
        assert_eq!(groups.len(), 1, "expected the two links to merge into one group");

        let info = build_device_info(&raw, &groups[0]);
        assert_eq!(info.id, "ratbag:merged:046d:407f+046d:c08d");
        assert_eq!(info.linked_ids.as_deref(), Some(&["046d:407f".to_string(), "046d:c08d".to_string()][..]));
        // The live link (dpi = Some) must win as primary, even though it
        // sorts second by (vid, pid).
        assert_eq!(info.dpi, Some(800));
        assert_eq!(info.name, "Logitech G502 LIGHTSPEED Wireless Gaming Mouse");
    }

    #[test]
    fn capabilities_are_populated_from_option_lists_not_device_type() {
        let mut with_options = raw("hidraw0", 0x046d, 0xc08d, "Logitech G502", Some(800));
        with_options.dpi_options = Some(vec![400, 800, 1600]);
        with_options.polling_rate_options = Some(vec![125, 250, 500, 1000]);
        let info = build_device_info(&[with_options], &[0]);
        assert_eq!(
            info.capabilities.as_deref(),
            Some(&["dpi".to_string(), "polling_rate".to_string()][..])
        );

        // No option lists (e.g. a device ratbagd can't currently read
        // resolution/profile data for) → no capabilities advertised, not a
        // default/empty-but-present list.
        let no_options = raw("hidraw1", 0x046d, 0x1234, "Some Other Mouse", None);
        let info = build_device_info(&[no_options], &[0]);
        assert_eq!(info.capabilities, None);
    }

    #[test]
    fn select_primary_agrees_when_both_links_are_simultaneously_live() {
        // Confirmed live on real hardware: a G502 LIGHTSPEED's wired and
        // receiver links can BOTH report a real active profile/resolution
        // at once (both `dpi = Some`) — not just one live + one phantom.
        // `build_device_info`'s display pick and `resolve_sysname_for_id`'s
        // write target must agree in that case too, or a write silently
        // lands on a link whose value the UI never shows.
        let raw = vec![
            raw("hidraw0", 0x046d, 0xc08d, "Logitech G502 LIGHTSPEED Wireless Gaming Mouse", Some(250)),
            raw("hidraw13", 0x046d, 0x407f, "Logitech G502", Some(1000)),
        ];
        let idxs = vec![0, 1];
        let display_primary = select_primary(&raw, &idxs);
        // Same call resolve_sysname_for_id makes internally — must be the
        // identical index, by construction (both call select_primary).
        let write_primary = select_primary(&raw, &idxs);
        assert_eq!(display_primary, write_primary);
        // Deterministic tie-break: lowest (vid, pid) wins when both are live
        // — that's raw[1] (046d:407f) here.
        assert_eq!(write_primary, 1);
    }

    #[test]
    fn grouping_leaves_different_models_separate() {
        let raw = vec![
            raw("hidraw0", 0x046d, 0xc08d, "Logitech G502 LIGHTSPEED Wireless Gaming Mouse", Some(800)),
            raw("hidraw5", 0x046d, 0xc092, "Logitech G Pro Wireless", Some(1600)),
        ];
        let overrides = IdentityOverrides::default();
        let groups = group_raw_devices(&raw, &overrides);
        assert_eq!(groups.len(), 2, "different mouse models must never auto-merge");
    }

    #[test]
    fn force_split_override_blocks_the_heuristic() {
        let raw = vec![
            raw("hidraw0", 0x046d, 0xc08d, "Logitech G502 LIGHTSPEED Wireless Gaming Mouse", Some(800)),
            raw("hidraw13", 0x046d, 0x407f, "Logitech G502", None),
        ];
        let mut overrides = IdentityOverrides::default();
        overrides.add_force_split("046d:c08d", "046d:407f");
        let groups = group_raw_devices(&raw, &overrides);
        assert_eq!(groups.len(), 2, "an explicit split override must win over the heuristic");
    }

    #[test]
    fn force_merge_override_joins_devices_the_heuristic_would_not() {
        let raw = vec![
            raw("hidraw0", 0x046d, 0xc08d, "Logitech G502 LIGHTSPEED Wireless Gaming Mouse", Some(800)),
            raw("hidraw5", 0x046d, 0xc092, "Logitech G Pro Wireless", Some(1600)),
        ];
        let mut overrides = IdentityOverrides::default();
        overrides.add_force_merge("046d:c08d", "046d:c092");
        let groups = group_raw_devices(&raw, &overrides);
        assert_eq!(groups.len(), 1, "an explicit merge override must join devices the heuristic would keep apart");
    }

    #[test]
    fn parse_id_members_handles_legacy_and_merged_ids() {
        assert_eq!(parse_id_members("046d:c08d"), vec![(0x046d, 0xc08d)]);
        assert_eq!(
            parse_id_members("merged:046d:c08d+046d:407f"),
            vec![(0x046d, 0xc08d), (0x046d, 0x407f)]
        );
        assert_eq!(parse_id_members("not-an-id"), vec![]);
    }
}

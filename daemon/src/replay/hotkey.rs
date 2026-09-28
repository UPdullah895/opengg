//! Global Hotkey Listener
//!
//! Reads keyboard events straight from `/dev/input/eventN` via evdev, so the
//! bindings fire whether or not the OpenGG window has focus and regardless of
//! X11 vs Wayland — neither offers an unprivileged global-grab API that works
//! everywhere, but the kernel input subsystem is the same on both.
//!
//! Requires membership of the `input` group. `./dev.sh setup` adds you (the
//! project's one privileged step); by hand it is
//! `sudo usermod -aG input $USER`. Either way it takes effect at the next
//! login — a new terminal is not enough.
//!
//! # What this module does *not* do
//!
//! It does not perform the actions. The daemon cannot: live capture is owned
//! by the OpenGG app (`core::gsr`), whose `gpu-screen-recorder` child lives in
//! the app's address space — see the note on [`crate::replay::recorder`]. So a
//! recognised combo is published as the `HotkeyPressed` D-Bus signal and the
//! app, which *can* act, does. That also keeps a hotkey press a no-op rather
//! than an error when the app is not running.

use anyhow::{Context, Result};
use evdev::{Device, EventSummary, KeyCode};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// The actions a global hotkey can trigger.
///
/// Deliberately only the three the daemon has ever declared bindings for
/// (`[replay.shortcuts]` in `daemon.toml`). The app's other shortcuts —
/// undo, split clip, export — are editor-local and belong to the window that
/// has focus, not to a system-wide grab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    SaveReplay,
    ToggleRecording,
    Screenshot,
}

impl HotkeyAction {
    /// The wire name carried by the `HotkeyPressed` signal. Matches the key
    /// used in `ui-settings.json`'s `shortcuts` object so the app can
    /// dispatch on it without a second translation table.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SaveReplay => "saveReplay",
            Self::ToggleRecording => "toggleRecording",
            Self::Screenshot => "screenshot",
        }
    }
}

/// Modifier state, with left and right treated as the same key — a binding
/// written "Alt+F10" must fire from either Alt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Mods {
    /// Fold a set of held keycodes down to the four modifier flags.
    fn from_pressed(pressed: &HashSet<u16>) -> Self {
        let has = |a: KeyCode, b: KeyCode| pressed.contains(&a.0) || pressed.contains(&b.0);
        Self {
            ctrl: has(KeyCode::KEY_LEFTCTRL, KeyCode::KEY_RIGHTCTRL),
            shift: has(KeyCode::KEY_LEFTSHIFT, KeyCode::KEY_RIGHTSHIFT),
            alt: has(KeyCode::KEY_LEFTALT, KeyCode::KEY_RIGHTALT),
            meta: has(KeyCode::KEY_LEFTMETA, KeyCode::KEY_RIGHTMETA),
        }
    }
}

/// A parsed key combination: some modifiers plus exactly one ordinary key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combo {
    pub mods: Mods,
    pub key: u16,
}

/// Map a binding's main-key spelling to a kernel keycode.
///
/// The vocabulary is whatever `ShortcutsPanel.qml`'s `comboFromEvent` can
/// produce: single letters, digits, `F1`–`F24`, and a handful of named keys.
/// Anything else returns `None` so the caller can say which binding it could
/// not honour instead of silently never firing.
fn key_code(name: &str) -> Option<u16> {
    let upper = name.to_ascii_uppercase();

    // F-keys. F1–F10 are contiguous from KEY_F1; F11/F12 sit elsewhere in the
    // table and F13+ start another run, so this cannot be pure arithmetic.
    if let Some(n) = upper.strip_prefix('F').and_then(|d| d.parse::<u8>().ok()) {
        return match n {
            1..=10 => Some(KeyCode::KEY_F1.0 + (n as u16 - 1)),
            11 => Some(KeyCode::KEY_F11.0),
            12 => Some(KeyCode::KEY_F12.0),
            13..=24 => Some(KeyCode::KEY_F13.0 + (n as u16 - 13)),
            _ => None,
        };
    }

    if upper.len() == 1 {
        let c = upper.as_bytes()[0];
        // Letters are not contiguous in the kernel table — it follows the
        // QWERTY rows, not the alphabet — so this needs a literal map.
        const LETTERS: [u16; 26] = [
            30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, // A..M
            49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44, // N..Z
        ];
        if c.is_ascii_uppercase() {
            return Some(LETTERS[(c - b'A') as usize]);
        }
        if c.is_ascii_digit() {
            // KEY_1..KEY_9 run from 2; KEY_0 follows 9 rather than preceding 1.
            return Some(if c == b'0' {
                KeyCode::KEY_0.0
            } else {
                KeyCode::KEY_1.0 + (c - b'1') as u16
            });
        }
    }

    Some(match upper.as_str() {
        "SPACE" => KeyCode::KEY_SPACE.0,
        "ENTER" | "RETURN" => KeyCode::KEY_ENTER.0,
        "TAB" => KeyCode::KEY_TAB.0,
        "ESC" | "ESCAPE" => KeyCode::KEY_ESC.0,
        "BACKSPACE" => KeyCode::KEY_BACKSPACE.0,
        "DELETE" | "DEL" => KeyCode::KEY_DELETE.0,
        "INSERT" | "INS" => KeyCode::KEY_INSERT.0,
        "HOME" => KeyCode::KEY_HOME.0,
        "END" => KeyCode::KEY_END.0,
        "PAGEUP" | "PGUP" => KeyCode::KEY_PAGEUP.0,
        "PAGEDOWN" | "PGDN" => KeyCode::KEY_PAGEDOWN.0,
        "UP" => KeyCode::KEY_UP.0,
        "DOWN" => KeyCode::KEY_DOWN.0,
        "LEFT" => KeyCode::KEY_LEFT.0,
        "RIGHT" => KeyCode::KEY_RIGHT.0,
        "PRINT" | "PRINTSCREEN" | "SYSRQ" => KeyCode::KEY_SYSRQ.0,
        "PAUSE" => KeyCode::KEY_PAUSE.0,
        "-" | "MINUS" => KeyCode::KEY_MINUS.0,
        "=" | "EQUAL" => KeyCode::KEY_EQUAL.0,
        "[" => KeyCode::KEY_LEFTBRACE.0,
        "]" => KeyCode::KEY_RIGHTBRACE.0,
        ";" => KeyCode::KEY_SEMICOLON.0,
        "'" => KeyCode::KEY_APOSTROPHE.0,
        "`" => KeyCode::KEY_GRAVE.0,
        "\\" => KeyCode::KEY_BACKSLASH.0,
        "," => KeyCode::KEY_COMMA.0,
        "." => KeyCode::KEY_DOT.0,
        "/" => KeyCode::KEY_SLASH.0,
        _ => return None,
    })
}

/// Parse a binding such as `"Alt+F10"` or `"Ctrl+Shift+S"`.
///
/// An empty string means "unbound" and yields `None` without complaint; that
/// is how `toggleEarBlast` ships by default.
pub fn parse_combo(spec: &str) -> Option<Combo> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }

    let mut mods = Mods::default();
    let mut parts = spec.split('+').map(str::trim).peekable();
    let mut key = None;

    while let Some(part) = parts.next() {
        // A trailing "+" binding (Ctrl++) leaves an empty final segment; the
        // last non-empty segment is always the main key.
        let is_last = parts.peek().is_none();
        match part.to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" if !is_last => mods.ctrl = true,
            "SHIFT" if !is_last => mods.shift = true,
            "ALT" | "OPTION" if !is_last => mods.alt = true,
            "META" | "SUPER" | "WIN" | "CMD" if !is_last => mods.meta = true,
            _ => key = key_code(part),
        }
    }

    key.map(|key| Combo { mods, key })
}

/// The live binding table. Shared with the D-Bus interface so the app can
/// push edits without restarting the listener.
#[derive(Debug, Clone, Default)]
pub struct Bindings {
    entries: Vec<(Combo, HotkeyAction)>,
}

impl Bindings {
    /// Build from the three raw binding strings, logging any it cannot parse.
    pub fn parse(save_replay: &str, toggle_recording: &str, screenshot: &str) -> Self {
        let mut entries = Vec::new();
        for (spec, action) in [
            (save_replay, HotkeyAction::SaveReplay),
            (toggle_recording, HotkeyAction::ToggleRecording),
            (screenshot, HotkeyAction::Screenshot),
        ] {
            match parse_combo(spec) {
                Some(combo) => entries.push((combo, action)),
                None if spec.trim().is_empty() => {}
                None => tracing::warn!(
                    "Hotkey for {} is set to '{spec}', which this listener cannot \
                     express — it will never fire",
                    action.as_str()
                ),
            }
        }
        Self { entries }
    }

    /// The action bound to `key` given the modifiers currently held, if any.
    ///
    /// Modifiers must match exactly: Ctrl+Alt+F10 must not trigger a plain
    /// Alt+F10 binding, or every Alt binding would also fire from every
    /// superset a user presses.
    pub fn match_press(&self, key: u16, mods: Mods) -> Option<HotkeyAction> {
        self.entries
            .iter()
            .find(|(combo, _)| combo.key == key && combo.mods == mods)
            .map(|(_, action)| *action)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A handle on the running listener, so the D-Bus interface can swap bindings
/// in place when the user edits them in Settings → Shortcuts.
#[derive(Clone)]
pub struct HotkeyHandle {
    bindings: Arc<Mutex<Bindings>>,
    /// How many keyboards are actually being read. Zero means the listener
    /// exists but can never fire, which callers must be able to tell apart
    /// from a working one — otherwise the UI reports global shortcuts as
    /// live on a machine that cannot see a single keypress.
    devices: usize,
}

impl HotkeyHandle {
    pub fn replace(&self, bindings: Bindings) {
        *self.bindings.lock().unwrap() = bindings;
    }

    pub fn is_listening(&self) -> bool {
        self.devices > 0
    }
}

/// Start listening. Returns the receiver actions arrive on, plus a handle for
/// rebinding.
///
/// Missing keyboards or an unreadable `/dev/input` are reported and then
/// tolerated: the daemon's other modules must keep working on a machine where
/// the user is not in the `input` group.
pub fn start_listener(
    bindings: Bindings,
) -> Result<(mpsc::UnboundedReceiver<HotkeyAction>, HotkeyHandle)> {
    let (action_tx, action_rx) = mpsc::unbounded_channel();
    let bindings = Arc::new(Mutex::new(bindings));
    let no_devices = HotkeyHandle {
        bindings: bindings.clone(),
        devices: 0,
    };

    let keyboards = find_keyboards();
    if keyboards.is_empty() {
        tracing::warn!(
            "No readable keyboard devices in /dev/input — global hotkeys are off. \
             Run `./dev.sh setup` to join the 'input' group, then log back in."
        );
        return Ok((action_rx, no_devices));
    }

    // One channel for every device: modifiers and the main key can arrive on
    // different event nodes of the same physical keyboard (many boards expose
    // their media keys as a second node), so the pressed-key set has to be
    // shared rather than per-device.
    let (key_tx, mut key_rx) = mpsc::unbounded_channel::<(u16, i32)>();

    let mut watched = 0usize;
    for (path, device) in keyboards {
        let key_tx = key_tx.clone();
        let stream = match device.into_event_stream() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Hotkeys: cannot stream {}: {e}", path.display());
                continue;
            }
        };
        watched += 1;
        tokio::spawn(read_device(path, stream, key_tx));
    }
    drop(key_tx);

    if watched == 0 {
        tracing::warn!("Hotkeys: found keyboards but could not read any of them");
        return Ok((action_rx, no_devices));
    }

    let matcher_bindings = bindings.clone();
    tokio::spawn(async move {
        let mut pressed: HashSet<u16> = HashSet::new();
        while let Some((code, value)) = key_rx.recv().await {
            match value {
                // 1 = initial press, 2 = autorepeat. Acting on autorepeat
                // would save a burst of replays from one held key.
                1 => {
                    pressed.insert(code);
                    let mods = Mods::from_pressed(&pressed);
                    let hit = matcher_bindings.lock().unwrap().match_press(code, mods);
                    if let Some(action) = hit {
                        tracing::info!("Hotkey: {}", action.as_str());
                        if action_tx.send(action).is_err() {
                            break; // receiver gone — daemon is shutting down
                        }
                    }
                }
                0 => {
                    pressed.remove(&code);
                }
                _ => {}
            }
        }
    });

    Ok((
        action_rx,
        HotkeyHandle {
            bindings,
            devices: watched,
        },
    ))
}

/// Pump one device's events into the shared key channel.
async fn read_device(
    path: PathBuf,
    mut stream: evdev::EventStream,
    tx: mpsc::UnboundedSender<(u16, i32)>,
) {
    loop {
        match stream.next_event().await {
            Ok(event) => {
                if let EventSummary::Key(_, code, value) = event.destructure() {
                    if tx.send((code.0, value)).is_err() {
                        return;
                    }
                }
            }
            Err(e) => {
                // Unplugging a keyboard ends its stream; that is normal and
                // must not take the other devices' tasks down with it.
                tracing::debug!("Hotkeys: {} closed ({e})", path.display());
                return;
            }
        }
    }
}

/// Open every readable keyboard under `/dev/input`.
///
/// "Keyboard" means it reports the letter, space and enter keys. The previous
/// heuristic — a `capabilities/key` sysfs string longer than ten characters —
/// also matched this machine's power button, Xbox controller, USB speaker bar,
/// headset and both mice.
fn find_keyboards() -> Vec<(PathBuf, Device)> {
    evdev::enumerate()
        .filter(|(_, dev)| is_keyboard(dev))
        .inspect(|(path, dev)| {
            tracing::info!(
                "Hotkeys: watching {} ({})",
                dev.name().unwrap_or("unnamed"),
                path.display()
            );
        })
        .collect()
}

fn is_keyboard(dev: &Device) -> bool {
    dev.supported_keys().is_some_and(|keys| {
        keys.contains(KeyCode::KEY_A)
            && keys.contains(KeyCode::KEY_Z)
            && keys.contains(KeyCode::KEY_SPACE)
            && keys.contains(KeyCode::KEY_ENTER)
    })
}

/// Parse the three configured bindings and start the listener, wiring each
/// recognised press to the `HotkeyPressed` D-Bus signal.
///
/// Never fails the daemon: a machine without input access still gets audio,
/// devices and the rest of the replay interface.
pub fn spawn(conn: &zbus::Connection, bindings: Bindings) -> Option<HotkeyHandle> {
    if bindings.is_empty() {
        tracing::info!("No global hotkeys configured");
    }
    let (mut rx, handle) = match start_listener(bindings) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Global hotkeys unavailable: {e}");
            return None;
        }
    };
    if !handle.is_listening() {
        // No point publishing a handle: `SetHotkeys` would accept bindings
        // that can never fire and the app would report them as working.
        return None;
    }

    let conn = conn.clone();
    tokio::spawn(async move {
        while let Some(action) = rx.recv().await {
            if let Err(e) = emit(&conn, action).await {
                tracing::warn!("Hotkey signal failed: {e}");
            }
        }
    });
    Some(handle)
}

async fn emit(conn: &zbus::Connection, action: HotkeyAction) -> Result<()> {
    let ctxt = zbus::object_server::SignalContext::new(conn, super::dbus::REPLAY_PATH)
        .context("signal context")?;
    super::dbus::ReplayInterface::hotkey_pressed(&ctxt, action.as_str()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods(ctrl: bool, shift: bool, alt: bool, meta: bool) -> Mods {
        Mods { ctrl, shift, alt, meta }
    }

    #[test]
    fn parses_the_shipped_defaults() {
        let f10 = parse_combo("Alt+F10").expect("Alt+F10");
        assert_eq!(f10.key, KeyCode::KEY_F10.0);
        assert_eq!(f10.mods, mods(false, false, true, false));

        assert_eq!(parse_combo("Alt+F9").unwrap().key, KeyCode::KEY_F9.0);
        assert_eq!(parse_combo("Alt+F12").unwrap().key, KeyCode::KEY_F12.0);
    }

    /// The panel writes "Ctrl+Shift+Z"; order and case must not matter.
    #[test]
    fn modifier_names_are_case_and_order_insensitive() {
        let a = parse_combo("Ctrl+Shift+Z").unwrap();
        let b = parse_combo("shift+ctrl+z").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.mods, mods(true, true, false, false));
        assert_eq!(parse_combo("Super+S"), parse_combo("Meta+S"));
    }

    /// Letters are not alphabetically ordered in the kernel table, so a naive
    /// `KEY_A + (c - 'A')` would silently bind the wrong physical key.
    #[test]
    fn letters_map_to_their_real_keycodes() {
        assert_eq!(parse_combo("A").unwrap().key, KeyCode::KEY_A.0);
        assert_eq!(parse_combo("Z").unwrap().key, KeyCode::KEY_Z.0);
        assert_eq!(parse_combo("S").unwrap().key, KeyCode::KEY_S.0);
        assert_eq!(parse_combo("M").unwrap().key, KeyCode::KEY_M.0);
        assert_eq!(parse_combo("Q").unwrap().key, KeyCode::KEY_Q.0);
        assert_eq!(parse_combo("P").unwrap().key, KeyCode::KEY_P.0);
    }

    /// KEY_0 sits after KEY_9, not before KEY_1.
    #[test]
    fn digits_map_to_their_real_keycodes() {
        assert_eq!(parse_combo("1").unwrap().key, KeyCode::KEY_1.0);
        assert_eq!(parse_combo("9").unwrap().key, KeyCode::KEY_9.0);
        assert_eq!(parse_combo("0").unwrap().key, KeyCode::KEY_0.0);
    }

    #[test]
    fn f_keys_span_the_two_kernel_runs() {
        assert_eq!(parse_combo("F1").unwrap().key, KeyCode::KEY_F1.0);
        assert_eq!(parse_combo("F11").unwrap().key, KeyCode::KEY_F11.0);
        assert_eq!(parse_combo("F13").unwrap().key, KeyCode::KEY_F13.0);
        assert_eq!(parse_combo("F24").unwrap().key, KeyCode::KEY_F24.0);
        assert!(parse_combo("F25").is_none());
    }

    #[test]
    fn unbound_and_unspellable_bindings_are_rejected() {
        assert!(parse_combo("").is_none());
        assert!(parse_combo("   ").is_none());
        assert!(parse_combo("Alt+").is_none());
        assert!(parse_combo("Ctrl+Nonsense").is_none());
    }

    #[test]
    fn right_hand_modifiers_count_as_the_same_modifier() {
        let mut pressed = HashSet::new();
        pressed.insert(KeyCode::KEY_RIGHTALT.0);
        assert_eq!(Mods::from_pressed(&pressed), mods(false, false, true, false));

        pressed.clear();
        pressed.insert(KeyCode::KEY_LEFTALT.0);
        assert_eq!(Mods::from_pressed(&pressed), mods(false, false, true, false));
    }

    #[test]
    fn matches_the_bound_combo() {
        let b = Bindings::parse("Alt+F10", "Alt+F9", "Alt+F12");
        assert_eq!(
            b.match_press(KeyCode::KEY_F10.0, mods(false, false, true, false)),
            Some(HotkeyAction::SaveReplay)
        );
        assert_eq!(
            b.match_press(KeyCode::KEY_F9.0, mods(false, false, true, false)),
            Some(HotkeyAction::ToggleRecording)
        );
    }

    /// Without exact modifier matching, Ctrl+Alt+F10 (a common desktop VT
    /// switch chord) would also flush the replay buffer.
    #[test]
    fn a_superset_of_modifiers_does_not_match() {
        let b = Bindings::parse("Alt+F10", "", "");
        assert_eq!(
            b.match_press(KeyCode::KEY_F10.0, mods(true, false, true, false)),
            None
        );
        assert_eq!(
            b.match_press(KeyCode::KEY_F10.0, mods(false, false, false, false)),
            None
        );
    }

    #[test]
    fn empty_bindings_are_dropped_not_matched_as_bare_keys() {
        let b = Bindings::parse("", "", "");
        assert!(b.is_empty());
        assert_eq!(b.match_press(KeyCode::KEY_F10.0, Mods::default()), None);
    }

    /// An unparseable binding must not take the parseable ones down with it.
    #[test]
    fn one_bad_binding_does_not_disable_the_others() {
        let b = Bindings::parse("Alt+F10", "Ctrl+Nonsense", "Alt+F12");
        assert_eq!(
            b.match_press(KeyCode::KEY_F10.0, mods(false, false, true, false)),
            Some(HotkeyAction::SaveReplay)
        );
        assert_eq!(
            b.match_press(KeyCode::KEY_F12.0, mods(false, false, true, false)),
            Some(HotkeyAction::Screenshot)
        );
    }

    /// The signal payload is the same string `ui-settings.json` uses, so the
    /// app can dispatch straight on it.
    #[test]
    fn action_names_match_the_settings_keys() {
        assert_eq!(HotkeyAction::SaveReplay.as_str(), "saveReplay");
        assert_eq!(HotkeyAction::ToggleRecording.as_str(), "toggleRecording");
        assert_eq!(HotkeyAction::Screenshot.as_str(), "screenshot");
    }
}

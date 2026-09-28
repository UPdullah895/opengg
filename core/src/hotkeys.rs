//! Global hotkeys: pushing bindings down and receiving presses back.
//!
//! The daemon owns the evdev listener — it is the piece that can read
//! `/dev/input` regardless of X11 vs Wayland and window focus. It cannot
//! *perform* the actions, though: the `gpu-screen-recorder` child belongs to
//! this process (see [`crate::gsr`]). So the split is
//!
//! ```text
//! app  ──SetHotkeys──▶  daemon (evdev)
//! app  ◀─HotkeyPressed─ daemon
//! ```
//!
//! and this module is both halves of that conversation.

use crate::daemon::{call_dbus_void, RP_IFACE, RP_PATH};

/// Push the user's bindings to the daemon's listener.
///
/// Call this on startup and whenever Settings → Shortcuts changes, so the
/// daemon honours the UI rather than the copy in its own `daemon.toml`.
/// `Err` means the daemon has no listener running (usually: this user is not
/// in the `input` group) — worth showing, since the keys will not work.
pub fn push_bindings(
    save_replay: &str,
    toggle_recording: &str,
    screenshot: &str,
) -> Result<(), String> {
    call_dbus_void(
        "SetHotkeys",
        RP_PATH,
        RP_IFACE,
        (save_replay, toggle_recording, screenshot),
    )
}

/// Listen for `HotkeyPressed` and hand each action name to `on_action`.
///
/// Blocks forever, so callers run it on a dedicated thread. Uses its own
/// connection rather than the shared one in [`crate::daemon`]: a match rule
/// makes the bus deliver every matching signal to that connection, and the
/// shared one is used for ordinary method calls whose replies must not queue
/// up behind a stream nobody is draining.
pub fn listen(mut on_action: impl FnMut(String)) -> Result<(), String> {
    let conn = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;

    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(RP_IFACE)
        .map_err(|e| e.to_string())?
        .member("HotkeyPressed")
        .map_err(|e| e.to_string())?
        .path(RP_PATH)
        .map_err(|e| e.to_string())?
        .build();

    let iter = zbus::blocking::MessageIterator::for_match_rule(rule, &conn, None)
        .map_err(|e| e.to_string())?;

    for msg in iter {
        let Ok(msg) = msg else { continue };
        // A malformed or unexpected payload is not worth tearing the listener
        // down for — the next real press should still arrive.
        if let Ok(action) = msg.body().deserialize::<String>() {
            on_action(action);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The match rule has to name the same path and interface the daemon
    /// serves the signal on, or the bus silently delivers nothing and the
    /// hotkeys look broken with no error anywhere.
    #[test]
    fn subscribes_to_the_path_the_daemon_signals_on() {
        assert_eq!(RP_PATH, "/org/opengg/Daemon/Replay");
        assert_eq!(RP_IFACE, "org.opengg.Daemon.Replay");
    }
}

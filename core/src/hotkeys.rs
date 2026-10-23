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
//! app  ◀─MomentDetected─ daemon (auto-clip, see `crate::autoclip`)
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

/// A signal from the daemon's Replay interface that the app acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplaySignal {
    /// `HotkeyPressed`: "saveReplay", "toggleRecording" or "screenshot".
    Hotkey(String),
    /// `MomentDetected`: a JSON moment for [`crate::autoclip::tag_clip`].
    Moment(String),
}

/// Listen for `HotkeyPressed` and `MomentDetected` and hand each to
/// `on_signal`.
///
/// Blocks forever, so callers run it on a dedicated thread. Uses its own
/// connection rather than the shared one in [`crate::daemon`]: a match rule
/// makes the bus deliver every matching signal to that connection, and the
/// shared one is used for ordinary method calls whose replies must not queue
/// up behind a stream nobody is draining.
pub fn listen(mut on_signal: impl FnMut(ReplaySignal)) -> Result<(), String> {
    use std::pin::Pin;
    use zbus::export::futures_core::Stream;

    let conn = zbus::block_on(zbus::Connection::session()).map_err(|e| e.to_string())?;

    // No member filter: both signals share the interface and path, and one
    // stream keeps their order.
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(RP_IFACE)
        .path(RP_PATH)
        .build()
        .map_err(|e| e.to_string())?;

    let mut stream = zbus::block_on(zbus::MessageStream::for_match_rule(rule, &conn, None))
        .map_err(|e| e.to_string())?;

    // One `block_on` per message, with the callback run outside it: the
    // callback may itself call into `crate::daemon`, whose `block_on` would
    // panic if nested inside this one.
    while let Some(msg) =
        zbus::block_on(std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)))
    {
        let Ok(msg) = msg else { continue };
        let member = msg.header().member().map(|m| m.to_string()).unwrap_or_default();
        // A malformed or unexpected payload is not worth tearing the listener
        // down for — the next real press should still arrive.
        let Ok(arg) = msg.body().deserialize::<String>() else { continue };
        match member.as_str() {
            "HotkeyPressed" => on_signal(ReplaySignal::Hotkey(arg)),
            "MomentDetected" => on_signal(ReplaySignal::Moment(arg)),
            _ => {}
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

    /// Exercises the zbus-6 blocking facade against a real session bus
    /// (skipped when there is none, e.g. most CI runners): a stand-in daemon
    /// on its own connection, the shared `crate::daemon` connection created
    /// on a thread that has since exited and then used from several threads
    /// at once, and the `listen` callback calling back into `crate::daemon`
    /// — the nesting that would panic if the callback ran inside `block_on`.
    #[test]
    fn blocking_facade_works_across_threads_and_from_the_listener() {
        if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
            eprintln!("no session bus — skipping");
            return;
        }

        struct FakeReplay;
        #[zbus::interface(name = "org.opengg.Daemon.Replay")]
        impl FakeReplay {
            fn echo(&self, s: String) -> String {
                format!("echo:{s}")
            }
        }

        let service = zbus::block_on(async {
            let c = zbus::connection::Builder::session()
                .name("org.opengg.Daemon")
                .serve_at(RP_PATH, FakeReplay)
                .build()
                .await?;
            Ok::<_, zbus::Error>(c)
        })
        .expect("serve fake daemon");

        // First use of the shared connection happens on a short-lived thread.
        std::thread::spawn(|| {
            let r: String = crate::daemon::call_dbus("Echo", RP_PATH, RP_IFACE, ("first",)).unwrap();
            assert_eq!(r, "echo:first");
        })
        .join()
        .unwrap();

        // Then several threads share it at once.
        let workers: Vec<_> = (0..4)
            .map(|i| {
                std::thread::spawn(move || {
                    for j in 0..10 {
                        let arg = format!("{i}-{j}");
                        let r: String =
                            crate::daemon::call_dbus("Echo", RP_PATH, RP_IFACE, (arg.as_str(),)).unwrap();
                        assert_eq!(r, format!("echo:{arg}"));
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().unwrap();
        }

        // The listener's callback calls back into crate::daemon.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = listen(move |signal| {
                let arg = match &signal {
                    ReplaySignal::Hotkey(a) => format!("hotkey:{a}"),
                    ReplaySignal::Moment(m) => format!("moment:{m}"),
                };
                let r: Result<String, String> =
                    crate::daemon::call_dbus("Echo", RP_PATH, RP_IFACE, (arg.as_str(),));
                let _ = tx.send(r);
            });
        });
        // Give the match rule time to register before emitting.
        let mut got = None;
        for _ in 0..20 {
            zbus::block_on(service.emit_signal(
                None::<()>,
                RP_PATH,
                RP_IFACE,
                "HotkeyPressed",
                &("save_replay",),
            ))
            .unwrap();
            if let Ok(r) = rx.recv_timeout(std::time::Duration::from_millis(250)) {
                got = Some(r);
                break;
            }
        }
        assert_eq!(got, Some(Ok("echo:hotkey:save_replay".to_string())));

        // The same stream carries auto-clip moments, told apart by member.
        zbus::block_on(service.emit_signal(
            None::<()>,
            RP_PATH,
            RP_IFACE,
            "MomentDetected",
            &("{\"kind\":\"ace\"}",),
        ))
        .unwrap();
        let r = rx.recv_timeout(std::time::Duration::from_secs(2)).expect("moment delivered");
        assert_eq!(r, Ok("echo:moment:{\"kind\":\"ace\"}".to_string()));
    }
}

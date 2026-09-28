//! Replay-buffer control over the daemon's Replay D-Bus interface (blocking).
//!
//! Moved from src-tauri/commands.rs (async→blocking per the core D-Bus client
//! decision). GSR (gpu-screen-recorder) control stays in the Tauri host for now
//! — it is AppHandle / process-state coupled and not a plain daemon call.

use crate::daemon::{call_dbus_void, RP_IFACE, RP_PATH};

/// Start the daemon replay buffer for `duration` seconds.
pub fn start_replay(duration: u32) -> Result<(), String> {
    call_dbus_void("StartReplay", RP_PATH, RP_IFACE, (duration,))
}

/// Stop the recorder / replay buffer.
pub fn stop_recorder() -> Result<(), String> {
    call_dbus_void("Stop", RP_PATH, RP_IFACE, ())
}

/// Flush the replay buffer to a saved clip.
pub fn save_replay() -> Result<(), String> {
    call_dbus_void("SaveReplay", RP_PATH, RP_IFACE, ())
}

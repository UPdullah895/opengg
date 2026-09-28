//! Clip-saved desktop notifications. Ported from the Tauri host's
//! `show_clip_notification` (`frontend/src-tauri/src/commands.rs`).
//!
//! Scope cut: the Tauri original's `"x11-overlay"` backend spawns a second,
//! click-through `WebviewWindow` positioned via raw X11 calls
//! (`set_ignore_cursor_events` + XShape) — pure-X11 only, explicitly skipped
//! on Wayland in the original (`on_wayland` check falls through to
//! `gsr-notify`/`notify-send` instead). This dev environment (and the
//! primary target platform for this migration) is Hyprland/Wayland, where
//! that path never actually fires — every real invocation already resolves
//! to `gsr-notify` or `notify-send`. So this port implements only those two
//! backends; `"x11-overlay"` mode falls back to `notify-send` the same way
//! `"auto"` does on Wayland today. A QML/LayerShellQt overlay window is a
//! straightforward future addition if X11 desktop-notification-daemon-free
//! support is ever needed, but isn't a behavior regression for any
//! currently-live path.

/// Show a clip-saved (or clip-save-failed) desktop notification.
///
/// `mode` mirrors the Tauri original's `notificationStyle` setting:
/// `"system"` forces `notify-send`; `"gsr-notify"` forces the `gsr-notify`
/// binary (falling back to `notify-send` if not on `$PATH`); `"disabled"` is
/// a no-op; anything else (`"auto"`, `"x11-overlay"`) tries `gsr-notify`
/// first, then falls back to `notify-send`.
#[allow(clippy::too_many_arguments)]
pub fn show_clip_notification(
    game: &str,
    filename: &str,
    filesize_mb: f64,
    success: bool,
    enabled: bool,
    mode: &str,
    duration_secs: Option<u64>,
) {
    if !enabled || mode == "disabled" {
        return;
    }

    // Clamp duration to [1, 30] seconds; default 4s — matches the Tauri original.
    let duration_ms = duration_secs.unwrap_or(4).clamp(1, 30) * 1000;

    match mode {
        "system" => notify_system(success, game, filename, duration_ms),
        "gsr-notify" => {
            if !try_gsr_notify(success, game, filename, filesize_mb, duration_ms) {
                notify_system(success, game, filename, duration_ms);
            }
        }
        _ => {
            // "auto" / "x11-overlay" — the overlay backend isn't ported (see
            // module doc); behaves like "auto" always does on Wayland.
            if !try_gsr_notify(success, game, filename, filesize_mb, duration_ms) {
                notify_system(success, game, filename, duration_ms);
            }
        }
    }
}

fn notify_system(success: bool, game: &str, filename: &str, duration_ms: u64) {
    let summary = if success { "Clip Saved" } else { "Clip Save Failed" };
    let body = format!("{game} — {filename}");
    if let Err(e) = std::process::Command::new("notify-send")
        .args([
            "--app-name=OpenGG",
            "--urgency=normal",
            &format!("--expire-time={duration_ms}"),
            summary,
            &body,
        ])
        .spawn()
    {
        log::warn!("notify-send failed: {e}");
    }
}

/// Returns true if the gsr-notify binary was found and launched.
fn try_gsr_notify(success: bool, game: &str, filename: &str, filesize_mb: f64, duration_ms: u64) -> bool {
    let summary = if success { "Clip Saved" } else { "Clip Save Failed" };
    let body = format!("{game} — {filename} ({filesize_mb:.1} MB)");
    std::process::Command::new("gsr-notify")
        .args([
            "--app-name=OpenGG",
            &format!("--expire-time={duration_ms}"),
            summary,
            &body,
        ])
        .spawn()
        .is_ok()
}

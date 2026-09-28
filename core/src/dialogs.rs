//! Native folder picking by delegating to the desktop's own dialog.
//!
//! The shell runs on `QGuiApplication`, not `QApplication`, so
//! `Qt.labs.platform`'s FileDialog refuses to load ("Qt Labs Platform
//! requires Qt Widgets"), and `QtQuick.Dialogs` falls back to Qt's own
//! bare-bones browser — no places sidebar, no recent locations, none of the
//! desktop's conventions. Linking Qt Widgets purely for a folder picker
//! would pull a second toolkit into a QML-only shell.
//!
//! Shelling out to the desktop's picker avoids both. `kdialog` covers KDE,
//! `zenity` covers GNOME and most of the rest; both are ordinary binaries
//! this project already has company for (pactl, ffmpeg, gpu-screen-recorder).
//! When neither exists, [`native_picker`] reports `None` and the caller is
//! expected to fall back to the in-app dialog rather than leaving the user
//! with a dead button.

use crate::subprocess::{command, is_available};

/// The picker this desktop can offer, or `None` when the caller must fall
/// back to the in-app dialog. `kdialog` is preferred where both exist: it
/// matches the rest of a Plasma session, which is this project's primary
/// target.
pub fn native_picker() -> Option<&'static str> {
    ["kdialog", "zenity"].into_iter().find(|bin| is_available(bin))
}

/// Ask the desktop for a directory. Blocks until the user picks one or
/// cancels — callers run it on a worker thread (blocking-in-core rule, plan
/// §2.2/§2.3).
///
/// Returns `Ok(None)` when the user cancelled, which is a normal outcome and
/// not an error. `Err` means no picker was available or it failed to launch.
pub fn pick_folder(title: &str, start_dir: &str) -> Result<Option<String>, String> {
    let Some(bin) = native_picker() else {
        return Err("no native folder picker (kdialog/zenity) on PATH".into());
    };

    // Both accept a starting location, but disagree on everything else.
    let args: Vec<String> = match bin {
        "kdialog" => vec![
            "--title".into(),
            title.into(),
            "--getexistingdirectory".into(),
            start_dir.into(),
        ],
        _ => vec![
            "--file-selection".into(),
            "--directory".into(),
            format!("--title={title}"),
            format!("--filename={}/", start_dir.trim_end_matches('/')),
        ],
    };

    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = command(bin)
        .args(&argv)
        .output()
        .map_err(|e| format!("{bin}: {e}"))?;

    // Exit 1 is "cancelled" for both, and carries no stderr worth reporting.
    if !out.status.success() {
        return Ok(None);
    }

    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() {
        return Ok(None);
    }
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The picker must either name a binary that actually exists or report
    /// none — a name that is not on PATH would produce a button that opens
    /// nothing and never falls back.
    #[test]
    fn reported_picker_is_actually_present() {
        match native_picker() {
            Some(bin) => assert!(is_available(bin), "{bin} reported but not on PATH"),
            None => {
                assert!(!is_available("kdialog"));
                assert!(!is_available("zenity"));
            }
        }
    }
}

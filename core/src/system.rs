//! Small host-agnostic system integrations moved verbatim from
//! `frontend/src-tauri/src/commands.rs` (Epic 2/4 crash-log + autostart +
//! clipboard commands). Converted async→sync per the blocking-in-core rule;
//! the Tauri host's wrappers gain `spawn_blocking`.

use std::io::Write;
use std::path::PathBuf;

/// Directory where per-session log files live (matches `main::LOGS_DIR`).
pub fn crash_log_dir() -> PathBuf {
    const LOGS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../Logs");
    let p = PathBuf::from(LOGS_DIR);
    let _ = std::fs::create_dir_all(&p);
    p.canonicalize().unwrap_or(p)
}

/// Opens the OS file manager at the crash-log directory.
pub fn open_crash_logs_folder() -> Result<(), String> {
    let dir = crash_log_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{e}"))?;
    open::that(&dir).map_err(|e| format!("{e}"))?;
    Ok(())
}

/// Returns true if the XDG autostart entry for OpenGG exists.
pub fn get_autostart() -> Result<bool, String> {
    let desktop = dirs::home_dir()
        .ok_or_else(|| "no home dir".to_string())?
        .join(".config/autostart/opengg.desktop");
    Ok(desktop.exists())
}

/// Creates or removes the XDG autostart `.desktop` entry.
pub fn set_autostart(enable: bool) -> Result<(), String> {
    let dir = dirs::home_dir()
        .ok_or_else(|| "no home dir".to_string())?
        .join(".config/autostart");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{e}"))?;
    let desktop = dir.join("opengg.desktop");

    if enable {
        let exe = std::env::current_exe().map_err(|e| format!("{e}"))?;
        let exe_parent = exe.parent().unwrap_or(std::path::Path::new("/"));
        let content = format!(
            "[Desktop Entry]\n\
            Type=Application\n\
            Name=OpenGG\n\
            Exec=env OPENGG_AUTOSTART=1 \"{}\"\n\
            Path={}\n\
            Terminal=false\n\
            Icon=opengg\n\
            Hidden=false\n\
            NoDisplay=false\n\
            StartupNotify=false\n\
            X-GNOME-Autostart-enabled=true\n",
            exe.display(),
            exe_parent.display()
        );
        std::fs::write(&desktop, content).map_err(|e| format!("{e}"))?;
    } else if desktop.exists() {
        std::fs::remove_file(&desktop).map_err(|e| format!("{e}"))?;
    }
    Ok(())
}

/// Write text to the system clipboard. Tries Wayland (wl-copy) then X11 (xclip).
pub fn write_clipboard(text: &str) -> Result<(), String> {
    if let Ok(mut child) = std::process::Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        if let Ok(status) = child.wait() {
            if status.success() {
                return Ok(());
            }
        }
    }

    if let Ok(mut child) = std::process::Command::new("xclip")
        .args(["-selection", "clipboard", "-in"])
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        if let Ok(status) = child.wait() {
            if status.success() {
                return Ok(());
            }
        }
    }

    Err("no clipboard backend available (wl-copy/xclip)".into())
}

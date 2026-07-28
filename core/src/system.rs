//! Small host-agnostic system integrations moved verbatim from
//! `frontend/src-tauri/src/commands.rs` (Epic 2/4 crash-log + autostart +
//! clipboard commands). Converted async→sync per the blocking-in-core rule;
//! the Tauri host's wrappers gain `spawn_blocking`.

use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;

use crate::subprocess;

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

#[derive(Serialize, Clone, Debug)]
pub struct DependencyStatus {
    pub binary: String,
    pub available: bool,
    pub feature: String,
}

/// Probes for the external binaries OpenGG's features depend on
/// (recording/export/mixer/routing/EQ/headset/window-tools).
pub fn get_dependency_status() -> Vec<DependencyStatus> {
    // X11-only tools are irrelevant on Wayland — don't probe or list them there,
    // so we don't ask Wayland users to install dependencies they can't use.
    let on_wayland = std::env::var("XDG_SESSION_TYPE")
        .map(|v| v.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
        || std::env::var_os("WAYLAND_DISPLAY").is_some();
    const X11_ONLY: &[&str] = &["xdotool"];

    let deps = [
        ("gpu-screen-recorder", "recording"),
        ("ffmpeg", "export"),
        ("ffprobe", "mediaInfo"),
        ("pactl", "audioMixer"),
        ("pw-link", "audioRouting"),
        ("jalv", "equalizer"),
        ("headsetcontrol", "headset"),
        ("xdotool", "windowTools"),
    ];

    let mut results = Vec::new();
    for (binary, feature) in deps {
        if on_wayland && X11_ONLY.contains(&binary) {
            continue;
        }
        results.push(DependencyStatus {
            binary: binary.to_string(),
            available: subprocess::is_available(binary),
            feature: feature.to_string(),
        });
    }

    results
}

#[derive(Serialize, Clone, Debug)]
pub struct DistroInfo {
    pub id: String,
    pub id_like: String,
}

/// Reads `ID`/`ID_LIKE` from `/etc/os-release` (falling back to
/// `/usr/lib/os-release`) to drive distro-specific install command hints.
pub fn get_distro_info() -> DistroInfo {
    let os_release_paths = ["/etc/os-release", "/usr/lib/os-release"];
    let mut content = String::new();

    for path in &os_release_paths {
        match std::fs::read_to_string(path) {
            Ok(data) => {
                content = data;
                break;
            }
            Err(_) => continue,
        }
    }

    let mut id = String::new();
    let mut id_like = String::new();

    for line in content.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("ID=") {
            id = value.trim_matches('"').trim_matches('\'').to_string();
        } else if let Some(value) = line.strip_prefix("ID_LIKE=") {
            id_like = value.trim_matches('"').trim_matches('\'').to_string();
        }
    }

    DistroInfo { id, id_like }
}

#[derive(Serialize, Clone, Debug)]
pub struct DeviceAccessStatus {
    pub ratbagd_available: bool,
    pub in_input_group: bool,
    pub in_audio_group: bool,
    pub in_video_group: bool,
    pub udev_rules_present: bool,
}

/// Probes ratbagd (device manager D-Bus service) availability, `input`/
/// `audio`/`video` group membership, and whether OpenGG's udev rules are
/// installed — surfaced in Settings → About so users can self-diagnose
/// permission issues without reading logs.
pub fn get_device_access_status() -> DeviceAccessStatus {
    let mut status = DeviceAccessStatus {
        ratbagd_available: false,
        in_input_group: false,
        in_audio_group: false,
        in_video_group: false,
        udev_rules_present: false,
    };

    match subprocess::run("busctl", &["--system", "list", "--no-pager"]) {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            status.ratbagd_available = stdout.contains("org.freedesktop.ratbag1");
        }
        Err(_) => {
            status.ratbagd_available = false;
        }
    }

    if let Ok(output) = std::process::Command::new("id").args(["-Gn"]).output() {
        let group_str = String::from_utf8_lossy(&output.stdout);
        status.in_input_group = group_str.contains("input");
        status.in_audio_group = group_str.contains("audio");
        status.in_video_group = group_str.contains("video");
    }

    status.udev_rules_present = std::path::Path::new("/etc/udev/rules.d/99-opengg.rules").exists()
        || std::path::Path::new("/usr/lib/udev/rules.d/99-opengg.rules").exists();

    status
}

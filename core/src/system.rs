//! Small host-agnostic system integrations moved verbatim from
//! `frontend/src-tauri/src/commands.rs` (Epic 2/4 crash-log + autostart +
//! clipboard commands). Converted async→sync per the blocking-in-core rule;
//! the Tauri host's wrappers gain `spawn_blocking`.

use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;

use crate::subprocess;

/// Where logs and crash reports live: `$XDG_STATE_HOME/opengg`. (This used to
/// be `CARGO_MANIFEST_DIR/../Logs` — a path on the machine that *built* the
/// binary, so the button opened nothing on users' machines.)
pub fn crash_log_dir() -> PathBuf {
    crate::diagnostics::log_dir()
}

pub fn open_crash_logs_folder() -> Result<(), String> {
    let dir = crash_log_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{e}"))?;
    open::that(&dir).map_err(|e| format!("{e}"))?;
    Ok(())
}

/// Open the directory containing `path` in the desktop file manager — the
/// Clips context menu's "Show in folder". Opens the parent, not the file
/// itself, so it never launches a video player by accident.
pub fn reveal_in_folder(path: &str) -> Result<(), String> {
    let p = std::path::Path::new(path);
    let dir = p.parent().unwrap_or(p);
    open::that(dir).map_err(|e| format!("{e}"))?;
    Ok(())
}

/// Open any path (file or directory) with the desktop's default handler —
/// e.g. the locales folder, so a translator can open `en.json` in their
/// text editor and see `ar.json` alongside it as a second reference.
pub fn open_path(path: &std::path::Path) -> Result<(), String> {
    open::that(path).map_err(|e| format!("{e}"))
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
        // The EQ runs on PipeWire's own filter-chain now, so `pw-cli` is
        // what it actually needs. `jalv` was listed here for an LV2 host
        // that never worked and, worse, was reported "available" on
        // machines with no LV2 plugins at all — jalv alone is useless.
        ("pw-cli", "equalizer"),
        ("headsetcontrol", "headset"),
        ("xdotool", "windowTools"),
    ];

    // This backs the UI's Recheck: always look again.
    subprocess::forget_availability();
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
    pub in_video_group: bool,
}

/// Whether `group` is one of the whitespace-separated names `id -Gn` printed.
/// Exact match: a substring test counted e.g. `input-devs` as `input`.
fn has_group(id_gn: &str, group: &str) -> bool {
    id_gn.split_whitespace().any(|g| g == group)
}

/// Probes ratbagd (device manager D-Bus service) availability and the group
/// memberships OpenGG relies on — surfaced in Settings → About so users can
/// self-diagnose permission issues without reading logs.
///
/// Only `input` is required (global hotkeys read /dev/input). `video` is
/// reported because some GPU capture setups need it. The old `audio` group
/// and udev-rule checks are gone: PipeWire/logind grant audio access per
/// seat, and OpenGG no longer ships udev rules (see docs/CRITIQUE.md, C3).
pub fn get_device_access_status() -> DeviceAccessStatus {
    let mut status = DeviceAccessStatus {
        ratbagd_available: false,
        in_input_group: false,
        in_video_group: false,
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
        status.in_input_group = has_group(&group_str, "input");
        status.in_video_group = has_group(&group_str, "video");
    }

    status
}

#[cfg(test)]
mod tests {
    use super::has_group;

    #[test]
    fn group_membership_is_exact() {
        assert!(has_group("user wheel input video\n", "input"));
        assert!(!has_group("user input-devs", "input"));
        assert!(!has_group("", "input"));
    }
}

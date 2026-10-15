//! Small host-agnostic system integrations moved verbatim from
//! `frontend/src-tauri/src/commands.rs` (Epic 2/4 crash-log + autostart +
//! clipboard commands). Converted async→sync per the blocking-in-core rule;
//! the Tauri host's wrappers gain `spawn_blocking`.

use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;

use crate::subprocess;

/// Directory where the session log files live — the same directory the
/// logger writes to (`$XDG_STATE_HOME/opengg/logs`), so "Open crash logs
/// folder" opens the files a user would attach to a bug report.
pub fn crash_log_dir() -> PathBuf {
    crate::logging::log_dir()
}

/// Opens the OS file manager at the crash-log directory.
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

/// Full install command per distro family for one tool.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct InstallHints {
    pub arch: String,
    pub debian: String,
    pub fedora: String,
    pub suse: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct DependencyStatus {
    pub binary: String,
    pub available: bool,
    pub feature: String,
    /// Additive (the QML only reads binary/available/feature today): the
    /// correct per-distro install command, so no UI has to keep its own table.
    pub install: InstallHints,
}

struct Tool {
    binary: &'static str,
    /// An existing `settings.deps.feature.*` locale key (new tools reuse one
    /// so the About panel never shows a raw key).
    feature: &'static str,
    /// Not needed for the core experience; reported in the startup log but
    /// not listed in the About panel.
    optional: bool,
    x11_only: bool,
    /// Package per family: arch, debian/ubuntu, fedora, opensuse. A value
    /// containing a space is a full command used verbatim (the Flatpak route).
    pkgs: [&'static str; 4],
}

const PW_UTILS: [&str; 4] = ["pipewire", "pipewire-bin", "pipewire-utils", "pipewire-tools"];
const GSR_FLATPAK: &str = "flatpak install flathub com.dec05eba.gpu_screen_recorder";

/// Single source of truth for every external tool the app shells out to.
/// The `pw-*` tools are NOT in the `pipewire` package outside Arch: Fedora
/// ships them in pipewire-utils, Debian/Ubuntu in pipewire-bin, openSUSE in
/// pipewire-tools.
const TOOLS: &[Tool] = &[
    Tool { binary: "gpu-screen-recorder", feature: "recording", optional: false, x11_only: false,
        pkgs: ["gpu-screen-recorder", GSR_FLATPAK, GSR_FLATPAK, GSR_FLATPAK] },
    Tool { binary: "ffmpeg", feature: "export", optional: false, x11_only: false,
        pkgs: ["ffmpeg", "ffmpeg", "ffmpeg-free", "ffmpeg"] },
    Tool { binary: "ffprobe", feature: "mediaInfo", optional: false, x11_only: false,
        pkgs: ["ffmpeg", "ffmpeg", "ffmpeg-free", "ffmpeg"] },
    Tool { binary: "pactl", feature: "audioMixer", optional: false, x11_only: false,
        pkgs: ["libpulse", "pulseaudio-utils", "pulseaudio-utils", "pulseaudio-utils"] },
    Tool { binary: "pw-link", feature: "audioRouting", optional: false, x11_only: false, pkgs: PW_UTILS },
    Tool { binary: "pw-metadata", feature: "audioRouting", optional: false, x11_only: false, pkgs: PW_UTILS },
    // The EQ runs on PipeWire's own filter-chain: it spawns `pipewire -c`,
    // drives it with `pw-cli` and verifies through `pw-dump`.
    Tool { binary: "pw-cli", feature: "equalizer", optional: false, x11_only: false, pkgs: PW_UTILS },
    Tool { binary: "pw-dump", feature: "equalizer", optional: false, x11_only: false, pkgs: PW_UTILS },
    Tool { binary: "pipewire", feature: "equalizer", optional: false, x11_only: false,
        pkgs: ["pipewire", "pipewire", "pipewire", "pipewire"] },
    Tool { binary: "headsetcontrol", feature: "headset", optional: false, x11_only: false,
        pkgs: ["headsetcontrol", "headsetcontrol", "headsetcontrol", "headsetcontrol"] },
    // X11-only tools are irrelevant on Wayland — not probed or listed there,
    // so Wayland users are not asked to install what they cannot use.
    Tool { binary: "xdotool", feature: "windowTools", optional: false, x11_only: true,
        pkgs: ["xdotool", "xdotool", "xdotool", "xdotool"] },
    // Optional helpers: a missing one degrades a nicety, never a feature.
    Tool { binary: "notify-send", feature: "recording", optional: true, x11_only: false,
        pkgs: ["libnotify", "libnotify-bin", "libnotify", "libnotify-tools"] },
    Tool { binary: "gio", feature: "export", optional: true, x11_only: false,
        pkgs: ["glib2", "libglib2.0-bin", "glib2", "glib2-tools"] },
    Tool { binary: "parec", feature: "audioMixer", optional: true, x11_only: false,
        pkgs: ["libpulse", "pulseaudio-utils", "pulseaudio-utils", "pulseaudio-utils"] },
    Tool { binary: "wl-copy", feature: "export", optional: true, x11_only: false,
        pkgs: ["wl-clipboard", "wl-clipboard", "wl-clipboard", "wl-clipboard"] },
    Tool { binary: "busctl", feature: "headset", optional: true, x11_only: false,
        pkgs: ["systemd", "systemd", "systemd", "systemd"] },
];

fn install_cmd(manager: &str, pkg: &str) -> String {
    if pkg.contains(' ') {
        pkg.to_string()
    } else {
        format!("{manager} {pkg}")
    }
}

fn install_hints(t: &Tool) -> InstallHints {
    InstallHints {
        arch: install_cmd("sudo pacman -S", t.pkgs[0]),
        debian: install_cmd("sudo apt install", t.pkgs[1]),
        fedora: install_cmd("sudo dnf install", t.pkgs[2]),
        suse: install_cmd("sudo zypper install", t.pkgs[3]),
    }
}

/// Uncached (the shared cache in `subprocess` would keep reporting "missing"
/// after the user installs a tool and hits Recheck). The recorder also counts
/// when it is installed as a Flatpak.
fn tool_available(bin: &str) -> bool {
    if bin == "gpu-screen-recorder" {
        crate::gsr::gsr_install().is_some()
    } else {
        subprocess::find_in_path(bin).is_some()
    }
}

fn running_on_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|v| v.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
        || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

fn probe(include_optional: bool) -> Vec<(DependencyStatus, bool)> {
    let wayland = running_on_wayland();
    TOOLS
        .iter()
        .filter(|t| (include_optional || !t.optional) && !(wayland && t.x11_only))
        .map(|t| {
            (
                DependencyStatus {
                    binary: t.binary.to_string(),
                    available: tool_available(t.binary),
                    feature: t.feature.to_string(),
                    install: install_hints(t),
                },
                t.optional,
            )
        })
        .collect()
}

/// Probes for the external binaries OpenGG's features depend on
/// (recording/export/mixer/routing/EQ/headset/window-tools).
pub fn get_dependency_status() -> Vec<DependencyStatus> {
    probe(false).into_iter().map(|(d, _)| d).collect()
}

/// One-line summary for the startup log, optional helpers marked `(optional)`.
pub fn dependency_summary() -> String {
    let all = probe(true);
    let list = |want: bool| -> String {
        all.iter()
            .filter(|(d, _)| d.available == want)
            .map(|(d, opt)| {
                if *opt {
                    format!("{}(optional)", d.binary)
                } else {
                    d.binary.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    format!("tools ok: [{}] missing: [{}]", list(true), list(false))
}

/// `Server Name:` from `pactl info` (e.g. "PulseAudio (on PipeWire 1.2.7)"),
/// or the reason it could not be read. Bounded to ~1 s so a wedged audio
/// server cannot delay startup.
pub fn audio_server_summary() -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(std::process::Command::new("pactl").arg("info").output());
    });
    match rx.recv_timeout(std::time::Duration::from_secs(1)) {
        Ok(Ok(o)) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .find_map(|l| l.strip_prefix("Server Name:"))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "pactl info ok (no Server Name)".into()),
        Ok(Ok(o)) => format!("pactl info failed ({})", o.status),
        Ok(Err(e)) => format!("pactl unavailable ({e})"),
        Err(_) => "pactl info timed out (>1s)".into(),
    }
}

/// Whether `org.opengg.Daemon` currently owns its name on the session bus.
/// "not running" is normal before first use: the service auto-activates on
/// demand. Uses NameHasOwner only, which never triggers bus activation, so
/// logging the summary cannot start the daemon. Bounded to ~1 s.
pub fn daemon_status() -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let r = zbus::blocking::Connection::session()
            .map_err(|e| format!("session bus unavailable ({e})"))
            .and_then(|c| {
                c.call_method(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    Some("org.freedesktop.DBus"),
                    "NameHasOwner",
                    &"org.opengg.Daemon",
                )
                .map_err(|e| format!("bus query failed ({e})"))
                .and_then(|m| {
                    m.body()
                        .deserialize::<bool>()
                        .map_err(|e| format!("bad reply ({e})"))
                })
            });
        let _ = tx.send(r);
    });
    match rx.recv_timeout(std::time::Duration::from_secs(1)) {
        Ok(Ok(true)) => "running".into(),
        Ok(Ok(false)) => "not running (auto-activates on demand)".into(),
        Ok(Err(e)) => e,
        Err(_) => "session bus query timed out (>1s)".into(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn hints(bin: &str) -> InstallHints {
        install_hints(TOOLS.iter().find(|t| t.binary == bin).unwrap())
    }

    #[test]
    fn pw_tools_use_the_right_package_per_distro() {
        for bin in ["pw-link", "pw-cli", "pw-dump", "pw-metadata"] {
            let h = hints(bin);
            assert_eq!(h.arch, "sudo pacman -S pipewire");
            assert_eq!(h.debian, "sudo apt install pipewire-bin");
            assert_eq!(h.fedora, "sudo dnf install pipewire-utils");
            assert_eq!(h.suse, "sudo zypper install pipewire-tools");
        }
    }

    #[test]
    fn obsolete_jalv_is_gone_and_flatpak_route_is_verbatim() {
        assert!(TOOLS.iter().all(|t| t.binary != "jalv"));
        assert!(hints("gpu-screen-recorder").fedora.starts_with("flatpak install"));
    }

    #[test]
    fn optional_tools_stay_out_of_the_ui_list() {
        let ui = get_dependency_status();
        assert!(ui.iter().all(|d| d.binary != "notify-send"));
        assert!(ui.iter().any(|d| d.binary == "pw-dump"));
    }
}

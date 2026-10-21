//! Which game is the user playing right now? Used to name a clip at the
//! moment the save key is pressed.
//!
//! The old answer was `xdotool getactivewindow getwindowname`, which only
//! works on X11 (every Wayland clip was "Unknown") and returns a window
//! *title* ("README.md — Kate"), not an app name. This module asks, in
//! order:
//!
//! 1. the compositor for the focused window's PID, class and title —
//!    Hyprland (`hyprctl`), Sway (`swaymsg`), KDE (`kdotool`) or X11 /
//!    XWayland (`xdotool`), whichever is running;
//! 2. that process's environment for `SteamAppId`, resolved to the store
//!    name through the local Steam app manifests ("ELDEN RING™" rather than
//!    "eldenring.exe");
//! 3. the app's `.desktop` entry for its display name;
//! 4. failing a focused window entirely (GNOME on Wayland exposes none), the
//!    most recently started process that Steam launched — on any desktop,
//!    a running Steam game is the best guess for what is being played.
//!
//! Every external tool is optional; a missing one just skips that source.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What the compositor reports about the focused window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowInfo {
    pub pid: Option<u32>,
    pub class: String,
    pub title: String,
}

/// Best name for the game in front, already passed through
/// [`crate::clips::organizer::app_folder_name`] ("Desktop" when nothing
/// game-like is focused).
pub fn current_game_name() -> String {
    let win = active_window();
    let steam = win
        .as_ref()
        .and_then(|w| w.pid)
        .and_then(steam_app_id_of)
        .or_else(|| win.as_ref().and_then(|w| app_id_from_class(&w.class)))
        .or_else(|| if win.is_none() { newest_steam_game().map(|(_, id)| id) } else { None })
        .and_then(|id| steam_app_name(&id));
    let comm = win.as_ref().and_then(|w| w.pid).and_then(process_name).unwrap_or_default();
    let desktop = win.as_ref().and_then(|w| desktop_entry_name(&w.class)).unwrap_or_default();
    let (class, title) = win.map(|w| (w.class, w.title)).unwrap_or_default();
    choose_name(steam.as_deref(), &desktop, &class, &comm, &title)
}

/// Order the candidates: Steam store name, `.desktop` name, then — for a
/// shell process the folder-namer maps to "Desktop" — the class/process,
/// and only then the window title (titles carry document names and
/// unsaved-marks, so they are the last resort).
pub fn choose_name(steam: Option<&str>, desktop: &str, class: &str, comm: &str, title: &str) -> String {
    use crate::clips::organizer::app_folder_name;
    if let Some(s) = steam {
        return app_folder_name([s]);
    }
    // A shell or launcher in front means "not in a game", whatever its title.
    let shell = app_folder_name([class]) == "Desktop" && !class.is_empty()
        || app_folder_name([comm]) == "Desktop" && !comm.is_empty();
    if shell {
        return "Desktop".into();
    }
    app_folder_name([desktop, &prettify_class(class), comm, title])
}

/// Window classes are usually reverse-DNS or lowercase binary names:
/// "org.kde.kate" → "kate", "steam_app_123" → "" (handled via the app id).
fn prettify_class(class: &str) -> String {
    if class.starts_with("steam_app_") {
        return String::new();
    }
    class.rsplit('.').next().unwrap_or(class).to_string()
}

/// `steam_app_<id>` is the X11 class Proton gives a Steam game's window.
fn app_id_from_class(class: &str) -> Option<String> {
    class
        .strip_prefix("steam_app_")
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) && *id != "0")
        .map(str::to_string)
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Ask whichever compositor is running for the focused window.
pub fn active_window() -> Option<WindowInfo> {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        if let Some(w) = run("hyprctl", &["activewindow", "-j"]).and_then(|j| parse_hyprctl(&j)) {
            return Some(w);
        }
    }
    if std::env::var_os("SWAYSOCK").is_some() {
        if let Some(w) = run("swaymsg", &["-t", "get_tree", "-r"]).and_then(|j| parse_sway_tree(&j)) {
            return Some(w);
        }
    }
    let kde = std::env::var("XDG_CURRENT_DESKTOP").map(|d| d.contains("KDE")).unwrap_or(false);
    if kde {
        if let Some(w) = dotool("kdotool") {
            return Some(w);
        }
    }
    if std::env::var_os("DISPLAY").is_some() {
        if let Some(w) = dotool("xdotool") {
            return Some(w);
        }
    }
    None
}

/// `xdotool` and its KDE twin `kdotool` share a command line.
fn dotool(tool: &str) -> Option<WindowInfo> {
    let id = run(tool, &["getactivewindow"])?;
    if id.is_empty() {
        return None;
    }
    let pid = run(tool, &["getwindowpid", &id]).and_then(|p| p.parse().ok());
    let title = run(tool, &["getwindowname", &id]).unwrap_or_default();
    let class = run(tool, &["getwindowclassname", &id]).unwrap_or_default();
    Some(WindowInfo { pid, class, title })
}

/// `hyprctl activewindow -j` → focused window ("{}" when none).
pub fn parse_hyprctl(json: &str) -> Option<WindowInfo> {
    let v: Value = serde_json::from_str(json).ok()?;
    let class = v["class"].as_str().unwrap_or("").to_string();
    let title = v["title"].as_str().unwrap_or("").to_string();
    let pid = v["pid"].as_i64().filter(|p| *p > 0).map(|p| p as u32);
    if class.is_empty() && title.is_empty() && pid.is_none() {
        return None;
    }
    Some(WindowInfo { pid, class, title })
}

/// `swaymsg -t get_tree -r` → the focused leaf (native or XWayland).
pub fn parse_sway_tree(json: &str) -> Option<WindowInfo> {
    fn walk(n: &Value) -> Option<WindowInfo> {
        if n["focused"].as_bool() == Some(true) && n["pid"].is_number() {
            let class = n["app_id"]
                .as_str()
                .or_else(|| n["window_properties"]["class"].as_str())
                .unwrap_or("")
                .to_string();
            return Some(WindowInfo {
                pid: n["pid"].as_u64().map(|p| p as u32),
                class,
                title: n["name"].as_str().unwrap_or("").to_string(),
            });
        }
        for key in ["nodes", "floating_nodes"] {
            if let Some(arr) = n[key].as_array() {
                if let Some(w) = arr.iter().find_map(walk) {
                    return Some(w);
                }
            }
        }
        None
    }
    walk(&serde_json::from_str(json).ok()?)
}

/// `SteamAppId` / `SteamGameId` from a NUL-separated environ block.
pub fn steam_app_id_from_environ(environ: &[u8]) -> Option<String> {
    let mut game_id = None;
    for var in environ.split(|b| *b == 0) {
        let s = String::from_utf8_lossy(var);
        if let Some(v) = s.strip_prefix("SteamAppId=") {
            if !v.is_empty() && v != "0" {
                return Some(v.to_string());
            }
        } else if let Some(v) = s.strip_prefix("SteamGameId=") {
            // Non-Steam shortcuts get a 64-bit id with no manifest; only a
            // small id names a real store app.
            if v.len() <= 10 && v.bytes().all(|b| b.is_ascii_digit()) && v != "0" {
                game_id = Some(v.to_string());
            }
        }
    }
    game_id
}

fn steam_app_id_of(pid: u32) -> Option<String> {
    let env = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
    steam_app_id_from_environ(&env)
}

fn process_name(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Start time (clock ticks since boot) of a process, field 22 of `stat`.
fn start_ticks(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name (field 2) may contain spaces; fields resume after ')'.
    let rest = &stat[stat.rfind(')')? + 2..];
    rest.split(' ').nth(19)?.parse().ok()
}

/// The newest process Steam launched for a game, as (pid, app id). Steam's
/// own processes and helpers carry no app id, so they never match.
pub fn newest_steam_game() -> Option<(u32, String)> {
    let me = std::process::id();
    std::fs::read_dir("/proc")
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| *pid != me)
        .filter_map(|pid| Some((pid, steam_app_id_of(pid)?)))
        .max_by_key(|(pid, _)| start_ticks(*pid).unwrap_or(0))
}

/// Library folders listed in `libraryfolders.vdf` (`"path" "<dir>"` lines).
pub fn parse_library_folders(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|l| {
            let t = l.trim();
            if !t.starts_with("\"path\"") {
                return None;
            }
            let parts: Vec<&str> = t.split('"').collect();
            parts.get(3).map(|p| PathBuf::from(p.replace("\\\\", "\\")))
        })
        .collect()
}

/// Every `steamapps` directory on this machine.
fn steamapps_dirs() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else { return Vec::new() };
    let mut out: Vec<PathBuf> = Vec::new();
    for root in crate::steam::steam_root_candidates(&home) {
        let apps = root.join("steamapps");
        if !apps.is_dir() {
            continue;
        }
        if let Ok(vdf) = std::fs::read_to_string(apps.join("libraryfolders.vdf")) {
            for lib in parse_library_folders(&vdf) {
                let d = lib.join("steamapps");
                if d.is_dir() && !out.contains(&d) {
                    out.push(d);
                }
            }
        }
        if !out.contains(&apps) {
            out.push(apps);
        }
    }
    out
}

/// Store name of an installed Steam app, from its `appmanifest_<id>.acf`.
pub fn steam_app_name(app_id: &str) -> Option<String> {
    steamapps_dirs().iter().find_map(|d| {
        let acf = std::fs::read_to_string(d.join(format!("appmanifest_{app_id}.acf"))).ok()?;
        crate::steam::parse_acf_field(&acf, "name").filter(|n| !n.is_empty())
    })
}

/// `Name=` of the `.desktop` entry for a window class, matched by file name
/// (`<class>.desktop`) or `StartupWMClass=`.
pub fn desktop_entry_name(class: &str) -> Option<String> {
    if class.is_empty() {
        return None;
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = dirs::data_dir() {
        dirs.push(d.join("applications"));
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    dirs.extend(data_dirs.split(':').filter(|s| !s.is_empty()).map(|d| Path::new(d).join("applications")));
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));

    let lower = class.to_lowercase();
    for d in &dirs {
        let Ok(rd) = std::fs::read_dir(d) else { continue };
        for e in rd.flatten() {
            let fname = e.file_name().to_string_lossy().to_lowercase();
            let Some(stem) = fname.strip_suffix(".desktop") else { continue };
            let by_name = stem == lower || stem.rsplit('.').next() == Some(lower.as_str());
            let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
            let by_wmclass = text
                .lines()
                .any(|l| l.strip_prefix("StartupWMClass=").is_some_and(|v| v.eq_ignore_ascii_case(class)));
            if by_name || by_wmclass {
                if let Some(n) = parse_desktop_name(&text) {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// The unlocalised `Name=` in the `[Desktop Entry]` group.
pub fn parse_desktop_name(text: &str) -> Option<String> {
    let mut in_entry = false;
    for l in text.lines() {
        let l = l.trim();
        if l.starts_with('[') {
            in_entry = l == "[Desktop Entry]";
            continue;
        }
        if in_entry {
            if let Some(v) = l.strip_prefix("Name=") {
                return Some(v.trim().to_string()).filter(|s| !s.is_empty());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyprctl_json() {
        let j = r#"{"address":"0x1","class":"steam_app_1245620","title":"ELDEN RING™","pid":4242}"#;
        assert_eq!(
            parse_hyprctl(j),
            Some(WindowInfo { pid: Some(4242), class: "steam_app_1245620".into(), title: "ELDEN RING™".into() })
        );
        assert_eq!(parse_hyprctl("{}"), None);
        assert_eq!(parse_hyprctl("not json"), None);
    }

    #[test]
    fn sway_tree_finds_the_focused_leaf() {
        let j = r#"{"focused":false,"nodes":[{"focused":false,"nodes":[
            {"focused":false,"pid":1,"app_id":"foot","name":"shell"},
            {"focused":true,"pid":77,"app_id":null,"window_properties":{"class":"Hades"},"name":"Hades II"}
        ]}],"floating_nodes":[]}"#;
        let w = parse_sway_tree(j).unwrap();
        assert_eq!((w.pid, w.class.as_str(), w.title.as_str()), (Some(77), "Hades", "Hades II"));
    }

    #[test]
    fn environ_app_ids() {
        assert_eq!(steam_app_id_from_environ(b"A=1\0SteamAppId=1245620\0B=2\0").as_deref(), Some("1245620"));
        assert_eq!(steam_app_id_from_environ(b"SteamAppId=0\0").as_deref(), None);
        assert_eq!(steam_app_id_from_environ(b"SteamGameId=730\0").as_deref(), Some("730"));
        // A non-Steam shortcut's 64-bit game id names no manifest.
        assert_eq!(steam_app_id_from_environ(b"SteamGameId=12345678901234567890\0").as_deref(), None);
        assert_eq!(steam_app_id_from_environ(b"").as_deref(), None);
    }

    #[test]
    fn class_helpers() {
        assert_eq!(app_id_from_class("steam_app_1245620").as_deref(), Some("1245620"));
        assert_eq!(app_id_from_class("steam_app_0"), None);
        assert_eq!(app_id_from_class("firefox"), None);
        assert_eq!(prettify_class("org.kde.kate"), "kate");
        assert_eq!(prettify_class("steam_app_1"), "");
    }

    #[test]
    fn name_priority() {
        assert_eq!(choose_name(Some("ELDEN RING™"), "", "steam_app_1", "eldenring.exe", "x"), "ELDEN RING™");
        assert_eq!(choose_name(None, "Kate", "org.kde.kate", "kate", "README.md — Kate"), "Kate");
        assert_eq!(choose_name(None, "", "minecraft", "java", "Minecraft 1.21"), "minecraft");
        assert_eq!(choose_name(None, "", "", "", "Some Game"), "Some Game");
        assert_eq!(choose_name(None, "", "plasmashell", "plasmashell", "Desktop — Plasma"), "Desktop");
        assert_eq!(choose_name(None, "", "", "", ""), "Desktop");
    }

    #[test]
    fn library_folders_and_desktop_names() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"/home/u/.local/share/Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"/mnt/games/SteamLibrary\"\n\t}\n}";
        assert_eq!(
            parse_library_folders(vdf),
            vec![PathBuf::from("/home/u/.local/share/Steam"), PathBuf::from("/mnt/games/SteamLibrary")]
        );
        let entry = "[Desktop Entry]\nName=Heroic Games Launcher\nName[de]=Heroic\n[Desktop Action x]\nName=Other\n";
        assert_eq!(parse_desktop_name(entry).as_deref(), Some("Heroic Games Launcher"));
        assert_eq!(parse_desktop_name("[Other]\nName=X\n"), None);
    }

    #[test]
    fn own_process_start_time_is_readable() {
        assert!(start_ticks(std::process::id()).is_some());
    }
}

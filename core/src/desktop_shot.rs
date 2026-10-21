//! Desktop screenshots for the global Screenshot hotkey.
//!
//! The binding existed end to end but had no handler. There is no single
//! Linux screenshot API that works everywhere without a dialog, so this
//! picks the first tool that fits the running session:
//!
//! | session              | tool                                    |
//! |----------------------|-----------------------------------------|
//! | Hyprland             | `grim -o <focused monitor>`             |
//! | other wlroots (Sway) | `grim`                                  |
//! | KDE Plasma           | `spectacle -b -n -f -o`                 |
//! | GNOME                | `gnome-screenshot -f`                   |
//! | X11                  | `maim`, `import -window root`, `ffmpeg x11grab` |
//!
//! The shot is written to a hidden part file and moved into place under the
//! same naming and folder rules as clips (`<Game>_<stamp>.png`, " (n)" on a
//! collision), then recorded as OpenGG's own for the folder size limit.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::clips::organizer;

/// One screenshot at a time: a second press while the first is being
/// written is dropped rather than racing it.
static BUSY: AtomicBool = AtomicBool::new(false);

/// A capture command and its arguments; `{out}` is replaced by the path.
#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub program: &'static str,
    pub args: Vec<String>,
}

/// Session facts that decide the tool, gathered from the environment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Session {
    pub wayland: bool,
    pub hyprland: bool,
    pub sway: bool,
    pub desktop: String,
    pub x11_display: Option<String>,
    pub focused_monitor: Option<String>,
}

impl Session {
    pub fn detect() -> Self {
        let hyprland = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some();
        let focused_monitor = if hyprland { hyprland_focused_monitor() } else { None };
        Session {
            wayland: std::env::var_os("WAYLAND_DISPLAY").is_some(),
            hyprland,
            sway: std::env::var_os("SWAYSOCK").is_some(),
            desktop: std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
            x11_display: std::env::var("DISPLAY").ok().filter(|d| !d.is_empty()),
            focused_monitor,
        }
    }
}

fn hyprland_focused_monitor() -> Option<String> {
    let out = Command::new("hyprctl").args(["monitors", "-j"]).output().ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    v.as_array()?
        .iter()
        .find(|m| m["focused"].as_bool() == Some(true))
        .and_then(|m| m["name"].as_str())
        .map(str::to_string)
}

/// Candidate tools for `session`, best first. `has` says whether a program
/// is installed (injected for tests).
pub fn candidates(session: &Session, has: impl Fn(&str) -> bool) -> Vec<Tool> {
    let out = "{out}".to_string();
    let mut v = Vec::new();
    let tool = |program: &'static str, args: &[&str]| Tool { program, args: args.iter().map(|s| s.to_string()).collect() };
    if session.wayland {
        if (session.hyprland || session.sway) && has("grim") {
            let mut t = tool("grim", &[]);
            if let Some(m) = &session.focused_monitor {
                t.args.extend(["-o".to_string(), m.clone()]);
            }
            t.args.push(out.clone());
            v.push(t);
        }
        if session.desktop.contains("KDE") && has("spectacle") {
            v.push(tool("spectacle", &["-b", "-n", "-f", "-o", "{out}"]));
        }
        if session.desktop.contains("GNOME") && has("gnome-screenshot") {
            v.push(tool("gnome-screenshot", &["-f", "{out}"]));
        }
        // Any other wlroots compositor.
        if has("grim") && !v.iter().any(|t| t.program == "grim") {
            v.push(tool("grim", &["{out}"]));
        }
    }
    if let Some(d) = &session.x11_display {
        if has("maim") {
            v.push(tool("maim", &["{out}"]));
        }
        if has("import") {
            v.push(tool("import", &["-window", "root", "{out}"]));
        }
        if has("ffmpeg") {
            v.push(Tool {
                program: "ffmpeg",
                args: ["-hide_banner", "-loglevel", "error", "-y", "-f", "x11grab", "-i", d, "-frames:v", "1", "{out}"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            });
        }
    }
    v
}

fn installed(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
        .unwrap_or(false)
}

/// Take a screenshot of the desktop into `root` (per-game folder optional),
/// named after `game`. Returns the saved path.
pub fn take(root: &Path, game: &str, per_game_folder: bool) -> Result<PathBuf, String> {
    if BUSY.swap(true, Ordering::AcqRel) {
        return Err("A screenshot is already being taken".into());
    }
    let res = take_inner(root, game, per_game_folder);
    BUSY.store(false, Ordering::Release);
    res
}

fn take_inner(root: &Path, game: &str, per_game_folder: bool) -> Result<PathBuf, String> {
    let stamp = organizer::local_stamp(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
    );
    let target = organizer::target_path(root, game, &stamp, "png", per_game_folder);
    let dir = target.parent().unwrap_or(root).to_path_buf();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let part = dir.join(format!(".{stamp}.{}.part.png", std::process::id()));

    let tools = candidates(&Session::detect(), installed);
    if tools.is_empty() {
        return Err("No screenshot tool found — install grim (wlroots), spectacle (KDE), gnome-screenshot (GNOME) or maim (X11)".into());
    }
    let mut failures = Vec::new();
    for t in tools {
        let args: Vec<String> = t.args.iter().map(|a| a.replace("{out}", &part.to_string_lossy())).collect();
        let ok = Command::new(t.program)
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok && std::fs::metadata(&part).map(|m| m.len() > 0).unwrap_or(false) {
            let dest = organizer::move_into_place(&part, &target).map_err(|e| e.to_string())?;
            if let Ok(db) = crate::clips::open_db() {
                let _ = crate::clips::retention::record_owned(&db, &dest);
            }
            return Ok(dest);
        }
        let _ = std::fs::remove_file(&part);
        failures.push(t.program);
    }
    Err(format!("Screenshot failed (tried {})", failures.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(_: &str) -> bool {
        true
    }

    #[test]
    fn hyprland_uses_grim_on_the_focused_monitor() {
        let s = Session { wayland: true, hyprland: true, focused_monitor: Some("DP-1".into()), ..Default::default() };
        let c = candidates(&s, all);
        assert_eq!(c[0].program, "grim");
        assert_eq!(c[0].args, ["-o", "DP-1", "{out}"]);
        assert_eq!(c.iter().filter(|t| t.program == "grim").count(), 1);
    }

    #[test]
    fn desktop_specific_tools() {
        let kde = Session { wayland: true, desktop: "KDE".into(), ..Default::default() };
        assert_eq!(candidates(&kde, all)[0].program, "spectacle");
        let gnome = Session { wayland: true, desktop: "ubuntu:GNOME".into(), ..Default::default() };
        assert_eq!(candidates(&gnome, all)[0].program, "gnome-screenshot");
    }

    #[test]
    fn x11_fallbacks_in_order_and_only_installed() {
        let x = Session { x11_display: Some(":0".into()), ..Default::default() };
        let names: Vec<_> = candidates(&x, all).iter().map(|t| t.program).collect();
        assert_eq!(names, ["maim", "import", "ffmpeg"]);
        let only_ffmpeg: Vec<_> = candidates(&x, |p| p == "ffmpeg").iter().map(|t| t.program).collect();
        assert_eq!(only_ffmpeg, ["ffmpeg"]);
        assert!(candidates(&Session::default(), all).is_empty());
    }
}

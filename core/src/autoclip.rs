//! Auto-clip, app side: installing the game integration and naming the clips.
//!
//! The daemon (`daemon/src/autoclip`) listens for Counter-Strike 2's Game
//! State Integration and emits `MomentDetected` (see [`crate::hotkeys`]). For
//! CS2 to talk to it at all, a `gamestate_integration_*.cfg` must sit in the
//! game's cfg folder, carrying the port and a token. [`install_cs2`] writes
//! one into every Steam library that has the game; the token lives in
//! `~/.config/opengg/cs2-gsi.token`, which the daemon checks each payload
//! against.
//!
//! When the app saves a clip for a moment, [`tag_clip`] names it after the
//! moment — `CS2 – Ace – Mirage` — so it arrives labelled.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::daemon::{call_dbus, RP_IFACE, RP_PATH};

/// Must match `GSI_PORT` in `daemon/src/autoclip/mod.rs`.
pub const GSI_PORT: u16 = 37820;

/// The cfg's file name. CS2 loads every `gamestate_integration_*.cfg` in its
/// cfg folder at launch.
pub const CFG_NAME: &str = "gamestate_integration_opengg.cfg";

/// Event switches the Settings card offers, as stored in
/// `settings.autoClip.events`. Must match `Kind::pref_key` in the daemon.
pub const EVENTS: [&str; 6] = ["ace", "multikill", "mvp", "headshot", "kill", "death"];

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"))
        .join("opengg")
}

pub fn token_path() -> PathBuf {
    config_dir().join("cs2-gsi.token")
}

/// The token, created on first use (128 bits from the kernel, hex).
fn ensure_token() -> Result<String, String> {
    let path = token_path();
    if let Ok(t) = std::fs::read_to_string(&path) {
        let t = t.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    let mut raw = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut raw))
        .map_err(|e| format!("cannot read /dev/urandom: {e}"))?;
    let token: String = raw.iter().map(|b| format!("{b:02x}")).collect();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    write_private(&path, &token)?;
    Ok(token)
}

/// Write a file only this user can read.
fn write_private(path: &Path, contents: &str) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    f.write_all(contents.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
}

/// The cfg CS2 reads. Only the sections the tracker diffs are requested:
/// the less the game sends, the less there is to parse every half second.
pub fn cfg_contents(token: &str) -> String {
    format!(
        r#""OpenGG auto-clip"
{{
    "uri"       "http://127.0.0.1:{GSI_PORT}/"
    "timeout"   "5.0"
    "buffer"    "0.1"
    "throttle"  "0.5"
    "heartbeat" "30.0"
    "auth"
    {{
        "token" "{token}"
    }}
    "data"
    {{
        "provider"           "1"
        "map"                "1"
        "round"              "1"
        "player_id"          "1"
        "player_state"       "1"
        "player_match_stats" "1"
    }}
}}
"#
    )
}

/// CS2's cfg folder in each Steam library that has the game. `~/.steam/root`
/// is usually a symlink to `~/.local/share/Steam`, so paths are compared
/// after resolving links.
pub fn cs2_cfg_dirs(home: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for lib in crate::steam::steam_library_dirs(home) {
        let d = lib.join("common/Counter-Strike Global Offensive/game/csgo/cfg");
        if let Ok(real) = d.canonicalize() {
            if real.is_dir() && !out.contains(&real) {
                out.push(real);
            }
        }
    }
    out
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

/// Write the integration cfg into every CS2 install. Returns the files
/// written. CS2 only reads it at launch, so a running game needs a restart.
pub fn install_cs2() -> Result<Vec<String>, String> {
    let dirs = cs2_cfg_dirs(&home());
    if dirs.is_empty() {
        return Err("Counter-Strike 2 was not found in any Steam library".into());
    }
    let token = ensure_token()?;
    let cfg = cfg_contents(&token);
    let mut written = Vec::new();
    for d in dirs {
        let path = d.join(CFG_NAME);
        // The cfg carries the token, so it gets the same permissions.
        write_private(&path, &cfg)?;
        written.push(path.to_string_lossy().into_owned());
    }
    Ok(written)
}

/// Remove the cfg from every CS2 install. The token stays, so reinstalling
/// later does not invalidate anything.
pub fn uninstall_cs2() -> Result<(), String> {
    for d in cs2_cfg_dirs(&home()) {
        let path = d.join(CFG_NAME);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// Whether CS2 was found, and whether every copy has our current cfg.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallState {
    pub game_found: bool,
    pub installed: bool,
}

pub fn cs2_install_state() -> InstallState {
    let dirs = cs2_cfg_dirs(&home());
    let token = std::fs::read_to_string(token_path()).unwrap_or_default();
    let want = (!token.trim().is_empty()).then(|| cfg_contents(token.trim()));
    let installed = !dirs.is_empty()
        && want.is_some_and(|w| {
            dirs.iter()
                .all(|d| std::fs::read_to_string(d.join(CFG_NAME)).is_ok_and(|c| c == w))
        });
    InstallState { game_found: !dirs.is_empty(), installed }
}

/// The daemon listener's status JSON (`GetAutoClipStatus`).
pub fn daemon_status() -> Result<String, String> {
    call_dbus("GetAutoClipStatus", RP_PATH, RP_IFACE, ())
}

/// The `MomentDetected` payload.
#[derive(Debug, Clone, Deserialize)]
pub struct Moment {
    pub game: String,
    pub kind: String,
    #[serde(default)]
    pub map: String,
    #[serde(default)]
    pub round: i64,
    #[serde(default)]
    pub kills: u64,
}

/// `de_mirage` → `Mirage`, `cs_office` → `Office`, `de_dust2` → `Dust2`.
pub fn pretty_map(raw: &str) -> String {
    let name = raw.rsplit('/').next().unwrap_or(raw);
    let name = ["de_", "cs_", "ar_", "dz_", "gd_", "dm_"]
        .iter()
        .find_map(|p| name.strip_prefix(p))
        .unwrap_or(name);
    name.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "ace" => "Ace",
        "quad" => "Quad Kill",
        "triple" => "Triple Kill",
        "mvp" => "MVP",
        "headshot" => "Headshot",
        "kill" => "Kill",
        "death" => "Death",
        _ => "Highlight",
    }
}

fn game_label(game: &str) -> &str {
    match game {
        "cs2" => "CS2",
        g => g,
    }
}

/// `CS2 – Ace – Mirage`.
pub fn clip_name(m: &Moment) -> String {
    let mut parts = vec![game_label(&m.game).to_string(), kind_label(&m.kind).to_string()];
    let map = pretty_map(&m.map);
    if !map.is_empty() {
        parts.push(map);
    }
    parts.join(" – ")
}

/// Name a freshly saved clip after the moment that triggered it.
pub fn tag_clip(filepath: &str, moment_json: &str) -> Result<String, String> {
    let m: Moment = serde_json::from_str(moment_json).map_err(|e| format!("bad moment: {e}"))?;
    let name = clip_name(&m);
    crate::clips::set_clip_meta(crate::clips::ClipMetaUpdate {
        filepath: filepath.to_string(),
        custom_name: Some(name.clone()),
        favorite: None,
        game_tag: None,
        notes: Some(format!("Auto-clip: round {}, {} kills", m.round + 1, m.kills)),
    })?;
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_read_like_their_names() {
        assert_eq!(pretty_map("de_mirage"), "Mirage");
        assert_eq!(pretty_map("de_dust2"), "Dust2");
        assert_eq!(pretty_map("cs_office"), "Office");
        assert_eq!(pretty_map("workshop/123/de_cache_b"), "Cache B");
        assert_eq!(pretty_map(""), "");
    }

    #[test]
    fn clips_are_named_game_kind_map() {
        let m: Moment = serde_json::from_str(
            r#"{"game":"cs2","kind":"ace","map":"de_mirage","round":7,"kills":5,"at":1}"#,
        )
        .unwrap();
        assert_eq!(clip_name(&m), "CS2 – Ace – Mirage");
        let m: Moment = serde_json::from_str(r#"{"game":"cs2","kind":"triple"}"#).unwrap();
        assert_eq!(clip_name(&m), "CS2 – Triple Kill");
    }

    #[test]
    fn the_cfg_points_at_the_listener_with_the_token() {
        let cfg = cfg_contents("deadbeef");
        assert!(cfg.contains(&format!("\"http://127.0.0.1:{GSI_PORT}/\"")));
        assert!(cfg.contains("\"token\" \"deadbeef\""));
        for section in ["provider", "map", "player_id", "player_state", "player_match_stats"] {
            assert!(cfg.contains(&format!("\"{section}\"")), "{section}");
        }
        // Balanced braces, or CS2 ignores the file without a word.
        assert_eq!(cfg.matches('{').count(), cfg.matches('}').count());
    }

    #[test]
    fn finds_cs2_in_a_secondary_library() {
        let tmp = std::env::temp_dir().join(format!("opengg-autoclip-{}", std::process::id()));
        let root = tmp.join(".local/share/Steam");
        let extra = tmp.join("games/SteamLibrary");
        std::fs::create_dir_all(root.join("steamapps")).unwrap();
        let cfg = extra.join("steamapps/common/Counter-Strike Global Offensive/game/csgo/cfg");
        std::fs::create_dir_all(&cfg).unwrap();
        std::fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            format!("\"libraryfolders\"\n{{\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n", extra.display()),
        )
        .unwrap();
        // The usual `~/.steam/root` symlink must not list it twice.
        std::fs::create_dir_all(tmp.join(".steam")).unwrap();
        std::os::unix::fs::symlink(&root, tmp.join(".steam/root")).unwrap();
        assert_eq!(cs2_cfg_dirs(&tmp), vec![cfg.canonicalize().unwrap()]);
        std::fs::remove_dir_all(&tmp).ok();
    }
}

//! Where a saved clip lands and what it is called.
//!
//! Every clip is named `<Game>_<YYYY-MM-DD>_<HH-MM-SS>.<ext>` — the
//! timestamped shape gpu-screen-recorder itself uses, so `date_from_stem`
//! keeps reading it — optionally inside a `<root>/<Game>/` folder.
//!
//! Two bugs this module exists to close:
//!
//! * The old sanitiser turned every space into `_`, and the lister took the
//!   game as everything before the *first* `_`, so "Elden Ring" came back as
//!   "Elden". Names now keep their spaces, and [`game_from_stem`] takes the
//!   game as everything before the *timestamp*, which also repairs the clips
//!   already saved under the old scheme ("Elden_Ring_2026-…" → "Elden Ring").
//! * Two saves inside the same second shared one name and the second
//!   overwrote the first. [`free_path`] appends " (2)", " (3)", … instead.

use std::path::{Path, PathBuf};

/// Longest game-name component, in characters. Long window titles would
/// otherwise push full paths toward the 255-byte filename limit.
const MAX_NAME_CHARS: usize = 60;

/// Desktop shells, compositors and launchers: being "in front" while the
/// user presses save means they were not in a game, so the clip is filed
/// under one honest bucket instead of e.g. "plasmashell".
const SHELL_PROCESSES: &[&str] = &[
    "plasmashell", "kwin_wayland", "kwin_x11", "gnome-shell", "mutter",
    "Hyprland", "hyprland", "sway", "weston", "xfwm4", "xfdesktop",
    "xfce4-panel", "cinnamon", "budgie-wm", "waybar", "nautilus", "dolphin",
    "opengg-qt", "openggd", "Xwayland", "steamwebhelper",
];

/// Turn an arbitrary app/game/window name into a safe folder and file-name
/// component, or `None` when nothing printable survives.
///
/// Characters that are illegal on Linux (`/`, NUL) or on the exFAT/NTFS
/// drives people keep clips on (`\:*?"<>|`) and control characters become
/// spaces; runs of spaces collapse; trailing dots go (Windows strips them
/// silently, which breaks a library shared over a dual-boot partition);
/// the result is capped at [`MAX_NAME_CHARS`].
pub fn sanitize_name(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut pending_space = false;
    for c in raw.chars() {
        let bad = c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|');
        if bad || c.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && !out.is_empty() {
            out.push(' ');
        }
        pending_space = false;
        out.push(c);
    }
    let mut s: String = out.chars().take(MAX_NAME_CHARS).collect();
    loop {
        let trimmed = s.trim_end().trim_end_matches('.').trim_end();
        if trimmed.len() == s.len() {
            break;
        }
        s = trimmed.to_string();
    }
    // A leading dot would hide the clip from the library (dotfiles are
    // skipped as caches/part files).
    let s = s.trim_start_matches('.').trim_start().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// The game name a clip is filed under: the first candidate that sanitises
/// to something non-empty, with desktop shells mapped to "Desktop".
///
/// `candidates` are ordered best-first by the caller (e.g. Steam store name,
/// then window class, then process name, then window title).
pub fn app_folder_name<'a>(candidates: impl IntoIterator<Item = &'a str>) -> String {
    for c in candidates {
        let c = c.trim();
        if c.is_empty() {
            continue;
        }
        if SHELL_PROCESSES.iter().any(|s| s.eq_ignore_ascii_case(c)) {
            return "Desktop".into();
        }
        if let Some(name) = sanitize_name(c) {
            return name;
        }
    }
    "Desktop".into()
}

/// Local wall-clock time as the `YYYY-MM-DD_HH-MM-SS` stamp used in names.
/// Always Gregorian with ASCII digits, whatever the user's locale, so the
/// lister can parse every name back.
pub fn local_stamp(unix_secs: i64) -> String {
    #[cfg(unix)]
    {
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe {
            libc::localtime_r(&unix_secs, &mut tm);
        }
        format!(
            "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
    #[cfg(not(unix))]
    {
        let _ = unix_secs;
        "1970-01-01_00-00-00".into()
    }
}

/// Where a new clip for `game` taken at `stamp` should go, before collision
/// handling. With `per_game_folder` the clip goes to `<root>/<game>/`.
pub fn target_path(root: &Path, game: &str, stamp: &str, ext: &str, per_game_folder: bool) -> PathBuf {
    let game = sanitize_name(game).unwrap_or_else(|| "Desktop".into());
    let file = format!("{game}_{stamp}.{ext}");
    if per_game_folder {
        root.join(&game).join(file)
    } else {
        root.join(file)
    }
}

/// `path` itself if nothing is there, else the first free `stem (n).ext`
/// for n = 2, 3, … — never an overwrite.
pub fn free_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("clip");
    let ext = path.extension().and_then(|s| s.to_str());
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    for n in 2u32.. {
        let name = match ext {
            Some(e) => format!("{stem} ({n}).{e}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u32 ran out of collision suffixes")
}

/// Move a finished file to `target` (or its first free variant), creating
/// the folder. Returns the final path.
///
/// `rename` is atomic on the same filesystem; across filesystems it falls
/// back to copy + remove, so a clips folder on another drive still works.
pub fn move_into_place(src: &Path, target: &Path) -> std::io::Result<PathBuf> {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let dest = free_path(target);
    match std::fs::rename(src, &dest) {
        Ok(()) => Ok(dest),
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            std::fs::copy(src, &dest)?;
            std::fs::remove_file(src)?;
            Ok(dest)
        }
        Err(e) => Err(e),
    }
}

/// Byte offset of a `YYYY-MM-DD_HH-MM-SS` (or SteelSeries
/// `YYYY-MM-DD__HH-MM-SS`) timestamp in `stem`, searched from the end so a
/// game whose own name contains digits cannot shadow the real stamp.
fn stamp_offset(stem: &str) -> Option<usize> {
    let b = stem.as_bytes();
    let digits = |s: &[u8]| s.iter().all(u8::is_ascii_digit);
    let is_date = |s: &[u8]| s.len() >= 10 && digits(&s[..4]) && s[4] == b'-' && digits(&s[5..7]) && s[7] == b'-' && digits(&s[8..10]);
    let is_time = |s: &[u8]| s.len() >= 8 && digits(&s[..2]) && s[2] == b'-' && digits(&s[3..5]) && s[5] == b'-' && digits(&s[6..8]);
    (0..b.len()).rev().find(|&i| {
        let s = &b[i..];
        if !is_date(s) {
            return false;
        }
        let rest = &s[10..];
        (rest.first() == Some(&b'_') && is_time(&rest[1..]))
            || (rest.starts_with(b"__") && is_time(&rest[2..]))
    })
}

/// The game a clip belongs to, read back from its file stem: everything
/// before the timestamp, with the separators turned back into spaces.
///
/// Handles OpenGG names (`Elden Ring_2026-…`), the old underscore-for-space
/// names (`Elden_Ring_2026-…`), SteelSeries (`Elden_Ring__2026-…__…`) and
/// gpu-screen-recorder's own `Replay_2026-…`. Returns `None` when the stem
/// carries no timestamp, so the caller can fall back to the folder.
pub fn game_from_stem(stem: &str) -> Option<String> {
    let at = stamp_offset(stem)?;
    let prefix = stem[..at].trim_end_matches(['_', ' ', '-']);
    let name: String = prefix.replace('_', " ");
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// The game for a clip at `path` found while walking `root`: the name in
/// the file stem if it has a timestamp, else the first folder under `root`
/// (per-game folders, or folders the user sorted by hand), else the stem's
/// leading word — the long-standing fallback for arbitrary file names.
pub fn game_for_path(root: &Path, path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if let Some(g) = game_from_stem(stem) {
        return g;
    }
    if let Ok(rel) = path.strip_prefix(root) {
        let mut comps = rel.components();
        if let (Some(first), Some(_)) = (comps.next(), comps.next()) {
            let f = first.as_os_str().to_string_lossy().to_string();
            if !f.is_empty() {
                return f;
            }
        }
    }
    // A bare timestamp has no game in it; its leading "word" is the date.
    let lead = if stamp_offset(stem).is_some() {
        String::new()
    } else {
        stem.split('_').next().unwrap_or("").replace('-', " ")
    };
    if lead.trim().is_empty() {
        "Unknown".into()
    } else {
        lead
    }
}

/// Whether a library walk should skip `name`: dotfiles are caches and the
/// hidden `.part` files clips are written through before they are complete.
pub fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "opengg-org-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sanitize_keeps_spaces_and_symbols() {
        assert_eq!(sanitize_name("ELDEN RING™").as_deref(), Some("ELDEN RING™"));
        assert_eq!(sanitize_name("Counter-Strike 2").as_deref(), Some("Counter-Strike 2"));
        assert_eq!(sanitize_name("لعبة تجريبية").as_deref(), Some("لعبة تجريبية"));
    }

    #[test]
    fn sanitize_replaces_illegal_and_collapses() {
        assert_eq!(sanitize_name("My:Game/2  *Ultimate*").as_deref(), Some("My Game 2 Ultimate"));
        assert_eq!(sanitize_name("a\tb\nc").as_deref(), Some("a b c"));
        assert_eq!(sanitize_name("Game...").as_deref(), Some("Game"));
        assert_eq!(sanitize_name(".hidden").as_deref(), Some("hidden"));
        assert_eq!(sanitize_name("/:*?").as_deref(), None);
        assert_eq!(sanitize_name("   ").as_deref(), None);
    }

    #[test]
    fn sanitize_caps_length_by_chars() {
        let long = "A".repeat(100);
        assert_eq!(sanitize_name(&long).unwrap().chars().count(), 60);
        let wide = "é".repeat(100);
        assert_eq!(sanitize_name(&wide).unwrap().chars().count(), 60);
    }

    #[test]
    fn folder_name_prefers_first_usable_candidate() {
        assert_eq!(app_folder_name(["", "/:*", "Hades II", "hades2.exe"]), "Hades II");
        assert_eq!(app_folder_name(["plasmashell"]), "Desktop");
        assert_eq!(app_folder_name(["GNOME-SHELL"]), "Desktop");
        assert_eq!(app_folder_name(Vec::<&str>::new()), "Desktop");
    }

    #[test]
    fn target_path_with_and_without_folders() {
        let root = Path::new("/clips");
        assert_eq!(
            target_path(root, "Elden Ring", "2026-10-20_12-00-00", "mp4", false),
            PathBuf::from("/clips/Elden Ring_2026-10-20_12-00-00.mp4")
        );
        assert_eq!(
            target_path(root, "Elden Ring", "2026-10-20_12-00-00", "mp4", true),
            PathBuf::from("/clips/Elden Ring/Elden Ring_2026-10-20_12-00-00.mp4")
        );
        assert_eq!(
            target_path(root, "", "2026-10-20_12-00-00", "png", false),
            PathBuf::from("/clips/Desktop_2026-10-20_12-00-00.png")
        );
    }

    #[test]
    fn free_path_never_overwrites() {
        let d = tmpdir("free");
        let p = d.join("Game_2026-10-20_12-00-00.mp4");
        assert_eq!(free_path(&p), p);
        std::fs::write(&p, b"x").unwrap();
        let p2 = free_path(&p);
        assert_eq!(p2, d.join("Game_2026-10-20_12-00-00 (2).mp4"));
        std::fs::write(&p2, b"x").unwrap();
        assert_eq!(free_path(&p), d.join("Game_2026-10-20_12-00-00 (3).mp4"));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn move_into_place_creates_folder_and_dodges_collisions() {
        let d = tmpdir("move");
        let target = d.join("Hades").join("Hades_2026-10-20_12-00-00.mp4");
        for i in 0..2 {
            let src = d.join(format!("raw{i}.mp4"));
            std::fs::write(&src, b"x").unwrap();
            let got = move_into_place(&src, &target).unwrap();
            assert!(got.exists() && !src.exists());
        }
        assert!(target.exists());
        assert!(d.join("Hades").join("Hades_2026-10-20_12-00-00 (2).mp4").exists());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn game_from_stem_reads_every_naming_scheme() {
        assert_eq!(game_from_stem("Elden Ring_2026-10-20_12-00-00").as_deref(), Some("Elden Ring"));
        assert_eq!(game_from_stem("Elden Ring_2026-10-20_12-00-00 (2)").as_deref(), Some("Elden Ring"));
        // Old OpenGG names stored spaces as underscores — repaired on read.
        assert_eq!(game_from_stem("Elden_Ring_2026-10-20_12-00-00").as_deref(), Some("Elden Ring"));
        // SteelSeries GG export names.
        assert_eq!(game_from_stem("Elden_Ring__2026-10-20__12-00-00").as_deref(), Some("Elden Ring"));
        assert_eq!(game_from_stem("Replay_2026-10-20_12-00-00").as_deref(), Some("Replay"));
        assert_eq!(game_from_stem("Counter-Strike 2_2026-10-20_12-00-00").as_deref(), Some("Counter-Strike 2"));
        // Digits in a game name never shadow the real stamp.
        assert_eq!(game_from_stem("2077_2026-10-20_12-00-00").as_deref(), Some("2077"));
        assert_eq!(game_from_stem("2026-10-20_12-00-00"), None);
        assert_eq!(game_from_stem("holiday video"), None);
    }

    #[test]
    fn game_for_path_falls_back_to_folder_then_lead_word() {
        let root = Path::new("/clips");
        assert_eq!(game_for_path(root, Path::new("/clips/Hades II/Hades II_2026-10-20_12-00-00.mp4")), "Hades II");
        assert_eq!(game_for_path(root, Path::new("/clips/Apex Legends/Highlights/y.mp4")), "Apex Legends");
        assert_eq!(game_for_path(root, Path::new("/clips/holiday_video.mp4")), "holiday");
        assert_eq!(game_for_path(root, Path::new("/clips/2026-10-20_12-00-00.mp4")), "Unknown");
    }

    #[test]
    fn hidden_names() {
        assert!(is_hidden(".Game_2026.part.mp4"));
        assert!(!is_hidden("Game.mp4"));
    }
}

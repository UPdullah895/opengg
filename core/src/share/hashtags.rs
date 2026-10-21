//! Caption / hashtag helpers for the share sheet.
//!
//! The game tag is generated from the detected game title so the user gets a
//! sensible caption with zero typing; their own tags are remembered (minus the
//! per-clip game tag) and appended next time.

use super::planner::Platform;
use super::util::percent_encode;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// Trademark-style symbols are dropped before NFKC, which would otherwise
/// expand "™" into the letters "TM" ("ELDEN RING™" → "#EldenRingtm").
fn is_trademark_symbol(c: char) -> bool {
    matches!(c, '\u{2122}' | '\u{2120}' | '\u{00AE}' | '\u{00A9}')
}

/// "elden"/"ELDEN" → "Elden"; a word that is already mixed case ("TikTok",
/// "iRacing") is kept verbatim because its casing is deliberate branding.
fn pascal_word(w: &str) -> String {
    let has_upper = w.chars().any(char::is_uppercase);
    let has_lower = w.chars().any(char::is_lowercase);
    if has_upper && has_lower {
        return w.to_string();
    }
    let mut cs = w.chars();
    match cs.next() {
        Some(f) => f
            .to_uppercase()
            .chain(cs.flat_map(char::to_lowercase))
            .collect(),
        None => String::new(),
    }
}

/// Hashtag for a game title, or None when there is no real game
/// (empty, "Desktop", "Unknown").
///
/// Latin-script titles become PascalCase (the convention on every platform,
/// and readable without separators); other scripts have no letter case, so
/// words are joined with '_' to stay readable ("#لعبة_تجريبية").
pub fn game_tag(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("desktop")
        || trimmed.eq_ignore_ascii_case("unknown")
    {
        return None;
    }
    let norm: String = trimmed
        .chars()
        .filter(|&c| !is_trademark_symbol(c))
        .nfkc()
        .collect();
    let words: Vec<&str> = norm
        .split(|c: char| !(c.is_alphanumeric() || is_combining_mark(c)))
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        return None;
    }
    let latin = words.iter().all(|w| w.chars().all(|c| (c as u32) < 0x250));
    let body = if latin {
        words.iter().map(|w| pascal_word(w)).collect::<String>()
    } else {
        words.join("_")
    };
    Some(format!("#{body}"))
}

/// Approximates the `[\p{L}\p{Mn}\p{Nd}_]` class without a regex crate.
fn is_tag_char(c: char) -> bool {
    c == '_' || c.is_alphabetic() || c.is_numeric() || is_combining_mark(c)
}

/// All `#tag`s in `text`, in order (duplicates included).
pub fn extract_tags(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '#' {
            continue;
        }
        let mut tag = String::from("#");
        while let Some(&n) = chars.peek() {
            if !is_tag_char(n) {
                break;
            }
            tag.push(n);
            chars.next();
        }
        if tag.len() > 1 {
            out.push(tag);
        }
    }
    out
}

fn push_unique(out: &mut Vec<String>, tag: String) {
    let key = tag.to_lowercase();
    if !out.iter().any(|t| t.to_lowercase() == key) {
        out.push(tag);
    }
}

/// Default caption: the game's tag, then the user's remembered tags,
/// case-insensitively deduplicated.
pub fn caption(game: &str, saved_tags: &str) -> String {
    let mut out = Vec::new();
    if let Some(t) = game_tag(game) {
        out.push(t);
    }
    for t in extract_tags(saved_tags) {
        push_unique(&mut out, t);
    }
    out.join(" ")
}

/// What to persist from a caption the user just shared: their own tags only.
/// The game tag is dropped because it is regenerated per clip — remembering
/// it would stamp "#Valorant" onto every future Minecraft clip.
pub fn remember_tags(caption: &str, game: &str) -> String {
    let game_key = game_tag(game).map(|t| t.to_lowercase());
    let mut out = Vec::new();
    for t in extract_tags(caption) {
        if Some(t.to_lowercase()) != game_key {
            push_unique(&mut out, t);
        }
    }
    out.join(" ")
}

/// X "compose post" deep link with the caption pre-filled. X has no web
/// upload-by-URL, so the user attaches the file (from the clipboard) there.
pub fn x_intent_url(caption: &str) -> String {
    format!(
        "https://x.com/intent/post?text={}",
        percent_encode(caption, b"")
    )
}

/// Web upload page to open after copying the file. None where there is no
/// useful page (Discord: paste into a chat; X: use [`x_intent_url`]).
pub fn upload_page(platform: Platform) -> Option<&'static str> {
    match platform {
        Platform::TikTok => Some("https://www.tiktok.com/tiktokstudio/upload"),
        Platform::Instagram => Some("https://www.instagram.com/"),
        Platform::YouTubeShorts => Some("https://www.youtube.com/upload"),
        Platform::Original
        | Platform::Discord
        | Platform::DiscordNitro
        | Platform::X
        | Platform::XPremium => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_tags() {
        assert_eq!(game_tag("ELDEN RING™").as_deref(), Some("#EldenRing"));
        assert_eq!(
            game_tag("counter-strike 2").as_deref(),
            Some("#CounterStrike2")
        );
        assert_eq!(game_tag("TikTok").as_deref(), Some("#TikTok"));
        assert_eq!(
            game_tag("Pokémon Legends: Z-A").as_deref(),
            Some("#PokémonLegendsZA")
        );
        assert_eq!(game_tag("لعبة تجريبية").as_deref(), Some("#لعبة_تجريبية"));
        // NFKC folds full-width Latin.
        assert_eq!(game_tag("ＤＯＯＭ").as_deref(), Some("#Doom"));
        assert_eq!(game_tag(""), None);
        assert_eq!(game_tag("  "), None);
        assert_eq!(game_tag("Desktop"), None);
        assert_eq!(game_tag("unknown"), None);
        assert_eq!(game_tag("™ — !"), None);
    }

    #[test]
    fn tag_extraction() {
        assert_eq!(
            extract_tags("What a clutch #Valorant #gaming, #fps! # #a_b2"),
            vec!["#Valorant", "#gaming", "#fps", "#a_b2"]
        );
        assert_eq!(extract_tags("#لعبة_تجريبية رائع"), vec!["#لعبة_تجريبية"]);
    }

    #[test]
    fn captions_and_memory() {
        assert_eq!(
            caption("Valorant", "#gaming #fps"),
            "#Valorant #gaming #fps"
        );
        assert_eq!(caption("Valorant", "#valorant #FPS #fps"), "#Valorant #FPS");
        assert_eq!(caption("Desktop", "#gaming"), "#gaming");
        assert_eq!(caption("", ""), "");
        assert_eq!(
            remember_tags("What a clutch #Valorant #gaming #fps", "Valorant"),
            "#gaming #fps"
        );
        assert_eq!(remember_tags("#gaming #Gaming", "Desktop"), "#gaming");
    }

    #[test]
    fn urls() {
        assert_eq!(
            x_intent_url("Nice #EldenRing & co/op"),
            "https://x.com/intent/post?text=Nice%20%23EldenRing%20%26%20co%2Fop"
        );
        assert_eq!(
            upload_page(Platform::TikTok),
            Some("https://www.tiktok.com/tiktokstudio/upload")
        );
        assert_eq!(
            upload_page(Platform::YouTubeShorts),
            Some("https://www.youtube.com/upload")
        );
        assert_eq!(
            upload_page(Platform::Instagram),
            Some("https://www.instagram.com/")
        );
        assert_eq!(upload_page(Platform::Discord), None);
    }
}

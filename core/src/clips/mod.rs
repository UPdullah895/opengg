//! Clip library logic.
//!
//! Search-key helpers ported (behavior-preserving) from
//! `frontend/src/stores/replay.ts` per plan §2.1 rule 4. The SQLite clip DB and
//! FFmpeg-backed operations land here in later Phase 0 extraction steps; this
//! initial slice is the pure text/search logic covered by
//! `stores/replay.test.ts`.

mod db;
pub use db::*;
mod listers;
pub use listers::{get_clip_by_path, get_clips, get_clips_fast};

use unicode_normalization::UnicodeNormalization;

/// How ambiguous `YYYY/N/N` dates are read. Mirrors `DateFormat` in `replay.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateFormat {
    /// YYYY/MM/DD
    Ymd,
    /// YYYY/DD/MM
    Ydm,
}

const MONTH_NAMES_FULL: [&str; 13] = [
    "", "january", "february", "march", "april", "may", "june", "july", "august",
    "september", "october", "november", "december",
];

/// Normalize a game title into a stable, diacritic-free, lowercase search key.
/// Mirrors `normalizeGameTitle` in `replay.ts`:
/// strip ©®™ → NFKD → drop combining marks → collapse non-alphanumeric runs to
/// single spaces → trim → lowercase.
pub fn normalize_game_title(title: &str) -> String {
    let de_marked: String = title
        .chars()
        .map(|c| if matches!(c, '©' | '®' | '™') { ' ' } else { c })
        .collect::<String>()
        .nfkd()
        .filter(|&c| !('\u{300}'..='\u{36f}').contains(&c))
        .collect();

    // Replace `[^\p{L}\p{N}]+` with a single space (Rust's `char::is_alphanumeric`
    // is Unicode-aware, matching `\p{L}\p{N}`).
    let mut out = String::with_capacity(de_marked.len());
    let mut pending_space = false;
    for c in de_marked.chars() {
        if c.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.extend(c.to_lowercase());
        } else {
            pending_space = true;
        }
    }
    out
}

/// Parse a free-text search query into normalized match tokens.
/// Mirrors `parseSearchQuery` in `replay.ts`.
pub fn parse_search_query(q: &str, date_format: DateFormat) -> Vec<String> {
    let raw = q.trim().to_lowercase();
    if raw.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for tok in raw.split_whitespace() {
        if let Some(mm) = month_number(tok) {
            out.push(format!("__m{mm}"));
            continue;
        }
        let parts: Vec<&str> = tok.split(|c| c == '/' || c == '-').collect();
        // ^(\d{4})[/-](\d{1,2})[/-](\d{1,2})$
        if parts.len() == 3
            && is_year(parts[0])
            && is_1_2_digits(parts[1])
            && is_1_2_digits(parts[2])
        {
            let y = parts[0];
            let a = pad2(parts[1]);
            let b = pad2(parts[2]);
            let (mo, da) = match date_format {
                DateFormat::Ymd => (a, b),
                DateFormat::Ydm => (b, a),
            };
            out.push(format!("{y}/{mo}/{da}"));
            continue;
        }
        // ^(\d{4})[/-](\d{1,2})$
        if parts.len() == 2 && is_year(parts[0]) && is_1_2_digits(parts[1]) {
            out.push(format!("{}/{}", parts[0], pad2(parts[1])));
            continue;
        }
        // ^\d{4}$  (and the fall-through plain token both push the token as-is)
        out.push(tok.to_string());
    }
    out
}

/// Build the concatenated search index string for a clip.
/// Mirrors `buildSearch` in `replay.ts`.
pub fn build_search(clip: &ClipSearchFields) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !clip.custom_name.is_empty() {
        parts.push(clip.custom_name.to_lowercase());
    }
    if !clip.filename.is_empty() {
        parts.push(clip.filename.to_lowercase());
    }
    if !clip.game.is_empty() {
        parts.push(clip.game.to_lowercase());
    }
    if clip.created.len() >= 10 {
        let created = &clip.created;
        let y = &created[0..4];
        let m = &created[5..7];
        let d = &created[8..10];
        let mn: usize = m.parse().unwrap_or(0);
        parts.push(y.to_string());
        parts.push(format!("__m{m}"));
        if (1..=12).contains(&mn) {
            parts.push(MONTH_NAMES_FULL[mn].to_string());
        }
        parts.push(format!("{y}/{m}/{d}"));
        parts.push(format!("{y}/{d}/{m}"));
        parts.push(format!("{y}/{m}"));
        parts.push(format!("{y}-{m}"));
    }
    parts.join(" ")
}

/// Input fields for [`build_search`] (mirrors the JS object literal).
pub struct ClipSearchFields<'a> {
    pub custom_name: &'a str,
    pub filename: &'a str,
    pub game: &'a str,
    /// ISO-ish timestamp, e.g. `2024-03-15T10:00:00`.
    pub created: &'a str,
}

/// Two-digit zero-left-padded month/day, matching JS `String.padStart(2, '0')`.
fn pad2(s: &str) -> String {
    format!("{s:0>2}")
}

fn is_year(s: &str) -> bool {
    s.len() == 4 && s.bytes().all(|b| b.is_ascii_digit())
}

fn is_1_2_digits(s: &str) -> bool {
    (1..=2).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
}

/// Map an English month name/abbreviation to its two-digit number (MONTH_MAP).
fn month_number(tok: &str) -> Option<&'static str> {
    Some(match tok {
        "january" | "jan" => "01",
        "february" | "feb" => "02",
        "march" | "mar" => "03",
        "april" | "apr" => "04",
        "may" => "05",
        "june" | "jun" => "06",
        "july" | "jul" => "07",
        "august" | "aug" => "08",
        "september" | "sep" | "sept" => "09",
        "october" | "oct" => "10",
        "november" | "nov" => "11",
        "december" | "dec" => "12",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── normalizeGameTitle (mirrors stores/replay.test.ts) ──

    #[test]
    fn lowercases_and_trims() {
        assert_eq!(normalize_game_title("  HELLO  "), "hello");
    }

    #[test]
    fn replaces_special_chars_with_spaces() {
        assert_eq!(normalize_game_title("Game: Name!"), "game name");
    }

    #[test]
    fn removes_diacritics() {
        assert_eq!(normalize_game_title("Café Racer"), "cafe racer");
    }

    #[test]
    fn removes_trademark_symbols() {
        assert_eq!(normalize_game_title("Game™ ©®"), "game");
    }

    #[test]
    fn collapses_multiple_spaces() {
        assert_eq!(normalize_game_title("a    b"), "a b");
    }

    #[test]
    fn handles_empty_string() {
        assert_eq!(normalize_game_title(""), "");
    }

    // ── parseSearchQuery (mirrors stores/replay.test.ts) ──

    #[test]
    fn empty_query_returns_empty() {
        assert!(parse_search_query("", DateFormat::Ymd).is_empty());
    }

    #[test]
    fn whitespace_only_query_returns_empty() {
        assert!(parse_search_query("   ", DateFormat::Ymd).is_empty());
    }

    #[test]
    fn parses_plain_tokens() {
        assert_eq!(parse_search_query("hello world", DateFormat::Ymd), ["hello", "world"]);
    }

    #[test]
    fn parses_month_names() {
        assert_eq!(parse_search_query("jan", DateFormat::Ymd), ["__m01"]);
        assert_eq!(parse_search_query("december", DateFormat::Ymd), ["__m12"]);
    }

    #[test]
    fn parses_ymd() {
        assert_eq!(parse_search_query("2024/03/15", DateFormat::Ymd), ["2024/03/15"]);
    }

    #[test]
    fn parses_ydm() {
        assert_eq!(parse_search_query("2024/15/03", DateFormat::Ydm), ["2024/03/15"]);
    }

    #[test]
    fn parses_iso_dashes() {
        assert_eq!(parse_search_query("2024-03-15", DateFormat::Ymd), ["2024/03/15"]);
    }

    #[test]
    fn parses_year_month() {
        assert_eq!(parse_search_query("2024/3", DateFormat::Ymd), ["2024/03"]);
    }

    #[test]
    fn parses_four_digit_year() {
        assert_eq!(parse_search_query("2024", DateFormat::Ymd), ["2024"]);
    }

    // ── buildSearch (mirrors stores/replay.test.ts) ──

    #[test]
    fn combines_all_parts() {
        let r = build_search(&ClipSearchFields {
            custom_name: "My Clip",
            filename: "clip.mp4",
            game: "Apex",
            created: "2024-03-15T10:00:00",
        });
        for needle in ["my clip", "clip.mp4", "apex", "2024", "__m03", "2024/03/15"] {
            assert!(r.contains(needle), "missing {needle:?} in {r:?}");
        }
    }

    #[test]
    fn handles_missing_fields() {
        let r = build_search(&ClipSearchFields {
            custom_name: "",
            filename: "",
            game: "",
            created: "",
        });
        assert_eq!(r, "");
    }
}

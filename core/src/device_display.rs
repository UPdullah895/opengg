//! Device name presentation — Devices regression pass.
//!
//! Vendors ship long marketing names ("Logitech G502 LIGHTSPEED Wireless
//! Gaming Mouse") where the tail mostly repeats what the UI already states
//! structurally: the device-type line under the title says "mouse", and the
//! connection badge beside the title says wired/wireless. Rendering all of it
//! verbatim wastes the whole card width on words the user already has, and
//! forces eliding on a string whose *informative* part — the model — sits at
//! the front.
//!
//! This trims only a trailing run of category/marketing words, and only from
//! the end, so anything load-bearing in the middle of a name survives. It is
//! deliberately conservative: an unrecognized word stops the trim, and a name
//! is never reduced below two tokens (so a device genuinely called "Gaming
//! Mouse" keeps its name rather than being trimmed to nothing).

/// Trailing words safe to drop, lowercase. Each either restates the device
/// type (already shown as its own line), restates the connection (already
/// shown as its own badge), or is pure marketing garnish.
///
/// Deliberately excludes words that are load-bearing parts of real model
/// names — "pro" ("G Pro"), "max", "elite", "s", any bare number — because
/// dropping one of those changes *which product* the name refers to, which is
/// far worse than leaving a word too many on screen.
const TRAILING_NOISE: &[&str] = &[
    // device category — duplicated by the type line
    "mouse",
    "mice",
    "keyboard",
    "headset",
    "headphones",
    "headphone",
    "gamepad",
    "controller",
    // connection — duplicated by the connection badge
    "wireless",
    "wired",
    "receiver",
    "dongle",
    // marketing garnish
    "gaming",
    "edition",
    "rgb",
    "lightspeed",
    "lightsync",
    "hero",
];

/// Number of tokens a trimmed name may never fall below.
const MIN_TOKENS: usize = 2;

/// Trim a vendor's full marketing name down to the brand+model part.
///
/// Returns the input unchanged (aside from whitespace normalization) when
/// nothing is safe to drop — including for already-clean names such as
/// `"Arctis Nova 7"`.
pub fn display_name(full_name: &str) -> String {
    let mut tokens: Vec<&str> = full_name.split_whitespace().collect();
    if tokens.is_empty() {
        return String::new();
    }

    while tokens.len() > MIN_TOKENS {
        let last = tokens[tokens.len() - 1].to_ascii_lowercase();
        // Compare on the bare word so trailing punctuation doesn't hide a
        // match, but keep the original token if it survives.
        let bare = last.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        if TRAILING_NOISE.contains(&bare) {
            tokens.pop();
        } else {
            break;
        }
    }

    tokens.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_the_trailing_marketing_run_off_a_real_vendor_name() {
        // The exact name the connected G502 reports over ratbagd.
        assert_eq!(
            display_name("Logitech G502 LIGHTSPEED Wireless Gaming Mouse"),
            "Logitech G502"
        );
    }

    #[test]
    fn leaves_an_already_clean_name_alone() {
        assert_eq!(display_name("Arctis Nova 7"), "Arctis Nova 7");
        assert_eq!(display_name("Logitech G502"), "Logitech G502");
    }

    #[test]
    fn keeps_load_bearing_model_words_that_merely_look_like_garnish() {
        // "Pro" is part of the product's identity, not a suffix to drop —
        // trimming it would name a different mouse.
        assert_eq!(
            display_name("Logitech G Pro Wireless Gaming Mouse"),
            "Logitech G Pro"
        );
    }

    #[test]
    fn stops_at_the_first_word_it_does_not_recognize() {
        // "Superlight" isn't in the table, so the trim must halt there rather
        // than continuing to chew through the name.
        assert_eq!(
            display_name("Logitech G Pro X Superlight Wireless Mouse"),
            "Logitech G Pro X Superlight"
        );
    }

    #[test]
    fn never_trims_below_two_tokens() {
        // Every token is noise; the floor keeps the name from vanishing.
        assert_eq!(display_name("Gaming Mouse"), "Gaming Mouse");
        assert_eq!(display_name("Wireless Gaming Mouse"), "Wireless Gaming");
    }

    #[test]
    fn handles_empty_and_whitespace_only_input() {
        assert_eq!(display_name(""), "");
        assert_eq!(display_name("   "), "");
    }

    #[test]
    fn normalizes_runs_of_whitespace() {
        assert_eq!(display_name("Logitech   G502  Gaming Mouse"), "Logitech G502");
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert_eq!(display_name("Logitech G502 WIRELESS mouse"), "Logitech G502");
    }
}

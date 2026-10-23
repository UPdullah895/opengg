//! Colour "looks": one-click grading presets rendered as ffmpeg filter chains.
//!
//! A look is stored in the edit project as a small enum (not as a filter
//! string) so the project JSON stays stable if a preset is retuned later, and
//! so QML can show a picker without knowing anything about ffmpeg.
//!
//! Every look has a *strength* `k` in `[0, 1]`. Rather than cross-fading the
//! graded frame with the original (which would need a `split` + `blend` per
//! look and double the per-frame cost), each filter parameter is interpolated
//! towards its identity value:
//! - multiplicative params (eq `saturation`/`contrast`) scale as `1 + (v-1)·k`
//!   because their identity is 1;
//! - offset params (colorbalance shifts, unsharp amount, eq `brightness`)
//!   scale as `v·k` because their identity is 0.
//!
//! The result at `k = 0` is a no-op chain, so `filter()` returns `None` there
//! and the graph builder skips the stage entirely.

use serde::{Deserialize, Serialize};

/// A colour-grading preset. Serialised as its lowercase id (e.g. `"vivid"`),
/// which is also what QML uses to name/select it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Look {
    #[default]
    None,
    Vivid,
    Cinematic,
    Warm,
    Cool,
    Sharp,
    Mono,
    Teal,
    Retro,
    Golden,
    Night,
    Pop,
    Grain,
}

impl Look {
    /// Every look in picker order (`None` first so "no look" is the default tile).
    pub fn all() -> &'static [Look] {
        &[
            Look::None,
            Look::Vivid,
            Look::Cinematic,
            Look::Warm,
            Look::Cool,
            Look::Sharp,
            Look::Mono,
            Look::Teal,
            Look::Retro,
            Look::Golden,
            Look::Night,
            Look::Pop,
            Look::Grain,
        ]
    }

    /// Stable lowercase id — identical to the serde representation, so a
    /// value read from `projectJson` can be passed straight back to Rust.
    pub fn id(self) -> &'static str {
        match self {
            Look::None => "none",
            Look::Vivid => "vivid",
            Look::Cinematic => "cinematic",
            Look::Warm => "warm",
            Look::Cool => "cool",
            Look::Sharp => "sharp",
            Look::Mono => "mono",
            Look::Teal => "teal",
            Look::Retro => "retro",
            Look::Golden => "golden",
            Look::Night => "night",
            Look::Pop => "pop",
            Look::Grain => "grain",
        }
    }

    /// Inverse of [`Look::id`]; unknown ids yield `None` (the Rust Option),
    /// so a typo from QML can't silently become a real look.
    pub fn from_id(id: &str) -> Option<Look> {
        Look::all().iter().copied().find(|l| l.id() == id)
    }
}

/// Format a filter parameter: at most 4 decimals, trailing zeros trimmed,
/// never `-0` (ffmpeg accepts it, but it makes pinned strings flaky).
fn n(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    };
    if s == "-0" {
        "0".to_string()
    } else {
        s
    }
}

/// Render `look` at `strength` as a comma-separated ffmpeg video filter chain.
///
/// Returns `None` for [`Look::None`] or a non-positive strength, so callers
/// can omit the stage entirely instead of emitting a no-op filter.
pub fn filter(look: Look, strength: f64) -> Option<String> {
    let k = strength.min(1.0);
    if look == Look::None || k.is_nan() || k <= 0.0 {
        return None;
    }
    // Multiplicative (identity 1) and offset (identity 0) interpolation.
    let m = |v: f64| n(1.0 + (v - 1.0) * k);
    let o = |v: f64| n(v * k);
    let s = match look {
        Look::None => return None,
        Look::Vivid => format!("eq=saturation={}:contrast={}", m(1.35), m(1.08)),
        Look::Cinematic => format!(
            "eq=contrast={}:saturation={},colorbalance=rs={}:bs={}:rh={}:bh={}",
            m(1.12),
            m(0.85),
            o(-0.05),
            o(0.07),
            o(0.06),
            o(-0.04)
        ),
        Look::Warm => format!(
            "colorbalance=rs={}:gs={}:bs={}:rm={}:bm={}",
            o(0.08),
            o(0.02),
            o(-0.08),
            o(0.05),
            o(-0.05)
        ),
        Look::Cool => format!("colorbalance=rs={}:bs={}:bm={}", o(-0.07), o(0.08), o(0.05)),
        Look::Sharp => format!("unsharp=5:5:{}:5:5:0", o(0.9)),
        Look::Mono => format!("hue=s={}", n(1.0 - k)),
        Look::Teal => format!(
            "colorbalance=rs={}:gs={}:bs={}:rh={}:gh={}:bh={},eq=saturation={}",
            o(-0.10),
            o(0.02),
            o(0.14),
            o(0.12),
            o(0.04),
            o(-0.14),
            m(1.1)
        ),
        Look::Retro => format!(
            "curves=all='0/{} 0.5/0.5 1/{}',eq=saturation={}:contrast={},colorbalance=rm={}:bm={}",
            n(0.07 * k),
            n(1.0 - 0.06 * k),
            m(0.82),
            m(0.95),
            o(0.05),
            o(-0.05)
        ),
        Look::Golden => format!(
            "colorbalance=rm={}:gm={}:bm={}:rh={}:bh={},eq=saturation={}:brightness={}",
            o(0.10),
            o(0.04),
            o(-0.12),
            o(0.08),
            o(-0.08),
            m(1.1),
            o(0.02)
        ),
        Look::Night => format!(
            "eq=brightness={}:contrast={}:saturation={},colorbalance=rs={}:bs={}:bm={}",
            o(-0.05),
            m(1.08),
            m(0.9),
            o(-0.06),
            o(0.14),
            o(0.06)
        ),
        Look::Pop => format!(
            "eq=contrast={}:saturation={},unsharp=5:5:{}:5:5:0",
            m(1.25),
            m(1.5),
            o(0.5)
        ),
        // Fixed seed: the same project must export to the same pixels, and a
        // random seed would also make re-exports needlessly differ.
        Look::Grain => format!(
            "noise=alls={}:allf=t+u:all_seed=7",
            (16.0 * k).round() as i64
        ),
    };
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_and_zero_strength_are_skipped() {
        assert_eq!(filter(Look::None, 1.0), None);
        assert_eq!(filter(Look::Vivid, 0.0), None);
        assert_eq!(filter(Look::Vivid, -1.0), None);
    }

    #[test]
    fn vivid_scales_multiplicatively() {
        assert_eq!(
            filter(Look::Vivid, 1.0).unwrap(),
            "eq=saturation=1.35:contrast=1.08"
        );
        assert_eq!(
            filter(Look::Vivid, 0.5).unwrap(),
            "eq=saturation=1.175:contrast=1.04"
        );
        // Strength above 1 is clamped, not extrapolated.
        assert_eq!(filter(Look::Vivid, 3.0), filter(Look::Vivid, 1.0));
    }

    #[test]
    fn offsets_scale_linearly() {
        assert_eq!(filter(Look::Sharp, 1.0).unwrap(), "unsharp=5:5:0.9:5:5:0");
        assert_eq!(filter(Look::Sharp, 0.5).unwrap(), "unsharp=5:5:0.45:5:5:0");
        assert_eq!(
            filter(Look::Cinematic, 1.0).unwrap(),
            "eq=contrast=1.12:saturation=0.85,colorbalance=rs=-0.05:bs=0.07:rh=0.06:bh=-0.04"
        );
    }

    #[test]
    fn full_strength_definitions() {
        assert_eq!(filter(Look::Mono, 1.0).unwrap(), "hue=s=0");
        assert_eq!(filter(Look::Mono, 0.25).unwrap(), "hue=s=0.75");
        assert_eq!(
            filter(Look::Teal, 1.0).unwrap(),
            "colorbalance=rs=-0.1:gs=0.02:bs=0.14:rh=0.12:gh=0.04:bh=-0.14,eq=saturation=1.1"
        );
        assert_eq!(
            filter(Look::Retro, 1.0).unwrap(),
            "curves=all='0/0.07 0.5/0.5 1/0.94',eq=saturation=0.82:contrast=0.95,colorbalance=rm=0.05:bm=-0.05"
        );
        assert_eq!(
            filter(Look::Grain, 1.0).unwrap(),
            "noise=alls=16:allf=t+u:all_seed=7"
        );
        assert_eq!(
            filter(Look::Grain, 0.5).unwrap(),
            "noise=alls=8:allf=t+u:all_seed=7"
        );
        for l in Look::all() {
            assert_eq!(filter(*l, 1.0).is_none(), *l == Look::None);
        }
    }

    #[test]
    fn ids_round_trip_and_match_serde() {
        for l in Look::all() {
            assert_eq!(Look::from_id(l.id()), Some(*l));
            assert_eq!(serde_json::to_string(l).unwrap(), format!("\"{}\"", l.id()));
        }
        assert_eq!(Look::from_id("bogus"), None);
    }
}

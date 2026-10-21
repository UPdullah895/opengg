//! Pure planning maths for "Share" exports.
//!
//! Everything here is deterministic and side-effect free so the QML preview
//! (size estimate, resolution ladder, trim window) can be recomputed on every
//! slider tick without touching ffmpeg, and so the numbers the user sees are
//! exactly the numbers [`super::transcode::render`] encodes with.

use serde::{Deserialize, Serialize};

/// Where the exported file is going. The id (serde, lowercase) is what QML and
/// the settings file use, so renaming a variant is a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Original,
    Discord,
    DiscordNitro,
    X,
    XPremium,
    TikTok,
    Instagram,
    YouTubeShorts,
}

/// The limits a platform imposes on an upload.
///
/// `None` means "no practical limit". Sizes are decimal bytes because that is
/// how every platform advertises them (Discord's "10 MB" is 10,000,000).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    pub max_bytes: Option<u64>,
    pub max_secs: Option<f64>,
    pub max_short_side: u32,
    pub max_fps: f64,
    /// The platform is a vertical feed — the UI should pre-tick the 9:16 crop.
    pub vertical_default: bool,
}

impl Platform {
    pub const ALL: [Platform; 8] = [
        Platform::Original,
        Platform::Discord,
        Platform::DiscordNitro,
        Platform::X,
        Platform::XPremium,
        Platform::TikTok,
        Platform::Instagram,
        Platform::YouTubeShorts,
    ];

    /// Stable lowercase id (same as the serde form).
    pub fn id(self) -> &'static str {
        match self {
            Platform::Original => "original",
            Platform::Discord => "discord",
            Platform::DiscordNitro => "discordnitro",
            Platform::X => "x",
            Platform::XPremium => "xpremium",
            Platform::TikTok => "tiktok",
            Platform::Instagram => "instagram",
            Platform::YouTubeShorts => "youtubeshorts",
        }
    }

    pub fn from_id(id: &str) -> Option<Platform> {
        Platform::ALL.into_iter().find(|p| p.id() == id)
    }

    /// Human-readable name for the UI.
    pub fn label(self) -> &'static str {
        match self {
            Platform::Original => "Original",
            Platform::Discord => "Discord",
            Platform::DiscordNitro => "Discord Nitro",
            Platform::X => "X",
            Platform::XPremium => "X Premium",
            Platform::TikTok => "TikTok",
            Platform::Instagram => "Instagram",
            Platform::YouTubeShorts => "YouTube Shorts",
        }
    }

    pub fn spec(self) -> Spec {
        let s = |max_bytes, max_secs, max_short_side, max_fps, vertical_default| Spec {
            max_bytes,
            max_secs,
            max_short_side,
            max_fps,
            vertical_default,
        };
        match self {
            Platform::Original => s(None, None, 2160, 240.0, false),
            // Free Discord upload limit is 10 MB (decimal).
            Platform::Discord => s(Some(10_000_000), None, 1080, 60.0, false),
            Platform::DiscordNitro => s(Some(500_000_000), None, 2160, 60.0, false),
            Platform::X => s(Some(512_000_000), Some(140.0), 1080, 60.0, false),
            Platform::XPremium => s(Some(8_000_000_000), Some(10800.0), 1080, 60.0, false),
            Platform::TikTok => s(None, Some(600.0), 2160, 60.0, true),
            Platform::Instagram => s(None, Some(180.0), 2160, 60.0, true),
            Platform::YouTubeShorts => s(None, Some(180.0), 2160, 60.0, true),
        }
    }
}

fn min_opt<T: PartialOrd + Copy>(a: Option<T>, b: Option<T>) -> Option<T> {
    match (a, b) {
        (Some(x), Some(y)) => Some(if y < x { y } else { x }),
        (x, None) => x,
        (None, y) => y,
    }
}

/// One file that must satisfy several platforms at once (e.g. "Discord + X"):
/// the minimum of every limit. `vertical_default` is only true when *every*
/// target is a vertical feed. An empty slice yields the Original spec.
pub fn strictest(platforms: &[Platform]) -> Spec {
    let mut it = platforms.iter().map(|p| p.spec());
    let Some(mut acc) = it.next() else {
        return Platform::Original.spec();
    };
    for s in it {
        acc.max_bytes = min_opt(acc.max_bytes, s.max_bytes);
        acc.max_secs = min_opt(acc.max_secs, s.max_secs);
        acc.max_short_side = acc.max_short_side.min(s.max_short_side);
        acc.max_fps = acc.max_fps.min(s.max_fps);
        acc.vertical_default = acc.vertical_default && s.vertical_default;
    }
    acc
}

// ─── Trim window ────────────────────────────────────────────────────────────

/// Shortest export we allow: one second, or the whole clip if it is shorter.
pub fn min_window(clip_dur: f64) -> f64 {
    clip_dur.clamp(0.0, 1.0)
}

/// Initial trim window: the LAST `max_secs` of the clip, because a replay
/// buffer is saved right after the moment happened, so the action is at the
/// end. Clips that already fit are exported whole.
pub fn default_window(clip_dur: f64, spec: &Spec) -> (f64, f64) {
    let clip = clip_dur.max(0.0);
    match spec.max_secs {
        Some(max) if clip > max => (clip - max, clip),
        _ => (0.0, clip),
    }
}

/// Force a user-chosen window into the clip and the platform's length limit.
///
/// When the window is too long we keep the user's END and move the start,
/// for the same reason as [`default_window`]: the end is the payoff.
pub fn clamp_window(start: f64, end: f64, clip_dur: f64, spec: &Spec) -> (f64, f64) {
    let clip = clip_dur.max(0.0);
    let min = min_window(clip);
    let mut s = start.clamp(0.0, clip);
    let mut e = end.clamp(0.0, clip);
    if e < s {
        std::mem::swap(&mut s, &mut e);
    }
    if e - s < min {
        e = clip.min(s + min);
        s = e - min;
    }
    if let Some(max) = spec.max_secs {
        if e - s > max {
            s = e - max;
        }
    }
    (s, e)
}

// ─── Geometry ───────────────────────────────────────────────────────────────

/// A crop rectangle in source pixels (always even, as yuv420p requires).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 9:16 crop for vertical feeds. `pos` (0 = left edge, 1 = right edge) is the
/// horizontal slider so the user can follow the action instead of a fixed
/// centre crop. Sources narrower than 9:16 keep their full width and are
/// cropped vertically around the centre instead.
pub fn vertical_crop(src_w: u32, src_h: u32, pos: f64) -> Crop {
    let pos = if pos.is_finite() {
        pos.clamp(0.0, 1.0)
    } else {
        0.5
    };
    let nine_sixteen_w = (src_h as u64 * 9 / 16) as u32;
    let wide = nine_sixteen_w <= src_w;
    let cw = (if wide { nine_sixteen_w } else { src_w }) & !1;
    let ch = (if wide {
        src_h
    } else {
        src_h.min((cw as u64 * 16 / 9) as u32)
    }) & !1;
    let x = (((src_w - cw) as f64) * pos).round() as u32 & !1;
    let y = ((src_h - ch) / 2) & !1;
    Crop { x, y, w: cw, h: ch }
}

/// Scale (w,h) so its short side becomes `short_side`, keeping aspect and
/// rounding both sides to even. Upscaling is allowed — some platforms
/// recompress anything under 1080p harder, so enlarging can be the better
/// export.
pub fn scale_to_short_side(w: u32, h: u32, short_side: u32) -> (u32, u32) {
    let short = w.min(h);
    if short == 0 {
        return (0, 0);
    }
    let f = short_side as f64 / short as f64;
    let sw = (w as f64 * f).round() as u32 & !1;
    let sh = (h as f64 * f).round() as u32 & !1;
    (sw, sh)
}

/// One entry of the resolution picker. `original` marks the source's own
/// short side (no resampling at all).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LadderStep {
    pub short_side: u32,
    pub original: bool,
}

/// Standard rungs offered in the resolution picker.
pub const LADDER_RUNGS: [u32; 5] = [480, 720, 1080, 1440, 2160];

/// Resolution choices for a source whose (post-crop) short side is
/// `src_short`: the standard rungs the platform accepts, plus the source's own
/// size (flagged) when allowed. Sorted ascending, deduplicated.
pub fn size_ladder(src_short: u32, max_short_side: u32) -> Vec<LadderStep> {
    let mut v: Vec<LadderStep> = LADDER_RUNGS
        .iter()
        .copied()
        .filter(|&s| s <= max_short_side)
        .map(|s| LadderStep {
            short_side: s,
            original: s == src_short,
        })
        .collect();
    if src_short > 0 && src_short <= max_short_side && !v.iter().any(|s| s.original) {
        v.push(LadderStep {
            short_side: src_short,
            original: true,
        });
    }
    v.sort_by_key(|s| s.short_side);
    v.dedup_by_key(|s| s.short_side);
    v
}

// ─── Bitrate ────────────────────────────────────────────────────────────────

/// Audio budget for a clip that has at least one selected track (AAC stereo).
pub const AUDIO_BPS: u64 = 160_000;
/// Floor so a hopeless size cap still produces *something* watchable-ish
/// instead of a zero/negative bitrate; the retry rule then decides.
pub const MIN_VIDEO_BPS: u64 = 100_000;
/// Reference quality: 12 Mb/s at 1080p, scaled linearly by pixel count.
pub const REF_BPS_1080P: f64 = 12_000_000.0;
/// Fraction of a byte cap we plan for, leaving room for container overhead
/// and encoder rate-control error.
pub const SIZE_SAFETY: f64 = 0.92;
/// Bits-per-pixel-per-frame below which we halve the frame rate (to 30):
/// fewer, cleaner frames look better than many mushy ones.
pub const FULL_FPS_BPP: f64 = 0.035;

pub fn audio_bps_for(has_audio: bool) -> u64 {
    if has_audio {
        AUDIO_BPS
    } else {
        0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitratePlan {
    pub video_bps: u64,
    pub audio_bps: u64,
    pub fps: f64,
    /// True when the bitrate is comfortable at the full frame rate. False means
    /// the frame rate was capped to 30 (a no-op for ≤30 fps sources, but the
    /// default-size walk still treats it as "starved" and steps down).
    pub full_fps: bool,
}

/// Choose the video bitrate and frame rate for an output of `w`×`h`.
pub fn bitrate_plan(
    w: u32,
    h: u32,
    src_fps: f64,
    duration: f64,
    spec: &Spec,
    audio_bps: u64,
) -> BitratePlan {
    let src = if src_fps > 0.0 && src_fps.is_finite() {
        src_fps
    } else {
        60.0
    };
    let fast_fps = src.min(spec.max_fps);
    let pixels = w as f64 * h as f64;
    let mut video = REF_BPS_1080P * pixels / (1920.0 * 1080.0);
    if let Some(max) = spec.max_bytes {
        if duration > 0.0 {
            let cap = max as f64 * SIZE_SAFETY * 8.0 / duration - audio_bps as f64;
            video = video.min(cap);
        }
    }
    let video_bps = (video.max(MIN_VIDEO_BPS as f64)) as u64;
    let bpp = if pixels > 0.0 {
        video_bps as f64 / (pixels * fast_fps)
    } else {
        f64::INFINITY
    };
    let full_fps = bpp >= FULL_FPS_BPP;
    let fps = if full_fps {
        fast_fps
    } else {
        fast_fps.min(30.0)
    };
    BitratePlan {
        video_bps,
        audio_bps,
        fps,
        full_fps,
    }
}

/// Pick the default resolution: start at the source's own size (or the top
/// rung when the source is too big for the platform) and step down until the
/// bitrate is high enough to keep the full frame rate. If nothing qualifies,
/// the smallest rung — smallest is the least-bad option under a hard cap.
pub fn default_short_side(
    ladder: &[LadderStep],
    base_w: u32,
    base_h: u32,
    src_fps: f64,
    duration: f64,
    spec: &Spec,
    audio_bps: u64,
) -> u32 {
    if ladder.is_empty() {
        return base_w.min(base_h);
    }
    let start = ladder
        .iter()
        .position(|s| s.original)
        .unwrap_or(ladder.len() - 1);
    for step in ladder[..=start].iter().rev() {
        let (w, h) = scale_to_short_side(base_w, base_h, step.short_side);
        if bitrate_plan(w, h, src_fps, duration, spec, audio_bps).full_fps {
            return step.short_side;
        }
    }
    ladder[0].short_side
}

/// Expected output size in bytes (without container overhead).
pub fn estimate_bytes(plan: &BitratePlan, duration: f64) -> u64 {
    ((plan.video_bps + plan.audio_bps) as f64 * duration.max(0.0) / 8.0) as u64
}

/// Decimal, platform-style size label: "8.4 MB", "213 MB", "1.2 GB".
pub fn format_bytes(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 10e6 {
        format!("{:.0} MB", b / 1e6)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} KB", b / 1e3)
    } else {
        format!("{bytes} B")
    }
}

// ─── Retry after overshoot ──────────────────────────────────────────────────

pub const MAX_RETRIES: u32 = 3;
pub const MIN_RETRY_BPS: u64 = 50_000;
pub const TOO_LONG_ERROR: &str = "Too long for this size limit — choose a shorter part";

/// After an encode came out bigger than `max_bytes`, compute the next video
/// bitrate. Encoders (especially hardware ones at low rates) overshoot, so we
/// measure the real video share of the file and scale against it, always
/// shrinking by at least 10 %. `retries_done` counts earlier retries.
pub fn retry_bitrate(
    bitrate: u64,
    file_size: u64,
    max_bytes: u64,
    audio_bps: u64,
    duration: f64,
    retries_done: u32,
) -> Result<u64, String> {
    if retries_done >= MAX_RETRIES {
        return Err(TOO_LONG_ERROR.into());
    }
    let audio_bytes = audio_bps as f64 * duration.max(0.0) / 8.0;
    let video_bytes = file_size as f64 - audio_bytes;
    let budget = max_bytes as f64 - (file_size as f64 - video_bytes);
    if budget <= 0.0 || video_bytes <= 0.0 {
        return Err(TOO_LONG_ERROR.into());
    }
    let factor = (0.95 * budget / video_bytes).min(0.9);
    let next = (bitrate as f64 * factor) as u64;
    if next < MIN_RETRY_BPS {
        return Err(TOO_LONG_ERROR.into());
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(l: &[LadderStep]) -> Vec<(u32, bool)> {
        l.iter().map(|s| (s.short_side, s.original)).collect()
    }

    #[test]
    fn platform_ids_roundtrip_serde() {
        for p in Platform::ALL {
            let j = serde_json::to_string(&p).unwrap();
            assert_eq!(j, format!("\"{}\"", p.id()));
            assert_eq!(serde_json::from_str::<Platform>(&j).unwrap(), p);
            assert_eq!(Platform::from_id(p.id()), Some(p));
        }
        assert_eq!(Platform::from_id("myspace"), None);
    }

    #[test]
    fn specs_match_table() {
        let d = Platform::Discord.spec();
        assert_eq!(d.max_bytes, Some(10_000_000));
        assert_eq!(d.max_secs, None);
        assert_eq!(d.max_short_side, 1080);
        let x = Platform::X.spec();
        assert_eq!((x.max_bytes, x.max_secs), (Some(512_000_000), Some(140.0)));
        assert!(Platform::TikTok.spec().vertical_default);
        assert!(!Platform::Original.spec().vertical_default);
        assert_eq!(Platform::Original.spec().max_fps, 240.0);
    }

    #[test]
    fn strictest_takes_min_of_each_field() {
        let s = strictest(&[Platform::Discord, Platform::X]);
        assert_eq!(s.max_bytes, Some(10_000_000));
        assert_eq!(s.max_secs, Some(140.0));
        assert_eq!(s.max_short_side, 1080);
        assert_eq!(s.max_fps, 60.0);
        assert!(!s.vertical_default);
        let v = strictest(&[Platform::TikTok, Platform::Instagram]);
        assert_eq!(v.max_secs, Some(180.0));
        assert!(v.vertical_default);
        assert_eq!(strictest(&[]), Platform::Original.spec());
    }

    #[test]
    fn default_window_takes_the_end() {
        let x = Platform::X.spec();
        assert_eq!(default_window(300.0, &x), (160.0, 300.0));
        assert_eq!(default_window(60.0, &x), (0.0, 60.0));
        assert_eq!(
            default_window(300.0, &Platform::Discord.spec()),
            (0.0, 300.0)
        );
    }

    #[test]
    fn clamp_window_pinned_cases() {
        let o = Platform::Original.spec();
        assert_eq!(clamp_window(100.0, 100.2, 300.0, &o), (100.0, 101.0));
        assert_eq!(clamp_window(300.0, 300.0, 300.0, &o), (299.0, 300.0));
        // Too long keeps the user's end.
        let x = Platform::X.spec();
        assert_eq!(clamp_window(0.0, 250.0, 300.0, &x), (110.0, 250.0));
        // Out of range values are pulled in; reversed is swapped.
        assert_eq!(clamp_window(-5.0, 400.0, 300.0, &o), (0.0, 300.0));
        assert_eq!(clamp_window(20.0, 10.0, 300.0, &o), (10.0, 20.0));
        // Clip shorter than the minimum: whole clip.
        assert_eq!(clamp_window(0.2, 0.3, 0.5, &o), (0.0, 0.5));
    }

    #[test]
    fn vertical_crop_pinned_cases() {
        assert_eq!(
            vertical_crop(2560, 1440, 0.5),
            Crop {
                x: 874,
                y: 0,
                w: 810,
                h: 1440
            }
        );
        assert_eq!(
            vertical_crop(1920, 1080, 1.0),
            Crop {
                x: 1314,
                y: 0,
                w: 606,
                h: 1080
            }
        );
        assert_eq!(vertical_crop(1920, 1080, 0.0).x, 0);
        // Already-narrow source (portrait): full width, centred vertically.
        let c = vertical_crop(720, 1600, 0.5);
        assert_eq!((c.w, c.h, c.x), (720, 1280, 0));
        assert_eq!(c.y, 160);
    }

    #[test]
    fn scaling_keeps_aspect_and_even() {
        assert_eq!(scale_to_short_side(2560, 1440, 1080), (1920, 1080));
        assert_eq!(scale_to_short_side(2560, 1440, 480), (852, 480));
        assert_eq!(scale_to_short_side(810, 1440, 1080), (1080, 1920));
        assert_eq!(scale_to_short_side(0, 0, 1080), (0, 0));
    }

    #[test]
    fn ladder_pinned_cases() {
        assert_eq!(
            steps(&size_ladder(1440, 2160)),
            vec![
                (480, false),
                (720, false),
                (1080, false),
                (1440, true),
                (2160, false)
            ]
        );
        assert_eq!(
            steps(&size_ladder(1440, 1080)),
            vec![(480, false), (720, false), (1080, false)]
        );
        assert_eq!(
            steps(&size_ladder(810, 1080)),
            vec![(480, false), (720, false), (810, true), (1080, false)]
        );
    }

    #[test]
    fn bitrate_pinned_cases() {
        let o = Platform::Original.spec();
        assert_eq!(
            bitrate_plan(3840, 2160, 60.0, 10.0, &o, 0).video_bps,
            48_000_000
        );
        assert_eq!(
            bitrate_plan(1280, 720, 60.0, 10.0, &o, 0).video_bps,
            5_333_333
        );
        assert_eq!(
            bitrate_plan(810, 1440, 60.0, 10.0, &o, 0).video_bps,
            6_750_000
        );
        // Unknown source fps assumes 60, capped by the platform.
        let p = bitrate_plan(1920, 1080, 0.0, 10.0, &Platform::X.spec(), AUDIO_BPS);
        assert_eq!(p.fps, 60.0);
        assert!(p.full_fps);
        // 144 fps source on a 60 fps platform.
        assert_eq!(
            bitrate_plan(1920, 1080, 144.0, 10.0, &Platform::X.spec(), 0).fps,
            60.0
        );
    }

    #[test]
    fn bitrate_cap_and_floor() {
        let d = Platform::Discord.spec();
        let p = bitrate_plan(852, 480, 60.0, 300.0, &d, AUDIO_BPS);
        assert_eq!(p.video_bps, MIN_VIDEO_BPS);
        assert_eq!(p.fps, 30.0);
        assert!(!p.full_fps);
        // 30 s on Discord: cap = 10e6*.92*8/30 - 160k = 2_293_333.
        let p = bitrate_plan(1920, 1080, 60.0, 30.0, &d, AUDIO_BPS);
        assert_eq!(p.video_bps, 2_293_333);
    }

    #[test]
    fn default_size_x_1440p() {
        let spec = Platform::X.spec();
        let (s, e) = default_window(300.0, &spec);
        assert_eq!((s, e), (160.0, 300.0));
        let ladder = size_ladder(1440, spec.max_short_side);
        let short = default_short_side(&ladder, 2560, 1440, 60.0, e - s, &spec, AUDIO_BPS);
        assert_eq!(short, 1080);
    }

    #[test]
    fn default_size_discord_10mb_300s() {
        let spec = Platform::Discord.spec();
        let (s, e) = default_window(300.0, &spec);
        assert_eq!(e - s, 300.0);
        let ladder = size_ladder(1440, spec.max_short_side);
        let short = default_short_side(&ladder, 2560, 1440, 60.0, e - s, &spec, AUDIO_BPS);
        assert_eq!(short, 480);
        let (w, h) = scale_to_short_side(2560, 1440, short);
        assert_eq!(bitrate_plan(w, h, 60.0, e - s, &spec, AUDIO_BPS).fps, 30.0);
    }

    #[test]
    fn default_size_original_keeps_source() {
        let spec = Platform::Original.spec();
        let ladder = size_ladder(1440, spec.max_short_side);
        assert_eq!(
            default_short_side(&ladder, 2560, 1440, 60.0, 30.0, &spec, AUDIO_BPS),
            1440
        );
    }

    #[test]
    fn estimate_and_format() {
        let p = BitratePlan {
            video_bps: 12_000_000,
            audio_bps: 160_000,
            fps: 60.0,
            full_fps: true,
        };
        assert_eq!(estimate_bytes(&p, 140.0), 212_800_000);
        assert_eq!(format_bytes(212_800_000), "213 MB");
        assert_eq!(format_bytes(8_400_000), "8.4 MB");
        assert_eq!(format_bytes(1_200_000_000), "1.2 GB");
        assert_eq!(format_bytes(10_000_000), "10 MB");
        assert_eq!(format_bytes(512_000), "512 KB");
        assert_eq!(format_bytes(12), "12 B");
    }

    #[test]
    fn retry_rule() {
        // 30 s, 160 kb/s audio = 600 KB audio; 12 MB file vs 10 MB cap.
        let next = retry_bitrate(2_000_000, 12_000_000, 10_000_000, AUDIO_BPS, 30.0, 0).unwrap();
        // video_bytes = 11.4 MB, budget = 9.4 MB → .95*9.4/11.4 = .7833.
        assert_eq!(next, 1_566_666);
        // Small overshoot still shrinks by at least 10 %.
        let next = retry_bitrate(2_000_000, 10_100_000, 10_000_000, 0, 30.0, 0).unwrap();
        assert_eq!(next, 1_800_000);
        // Give-up conditions.
        assert_eq!(
            retry_bitrate(
                2_000_000,
                12_000_000,
                10_000_000,
                AUDIO_BPS,
                30.0,
                MAX_RETRIES
            ),
            Err(TOO_LONG_ERROR.to_string())
        );
        assert!(retry_bitrate(100_000, 200_000, 150_000, AUDIO_BPS, 8.0, 0).is_err());
        assert!(retry_bitrate(55_000, 200_000, 190_000, 0, 6.0, 0).is_err());
    }
}

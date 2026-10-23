//! Pure translation of an [`EditProject`] into an ffmpeg `-filter_complex`.
//!
//! Kept free of I/O so the exact strings can be pinned in tests: filter-graph
//! syntax errors only surface at export time, deep inside a background job,
//! so we want them caught by `cargo test` instead.
//!
//! Shape of the graph (input 0 = the clip, seeked by the runner):
//!
//! ```text
//! [0:v] → look stage → split=n → trim/setpts per part → concat → fps,format,setsar → [vout]
//! [0:a:t] → asplit=n → atrim/asetpts/volume/atempo per part → concat → aformat → [a{t}]
//!                                                       (optional) amix of all [a{t}] → [amix]
//! ```
//!
//! Every audio stream is kept as its own output track (OpenGG clips carry one
//! stream per mixer channel and the user may want to rebalance them later in
//! another editor); a muted track is rendered silent rather than dropped so
//! stream indices — and therefore per-channel titles — stay stable.

use super::looks::{self, Look};
use super::project::EditProject;

/// Knobs that change the graph but aren't part of the edit itself.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GraphOptions {
    /// Also emit a pre-mixed `[amix]` track (placed first) so players that
    /// only play the first audio stream still hear every channel.
    pub mix_track: bool,
    /// Finish the video chain with `format=nv12,hwupload` for VAAPI encoders
    /// (`-vf` can't be combined with `-filter_complex`, so it must live here).
    pub hw_upload: bool,
}

/// A built graph plus the labels the runner must `-map`.
#[derive(Debug, Clone, PartialEq)]
pub struct FilterGraph {
    pub filter_complex: String,
    /// Video output label including brackets, e.g. `"[vout]"`.
    pub video_out: String,
    /// Audio output labels including brackets, in output stream order.
    pub audio_outs: Vec<String>,
    /// Earliest source time used (min part start), before `time_origin`.
    pub from: f64,
    /// Latest source time used (max part end), before `time_origin`.
    pub to: f64,
}

/// Normalised per-stream audio format so concat/amix never see mismatched
/// inputs and every output track encodes identically.
const AFORMAT: &str = "aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo";

/// Format a number for a filter argument: ≤ 6 decimals, trailing zeros
/// trimmed, `.` decimal separator regardless of locale, never `-0`.
pub fn num(v: f64) -> String {
    let s = format!("{v:.6}");
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

/// `atempo` only accepts `[0.5, 2]` per instance (beyond that it degrades),
/// so larger factors are chained. Returns `""` for 1×.
pub fn atempo_chain(f: f64) -> String {
    if (f - 1.0).abs() < 1e-9 || !f.is_finite() || f <= 0.0 {
        return String::new();
    }
    let mut out = Vec::new();
    let mut rem = f;
    while rem < 0.5 {
        out.push("atempo=0.5".to_string());
        rem /= 0.5;
    }
    while rem > 2.0 {
        out.push("atempo=2".to_string());
        rem /= 2.0;
    }
    if (rem - 1.0).abs() >= 1e-9 {
        out.push(format!("atempo={}", num(rem)));
    }
    out.join(",")
}

/// Split a filter chain at top-level commas (not inside `'…'`), so an
/// `enable=` can be appended to each filter even when one carries a quoted
/// argument such as `curves=all='0/0 1/1'`.
fn split_filters(chain: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (i, c) in chain.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            ',' if !quoted => {
                out.push(&chain[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&chain[start..]);
    out
}

/// Build the look stage (without a trailing comma), or `""` if there is none.
///
/// The look runs on the continuous input *before* the split into parts, so
/// one filter instance serves every part; per-part looks are realised with
/// timeline `enable=` expressions instead of one filter chain per part.
fn look_stage(p: &EditProject, origin: f64) -> String {
    if p.parts.iter().all(|x| x.look.is_none()) {
        return looks::filter(p.look, p.look_strength).unwrap_or_default();
    }
    // Group parts by their effective (look, strength), first-seen order.
    let mut groups: Vec<((Look, f64), Vec<usize>)> = Vec::new();
    for i in 0..p.parts.len() {
        let key = p.look_at(i);
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push(i),
            None => groups.push((key, vec![i])),
        }
    }
    let mut stages = Vec::new();
    for ((look, strength), idx) in groups {
        let Some(chain) = looks::filter(look, strength) else {
            continue;
        };
        let enable = idx
            .iter()
            .map(|&i| {
                let part = &p.parts[i];
                format!(
                    "between(t,{},{})",
                    num(shift(part.start, origin)),
                    num(shift(part.end, origin))
                )
            })
            .collect::<Vec<_>>()
            .join("+");
        for f in split_filters(&chain) {
            stages.push(format!("{f}:enable='{enable}'"));
        }
    }
    stages.join(",")
}

/// Source time → timestamp in the seeked input (`-ss` rebases to 0).
fn shift(t: f64, origin: f64) -> f64 {
    (t - origin).max(0.0)
}

/// Build the complete filter graph.
///
/// `fps` is the output frame rate (the concat of speed-changed pieces has
/// irregular timestamps; `fps` re-grids them). `audio_tracks` is the number
/// of audio streams in the input. `time_origin` is the runner's `-ss` seek
/// point, subtracted from every time written.
pub fn build(
    p: &EditProject,
    fps: f64,
    audio_tracks: usize,
    time_origin: f64,
    opts: GraphOptions,
) -> FilterGraph {
    let n = p.parts.len();
    let from = p
        .parts
        .iter()
        .map(|x| x.start)
        .fold(f64::INFINITY, f64::min);
    let to = p
        .parts
        .iter()
        .map(|x| x.end)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut stmts: Vec<String> = Vec::new();

    // ── Video ──
    let piece = |i: usize| {
        let part = &p.parts[i];
        let setpts = if (part.speed - 1.0).abs() < 1e-9 {
            "setpts=PTS-STARTPTS".to_string()
        } else {
            format!("setpts=(PTS-STARTPTS)/{}", num(part.speed))
        };
        format!(
            "trim=start={}:end={},{setpts}",
            num(shift(part.start, time_origin)),
            num(shift(part.end, time_origin))
        )
    };
    let tail = format!(
        "fps={},format=yuv420p,setsar=1{}[vout]",
        num(fps),
        if opts.hw_upload {
            ",format=nv12,hwupload"
        } else {
            ""
        }
    );
    let look = look_stage(p, time_origin);
    let look_prefix = if look.is_empty() {
        String::new()
    } else {
        format!("{look},")
    };
    if n == 1 {
        stmts.push(format!("[0:v]{look_prefix}{},{tail}", piece(0)));
    } else {
        let outs: String = (0..n).map(|i| format!("[vs{i}]")).collect();
        stmts.push(format!("[0:v]{look_prefix}split={n}{outs}"));
        for i in 0..n {
            stmts.push(format!("[vs{i}]{}[vp{i}]", piece(i)));
        }
        let ins: String = (0..n).map(|i| format!("[vp{i}]")).collect();
        stmts.push(format!("{ins}concat=n={n}:v=1:a=0,{tail}"));
    }

    // ── Audio ──
    let mix = opts.mix_track && audio_tracks > 1;
    let mut audio_outs = Vec::new();
    for t in 0..audio_tracks {
        let apiece = |i: usize| {
            let part = &p.parts[i];
            let mut s = format!(
                "atrim=start={}:end={},asetpts=PTS-STARTPTS,volume={}",
                num(shift(part.start, time_origin)),
                num(shift(part.end, time_origin)),
                num(p.effective_level(i, t))
            );
            let tempo = atempo_chain(part.speed);
            if !tempo.is_empty() {
                s.push(',');
                s.push_str(&tempo);
            }
            s
        };
        let out_tail = if mix {
            format!("{AFORMAT},asplit=2[a{t}][m{t}]")
        } else {
            format!("{AFORMAT}[a{t}]")
        };
        if n == 1 {
            stmts.push(format!("[0:a:{t}]{},{out_tail}", apiece(0)));
        } else {
            let outs: String = (0..n).map(|i| format!("[t{t}s{i}]")).collect();
            stmts.push(format!("[0:a:{t}]asplit={n}{outs}"));
            for i in 0..n {
                stmts.push(format!("[t{t}s{i}]{}[t{t}p{i}]", apiece(i)));
            }
            let ins: String = (0..n).map(|i| format!("[t{t}p{i}]")).collect();
            stmts.push(format!("{ins}concat=n={n}:v=0:a=1,{out_tail}"));
        }
        audio_outs.push(format!("[a{t}]"));
    }
    if mix {
        let ins: String = (0..audio_tracks).map(|t| format!("[m{t}]")).collect();
        stmts.push(format!(
            "{ins}amix=inputs={audio_tracks}:normalize=0,{AFORMAT}[amix]"
        ));
        audio_outs.insert(0, "[amix]".to_string());
    }

    FilterGraph {
        filter_complex: stmts.join(";"),
        video_out: "[vout]".to_string(),
        audio_outs,
        from,
        to,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::project::Part;

    fn plain(dur: f64, tracks: usize) -> EditProject {
        EditProject::new(dur, tracks)
    }

    #[test]
    fn numbers() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(0.5), "0.5");
        assert_eq!(num(1.0 / 3.0), "0.333333");
        assert_eq!(num(-0.0000001), "0");
        assert_eq!(num(60000.0 / 1001.0), "59.94006");
    }

    #[test]
    fn atempo_table() {
        assert_eq!(atempo_chain(1.0), "");
        assert_eq!(atempo_chain(0.25), "atempo=0.5,atempo=0.5");
        assert_eq!(atempo_chain(0.5), "atempo=0.5");
        assert_eq!(atempo_chain(0.75), "atempo=0.75");
        assert_eq!(atempo_chain(1.5), "atempo=1.5");
        assert_eq!(atempo_chain(2.0), "atempo=2");
        assert_eq!(atempo_chain(3.0), "atempo=2,atempo=1.5");
        assert_eq!(atempo_chain(4.0), "atempo=2,atempo=2");
    }

    #[test]
    fn one_part_one_track() {
        let g = build(&plain(10.0, 1), 60.0, 1, 0.0, GraphOptions::default());
        assert_eq!(
            g.filter_complex,
            "[0:v]trim=start=0:end=10,setpts=PTS-STARTPTS,fps=60,format=yuv420p,setsar=1[vout];\
             [0:a:0]atrim=start=0:end=10,asetpts=PTS-STARTPTS,volume=1,\
             aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo[a0]"
        );
        assert_eq!(g.video_out, "[vout]");
        assert_eq!(g.audio_outs, vec!["[a0]"]);
        assert_eq!((g.from, g.to), (0.0, 10.0));
    }

    #[test]
    fn no_audio() {
        let g = build(&plain(10.0, 0), 30.0, 0, 0.0, GraphOptions::default());
        assert!(g.audio_outs.is_empty());
        assert!(!g.filter_complex.contains("0:a"));
    }

    #[test]
    fn two_parts_two_speeds() {
        let mut p = plain(30.0, 1);
        p.parts = vec![Part::new(0.0, 5.0), Part::new(20.0, 30.0)];
        p.parts[1].speed = 2.0;
        let g = build(&p, 60.0, 1, 0.0, GraphOptions::default());
        assert_eq!(
            g.filter_complex,
            "[0:v]split=2[vs0][vs1];\
             [vs0]trim=start=0:end=5,setpts=PTS-STARTPTS[vp0];\
             [vs1]trim=start=20:end=30,setpts=(PTS-STARTPTS)/2[vp1];\
             [vp0][vp1]concat=n=2:v=1:a=0,fps=60,format=yuv420p,setsar=1[vout];\
             [0:a:0]asplit=2[t0s0][t0s1];\
             [t0s0]atrim=start=0:end=5,asetpts=PTS-STARTPTS,volume=1[t0p0];\
             [t0s1]atrim=start=20:end=30,asetpts=PTS-STARTPTS,volume=1,atempo=2[t0p1];\
             [t0p0][t0p1]concat=n=2:v=0:a=1,\
             aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo[a0]"
        );
        assert_eq!((g.from, g.to), (0.0, 30.0));
    }

    #[test]
    fn clip_look_applied_once() {
        let p = plain(10.0, 0).set_look(Look::Vivid, 0.5).unwrap();
        let g = build(&p, 30.0, 0, 0.0, GraphOptions::default());
        assert!(g
            .filter_complex
            .starts_with("[0:v]eq=saturation=1.175:contrast=1.04,trim=start=0:end=10,"));
    }

    #[test]
    fn per_part_look_enable() {
        let mut p = plain(30.0, 0);
        p.parts = vec![
            Part::new(0.0, 5.0),
            Part::new(10.0, 15.0),
            Part::new(20.0, 30.0),
        ];
        let p = p
            .set_look(Look::Mono, 1.0)
            .unwrap()
            .set_part_look(&[1], Some(Look::Cinematic), 1.0)
            .unwrap();
        let g = build(&p, 30.0, 0, 0.0, GraphOptions::default());
        let head = g.filter_complex.split(";").next().unwrap();
        assert_eq!(
            head,
            "[0:v]hue=s=0:enable='between(t,0,5)+between(t,20,30)',\
             eq=contrast=1.12:saturation=0.85:enable='between(t,10,15)',\
             colorbalance=rs=-0.05:bs=0.07:rh=0.06:bh=-0.04:enable='between(t,10,15)',\
             split=3[vs0][vs1][vs2]"
        );
        // Explicit "no look" on a part makes it ungraded even with a clip look.
        let q = p.set_part_look(&[0, 2], Some(Look::None), 1.0).unwrap();
        let g = build(&q, 30.0, 0, 0.0, GraphOptions::default());
        assert!(!g.filter_complex.contains("hue="));
        // Quoted args stay intact when enable= is appended.
        let r = p.set_part_look(&[0], Some(Look::Retro), 1.0).unwrap();
        let g = build(&r, 30.0, 0, 0.0, GraphOptions::default());
        assert!(g
            .filter_complex
            .contains("curves=all='0/0.07 0.5/0.5 1/0.94':enable='between(t,0,5)'"));
    }

    #[test]
    fn mix_track_first_and_muted_kept() {
        let p = plain(10.0, 3).set_track_muted(1, true).unwrap();
        let opts = GraphOptions {
            mix_track: true,
            hw_upload: false,
        };
        let g = build(&p, 60.0, 3, 0.0, opts);
        assert_eq!(g.audio_outs, vec!["[amix]", "[a0]", "[a1]", "[a2]"]);
        assert!(g
            .filter_complex
            .contains("[0:a:1]atrim=start=0:end=10,asetpts=PTS-STARTPTS,volume=0,"));
        assert!(g.filter_complex.contains(
            "[m0][m1][m2]amix=inputs=3:normalize=0,\
             aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo[amix]"
        ));
        assert!(g.filter_complex.contains("stereo,asplit=2[a0][m0]"));
        // One track: mix is ignored.
        let g1 = build(&plain(10.0, 1), 60.0, 1, 0.0, opts);
        assert_eq!(g1.audio_outs, vec!["[a0]"]);
        assert!(!g1.filter_complex.contains("amix"));
    }

    #[test]
    fn time_origin_shifts_everything() {
        let mut p = plain(30.0, 1);
        p.parts = vec![Part::new(12.0, 15.0), Part::new(20.0, 22.5)];
        let p = p.set_part_look(&[1], Some(Look::Warm), 1.0).unwrap();
        let g = build(&p, 60.0, 1, 10.0, GraphOptions::default());
        let fc = &g.filter_complex;
        for s in [
            "trim=start=2:end=5,",
            "trim=start=10:end=12.5,",
            "atrim=start=2:end=5,",
            "atrim=start=10:end=12.5,",
            "enable='between(t,10,12.5)'",
        ] {
            assert!(fc.contains(s), "missing {s} in {fc}");
        }
        assert!(!fc.contains("start=12"));
        assert!(!fc.contains("start=20"));
        // from/to stay in source time.
        assert_eq!((g.from, g.to), (12.0, 22.5));
    }

    #[test]
    fn hw_upload_suffix() {
        let opts = GraphOptions {
            mix_track: false,
            hw_upload: true,
        };
        let g = build(&plain(5.0, 0), 60.0, 0, 0.0, opts);
        assert!(g
            .filter_complex
            .ends_with("setsar=1,format=nv12,hwupload[vout]"));
    }

    #[test]
    fn levels_in_volume() {
        let p = plain(10.0, 2)
            .set_track_level(0, 0.5)
            .unwrap()
            .set_part_level(&[0], 0, 1.5)
            .unwrap();
        let g = build(&p, 60.0, 2, 0.0, GraphOptions::default());
        assert!(g.filter_complex.contains("volume=0.75,"));
        assert!(g
            .filter_complex
            .contains("[0:a:1]atrim=start=0:end=10,asetpts=PTS-STARTPTS,volume=1,"));
    }
}

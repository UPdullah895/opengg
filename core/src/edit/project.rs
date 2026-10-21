//! The non-destructive edit model: an ordered list of kept source ranges
//! ("parts"), each with its own speed, per-track level multipliers and an
//! optional colour look, plus clip-wide track levels/mutes and a clip look.
//!
//! Nothing here touches the media file. The source clip is never modified;
//! an [`EditProject`] is only rendered when exported (see `graph`/`export`).
//!
//! Two time bases are in play everywhere:
//! - **source** seconds: positions in the original recording; parts are
//!   stored in source time.
//! - **output** seconds: positions on the edited timeline the user sees and
//!   scrubs — cut-out gaps removed and each part stretched by `1/speed`.
//!
//! UI gestures arrive in output time (the playhead), so every mutator that
//! takes a time takes *output* time and maps it with [`EditProject::to_source`].
//!
//! All mutators are functional: they return `Some(new_project)` or `None` when
//! nothing would change. That lets the controller push to the undo history
//! only on a real change, without comparing projects itself.

use serde::{Deserialize, Serialize};

use super::looks::Look;

/// Shortest piece an operation may create, in source seconds (one frame at
/// 60 fps). Splitting/trimming closer than this to a part edge is ignored so
/// a slightly-off click can't produce an invisible zero-length part.
pub const MIN_PIECE: f64 = 1.0 / 60.0;
/// Speed range. Below 0.25× ffmpeg's `atempo` chain gets long and the result
/// is rarely useful; above 4× audio becomes noise.
pub const MIN_SPEED: f64 = 0.25;
pub const MAX_SPEED: f64 = 4.0;
/// Level multipliers allow up to +6 dB of boost.
pub const MAX_LEVEL: f64 = 2.0;

/// Float slack when comparing derived times (sums of divisions).
const EPS: f64 = 1e-9;

fn one() -> f64 {
    1.0
}

fn clamp_speed(f: f64) -> f64 {
    if f.is_finite() {
        f.clamp(MIN_SPEED, MAX_SPEED)
    } else {
        1.0
    }
}

fn clamp_level(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(0.0, MAX_LEVEL)
    } else {
        1.0
    }
}

fn clamp_strength(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// One kept range of the source, played in list order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    /// Source start, seconds.
    pub start: f64,
    /// Source end, seconds (exclusive in playback terms).
    pub end: f64,
    /// Playback speed, `[MIN_SPEED, MAX_SPEED]`.
    #[serde(default = "one")]
    pub speed: f64,
    /// Per-track level multiplier for this part. May be shorter than the
    /// track count — a missing entry means 1.0 — so a part created before any
    /// level edit doesn't need to know how many tracks the clip has.
    #[serde(default)]
    pub levels: Vec<f64>,
    /// This part's own look. `None` = inherit the clip look;
    /// `Some(Look::None)` = explicitly ungraded even if the clip has a look.
    #[serde(default)]
    pub look: Option<Look>,
    /// Strength of `look`, `[0, 1]`.
    #[serde(default = "one")]
    pub look_strength: f64,
}

impl Part {
    /// A plain 1× part with no overrides.
    pub fn new(start: f64, end: f64) -> Self {
        Self {
            start,
            end,
            speed: 1.0,
            levels: Vec::new(),
            look: None,
            look_strength: 1.0,
        }
    }

    /// Source length.
    pub fn len(&self) -> f64 {
        self.end - self.start
    }

    /// True for a degenerate (zero/negative length) part.
    pub fn is_empty(&self) -> bool {
        self.len() <= 0.0
    }

    /// Length on the output timeline.
    pub fn output_len(&self) -> f64 {
        self.len() / self.speed
    }

    /// Level multiplier for `track` (1.0 when not set).
    pub fn level(&self, track: usize) -> f64 {
        self.levels.get(track).copied().unwrap_or(1.0)
    }
}

/// The whole edit of one clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditProject {
    /// Source duration, seconds.
    pub duration: f64,
    /// Kept ranges in play order (never empty).
    pub parts: Vec<Part>,
    /// Clip-wide per-track level, `[0, MAX_LEVEL]`, one per audio stream.
    #[serde(default)]
    pub track_levels: Vec<f64>,
    /// Clip-wide per-track mute.
    #[serde(default)]
    pub track_muted: Vec<bool>,
    /// Clip-wide look (applies to parts without their own).
    #[serde(default)]
    pub look: Look,
    #[serde(default = "one")]
    pub look_strength: f64,
}

impl EditProject {
    /// An untouched edit: one 1× part over the whole clip, all tracks at unity.
    pub fn new(duration: f64, track_count: usize) -> Self {
        let duration = if duration.is_finite() {
            duration.max(0.0)
        } else {
            0.0
        };
        Self {
            duration,
            parts: vec![Part::new(0.0, duration)],
            track_levels: vec![1.0; track_count],
            track_muted: vec![false; track_count],
            look: Look::None,
            look_strength: 1.0,
        }
    }

    /// Number of audio tracks the project knows about.
    pub fn track_count(&self) -> usize {
        self.track_levels.len().max(self.track_muted.len())
    }

    /// True when exporting would reproduce the source unchanged, so the UI can
    /// skip the re-encode (or disable Export).
    pub fn is_unchanged(&self) -> bool {
        let one_full_part = self.parts.len() == 1 && {
            let p = &self.parts[0];
            p.start.abs() <= EPS
                && (p.end - self.duration).abs() <= EPS
                && (p.speed - 1.0).abs() <= EPS
                && p.levels.iter().all(|l| (l - 1.0).abs() <= EPS)
                && p.look
                    .is_none_or(|l| l == Look::None || p.look_strength <= 0.0)
        };
        one_full_part
            && self.track_levels.iter().all(|l| (l - 1.0).abs() <= EPS)
            && !self.track_muted.iter().any(|m| *m)
            && (self.look == Look::None || self.look_strength <= 0.0)
    }

    /// Effective gain of `track` in part `part_idx`: 0 when muted, otherwise
    /// clip level × part level.
    pub fn effective_level(&self, part_idx: usize, track: usize) -> f64 {
        if self.track_muted.get(track).copied().unwrap_or(false) {
            return 0.0;
        }
        let clip = self.track_levels.get(track).copied().unwrap_or(1.0);
        let part = self.parts.get(part_idx).map_or(1.0, |p| p.level(track));
        clip * part
    }

    /// The look (and its strength) that renders part `part_idx`: its own if
    /// set, else the clip look.
    pub fn look_at(&self, part_idx: usize) -> (Look, f64) {
        match self.parts.get(part_idx) {
            Some(Part {
                look: Some(l),
                look_strength,
                ..
            }) => (*l, *look_strength),
            _ => (self.look, self.look_strength),
        }
    }

    /// `Some(speed)` when every part plays at the same speed, else `None`
    /// (the UI shows "varies").
    pub fn clip_speed(&self) -> Option<f64> {
        let first = self.parts.first()?.speed;
        self.parts
            .iter()
            .all(|p| (p.speed - first).abs() <= EPS)
            .then_some(first)
    }

    // ── Time mapping ────────────────────────────────────────────────────

    /// Length of the edited timeline.
    pub fn output_duration(&self) -> f64 {
        self.parts.iter().map(Part::output_len).sum()
    }

    /// Map a source time onto the output timeline. A moment that was cut out
    /// maps to the start of the next kept part (where playback resumes);
    /// anything past the last part maps to the end.
    pub fn to_output(&self, src: f64) -> f64 {
        let mut total = 0.0;
        for p in &self.parts {
            if src < p.start {
                return total;
            }
            if src <= p.end {
                return total + (src - p.start) / p.speed;
            }
            total += p.output_len();
        }
        total
    }

    /// Locate output time `out`: `(part index, source time)`.
    fn locate(&self, out: f64) -> Option<(usize, f64)> {
        if out.is_nan() || out < 0.0 {
            return None;
        }
        let last = self.parts.len().checked_sub(1)?;
        let mut at = 0.0;
        for (i, p) in self.parts.iter().enumerate() {
            let l = p.output_len();
            if out < at + l || (i == last && out <= at + l + EPS) {
                return Some((i, p.end.min(p.start + (out - at) * p.speed)));
            }
            at += l;
        }
        None
    }

    /// Map an output time back to source time; `None` before 0 or past the end.
    pub fn to_source(&self, out: f64) -> Option<f64> {
        self.locate(out).map(|(_, s)| s)
    }

    /// Index of the part containing source time `s` (`start <= s < end`; the
    /// last part also accepts `s == end`).
    pub fn part_index_at_source(&self, s: f64) -> Option<usize> {
        let last = self.parts.len().checked_sub(1)?;
        self.parts
            .iter()
            .enumerate()
            .position(|(i, p)| p.start <= s && (s < p.end || (i == last && s == p.end)))
    }

    /// Index of the part under output time `out`.
    pub fn part_index_at_output(&self, out: f64) -> Option<usize> {
        self.locate(out).map(|(i, _)| i)
    }

    /// Output-time span `[from, to)` of part `idx`, for drawing the timeline.
    pub fn part_output_range(&self, idx: usize) -> Option<(f64, f64)> {
        let p = self.parts.get(idx)?;
        let from: f64 = self.parts[..idx].iter().map(Part::output_len).sum();
        Some((from, from + p.output_len()))
    }

    // ── Cutting ─────────────────────────────────────────────────────────

    /// Locate `out` and reject positions within `MIN_PIECE` of the part's
    /// edges (shared guard for split / trim-before / trim-after).
    fn locate_inner(&self, out: f64) -> Option<(usize, f64)> {
        let (i, s) = self.locate(out)?;
        let p = &self.parts[i];
        (s - p.start >= MIN_PIECE && p.end - s >= MIN_PIECE).then_some((i, s))
    }

    /// Split the part under the playhead in two. Both halves keep the
    /// original's speed, levels and look.
    pub fn split_at(&self, out: f64) -> Option<EditProject> {
        let (i, s) = self.locate_inner(out)?;
        let mut next = self.clone();
        let mut right = next.parts[i].clone();
        next.parts[i].end = s;
        right.start = s;
        next.parts.insert(i + 1, right);
        Some(next)
    }

    /// Drop the portion of the part under the playhead that lies before it.
    pub fn trim_part_before(&self, out: f64) -> Option<EditProject> {
        let (i, s) = self.locate_inner(out)?;
        let mut next = self.clone();
        next.parts[i].start = s;
        Some(next)
    }

    /// Drop the portion of the part under the playhead that lies after it.
    pub fn trim_part_after(&self, out: f64) -> Option<EditProject> {
        let (i, s) = self.locate_inner(out)?;
        let mut next = self.clone();
        next.parts[i].end = s;
        Some(next)
    }

    /// Remove parts by index. Invalid indices are ignored; refuses (`None`)
    /// to delete nothing or to delete every part — an empty edit can't export.
    pub fn delete_parts(&self, idx: &[usize]) -> Option<EditProject> {
        let mut kill = vec![false; self.parts.len()];
        for &i in idx {
            if let Some(k) = kill.get_mut(i) {
                *k = true;
            }
        }
        let n = kill.iter().filter(|k| **k).count();
        if n == 0 || n == self.parts.len() {
            return None;
        }
        let mut next = self.clone();
        let mut it = kill.iter();
        next.parts.retain(|_| !*it.next().unwrap());
        Some(next)
    }

    /// Single-window trim (used by the simple in/out trim UI): keep only the
    /// source window `[start_src, end_src]`, clipping parts at its edges and
    /// dropping parts entirely outside. With one part, it simply becomes the
    /// window.
    pub fn set_trim(&self, start_src: f64, end_src: f64) -> Option<EditProject> {
        if !start_src.is_finite() || !end_src.is_finite() {
            return None;
        }
        let a = start_src.clamp(0.0, self.duration);
        let b = end_src.clamp(0.0, self.duration);
        if b - a < MIN_PIECE {
            return None;
        }
        let mut next = self.clone();
        if next.parts.len() == 1 {
            next.parts[0].start = a;
            next.parts[0].end = b;
        } else {
            next.parts = self
                .parts
                .iter()
                .filter_map(|p| {
                    let s = p.start.max(a);
                    let e = p.end.min(b);
                    (e - s >= MIN_PIECE).then(|| Part {
                        start: s,
                        end: e,
                        ..p.clone()
                    })
                })
                .collect();
            if next.parts.is_empty() {
                return None;
            }
        }
        (next != *self).then_some(next)
    }

    // ── Speed ───────────────────────────────────────────────────────────

    /// Split the part containing source time `s` there, unless `s` is within
    /// `MIN_PIECE` of its edges (then the existing edge is used as-is).
    fn split_source(&mut self, s: f64) {
        if let Some(i) = self.part_index_at_source(s) {
            let p = &self.parts[i];
            if s - p.start >= MIN_PIECE && p.end - s >= MIN_PIECE {
                let mut right = p.clone();
                right.start = s;
                self.parts[i].end = s;
                self.parts.insert(i + 1, right);
            }
        }
    }

    /// Set the speed of the output range `[out_from, out_to]`, splitting
    /// parts at its edges so only that range changes.
    pub fn set_speed(&self, out_from: f64, out_to: f64, f: f64) -> Option<EditProject> {
        let f = clamp_speed(f);
        let total = self.output_duration();
        let (lo, hi) = if out_from <= out_to {
            (out_from, out_to)
        } else {
            (out_to, out_from)
        };
        let a = self.to_source(lo.clamp(0.0, total))?;
        let b = self.to_source(hi.clamp(0.0, total))?;
        if b - a < MIN_PIECE {
            return None;
        }
        let mut next = self.clone();
        next.split_source(a);
        next.split_source(b);
        // MIN_PIECE tolerance (not just EPS): when an edge fell within
        // MIN_PIECE of an existing boundary no split happened, and that
        // boundary — not `a`/`b` exactly — delimits the range.
        for p in next.parts.iter_mut() {
            if p.start >= a - MIN_PIECE && p.end <= b + MIN_PIECE {
                p.speed = f;
            }
        }
        (next != *self).then_some(next)
    }

    /// Apply `f` to the selected parts' fields; `None` when no valid index
    /// or nothing changed.
    fn with_parts(&self, idx: &[usize], mut f: impl FnMut(&mut Part)) -> Option<EditProject> {
        let mut next = self.clone();
        for &i in idx {
            if let Some(p) = next.parts.get_mut(i) {
                f(p);
            }
        }
        (next != *self).then_some(next)
    }

    /// Set the speed of whole parts.
    pub fn set_part_speed(&self, idx: &[usize], f: f64) -> Option<EditProject> {
        let f = clamp_speed(f);
        self.with_parts(idx, |p| p.speed = f)
    }

    /// Set one track's level multiplier on the selected parts.
    pub fn set_part_level(&self, idx: &[usize], track: usize, v: f64) -> Option<EditProject> {
        let v = clamp_level(v);
        self.with_parts(idx, |p| {
            if p.levels.len() <= track {
                p.levels.resize(track + 1, 1.0);
            }
            p.levels[track] = v;
        })
    }

    /// Set (or with `None`, clear back to inheriting) the selected parts' look.
    pub fn set_part_look(
        &self,
        idx: &[usize],
        look: Option<Look>,
        strength: f64,
    ) -> Option<EditProject> {
        let strength = clamp_strength(strength);
        self.with_parts(idx, |p| {
            p.look = look;
            p.look_strength = strength;
        })
    }

    // ── Clip-wide ───────────────────────────────────────────────────────

    /// Set a clip-wide track level. `None` for an unknown track.
    pub fn set_track_level(&self, t: usize, v: f64) -> Option<EditProject> {
        let mut next = self.clone();
        *next.track_levels.get_mut(t)? = clamp_level(v);
        (next != *self).then_some(next)
    }

    /// Mute/unmute a track clip-wide. `None` for an unknown track.
    pub fn set_track_muted(&self, t: usize, muted: bool) -> Option<EditProject> {
        if t >= self.track_count() {
            return None;
        }
        let mut next = self.clone();
        if next.track_muted.len() <= t {
            next.track_muted.resize(t + 1, false);
        }
        next.track_muted[t] = muted;
        (next != *self).then_some(next)
    }

    /// Set the clip look. This also clears every part's own look: picking a
    /// look for "the clip" should visibly apply to the whole clip.
    pub fn set_look(&self, look: Look, strength: f64) -> Option<EditProject> {
        let mut next = self.clone();
        next.look = look;
        next.look_strength = clamp_strength(strength);
        for p in next.parts.iter_mut() {
            p.look = None;
            p.look_strength = 1.0;
        }
        (next != *self).then_some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn two_parts() -> EditProject {
        let mut p = EditProject::new(30.0, 2);
        p.parts = vec![Part::new(0.0, 5.0), Part::new(20.0, 30.0)];
        p
    }

    fn ranges(p: &EditProject) -> Vec<(f64, f64)> {
        p.parts.iter().map(|p| (p.start, p.end)).collect()
    }

    #[test]
    fn new_is_unchanged() {
        let p = EditProject::new(30.0, 3);
        assert!(p.is_unchanged());
        assert_eq!(p.track_count(), 3);
        assert_eq!(p.clip_speed(), Some(1.0));
        assert!(!two_parts().is_unchanged());
        assert!(!p.set_track_muted(1, true).unwrap().is_unchanged());
        assert!(!p.set_look(Look::Warm, 0.5).unwrap().is_unchanged());
    }

    #[test]
    fn mapping_with_gap() {
        let p = two_parts();
        assert!(close(p.output_duration(), 15.0));
        assert!(close(p.to_output(22.0), 7.0));
        assert!(close(p.to_source(6.0).unwrap(), 21.0));
        // Cut-out moment maps to the start of the next kept part.
        assert!(close(p.to_output(10.0), 5.0));
        assert!(close(p.to_output(99.0), 15.0));
        assert!(close(p.to_source(15.0).unwrap(), 30.0));
        assert_eq!(p.to_source(15.1), None);
        assert_eq!(p.to_source(-0.1), None);
        assert_eq!(p.part_index_at_source(5.0), None);
        assert_eq!(p.part_index_at_source(30.0), Some(1));
        assert_eq!(p.part_index_at_source(10.0), None);
        assert_eq!(p.part_output_range(1), Some((5.0, 15.0)));
    }

    #[test]
    fn mapping_with_speed() {
        let p = EditProject::new(30.0, 1).set_speed(0.0, 10.0, 2.0).unwrap();
        assert_eq!(ranges(&p), vec![(0.0, 10.0), (10.0, 30.0)]);
        assert!(close(p.output_duration(), 25.0));
        assert!(close(p.to_output(10.0), 5.0));
        assert!(close(p.to_source(5.0).unwrap(), 10.0));
        assert_eq!(p.clip_speed(), None);
    }

    #[test]
    fn set_speed_middle() {
        let p = EditProject::new(30.0, 1).set_speed(5.0, 10.0, 0.5).unwrap();
        assert_eq!(ranges(&p), vec![(0.0, 5.0), (5.0, 10.0), (10.0, 30.0)]);
        assert_eq!(p.parts[1].speed, 0.5);
        assert_eq!(p.parts[0].speed, 1.0);
        assert!(close(p.output_duration(), 35.0));
        // Same again is a no-op; too-short range is refused; clamp applies.
        assert_eq!(p.set_speed(5.0, 15.0, 0.5), None);
        assert_eq!(p.set_speed(3.0, 3.001, 2.0), None);
        let q = EditProject::new(30.0, 1)
            .set_speed(0.0, 30.0, 100.0)
            .unwrap();
        assert_eq!(q.parts.len(), 1);
        assert_eq!(q.parts[0].speed, MAX_SPEED);
    }

    #[test]
    fn split_keeps_attributes() {
        let p = EditProject::new(30.0, 2)
            .set_part_level(&[0], 1, 0.5)
            .unwrap()
            .set_part_look(&[0], Some(Look::Warm), 0.7)
            .unwrap()
            .set_part_speed(&[0], 2.0)
            .unwrap();
        let s = p.split_at(5.0).unwrap();
        assert_eq!(ranges(&s), vec![(0.0, 10.0), (10.0, 30.0)]);
        for part in &s.parts {
            assert_eq!(part.speed, 2.0);
            assert_eq!(part.level(1), 0.5);
            assert_eq!(part.look, Some(Look::Warm));
        }
        // Too close to an edge.
        assert_eq!(p.split_at(0.001), None);
        assert_eq!(s.split_at(5.0), None);
        assert_eq!(p.split_at(100.0), None);
    }

    #[test]
    fn trim_before_after() {
        let mut p = EditProject::new(30.0, 1);
        p.parts = vec![Part::new(0.0, 10.0), Part::new(10.0, 30.0)];
        let b = p.trim_part_before(15.0).unwrap();
        assert_eq!(ranges(&b), vec![(0.0, 10.0), (15.0, 30.0)]);
        let a = p.trim_part_after(15.0).unwrap();
        assert_eq!(ranges(&a), vec![(0.0, 10.0), (10.0, 15.0)]);
        assert_eq!(p.trim_part_before(10.0), None);
    }

    #[test]
    fn delete() {
        let p = two_parts();
        assert_eq!(
            ranges(&p.delete_parts(&[0, 7]).unwrap()),
            vec![(20.0, 30.0)]
        );
        assert_eq!(p.delete_parts(&[0, 1]), None);
        assert_eq!(p.delete_parts(&[9]), None);
        assert_eq!(p.delete_parts(&[]), None);
    }

    #[test]
    fn trim_window() {
        let p = EditProject::new(30.0, 1);
        let t = p.set_trim(-5.0, 12.0).unwrap();
        assert_eq!(ranges(&t), vec![(0.0, 12.0)]);
        assert_eq!(t.set_trim(0.0, 12.0), None);
        assert_eq!(p.set_trim(4.0, 4.001), None);
        let m = two_parts().set_trim(3.0, 25.0).unwrap();
        assert_eq!(ranges(&m), vec![(3.0, 5.0), (20.0, 25.0)]);
        let d = two_parts().set_trim(21.0, 40.0).unwrap();
        assert_eq!(ranges(&d), vec![(21.0, 30.0)]);
        // Window entirely inside a cut gap leaves nothing — refused.
        assert_eq!(two_parts().set_trim(6.0, 19.0), None);
    }

    #[test]
    fn levels_mute_and_looks() {
        let p = EditProject::new(10.0, 3);
        let q = p.set_track_level(0, 5.0).unwrap();
        assert_eq!(q.track_levels[0], MAX_LEVEL);
        assert_eq!(p.set_track_level(3, 0.5), None);
        assert_eq!(p.set_track_level(0, 1.0), None);
        let q = q.set_part_level(&[0], 2, 0.5).unwrap();
        assert_eq!(q.parts[0].levels, vec![1.0, 1.0, 0.5]);
        assert!(close(q.effective_level(0, 0), 2.0));
        assert!(close(q.effective_level(0, 2), 0.5));
        let q = q.set_track_muted(0, true).unwrap();
        assert_eq!(q.effective_level(0, 0), 0.0);
        assert_eq!(q.set_track_muted(0, true), None);
        assert_eq!(q.set_track_muted(9, true), None);

        let l = p.set_look(Look::Cool, 2.0).unwrap();
        assert_eq!(l.look_at(0), (Look::Cool, 1.0));
        let l = l.set_part_look(&[0], Some(Look::Mono), 0.3).unwrap();
        assert_eq!(l.look_at(0), (Look::Mono, 0.3));
        let l = l.set_look(Look::Vivid, 1.0).unwrap();
        assert_eq!(l.parts[0].look, None);
        assert_eq!(l.look_at(0), (Look::Vivid, 1.0));
    }

    #[test]
    fn serde_round_trip_and_lenient_parse() {
        let p = two_parts()
            .set_part_look(&[1], Some(Look::Teal), 0.5)
            .unwrap();
        let j = serde_json::to_string(&p).unwrap();
        assert!(j.contains("\"trackLevels\""));
        assert!(j.contains("\"lookStrength\""));
        assert_eq!(serde_json::from_str::<EditProject>(&j).unwrap(), p);
        let min: EditProject =
            serde_json::from_str(r#"{"duration":4,"parts":[{"start":0,"end":4}]}"#).unwrap();
        assert_eq!(min.parts[0].speed, 1.0);
        assert_eq!(min.look, Look::None);
    }
}

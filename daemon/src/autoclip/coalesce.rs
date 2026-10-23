//! One clip per highlight, not one per kill.
//!
//! A triple kill arrives as three kill events within seconds, often followed
//! by an MVP. Saving on each would write four overlapping clips, and the save
//! taken at the first kill would end before the play does. So a qualifying
//! event opens a window of [`POST_ROLL_MS`]; each further qualifying event
//! pushes the end out again, up to [`MAX_SPAN_MS`] after the first. When the
//! window closes, one moment comes out, labelled with the best event in it.
//!
//! Times are milliseconds on any monotonic clock the caller chooses, which
//! keeps this testable without sleeping.

use super::Moment;

/// How long after the last event the clip keeps rolling.
pub const POST_ROLL_MS: u64 = 4_000;
/// The longest a window can be stretched, so a busy round still saves while
/// its first kill is inside the replay buffer.
pub const MAX_SPAN_MS: u64 = 12_000;

#[derive(Debug, Default)]
pub struct Coalescer {
    pending: Option<Pending>,
}

#[derive(Debug)]
struct Pending {
    best: Moment,
    first_ms: u64,
    deadline_ms: u64,
}

impl Coalescer {
    /// Add an event that qualifies for a clip.
    pub fn push(&mut self, m: Moment, now_ms: u64) {
        match &mut self.pending {
            None => {
                self.pending = Some(Pending { best: m, first_ms: now_ms, deadline_ms: now_ms + POST_ROLL_MS });
            }
            Some(p) => {
                p.deadline_ms = (now_ms + POST_ROLL_MS).min(p.first_ms + MAX_SPAN_MS);
                // Ties go to the later event: its kill count is the fuller one.
                if m.kind >= p.best.kind {
                    p.best = m;
                }
            }
        }
    }

    /// When the open window closes, if one is open.
    pub fn deadline(&self) -> Option<u64> {
        self.pending.as_ref().map(|p| p.deadline_ms)
    }

    /// The moment to save, once its window has closed.
    pub fn poll(&mut self, now_ms: u64) -> Option<Moment> {
        if self.pending.as_ref()?.deadline_ms <= now_ms {
            return self.pending.take().map(|p| p.best);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::Kind;
    use super::*;

    fn m(kind: Kind, kills: u64) -> Moment {
        Moment { game: "cs2".into(), kind, map: "de_inferno".into(), round: 4, kills, at: 0 }
    }

    #[test]
    fn a_lone_event_saves_after_the_post_roll() {
        let mut c = Coalescer::default();
        c.push(m(Kind::Mvp, 0), 1_000);
        assert_eq!(c.deadline(), Some(1_000 + POST_ROLL_MS));
        assert!(c.poll(1_000 + POST_ROLL_MS - 1).is_none());
        assert_eq!(c.poll(1_000 + POST_ROLL_MS).unwrap().kind, Kind::Mvp);
        assert!(c.poll(u64::MAX).is_none(), "fires once");
    }

    #[test]
    fn a_streak_becomes_one_moment_named_after_its_best_event() {
        let mut c = Coalescer::default();
        c.push(m(Kind::Triple, 3), 0);
        c.push(m(Kind::Quad, 4), 2_000);
        c.push(m(Kind::Mvp, 4), 3_000);
        assert!(c.poll(6_000).is_none(), "the MVP extended the window");
        let got = c.poll(3_000 + POST_ROLL_MS).unwrap();
        assert_eq!((got.kind, got.kills), (Kind::Quad, 4));
    }

    #[test]
    fn the_window_never_outgrows_its_cap() {
        let mut c = Coalescer::default();
        for t in (0..30_000).step_by(1_000) {
            c.push(m(Kind::Kill, 1), t);
            if c.deadline().is_some_and(|d| d <= t) {
                break;
            }
        }
        assert_eq!(c.deadline(), Some(MAX_SPAN_MS));
    }
}

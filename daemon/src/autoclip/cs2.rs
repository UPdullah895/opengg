//! Counter-Strike 2 Game State Integration → in-game events.
//!
//! CS2 POSTs its state as JSON whenever it changes (see the cfg written by
//! `core::autoclip`). The payload is a snapshot, not an event log, so the
//! tracker keeps the last snapshot of *the local player* and diffs against it:
//! `round_kills` going up is a kill, `match_stats.mvps` going up is an MVP,
//! health falling to 0 is a death.
//!
//! The `player` block follows whoever the client is watching, so after a
//! death it describes a teammate. Only snapshots where `player.steamid`
//! matches `provider.steamid` (the account running the game) count; the rest
//! are ignored without disturbing the baseline, so coming back from
//! spectating does not credit the teammate's kills.

use serde_json::Value;

use super::{Kind, Moment};

#[derive(Debug, Clone, PartialEq)]
struct Own {
    map: String,
    round: i64,
    round_kills: u64,
    round_killhs: u64,
    mvps: u64,
    health: i64,
}

#[derive(Debug, Default)]
pub struct Tracker {
    last: Option<Own>,
}

impl Tracker {
    /// Feed one GSI payload; returns what happened since the previous one.
    /// `now` is stamped onto each moment (unix seconds).
    pub fn update(&mut self, payload: &Value, now: u64) -> Vec<Moment> {
        let map = &payload["map"];
        // No map block: main menu or loading. Forget the baseline so the
        // next match starts clean.
        let Some(map_name) = map["name"].as_str() else {
            self.last = None;
            return Vec::new();
        };
        let provider = payload["provider"]["steamid"].as_str();
        let player = &payload["player"];
        if provider.is_none() || player["steamid"].as_str() != provider {
            return Vec::new();
        }
        let state = &player["state"];
        let Some(health) = state["health"].as_i64() else {
            return Vec::new();
        };

        let cur = Own {
            map: map_name.to_string(),
            round: map["round"].as_i64().unwrap_or(0),
            round_kills: state["round_kills"].as_u64().unwrap_or(0),
            round_killhs: state["round_killhs"].as_u64().unwrap_or(0),
            mvps: player["match_stats"]["mvps"].as_u64().unwrap_or(0),
            health,
        };
        let prev = self.last.replace(cur.clone());

        // Warmup kills are not highlights; keep the baseline moving so the
        // first live round compares against warmup's end, not against 0.
        if map["phase"].as_str() == Some("warmup") {
            return Vec::new();
        }

        let same_map = prev.as_ref().is_some_and(|p| p.map == cur.map);
        let same_round = same_map && prev.as_ref().is_some_and(|p| p.round == cur.round);
        // A new round resets the per-round counters, so they count from 0.
        // Without any previous snapshot there is nothing to diff against:
        // joining mid-round must not report the kills already on the board.
        let Some(prev) = prev else { return Vec::new() };
        let (base_kills, base_hs) = if same_round {
            (prev.round_kills, prev.round_killhs)
        } else {
            (0, 0)
        };
        let base_mvps = if same_map { prev.mvps } else { cur.mvps };

        let moment = |kind: Kind| Moment {
            game: "cs2".into(),
            kind,
            map: cur.map.clone(),
            round: cur.round,
            kills: cur.round_kills,
            at: now,
        };

        let mut out = Vec::new();
        if cur.round_kills > base_kills {
            for n in base_kills + 1..=cur.round_kills {
                out.push(moment(match n {
                    3 => Kind::Triple,
                    4 => Kind::Quad,
                    n if n >= 5 => Kind::Ace,
                    _ => Kind::Kill,
                }));
            }
        }
        if cur.round_killhs > base_hs {
            out.push(moment(Kind::Headshot));
        }
        if cur.mvps > base_mvps {
            out.push(moment(Kind::Mvp));
        }
        if same_round && prev.health > 0 && cur.health == 0 {
            out.push(moment(Kind::Death));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ME: &str = "76561190000000001";
    const MATE: &str = "76561190000000002";

    fn snap(phase: &str, round: i64, kills: u64, hs: u64, mvps: u64, health: i64) -> Value {
        snap_as(ME, phase, round, kills, hs, mvps, health)
    }

    fn snap_as(who: &str, phase: &str, round: i64, kills: u64, hs: u64, mvps: u64, health: i64) -> Value {
        json!({
            "provider": { "name": "Counter-Strike: Global Offensive", "appid": 730, "steamid": ME },
            "map": { "name": "de_mirage", "phase": phase, "round": round },
            "player": {
                "steamid": who,
                "state": { "health": health, "round_kills": kills, "round_killhs": hs },
                "match_stats": { "mvps": mvps }
            }
        })
    }

    fn kinds(v: &[Moment]) -> Vec<Kind> {
        v.iter().map(|m| m.kind).collect()
    }

    #[test]
    fn first_snapshot_is_only_a_baseline() {
        let mut t = Tracker::default();
        assert!(t.update(&snap("live", 3, 2, 1, 1, 100), 0).is_empty());
    }

    #[test]
    fn kills_count_up_to_an_ace() {
        let mut t = Tracker::default();
        t.update(&snap("live", 1, 0, 0, 0, 100), 0);
        assert_eq!(kinds(&t.update(&snap("live", 1, 1, 1, 0, 100), 1)), [Kind::Kill, Kind::Headshot]);
        assert_eq!(kinds(&t.update(&snap("live", 1, 2, 1, 0, 100), 2)), [Kind::Kill]);
        assert_eq!(kinds(&t.update(&snap("live", 1, 3, 1, 0, 100), 3)), [Kind::Triple]);
        // Two kills inside one throttle window still report both steps.
        assert_eq!(kinds(&t.update(&snap("live", 1, 5, 1, 0, 80), 4)), [Kind::Quad, Kind::Ace]);
        let last = t.update(&snap("live", 1, 5, 1, 1, 80), 5);
        assert_eq!(kinds(&last), [Kind::Mvp]);
        assert_eq!(last[0].map, "de_mirage");
        assert_eq!(last[0].kills, 5);
    }

    #[test]
    fn new_round_counts_from_zero() {
        let mut t = Tracker::default();
        t.update(&snap("live", 1, 4, 0, 0, 100), 0);
        assert!(t.update(&snap("live", 2, 0, 0, 0, 100), 1).is_empty());
        // The round's first snapshot already carrying a kill still reports it.
        let mut t = Tracker::default();
        t.update(&snap("live", 1, 4, 0, 0, 100), 0);
        assert_eq!(kinds(&t.update(&snap("live", 2, 1, 0, 0, 100), 1)), [Kind::Kill]);
    }

    #[test]
    fn warmup_is_ignored_but_moves_the_baseline() {
        let mut t = Tracker::default();
        t.update(&snap("warmup", 0, 0, 0, 0, 100), 0);
        assert!(t.update(&snap("warmup", 0, 3, 0, 0, 100), 1).is_empty());
        assert!(t.update(&snap("live", 0, 3, 0, 0, 100), 2).is_empty());
        assert_eq!(kinds(&t.update(&snap("live", 0, 4, 0, 0, 100), 3)), [Kind::Quad]);
    }

    #[test]
    fn spectating_a_teammate_credits_nothing() {
        let mut t = Tracker::default();
        t.update(&snap("live", 1, 1, 0, 0, 100), 0);
        assert_eq!(kinds(&t.update(&snap("live", 1, 1, 0, 0, 0), 1)), [Kind::Death]);
        // Now watching a teammate who goes on to get four.
        assert!(t.update(&snap_as(MATE, "live", 1, 4, 2, 0, 100), 2).is_empty());
        // Back on our own player next round.
        assert!(t.update(&snap("live", 2, 0, 0, 0, 100), 3).is_empty());
    }

    #[test]
    fn menu_resets_and_a_new_map_rebases_mvps() {
        let mut t = Tracker::default();
        t.update(&snap("live", 10, 0, 0, 4, 100), 0);
        assert!(t.update(&json!({ "provider": { "steamid": ME } }), 1).is_empty());
        // Next match: mvps from the old map must not count as new.
        assert!(t.update(&snap("live", 0, 0, 0, 0, 100), 2).is_empty());
        assert_eq!(kinds(&t.update(&snap("live", 0, 0, 0, 1, 100), 3)), [Kind::Mvp]);
    }
}

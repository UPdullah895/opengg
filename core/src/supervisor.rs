//! Keeps the replay buffer running when the user wants it running.
//!
//! gpu-screen-recorder dies in ordinary situations — a monitor is hot-plugged,
//! the session suspends and its KMS/portal capture goes stale, a driver
//! hiccups — and the replay buffer then silently stops holding anything.
//! The `gsrAutoRestart` setting promised to handle this but nothing read it.
//!
//! This module is the decision logic, pure and clock-injected: the app ticks
//! it every few seconds with what it observes and acts on the returned
//! [`Action`]. Restarts are rate-limited so a recorder that crashes on start
//! (bad target, missing permission) is not respawned forever.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Crash restarts allowed inside [`CRASH_WINDOW`] before giving up.
pub const MAX_CRASH_RESTARTS: usize = 3;
pub const CRASH_WINDOW: Duration = Duration::from_secs(300);
/// Let the system settle after resume before respawning the capture.
pub const RESUME_SETTLE: Duration = Duration::from_secs(5);
/// A jump in suspended time bigger than this means the machine slept.
const SLEEP_JUMP: Duration = Duration::from_secs(5);
/// Shortly after boot the GPU stack and portals are still coming up; a
/// recorder started then often fails or captures a black screen.
pub const BOOT_GRACE: Duration = Duration::from_secs(120);
pub const BOOT_DELAY: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    /// Start (or restart) the recorder now.
    Restart,
    /// Too many crashes in a row: stop trying and tell the user.
    GiveUp,
}

/// What the app saw on this tick.
#[derive(Clone, Copy, Debug)]
pub struct Observation {
    /// The user wants the buffer on (they started it, or auto-start did).
    pub wanted: bool,
    /// The recorder process is alive.
    pub alive: bool,
    /// The `gsrAutoRestart` setting.
    pub auto_restart: bool,
    /// Total time the machine has spent suspended (BOOTTIME − MONOTONIC).
    pub suspended_total: Duration,
}

#[derive(Debug, Default)]
pub struct Supervisor {
    crashes: VecDeque<Instant>,
    gave_up: bool,
    last_suspended: Option<Duration>,
    resume_at: Option<Instant>,
}

impl Supervisor {
    /// The user (re)started or stopped the buffer by hand: forget past
    /// crashes, so a manual start always gets a fresh restart budget.
    pub fn reset(&mut self) {
        self.crashes.clear();
        self.gave_up = false;
        self.resume_at = None;
    }

    pub fn tick(&mut self, now: Instant, o: Observation) -> Action {
        // Resume from suspend: schedule one restart after things settle,
        // even if the process looks alive — its capture is often frozen.
        if let Some(prev) = self.last_suspended {
            if o.suspended_total > prev + SLEEP_JUMP && o.wanted && o.auto_restart {
                self.resume_at = Some(now + RESUME_SETTLE);
            }
        }
        self.last_suspended = Some(o.suspended_total);
        if let Some(at) = self.resume_at {
            if now >= at {
                self.resume_at = None;
                return Action::Restart;
            }
            return Action::Nothing;
        }

        if !o.wanted || o.alive || !o.auto_restart || self.gave_up {
            return Action::Nothing;
        }
        while self.crashes.front().is_some_and(|t| now.saturating_duration_since(*t) > CRASH_WINDOW) {
            self.crashes.pop_front();
        }
        if self.crashes.len() >= MAX_CRASH_RESTARTS {
            self.gave_up = true;
            return Action::GiveUp;
        }
        self.crashes.push_back(now);
        Action::Restart
    }
}

/// How long to wait before an automatic start, given the system uptime.
pub fn auto_start_delay(uptime: Duration) -> Duration {
    if uptime < BOOT_GRACE {
        BOOT_DELAY
    } else {
        Duration::ZERO
    }
}

/// System uptime from `/proc/uptime`.
pub fn uptime() -> Option<Duration> {
    let s = std::fs::read_to_string("/proc/uptime").ok()?;
    let secs: f64 = s.split_whitespace().next()?.parse().ok()?;
    Some(Duration::from_secs_f64(secs))
}

/// Time spent suspended since boot: CLOCK_BOOTTIME counts sleep,
/// CLOCK_MONOTONIC does not.
pub fn suspended_total() -> Duration {
    #[cfg(target_os = "linux")]
    {
        let read = |clock| {
            let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
            unsafe { libc::clock_gettime(clock, &mut ts) };
            Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
        };
        read(libc::CLOCK_BOOTTIME).saturating_sub(read(libc::CLOCK_MONOTONIC))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Duration::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(wanted: bool, alive: bool, slept: u64) -> Observation {
        Observation { wanted, alive, auto_restart: true, suspended_total: Duration::from_secs(slept) }
    }

    #[test]
    fn restarts_a_crash_then_gives_up() {
        let t0 = Instant::now();
        let mut s = Supervisor::default();
        assert_eq!(s.tick(t0, obs(true, true, 0)), Action::Nothing);
        for i in 0..3 {
            assert_eq!(s.tick(t0 + Duration::from_secs(10 * i), obs(true, false, 0)), Action::Restart);
        }
        assert_eq!(s.tick(t0 + Duration::from_secs(40), obs(true, false, 0)), Action::GiveUp);
        assert_eq!(s.tick(t0 + Duration::from_secs(50), obs(true, false, 0)), Action::Nothing);
        // A manual start resets the budget.
        s.reset();
        assert_eq!(s.tick(t0 + Duration::from_secs(60), obs(true, false, 0)), Action::Restart);
    }

    #[test]
    fn crash_budget_refills_over_time() {
        let t0 = Instant::now();
        let mut s = Supervisor::default();
        for i in 0..3 {
            assert_eq!(s.tick(t0 + Duration::from_secs(i), obs(true, false, 0)), Action::Restart);
        }
        assert_eq!(s.tick(t0 + Duration::from_secs(400), obs(true, false, 0)), Action::Restart);
    }

    #[test]
    fn never_acts_when_unwanted_or_disabled() {
        let t0 = Instant::now();
        let mut s = Supervisor::default();
        assert_eq!(s.tick(t0, obs(false, false, 0)), Action::Nothing);
        let off = Observation { auto_restart: false, ..obs(true, false, 0) };
        assert_eq!(s.tick(t0, off), Action::Nothing);
    }

    #[test]
    fn restarts_once_after_resume() {
        let t0 = Instant::now();
        let mut s = Supervisor::default();
        assert_eq!(s.tick(t0, obs(true, true, 100)), Action::Nothing);
        // The machine slept for an hour; the process still looks alive.
        assert_eq!(s.tick(t0 + Duration::from_secs(3), obs(true, true, 3700)), Action::Nothing);
        assert_eq!(s.tick(t0 + Duration::from_secs(6), obs(true, true, 3700)), Action::Nothing);
        assert_eq!(s.tick(t0 + Duration::from_secs(9), obs(true, true, 3700)), Action::Restart);
        assert_eq!(s.tick(t0 + Duration::from_secs(12), obs(true, true, 3700)), Action::Nothing);
    }

    #[test]
    fn boot_delay() {
        assert_eq!(auto_start_delay(Duration::from_secs(30)), BOOT_DELAY);
        assert_eq!(auto_start_delay(Duration::from_secs(600)), Duration::ZERO);
        assert!(uptime().is_some());
    }
}

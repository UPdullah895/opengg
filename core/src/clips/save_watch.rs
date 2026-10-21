//! Rules that make a replay save either succeed or say why it did not.
//!
//! The failure mode this replaces: saving guessed at completion with a fixed
//! 5 s poll and reported nothing when a save was refused, timed out, wrote an
//! empty file or filled the disk — the user found out at the moment they
//! went looking for the clip. Every outcome here has a name the UI can show.
//!
//! Pure state + arithmetic with the clock injected, so every threshold is
//! unit-tested without a recorder.

use std::time::{Duration, Instant};

/// A buffer this young holds next to nothing; saving it would produce a
/// clip of a second or two that the user did not mean to make.
pub const WARMUP: Duration = Duration::from_secs(2);
/// Below this after a save, the clip most likely did not fit.
pub const DISK_FULL_BYTES: u64 = 64 * 1024 * 1024;
/// Below this after a save, warn so the next save does not fail.
pub const LOW_DISK_BYTES: u64 = 10 * 1024 * 1024 * 1024;
/// Low-disk warnings repeat at most this often.
pub const LOW_DISK_REPEAT: Duration = Duration::from_secs(6 * 3600);
/// Conservative sustained write speed used to size the save timeout: a
/// spinning disk or a busy USB drive still beats it.
const WRITE_BYTES_PER_SEC: u64 = 40 * 1024 * 1024;

/// What happened when the user asked for a save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveRequest {
    /// The replay buffer is not running.
    Off,
    /// The buffer started less than [`WARMUP`] ago.
    Starting,
    /// A save is already being written; that one will report.
    Busy,
    /// The save was triggered.
    Started,
}

impl SaveRequest {
    /// Stable id for the UI / translations (`clips.save.<id>`).
    pub fn id(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Starting => "starting",
            Self::Busy => "busy",
            Self::Started => "started",
        }
    }
}

/// How long a save of `buffer_bytes` may take before it counts as failed:
/// 30 s of slack plus the time to write the buffer at 40 MiB/s.
pub fn write_limit(buffer_bytes: u64) -> Duration {
    Duration::from_secs(30) + Duration::from_secs_f64(buffer_bytes as f64 / WRITE_BYTES_PER_SEC as f64)
}

/// Worst-case size of a replay buffer: `kbps` for `secs` seconds.
pub fn buffer_bytes(kbps: u64, secs: u64) -> u64 {
    kbps * 1000 / 8 * secs
}

/// Tracks the buffer's start and the save in flight.
#[derive(Debug, Default)]
pub struct SaveGate {
    started_at: Option<Instant>,
    in_flight: Option<Instant>,
}

impl SaveGate {
    /// The buffer (re)started at `now`.
    pub fn buffer_started(&mut self, now: Instant) {
        self.started_at = Some(now);
        self.in_flight = None;
    }

    /// The buffer stopped.
    pub fn buffer_stopped(&mut self) {
        self.started_at = None;
        self.in_flight = None;
    }

    /// Decide on a save request. `Started` marks a save in flight until
    /// [`SaveGate::finished`] or until its write limit passes.
    pub fn request(&mut self, now: Instant, running: bool, buffer_bytes: u64) -> SaveRequest {
        let Some(started) = self.started_at.filter(|_| running) else {
            return SaveRequest::Off;
        };
        if now.saturating_duration_since(started) < WARMUP {
            return SaveRequest::Starting;
        }
        if let Some(since) = self.in_flight {
            if now.saturating_duration_since(since) < write_limit(buffer_bytes) {
                return SaveRequest::Busy;
            }
        }
        self.in_flight = Some(now);
        SaveRequest::Started
    }

    /// The save in flight completed (either way).
    pub fn finished(&mut self) {
        self.in_flight = None;
    }
}

/// The verdict on a file the recorder reported as saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveVerdict {
    Ok,
    /// Zero bytes landed: the write failed.
    Empty,
    /// The disk is (nearly) full, so the clip is very likely truncated.
    DiskFull,
}

/// Judge a finished save. `free_bytes` is `None` when free space is unknown
/// (network shares, FUSE), in which case a non-empty file is trusted.
pub fn verdict(file_bytes: u64, free_bytes: Option<u64>) -> SaveVerdict {
    if file_bytes == 0 {
        return SaveVerdict::Empty;
    }
    match free_bytes {
        Some(free) if free < DISK_FULL_BYTES => SaveVerdict::DiskFull,
        _ => SaveVerdict::Ok,
    }
}

/// Rate-limits the "disk is getting full" warning.
#[derive(Debug, Default)]
pub struct LowDiskWarner {
    last: Option<Instant>,
}

impl LowDiskWarner {
    /// True when `free_bytes` is known, below [`LOW_DISK_BYTES`], and no
    /// warning was shown in the last [`LOW_DISK_REPEAT`].
    pub fn should_warn(&mut self, now: Instant, free_bytes: Option<u64>) -> bool {
        let Some(free) = free_bytes else { return false };
        if free >= LOW_DISK_BYTES {
            return false;
        }
        if let Some(last) = self.last {
            if now.saturating_duration_since(last) < LOW_DISK_REPEAT {
                return false;
            }
        }
        self.last = Some(now);
        true
    }
}

/// Free bytes on the filesystem holding `path`, `None` if it cannot be read.
pub fn free_bytes(path: &std::path::Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
            return None;
        }
        Some(st.f_bavail as u64 * st.f_frsize as u64)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn write_limit_scales_with_buffer() {
        assert_eq!(write_limit(0), Duration::from_secs(30));
        let two_gb = write_limit(2 * GB);
        assert!(two_gb > Duration::from_secs(60) && two_gb < Duration::from_secs(120), "{two_gb:?}");
    }

    #[test]
    fn buffer_bytes_math() {
        // 60 Mb/s for 300 s = 2.25 GB
        assert_eq!(buffer_bytes(60_000, 300), 2_250_000_000);
    }

    #[test]
    fn gate_refuses_when_off_or_warming_up() {
        let t0 = Instant::now();
        let mut g = SaveGate::default();
        assert_eq!(g.request(t0, true, 0), SaveRequest::Off);
        g.buffer_started(t0);
        assert_eq!(g.request(t0 + Duration::from_millis(500), true, 0), SaveRequest::Starting);
        assert_eq!(g.request(t0 + Duration::from_secs(3), false, 0), SaveRequest::Off);
        assert_eq!(g.request(t0 + Duration::from_secs(3), true, 0), SaveRequest::Started);
    }

    #[test]
    fn gate_is_busy_until_finished_or_timed_out() {
        let t0 = Instant::now();
        let mut g = SaveGate::default();
        g.buffer_started(t0);
        let t = t0 + Duration::from_secs(5);
        assert_eq!(g.request(t, true, 0), SaveRequest::Started);
        assert_eq!(g.request(t + Duration::from_secs(1), true, 0), SaveRequest::Busy);
        g.finished();
        assert_eq!(g.request(t + Duration::from_secs(2), true, 0), SaveRequest::Started);
        // A save that never reports stops blocking after its write limit.
        assert_eq!(g.request(t + Duration::from_secs(33), true, 0), SaveRequest::Started);
    }

    #[test]
    fn restart_clears_in_flight() {
        let t0 = Instant::now();
        let mut g = SaveGate::default();
        g.buffer_started(t0);
        assert_eq!(g.request(t0 + Duration::from_secs(3), true, 0), SaveRequest::Started);
        g.buffer_started(t0 + Duration::from_secs(4));
        assert_eq!(g.request(t0 + Duration::from_secs(7), true, 0), SaveRequest::Started);
    }

    #[test]
    fn verdicts() {
        assert_eq!(verdict(0, Some(100 * GB)), SaveVerdict::Empty);
        assert_eq!(verdict(10, Some(10)), SaveVerdict::DiskFull);
        assert_eq!(verdict(10, None), SaveVerdict::Ok);
        assert_eq!(verdict(10, Some(GB)), SaveVerdict::Ok);
    }

    #[test]
    fn low_disk_warning_is_rate_limited() {
        let t0 = Instant::now();
        let mut w = LowDiskWarner::default();
        assert!(w.should_warn(t0, Some(8 * GB)));
        assert!(!w.should_warn(t0 + Duration::from_secs(3600), Some(7 * GB)));
        assert!(w.should_warn(t0 + Duration::from_secs(7 * 3600), Some(6 * GB)));
        assert!(!w.should_warn(t0 + Duration::from_secs(20 * 3600), None));
        assert!(!w.should_warn(t0 + Duration::from_secs(40 * 3600), Some(11 * GB)));
    }

    #[test]
    fn free_bytes_reads_a_real_filesystem() {
        assert!(free_bytes(&std::env::temp_dir()).is_some());
        assert!(free_bytes(std::path::Path::new("/definitely/not/here")).is_none());
    }
}

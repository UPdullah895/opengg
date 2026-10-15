//! Process-wide logging for the UI process.
//!
//! WHY THIS EXISTS: opengg-core has ~50 `log::warn!/error!` call sites (GSR
//! crash tails, capture-source misses, watcher failures, route verification)
//! and, until a backend is installed, every one is a silent no-op. When the
//! app is launched from the desktop menu stderr goes to the journal or
//! nowhere, so "it just doesn't work" left no trace at all. This is a tiny
//! in-house `log` backend (no new dependency) that writes every record to
//! stderr AND to `$XDG_STATE_HOME/opengg/logs/opengg.log`.
//!
//! * Rotation happens once, at startup: `opengg.log` -> `.1` -> `.2` (the
//!   oldest is dropped), so the previous two sessions survive a crash+restart.
//! * Each file is capped (`MAX_FILE_BYTES`); past the cap the file stops
//!   growing but stderr keeps going, so a log-spamming bug cannot fill a disk.
//! * Level comes from `OPENGG_LOG` (error|warn|info|debug|trace, default
//!   info). Third-party crates (zbus, hyper, ...) are held at `warn` unless the
//!   level is `trace`, otherwise `debug` is unreadable.
//! * Qt/QML messages arrive through [`qt_message`] so a "QML failed to load"
//!   error lands in the same file with category and file:line.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use log::{Level, LevelFilter, Log, Metadata, Record};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Current log + this many rotated predecessors.
const KEEP_ROTATED: u32 = 2;
const LOG_NAME: &str = "opengg.log";

static LOGGER: OnceLock<FileLogger> = OnceLock::new();

struct FileLogger {
    level: LevelFilter,
    file: Mutex<Option<File>>,
    written: AtomicU64,
    path: Option<PathBuf>,
}

/// Directory holding the log files: `$XDG_STATE_HOME/opengg/logs`
/// (default `~/.local/state/opengg/logs`). Created on demand. This is also
/// what Settings → "Open crash logs folder" opens.
pub fn log_dir() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/state")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("opengg").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Path of the active log file, once [`init`] has run and the file opened.
pub fn log_file_path() -> Option<PathBuf> {
    LOGGER.get().and_then(|l| l.path.clone())
}

/// Parse an `OPENGG_LOG` value. `None` means unrecognised.
pub fn parse_level(s: &str) -> Option<LevelFilter> {
    match s.trim().to_ascii_lowercase().as_str() {
        "off" | "none" => Some(LevelFilter::Off),
        "error" => Some(LevelFilter::Error),
        "warn" | "warning" => Some(LevelFilter::Warn),
        "info" | "" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}

/// Shift `name` -> `name.1` -> `name.2`, dropping the oldest. Missing files
/// are fine; rename failures are ignored (a stale log is better than no app).
fn rotate(dir: &Path, name: &str) {
    let numbered = |n: u32| dir.join(format!("{name}.{n}"));
    let _ = std::fs::remove_file(numbered(KEEP_ROTATED));
    for n in (1..KEEP_ROTATED).rev() {
        let _ = std::fs::rename(numbered(n), numbered(n + 1));
    }
    let _ = std::fs::rename(dir.join(name), numbered(1));
}

/// Install the logger. Call FIRST in `main()`. Idempotent; never fails —
/// if the file cannot be opened it logs to stderr only (and says so).
pub fn init() {
    let (level, bad_level) = match std::env::var("OPENGG_LOG") {
        Ok(v) => match parse_level(&v) {
            Some(l) => (l, Some(v)),
            None => (LevelFilter::Info, Some(v)),
        },
        Err(_) => (LevelFilter::Info, None),
    };
    let level_invalid = bad_level.as_deref().map(|v| parse_level(v).is_none()).unwrap_or(false);

    let dir = log_dir();
    rotate(&dir, LOG_NAME);
    let path = dir.join(LOG_NAME);
    let file = OpenOptions::new().create(true).append(true).open(&path);
    let (file, path, file_err) = match file {
        Ok(f) => (Some(f), Some(path), None),
        Err(e) => (None, None, Some(format!("{}: {e}", path.display()))),
    };

    let logger = FileLogger {
        level,
        file: Mutex::new(file),
        written: AtomicU64::new(0),
        path,
    };
    if LOGGER.set(logger).is_err() {
        return;
    }
    let l = LOGGER.get().unwrap();
    if log::set_logger(l).is_err() {
        return;
    }
    log::set_max_level(level);

    if level_invalid {
        log::warn!(
            "OPENGG_LOG={:?} is not one of error|warn|info|debug|trace; using info",
            bad_level.unwrap_or_default()
        );
    }
    if let Some(e) = file_err {
        log::warn!("log file unavailable, logging to stderr only ({e})");
    }
}

/// Log panics (thread, location, message, backtrace when `RUST_BACKTRACE` is
/// set) to the log file, then run the previously installed hook so the usual
/// stderr output and `RUST_BACKTRACE` behaviour are unchanged. A panic in one
/// of the many detached worker threads otherwise vanishes when the app was
/// launched from the menu.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("<unnamed>");
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown location>".into());
        let payload = info.payload();
        let msg = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".into());
        // Captured only when RUST_BACKTRACE/RUST_LIB_BACKTRACE asks for it.
        let bt = std::backtrace::Backtrace::capture();
        let bt = if bt.status() == std::backtrace::BacktraceStatus::Captured {
            format!("\n{bt}")
        } else {
            String::new()
        };
        log::error!(target: "opengg::panic", "thread '{name}' panicked at {loc}: {msg}{bt}");
        previous(info);
    }));
}

/// Entry point for the Qt message handler (see `qt-shell/src/cpp/log_bridge.cpp`).
/// `qt_level`: 0 debug, 1 warning, 2 critical, 3 fatal, 4 info (Qt's QtMsgType).
pub fn qt_message(qt_level: i32, category: &str, file: &str, line: i32, msg: &str) {
    let category = if category.is_empty() { "default" } else { category };
    let level = match qt_level {
        1 => Level::Warn,
        2 | 3 => Level::Error,
        4 => Level::Info,
        // console.log()/qDebug() from the app's own QML are development
        // output; keep them visible at the default level. Qt's internal
        // debug categories stay at debug.
        _ if matches!(category, "qml" | "js" | "default") => Level::Info,
        _ => Level::Debug,
    };
    let target = format!("qt.{category}");
    let mut b = Record::builder();
    b.level(level).target(&target);
    if !file.is_empty() {
        b.file(Some(file)).line(u32::try_from(line).ok().filter(|l| *l > 0));
    }
    if let Some(l) = LOGGER.get() {
        l.log(&b.args(format_args!("{msg}")).build());
    } else {
        eprintln!("[qt.{category}] {msg}");
    }
}

fn local_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        now.subsec_millis()
    )
}

impl Log for FileLogger {
    fn enabled(&self, m: &Metadata) -> bool {
        if m.level() > self.level {
            return false;
        }
        let ours = m.target().starts_with("opengg") || m.target().starts_with("qt.");
        ours || self.level == LevelFilter::Trace || m.level() <= Level::Warn
    }

    fn log(&self, r: &Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let loc = match (r.file(), r.line()) {
            (Some(f), Some(l)) if r.target().starts_with("qt.") => format!(" ({f}:{l})"),
            _ => String::new(),
        };
        let line = format!(
            "{} {:<5} {}: {}{}\n",
            local_timestamp(),
            r.level(),
            r.target(),
            r.args(),
            loc
        );
        let _ = std::io::stderr().write_all(line.as_bytes());

        let mut guard = match self.file.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(f) = guard.as_mut() {
            let n = line.len() as u64;
            let before = self.written.fetch_add(n, Ordering::Relaxed);
            if before + n <= MAX_FILE_BYTES {
                let _ = f.write_all(line.as_bytes());
            } else if before <= MAX_FILE_BYTES {
                let _ = f.write_all(b"-- log file size cap reached; further messages go to stderr only --\n");
            }
        }
    }

    fn flush(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_parsing() {
        assert_eq!(parse_level("DEBUG"), Some(LevelFilter::Debug));
        assert_eq!(parse_level(" warn "), Some(LevelFilter::Warn));
        assert_eq!(parse_level("verbose"), None);
    }

    #[test]
    fn rotation_keeps_current_plus_two() {
        let dir = std::env::temp_dir().join(format!("opengg-rot-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for gen in ["a", "b", "c", "d"] {
            rotate(&dir, "t.log");
            std::fs::write(dir.join("t.log"), gen).unwrap();
        }
        assert_eq!(std::fs::read_to_string(dir.join("t.log")).unwrap(), "d");
        assert_eq!(std::fs::read_to_string(dir.join("t.log.1")).unwrap(), "c");
        assert_eq!(std::fs::read_to_string(dir.join("t.log.2")).unwrap(), "b");
        assert!(!dir.join("t.log.3").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

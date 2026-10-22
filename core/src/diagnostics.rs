//! Logs, crash reports and the "copy diagnostics" bundle.
//!
//! Before this module the app had no logger installed: every `log::warn!`
//! in this crate went nowhere, there was no panic hook, and "Open crash logs
//! folder" opened a path on the *build* machine (`CARGO_MANIFEST_DIR/../Logs`).
//! A failed save or a crash therefore left nothing a user could attach to a
//! bug report.
//!
//! Everything lives under `$XDG_STATE_HOME/opengg/` (default
//! `~/.local/state/opengg/`), stays on the machine, and is only shared when
//! the user copies it. No telemetry.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Rotate the log once it passes this size; one previous file is kept.
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
/// Crash reports kept; older ones are removed when a new one is written.
const MAX_CRASH_REPORTS: usize = 10;

/// `$XDG_STATE_HOME/opengg`, created on demand.
pub fn log_dir() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/state")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("opengg");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

struct FileLogger {
    file: Mutex<Option<std::fs::File>>,
    path: PathBuf,
    level: log::LevelFilter,
}

impl FileLogger {
    fn rotate_if_needed(&self, guard: &mut Option<std::fs::File>) {
        let big = std::fs::metadata(&self.path).map(|m| m.len() > MAX_LOG_BYTES).unwrap_or(false);
        if big {
            let _ = std::fs::rename(&self.path, self.path.with_extension("log.1"));
            *guard = open_append(&self.path);
        }
    }
}

fn open_append(path: &Path) -> Option<std::fs::File> {
    std::fs::OpenOptions::new().create(true).append(true).open(path).ok()
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} {}: {}\n",
            timestamp(),
            record.level(),
            record.target(),
            record.args()
        );
        // stderr too: `./dev.sh` and journald-captured launches still see it.
        let _ = std::io::stderr().write_all(line.as_bytes());
        if let Ok(mut guard) = self.file.lock() {
            self.rotate_if_needed(&mut guard);
            if let Some(f) = guard.as_mut() {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = self.file.lock() {
            if let Some(f) = guard.as_mut() {
                let _ = f.flush();
            }
        }
    }
}

/// Local time as `YYYY-MM-DD HH:MM:SS`.
fn timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let s = crate::clips::organizer::local_stamp(secs); // YYYY-MM-DD_HH-MM-SS
    format!("{} {}", &s[..10], s[11..].replace('-', ":"))
}

static APP: OnceLock<String> = OnceLock::new();

/// Install the file logger and the panic hook for `app` (e.g. "opengg-qt").
/// Call once, first thing in `main`. Level: `OPENGG_LOG` (error, warn, info,
/// debug, trace), default `info`.
pub fn init(app: &str) {
    let _ = APP.set(app.to_string());
    let level = std::env::var("OPENGG_LOG")
        .ok()
        .and_then(|v| v.parse::<log::LevelFilter>().ok())
        .unwrap_or(log::LevelFilter::Info);
    let path = log_dir().join(format!("{app}.log"));
    let logger = FileLogger { file: Mutex::new(open_append(&path)), path, level };
    if log::set_logger(Box::leak(Box::new(logger))).is_ok() {
        log::set_max_level(level);
    }
    install_panic_hook();
    log::info!("{app} {} starting", env!("CARGO_PKG_VERSION"));
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = crash_report(&info.to_string(), &std::backtrace::Backtrace::force_capture().to_string());
        let written = write_crash_report(&log_dir(), &report);
        log::error!("panic: {info}");
        if let Some(p) = written {
            let _ = writeln!(std::io::stderr(), "OpenGG crashed; report saved to {}", p.display());
        }
        previous(info);
    }));
}

/// The text of a crash report.
pub fn crash_report(message: &str, backtrace: &str) -> String {
    format!(
        "OpenGG crash report\napp: {}\nversion: {}\ntime: {}\nos: {}\nsession: {}\n\n{}\n\nbacktrace:\n{}\n",
        APP.get().map(String::as_str).unwrap_or("unknown"),
        env!("CARGO_PKG_VERSION"),
        timestamp(),
        os_pretty_name(),
        session_description(),
        message,
        backtrace
    )
}

/// Write `crash-<stamp>.log` into `dir`, keeping the newest
/// [`MAX_CRASH_REPORTS`]. Returns the path written.
pub fn write_crash_report(dir: &Path, report: &str) -> Option<PathBuf> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    // Nanoseconds in the name keep reports written in the same second
    // distinct *and* in chronological order when sorted by name, which the
    // pruning below relies on.
    let path = crate::clips::organizer::free_path(&dir.join(format!(
        "crash-{}.{:09}.log",
        crate::clips::organizer::local_stamp(now.as_secs() as i64),
        now.subsec_nanos()
    )));
    std::fs::write(&path, report).ok()?;
    let mut reports = crash_reports(dir);
    while reports.len() > MAX_CRASH_REPORTS {
        let _ = std::fs::remove_file(reports.remove(0));
    }
    Some(path)
}

/// Crash report files in `dir`, oldest first.
pub fn crash_reports(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .map(|n| {
                            let n = n.to_string_lossy();
                            n.starts_with("crash-") && n.ends_with(".log")
                        })
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn os_pretty_name() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string()))
        })
        .unwrap_or_else(|| "unknown".into())
}

fn session_description() -> String {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    format!("{} / {}", var("XDG_SESSION_TYPE"), var("XDG_CURRENT_DESKTOP"))
}

/// The last `n` lines of a text file.
fn tail(path: &Path, n: usize) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Everything useful for a bug report, as plain text the user can read
/// before pasting it anywhere: versions, session, tool availability, device
/// access, daemon reachability, the newest crash report and the log tail.
pub fn report() -> String {
    let mut out = String::new();
    let push = |out: &mut String, s: String| {
        out.push_str(&s);
        out.push('\n');
    };
    push(&mut out, format!("OpenGG diagnostics — {}", timestamp()));
    push(&mut out, format!("version: {}", env!("CARGO_PKG_VERSION")));
    push(&mut out, format!("os: {}", os_pretty_name()));
    push(&mut out, format!("session: {}", session_description()));
    let daemon = crate::daemon::call_dbus::<String>("GetStatus", crate::daemon::RP_PATH, crate::daemon::RP_IFACE, ())
        .map(|s| format!("reachable (replay status: {s})"))
        .unwrap_or_else(|e| format!("NOT reachable: {e}"));
    push(&mut out, format!("daemon: {daemon}"));
    out.push_str("\ntools:\n");
    for d in crate::system::get_dependency_status() {
        push(&mut out, format!("  {:<22} {}", d.binary, if d.available { "found" } else { "MISSING" }));
    }
    let access = crate::system::get_device_access_status();
    out.push_str("\naccess:\n");
    push(&mut out, format!("  ratbagd: {}", access.ratbagd_available));
    push(&mut out, format!("  input group: {}", access.in_input_group));
    push(&mut out, format!("  video group: {}", access.in_video_group));
    let dir = log_dir();
    if let Some(latest) = crash_reports(&dir).last() {
        out.push_str(&format!("\nlatest crash report ({}):\n", latest.display()));
        push(&mut out, tail(latest, 60));
    }
    let app = APP.get().cloned().unwrap_or_else(|| "opengg-qt".into());
    out.push_str(&format!("\nlog tail ({}):\n", dir.join(format!("{app}.log")).display()));
    push(&mut out, tail(&dir.join(format!("{app}.log")), 120));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_reports_are_written_and_capped() {
        let d = std::env::temp_dir().join(format!("opengg-diag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for i in 0..(MAX_CRASH_REPORTS + 3) {
            let p = write_crash_report(&d, &format!("report {i}")).unwrap();
            assert!(p.exists());
        }
        let all = crash_reports(&d);
        assert_eq!(all.len(), MAX_CRASH_REPORTS);
        // The newest survives.
        assert!(std::fs::read_to_string(all.last().unwrap()).unwrap().contains(&format!("report {}", MAX_CRASH_REPORTS + 2)));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn report_text_has_the_essentials() {
        let r = crash_report("boom at x.rs:1", "frame 0");
        assert!(r.contains("boom at x.rs:1") && r.contains("version:") && r.contains("frame 0"));
    }

    #[test]
    fn timestamp_shape() {
        let t = timestamp();
        // YYYY-MM-DD HH:MM:SS
        assert_eq!(t.len(), 19, "{t}");
        assert_eq!(&t[4..5], "-");
        assert_eq!(&t[10..11], " ");
        assert_eq!(&t[13..14], ":");
    }
}

//! Central subprocess module for spawning external binaries.
//!
//! This module provides:
//! - Binary availability checking via $PATH lookup (cached)
//! - Logged command construction for tracing execution
//! - Convenience runners for common query patterns
//!
//! The availability probe (`is_available`/`require`) and the `run`/`run_async`
//! convenience runners are consumed by the dependency-probing / graceful
//! degradation work (T2.1) and the ongoing subprocess migration; they are
//! allowed to be unused until those call sites land.
#![allow(dead_code)]

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::ffi::OsStr;
use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use std::sync::{Mutex, OnceLock};
use tracing::{debug, trace, warn};

/// Global cache for binary availability checks.
static BIN_CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();

/// Initialize or get the binary cache.
fn get_cache() -> &'static Mutex<HashMap<String, bool>> {
    BIN_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Check if a binary is available on $PATH.
///
/// Implements a `which`-style lookup: iterates $PATH entries and checks for an
/// executable file. Results are cached in a global `HashMap<String, bool>`.
pub fn is_available(bin: &str) -> bool {
    let cache = get_cache();
    let mut map = cache.lock().unwrap();

    if let Some(&cached) = map.get(bin) {
        return cached;
    }

    let found = find_in_path(bin).is_some();
    map.insert(bin.to_string(), found);
    found
}

/// Require a binary to be available, returning an error if it is not.
pub fn require(bin: &str) -> Result<(), String> {
    if is_available(bin) {
        Ok(())
    } else {
        Err(format!("Binary not found: {}", bin))
    }
}

/// Find a binary in $PATH by name.
fn find_in_path(bin: &str) -> Option<String> {
    let path_var = std::env::var("PATH").ok()?;

    for dir in path_var.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = Path::new(dir).join(bin);
        if candidate.exists() && is_executable(&candidate) {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

/// Check if a path is an executable file.
fn is_executable(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(meta) => {
            meta.is_file() && {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    (meta.permissions().mode() & 0o111) != 0
                }
                #[cfg(not(unix))]
                {
                    true
                }
            }
        }
        Err(_) => false,
    }
}

/// Default limit for one query or control command. pactl, pw-link and
/// headsetcontrol answer in milliseconds; when the sound server or a
/// headset's HID link wedges they can block forever, and before this every
/// caller (D-Bus handlers, the reconciler) blocked with them.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(8);

/// Construct a logged command for executing a binary, with the C locale and
/// [`DEFAULT_TIMEOUT`]. Only the builder methods callers actually use are
/// exposed, so every spawn goes through the timeout.
pub fn command(bin: &str) -> Cmd {
    debug!("subprocess: invoking {}", bin);
    let mut cmd = Command::new(bin);
    // Every caller (pactl, pw-link, pw-cli, headsetcontrol) parses the
    // output, and pactl's text output and error messages are translated.
    // Pin the C locale so the parsers see the strings they were written for.
    cmd.env("LC_ALL", "C");
    Cmd { cmd, timeout: DEFAULT_TIMEOUT }
}

/// A [`Command`] that is killed if it outlives its timeout.
pub struct Cmd {
    cmd: Command,
    timeout: Duration,
}

impl Cmd {
    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.cmd.arg(arg);
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.cmd.args(args);
        self
    }

    pub fn env<K: AsRef<OsStr>, V: AsRef<OsStr>>(&mut self, key: K, val: V) -> &mut Self {
        self.cmd.env(key, val);
        self
    }

    pub fn timeout(&mut self, timeout: Duration) -> &mut Self {
        self.timeout = timeout;
        self
    }

    /// Run to completion, capturing stdout and stderr. A command still
    /// running at the timeout is killed and reported as `TimedOut`.
    pub fn output(&mut self) -> io::Result<Output> {
        self.cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        run_with_timeout(&mut self.cmd, self.timeout)
    }

    /// Like [`Cmd::output`], discarding the output.
    pub fn status(&mut self) -> io::Result<ExitStatus> {
        self.cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        run_with_timeout(&mut self.cmd, self.timeout).map(|o| o.status)
    }
}

fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> io::Result<Output> {
    // Own process group, so a timeout can kill anything the command forked
    // too — a grandchild holding the output pipe would otherwise keep the
    // readers below blocked long after the command itself died.
    cmd.process_group(0);
    let mut child = cmd.spawn()?;
    let pid = child.id() as libc::pid_t;
    let program = format!("{:?}", cmd.get_program());
    // The child is waited on (and its pipes drained) by a helper thread, so
    // this thread can give up at the deadline. The killed child stays a
    // zombie until the helper reaps it, so its pid cannot be reused by an
    // unrelated process before the kill lands.
    let (tx, rx) = mpsc::channel();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    std::thread::spawn(move || {
        let read = |p: Option<Box<dyn Read + Send>>| {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                if let Some(mut p) = p {
                    let _ = p.read_to_end(&mut buf);
                }
                buf
            })
        };
        let out = read(stdout.map(|p| Box::new(p) as Box<dyn Read + Send>));
        let err = read(stderr.map(|p| Box::new(p) as Box<dyn Read + Send>));
        let status = child.wait();
        let out = out.join().unwrap_or_default();
        let err = err.join().unwrap_or_default();
        let _ = tx.send(status.map(|status| Output { status, stdout: out, stderr: err }));
    });
    match rx.recv_timeout(timeout) {
        Ok(res) => res,
        Err(_) => {
            // SAFETY: kill(2) on the process group we created; the leader
            // (our child) is not reaped until the helper's wait() returns,
            // so the group id cannot have been reused.
            unsafe { libc::kill(-pid, libc::SIGKILL) };
            // Reap it if we can; a process that left the group (setsid) and
            // still holds a pipe only keeps the helper thread alive.
            let _ = rx.recv_timeout(Duration::from_secs(1));
            warn!("subprocess: {program} timed out after {timeout:?} and was killed");
            Err(io::Error::new(io::ErrorKind::TimedOut, format!("{program} timed out after {timeout:?}")))
        }
    }
}

/// Run a command synchronously and return its output.
///
/// Logs the binary and arguments, runs `.output()`, and on non-zero status
/// returns an `Err` containing the stderr output.
///
/// # Arguments
/// - `bin` — binary name or path
/// - `args` — argument vector (enforces vector invariant, never interpolated into shell)
///
/// # Returns
/// - `Ok(Output)` on success (exit code 0)
/// - `Err(String)` on failure (non-zero exit or spawn error)
pub fn run(bin: &str, args: &[&str]) -> Result<std::process::Output, String> {
    trace!("subprocess::run: {} {:?}", bin, args);
    let output = command(bin)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to spawn {}: {}", bin, e))?;

    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("{} failed: {}", bin, stderr.trim()))
    }
}

/// Async version of `run` for use with tokio.
///
/// Spawns a tokio process, waits for it to complete, and returns the output.
pub async fn run_async(
    bin: &str,
    args: &[&str],
) -> Result<std::process::Output, String> {
    trace!("subprocess::run_async: {} {:?}", bin, args);
    let mut cmd = tokio::process::Command::new(bin);
    cmd.args(args);

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to spawn {}: {}", bin, e))?;

    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("{} failed: {}", bin, stderr.trim()))
    }
}

/// Construct a logged tokio command for executing a binary asynchronously.
///
/// Logs the binary invocation at debug level. The returned `tokio::process::Command`
/// can be further configured with args, stdin, stdout, etc., and then spawned by
/// the caller.
pub fn tokio_command(bin: &str) -> tokio::process::Command {
    debug!("subprocess: invoking {} (async)", bin);
    tokio::process::Command::new(bin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_in_path() {
        // "sh" should always exist on Unix systems
        assert!(is_available("sh"));
    }

    #[test]
    fn command_captures_output() {
        let out = command("sh").args(["-c", "echo hi; echo err >&2; exit 3"]).output().unwrap();
        assert_eq!(out.stdout, b"hi\n");
        assert_eq!(out.stderr, b"err\n");
        assert_eq!(out.status.code(), Some(3));
        assert!(command("sh").args(["-c", "exit 0"]).status().unwrap().success());
    }

    #[test]
    fn command_runs_in_the_c_locale() {
        let out = command("sh").args(["-c", "echo $LC_ALL"]).output().unwrap();
        assert_eq!(out.stdout, b"C\n");
    }

    #[test]
    fn hung_command_is_killed_at_the_timeout() {
        let start = std::time::Instant::now();
        let err = command("sleep").arg("30").timeout(Duration::from_millis(200)).output().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(5));
        // A grandchild holding the pipe open must not block the timeout either.
        let start = std::time::Instant::now();
        let err = command("sh")
            .args(["-c", "sleep 30 & sleep 30"])
            .timeout(Duration::from_millis(200))
            .output()
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn test_nonexistent_binary() {
        assert!(!is_available("nonexistent_binary_xyz_123"));
    }

    #[test]
    fn test_caching() {
        // Check twice; the second should hit the cache
        let _ = is_available("sh");
        let start = std::time::Instant::now();
        let _ = is_available("sh");
        let elapsed = start.elapsed();
        // Cache lookup should be very fast (< 1ms on any system)
        assert!(elapsed.as_millis() < 10);
    }
}

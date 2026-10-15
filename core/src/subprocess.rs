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

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, ChildStderr, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use log::{debug, warn};

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

/// Uncached `which`: the full path of `bin` on $PATH, if any.
///
/// For probes the user can re-run (the dependency list's "Recheck"), where
/// `is_available`'s process-lifetime cache would keep reporting a binary as
/// missing after it has been installed.
pub fn find_in_path(bin: &str) -> Option<String> {
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

/// Construct a logged command for executing a binary.
///
/// Logs the binary invocation at debug level. The returned `Command` can be
/// further configured with args, stdin, stdout, etc., and then spawned by
/// the caller.
pub fn command(bin: &str) -> Command {
    debug!("subprocess: invoking {}", bin);
    Command::new(bin)
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
    let output = Command::new(bin)
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

// ═══ Typed, bounded tool runner ═══
//
// Everything below exists because a bare `Command::output()` has three
// failure modes that all look like "nothing happened": a tool that is not
// installed, a tool that hangs (a wedged PipeWire never answers `pactl`, and
// on the UI thread that freezes the window), and a tool whose output is
// translated (`pactl` is gettext-localised, so parsers matching English
// labels quietly find nothing on a non-English desktop).

/// Default ceiling for PipeWire/PulseAudio tools. They answer in
/// milliseconds when healthy; five seconds means the server is wedged.
pub const TOOL_TIMEOUT: Duration = Duration::from_secs(5);
/// Ceiling for tools that legitimately do real work (ffprobe on a clip on a
/// network mount, a single-frame ffmpeg grab).
pub const SLOW_TOOL_TIMEOUT: Duration = Duration::from_secs(30);

/// Why a tool run did not produce usable output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// The binary is not on `$PATH`.
    ToolMissing { tool: String },
    /// The tool did not finish in time and was killed.
    TimedOut { tool: String, after: Duration },
    /// The tool ran (or could not be started for a reason other than being
    /// absent) and failed. `status` is `None` when it was killed by a signal
    /// or never started.
    Failed { tool: String, status: Option<i32>, stderr: String },
}

impl ToolError {
    /// Name of the tool this error is about.
    pub fn tool(&self) -> &str {
        match self {
            ToolError::ToolMissing { tool }
            | ToolError::TimedOut { tool, .. }
            | ToolError::Failed { tool, .. } => tool,
        }
    }

    /// True when the failure is "the sound server is not there" rather than
    /// a problem with the request: pactl/pw-* print one of these when
    /// pipewire-pulse (or PipeWire itself) is not running. Only meaningful
    /// because the runner pins `LC_ALL=C`, so the text is not translated.
    pub fn is_server_unreachable(&self) -> bool {
        match self {
            ToolError::Failed { stderr, .. } => {
                let s = stderr.to_ascii_lowercase();
                s.contains("connection refused")
                    || s.contains("failed to connect")
                    || s.contains("connection failure")
                    || s.contains("no pulseaudio daemon")
                    || s.contains("host is down")
            }
            ToolError::TimedOut { .. } => true,
            ToolError::ToolMissing { .. } => false,
        }
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolError::ToolMissing { tool } => write!(
                f,
                "`{tool}` is not installed or not on PATH (install {})",
                package_hint(tool)
            ),
            ToolError::TimedOut { tool, after } => write!(
                f,
                "`{tool}` did not answer within {}s and was stopped (is PipeWire hung?)",
                after.as_secs_f32().round()
            ),
            ToolError::Failed { tool, status, stderr } => {
                match status {
                    Some(code) => write!(f, "`{tool}` failed (exit {code})")?,
                    None => write!(f, "`{tool}` failed")?,
                }
                let tail = tail_lines(stderr, 3);
                if !tail.is_empty() {
                    write!(f, ": {tail}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ToolError {}

/// Last `n` non-empty lines of `s`, joined with " | ".
fn tail_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join(" | ")
}

/// Package that provides `tool` on the running distro, for error messages.
///
/// Kept local (rather than shared with `system.rs`) so this module stays
/// dependency-free; the distro table there serves the Settings dependency
/// card and this one only phrases a one-line hint.
pub fn package_hint(tool: &str) -> String {
    let os = fs::read_to_string("/etc/os-release").unwrap_or_default();
    package_hint_for(tool, &os)
}

/// Pure core of [`package_hint`], parameterised on `/etc/os-release` text.
pub fn package_hint_for(tool: &str, os_release: &str) -> String {
    let mut ids: Vec<String> = Vec::new();
    for line in os_release.lines() {
        if let Some(v) = line.strip_prefix("ID=").or_else(|| line.strip_prefix("ID_LIKE=")) {
            ids.extend(v.trim_matches('"').split_whitespace().map(str::to_lowercase));
        }
    }
    let has = |names: &[&str]| ids.iter().any(|i| names.contains(&i.as_str()));
    let (arch, debian, fedora, suse) = (
        has(&["arch", "manjaro", "cachyos", "endeavouros"]),
        has(&["debian", "ubuntu", "linuxmint", "pop"]),
        has(&["fedora", "rhel", "centos", "nobara"]),
        has(&["opensuse", "suse", "sles"]),
    );
    // (arch, debian, fedora, suse) package for each tool family.
    let pkgs: Option<[&str; 4]> = match tool {
        "pactl" | "pacmd" => Some(["libpulse", "pulseaudio-utils", "pulseaudio-utils", "pulseaudio-utils"]),
        "pw-cli" | "pw-dump" | "pw-link" | "pw-cat" | "pw-metadata" => {
            Some(["pipewire", "pipewire-bin", "pipewire-utils", "pipewire-tools"])
        }
        "pipewire" => Some(["pipewire", "pipewire", "pipewire", "pipewire"]),
        "ffmpeg" | "ffprobe" => Some(["ffmpeg", "ffmpeg", "ffmpeg", "ffmpeg"]),
        _ => None,
    };
    match pkgs {
        Some(p) if arch => p[0].to_string(),
        Some(p) if debian => p[1].to_string(),
        Some(p) if fedora => p[2].to_string(),
        Some(p) if suse => p[3].to_string(),
        Some(p) => format!("{} (Arch) / {} (Debian, Ubuntu) / {} (Fedora)", p[0], p[1], p[2]),
        None => format!("the package that provides `{tool}`"),
    }
}

/// Pin the locale for tools whose output is parsed. `LC_ALL=C` is the one
/// that matters (gettext ignores `LANGUAGE` when the locale is `C`).
pub fn pin_locale(cmd: &mut Command) {
    cmd.env("LC_ALL", "C").env("LANG", "C");
}

/// Default timeout for a tool: short for the audio stack, longer for media
/// tools that do real work.
fn default_timeout(tool: &str) -> Duration {
    if tool == "pactl" || tool.starts_with("pw-") || tool == "pipewire" {
        TOOL_TIMEOUT
    } else {
        SLOW_TOOL_TIMEOUT
    }
}

fn spawn_error(tool: &str, e: std::io::Error) -> ToolError {
    if e.kind() == std::io::ErrorKind::NotFound {
        ToolError::ToolMissing { tool: tool.to_string() }
    } else {
        ToolError::Failed { tool: tool.to_string(), status: None, stderr: e.to_string() }
    }
}

/// Read a pipe to the end on its own thread so a chatty tool can never block
/// on a full pipe while we wait for it to exit. The result arrives over a
/// channel so a caller can give up on it (a grandchild holding the pipe open
/// after we killed the tool) instead of joining forever.
fn drain<R: Read + Send + 'static>(r: Option<R>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut r) = r {
            let _ = r.read_to_end(&mut buf);
        }
        let _ = tx.send(buf);
    });
    rx
}

/// Run `tool` with the locale pinned to `C`, killing it after `timeout`.
/// Returns its stdout (trimmed) on exit status 0.
pub fn run_tool_timeout(tool: &str, args: &[&str], timeout: Duration) -> Result<String, ToolError> {
    debug!("subprocess: {tool} {args:?} (timeout {timeout:?})");
    let mut cmd = Command::new(tool);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    pin_locale(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| spawn_error(tool, e))?;
    let out_rx = drain(child.stdout.take());
    let err_rx = drain(child.stderr.take());

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                return Err(ToolError::Failed { tool: tool.into(), status: None, stderr: e.to_string() });
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            warn!("subprocess: `{tool} {}` timed out after {timeout:?}; killed", args.join(" "));
            return Err(ToolError::TimedOut { tool: tool.into(), after: timeout });
        }
        std::thread::sleep(Duration::from_millis(5));
    };

    let grace = Duration::from_millis(500);
    let stdout = out_rx.recv_timeout(grace).unwrap_or_default();
    let stderr = err_rx.recv_timeout(grace).unwrap_or_default();
    if status.success() {
        Ok(String::from_utf8_lossy(&stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
        debug!("subprocess: `{tool} {}` exited {:?}: {stderr}", args.join(" "), status.code());
        Err(ToolError::Failed { tool: tool.into(), status: status.code(), stderr })
    }
}

/// [`run_tool_timeout`] with the tool family's default timeout.
pub fn run_tool(tool: &str, args: &[&str]) -> Result<String, ToolError> {
    run_tool_timeout(tool, args, default_timeout(tool))
}

/// Run a command synchronously and return its trimmed stdout.
///
/// Legacy string-error runner kept for callers outside the audio path. It now
/// goes through [`run_tool`], so it too is locale-pinned and bounded; on
/// failure the error is the trimmed stderr (as before) or, when there is
/// none (missing tool, timeout), the typed error's message.
pub fn run_cmd_sync(c: &str, a: &[&str]) -> Result<String, String> {
    run_tool(c, a).map_err(|e| match e {
        ToolError::Failed { stderr, .. } if !stderr.is_empty() => stderr,
        other => other.to_string(),
    })
}

/// Last lines of a child's stderr, kept in a small ring buffer.
///
/// Long-running children (the EQ's `pipewire -c` host) cannot be `.output()`ed,
/// but when they die their last words are the only diagnosis. Dropping the
/// pipe unread would also eventually block the child on a full pipe.
#[derive(Clone, Default)]
pub struct StderrTail(Arc<Mutex<VecDeque<String>>>);

impl StderrTail {
    const KEEP: usize = 20;

    /// Start draining `stderr` on a background thread. Each line is also
    /// logged at debug level.
    pub fn capture(label: &str, stderr: ChildStderr) -> Self {
        let tail = Self::default();
        let sink = tail.clone();
        let label = label.to_string();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                debug!("{label}: {line}");
                sink.push(line);
            }
        });
        tail
    }

    fn push(&self, line: String) {
        let mut q = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if q.len() == Self::KEEP {
            q.pop_front();
        }
        q.push_back(line);
    }

    /// The last `n` non-empty lines, joined with " | ".
    pub fn tail(&self, n: usize) -> String {
        let q = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let joined = q.iter().cloned().collect::<Vec<_>>().join("\n");
        tail_lines(&joined, n)
    }
}

/// Convenience: take a spawned child's stderr into a [`StderrTail`].
pub fn capture_stderr(label: &str, child: &mut Child) -> StderrTail {
    match child.stderr.take() {
        Some(e) => StderrTail::capture(label, e),
        None => StderrTail::default(),
    }
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
    fn test_nonexistent_binary() {
        assert!(!is_available("nonexistent_binary_xyz_123"));
    }

    #[test]
    fn missing_tool_is_reported_as_such_with_a_package_hint() {
        let e = run_tool("opengg-no-such-tool-xyz", &[]).unwrap_err();
        assert!(matches!(e, ToolError::ToolMissing { ref tool } if tool == "opengg-no-such-tool-xyz"));
        assert!(e.to_string().contains("opengg-no-such-tool-xyz"));
    }

    #[test]
    fn a_hung_tool_is_killed_at_the_timeout() {
        let start = Instant::now();
        let e = run_tool_timeout("sleep", &["10"], Duration::from_millis(200)).unwrap_err();
        assert!(matches!(e, ToolError::TimedOut { .. }), "got {e:?}");
        assert!(start.elapsed() < Duration::from_secs(3), "was not killed promptly");
    }

    #[test]
    fn failure_carries_status_and_stderr() {
        let e = run_tool("sh", &["-c", "echo boom >&2; exit 3"]).unwrap_err();
        match e {
            ToolError::Failed { status, stderr, .. } => {
                assert_eq!(status, Some(3));
                assert_eq!(stderr, "boom");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn locale_is_pinned_to_c() {
        let out = run_tool("sh", &["-c", "printf '%s/%s' \"$LC_ALL\" \"$LANG\""]).unwrap();
        assert_eq!(out, "C/C");
    }

    #[test]
    fn large_output_does_not_deadlock() {
        // Well past a pipe buffer (64 KiB) on both streams at once.
        let script = "head -c 300000 /dev/zero | tr '\\0' x; head -c 300000 /dev/zero | tr '\\0' y >&2";
        let out = run_tool("sh", &["-c", script]).unwrap();
        assert_eq!(out.len(), 300000);
    }

    #[test]
    fn server_unreachable_is_recognised_from_pactl_text() {
        let e = ToolError::Failed {
            tool: "pactl".into(),
            status: Some(1),
            stderr: "Connection failure: Connection refused\npa_context_connect() failed: Connection refused".into(),
        };
        assert!(e.is_server_unreachable());
        let e = ToolError::Failed { tool: "pactl".into(), status: Some(1), stderr: "No such entity".into() };
        assert!(!e.is_server_unreachable());
    }

    #[test]
    fn package_hint_follows_the_distro() {
        let debian = "ID=debian\nVERSION_ID=\"13\"\n";
        let fedora = "ID=fedora\n";
        let mint = "ID=linuxmint\nID_LIKE=\"ubuntu debian\"\n";
        let arch = "ID=cachyos\nID_LIKE=arch\n";
        assert_eq!(package_hint_for("pw-link", debian), "pipewire-bin");
        assert_eq!(package_hint_for("pw-link", mint), "pipewire-bin");
        assert_eq!(package_hint_for("pw-link", fedora), "pipewire-utils");
        assert_eq!(package_hint_for("pactl", arch), "libpulse");
        assert!(package_hint_for("pactl", "").contains("pulseaudio-utils"));
    }

    #[test]
    fn stderr_tail_keeps_only_the_last_lines() {
        let t = StderrTail::default();
        for i in 0..50 {
            t.push(format!("line {i}"));
        }
        assert_eq!(t.tail(2), "line 48 | line 49");
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

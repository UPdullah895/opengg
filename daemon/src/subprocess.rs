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
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tracing::{debug, trace};

/// How long a "tool is missing" answer is trusted. A positive answer is cached
/// for the life of the process (a tool does not usually vanish), but a negative
/// one must expire: the user who installs `pipewire-utils` or `headsetcontrol`
/// while the daemon runs should not have to restart it to be noticed.
const MISSING_RECHECK: Duration = Duration::from_secs(60);

/// Global cache for binary availability checks: (found, when it was checked).
static BIN_CACHE: OnceLock<Mutex<HashMap<String, (bool, Instant)>>> = OnceLock::new();

/// Initialize or get the binary cache.
fn get_cache() -> &'static Mutex<HashMap<String, (bool, Instant)>> {
    BIN_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Whether a cached entry may still be used.
fn cache_entry_fresh(found: bool, age: Duration) -> bool {
    found || age < MISSING_RECHECK
}

/// Check if a binary is available on $PATH.
///
/// Implements a `which`-style lookup: iterates $PATH entries and checks for an
/// executable file. Hits are cached for the process lifetime, misses for
/// `MISSING_RECHECK` so a later install is picked up.
pub fn is_available(bin: &str) -> bool {
    let cache = get_cache();
    let mut map = match cache.lock() {
        Ok(m) => m,
        Err(p) => p.into_inner(),
    };

    if let Some(&(found, at)) = map.get(bin) {
        if cache_entry_fresh(found, at.elapsed()) {
            return found;
        }
    }

    let found = find_in_path(bin).is_some();
    map.insert(bin.to_string(), (found, Instant::now()));
    found
}

/// Absolute path of `bin` on $PATH, uncached (diagnostics only).
pub fn which(bin: &str) -> Option<String> {
    find_in_path(bin)
}

/// Human install hint for a tool the daemon shells out to, so "not found" tells
/// the user what to do instead of just what failed.
pub fn install_hint(bin: &str) -> &'static str {
    match bin {
        "pactl" => "install pipewire-pulse (Arch/Fedora) or pulseaudio-utils (Debian/Ubuntu)",
        "pw-link" | "pw-cli" | "pw-dump" => {
            "install pipewire (Arch/Debian/Ubuntu) or pipewire-utils (Fedora/openSUSE)"
        }
        "ffmpeg" | "ffprobe" => "install ffmpeg",
        "gpu-screen-recorder" => "install gpu-screen-recorder",
        "headsetcontrol" => "install headsetcontrol (optional, headset support)",
        _ => "install it and make sure it is on PATH",
    }
}

/// Describe a failed spawn: names the binary and, for "not found", the package.
pub fn spawn_error(bin: &str, e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        format!("{bin} not found on PATH - {}", install_hint(bin))
    } else {
        format!("could not run {bin}: {e}")
    }
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

/// Construct a logged command for executing a binary.
///
/// Logs the binary invocation at debug level. The returned `Command` can be
/// further configured with args, stdin, stdout, etc., and then spawned by
/// the caller.
///
/// `LC_ALL=C` is forced because several callers parse the human-readable text
/// of pactl / pw-link (`Mute: yes`, `Name: `, "already"), and pactl localises
/// those strings; in a translated session the parse would silently find
/// nothing.
pub fn command(bin: &str) -> Command {
    debug!("subprocess: invoking {}", bin);
    let mut cmd = Command::new(bin);
    cmd.env("LC_ALL", "C");
    cmd
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
    fn test_nonexistent_binary() {
        assert!(!is_available("nonexistent_binary_xyz_123"));
    }

    #[test]
    fn missing_results_expire_but_hits_do_not() {
        assert!(cache_entry_fresh(true, Duration::from_secs(86_400)));
        assert!(cache_entry_fresh(false, Duration::from_secs(5)));
        assert!(!cache_entry_fresh(false, MISSING_RECHECK + Duration::from_secs(1)));
    }

    #[test]
    fn spawn_error_names_the_package_for_missing_tools() {
        let e = std::io::Error::from(std::io::ErrorKind::NotFound);
        let msg = spawn_error("pw-link", &e);
        assert!(msg.contains("pw-link not found") && msg.contains("pipewire"), "{msg}");
        let e = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert!(spawn_error("pactl", &e).contains("could not run pactl"));
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

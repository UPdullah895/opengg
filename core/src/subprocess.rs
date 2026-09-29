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
use log::debug;

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

/// Run a command synchronously and return its trimmed stdout.
///
/// Convenience runner used across the codebase: logs+spawns via `command()`,
/// runs `.output()`, and on non-zero exit returns the trimmed stderr as the
/// error string. (Moved from `frontend/src-tauri/src/commands.rs` so both the
/// Tauri host and the Qt shell share one blocking subprocess primitive.)
pub fn run_cmd_sync(c: &str, a: &[&str]) -> Result<String, String> {
    let o = command(c)
        .args(a)
        .output()
        .map_err(|e| format!("{c}:{e}"))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().into())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().into())
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

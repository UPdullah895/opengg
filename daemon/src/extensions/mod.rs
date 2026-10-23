//! Extensions — manifest-based discovery + supervised daemon subprocesses.
//!
//! Each extension is a folder in `~/.local/share/opengg/extensions/<id>/` with a
//! `manifest.json`. If the manifest declares a `daemon` field (a relative path to
//! an executable), that executable is run as a supervised background subprocess:
//! crashes restart with exponential backoff; a clean exit (code 0) is left alone.
//!
//! ## Consent is enforced here, not in the UI
//!
//! Extension daemons run unsandboxed as the user and inherit this process's
//! `input` group (keyboard access for global hotkeys). So nothing runs until
//! the user has enabled it:
//!
//! * Enable-state lives in `~/.config/opengg/extensions.json`
//!   (`{ "<id>": bool }`). **Absent means disabled** — merely copying a folder
//!   into the extensions directory no longer runs it at the next login.
//! * When an extension is enabled, the SHA-256 of its executable is recorded
//!   in `~/.config/opengg/extension-consents.json`. If the executable later
//!   changes (an update, or tampering), it is not started again until the
//!   user re-enables it. An extension enabled while this daemon was not
//!   running is pinned the first time it starts.
//! * Bare executables dropped directly into the extensions directory are no
//!   longer run (the pre-manifest layout had no way to ask for consent).

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::{Mutex, Notify};

/// An extension that declares a daemon (background) part.
#[derive(Clone)]
struct DaemonExt {
    id: String,
    name: String,
    exec: PathBuf,
}

/// A live supervised subprocess and the handle used to stop it.
struct Running {
    task: tokio::task::JoinHandle<()>,
    stop: Arc<Notify>,
}

/// Discovers extensions, supervises their daemon parts, and exposes runtime
/// enable/disable. Shared (as `Arc`) between `main` and the D-Bus interface.
pub struct ExtensionManager {
    /// Every extension with a daemon part, keyed by id — running or not.
    known: Mutex<HashMap<String, DaemonExt>>,
    /// Currently supervised tasks, keyed by id (or legacy filename).
    running: Mutex<HashMap<String, Running>>,
}

impl Default for ExtensionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtensionManager {
    pub fn new() -> Self {
        Self {
            known: Mutex::new(HashMap::new()),
            running: Mutex::new(HashMap::new()),
        }
    }

    /// Discover extensions and start every enabled daemon part. Call once at startup.
    pub async fn start_all(self: &Arc<Self>) {
        let dir = extensions_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!("Extensions dir unavailable ({e}): {}", dir.display());
            return;
        }

        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Cannot read extensions dir: {e}");
                return;
            }
        };

        let enabled = load_enabled_map();
        let mut found_any = false;

        for entry in entries.flatten() {
            let path = entry.path();

            if !path.is_dir() {
                if is_executable(&path) {
                    tracing::warn!(
                        "Extension: ignoring bare executable {} — extensions need a folder with a manifest.json",
                        path.display()
                    );
                }
                continue;
            }
            let Some(ext) = parse_daemon_ext(&path) else { continue };
            found_any = true;
            self.known.lock().await.insert(ext.id.clone(), ext.clone());

            if !is_enabled(&enabled, &ext.id) {
                tracing::info!("Extension {}: not enabled — not starting", ext.id);
                continue;
            }
            match check_consent(&ext) {
                Consent::Pinned => self.spawn(ext).await,
                Consent::Changed => tracing::warn!(
                    "Extension {}: its executable changed since it was enabled — not starting until it is re-enabled",
                    ext.id
                ),
            }
        }

        if !found_any {
            tracing::info!("Extensions dir has no daemon extensions");
        }
    }

    /// Spawn + supervise an extension's daemon part. No-op if already running.
    async fn spawn(self: &Arc<Self>, ext: DaemonExt) {
        let mut running = self.running.lock().await;
        if running.contains_key(&ext.id) {
            return;
        }
        tracing::info!("Extension {}: loading daemon part", ext.id);
        let stop = Arc::new(Notify::new());
        let task = tokio::spawn(supervise(ext.exec.clone(), ext.name.clone(), stop.clone()));
        running.insert(ext.id, Running { task, stop });
    }

    /// Stop a running daemon part gracefully (SIGTERM, then reap).
    async fn stop(&self, id: &str) {
        let handle = self.running.lock().await.remove(id);
        if let Some(r) = handle {
            r.stop.notify_one();
            let _ = tokio::time::timeout(std::time::Duration::from_secs(6), r.task).await;
        }
    }

    /// Persist enable/disable to the shared state file and apply it live.
    pub async fn set_enabled(self: &Arc<Self>, id: &str, enabled: bool) {
        let mut map = load_enabled_map();
        map.insert(id.to_string(), enabled);
        save_enabled_map(&map);

        let known = self.known.lock().await.get(id).cloned();
        if let Some(ext) = known {
            if enabled {
                // Enabling is the consent: pin what is being consented to.
                record_consent(&ext);
                self.spawn(ext).await;
                tracing::info!("Extension {id}: daemon part enabled");
            } else {
                self.stop(id).await;
                forget_consent(id);
                tracing::info!("Extension {id}: daemon part stopped");
            }
        }
    }

    /// JSON array of known daemon extensions and whether each is running.
    pub async fn list_json(&self) -> String {
        let known = self.known.lock().await;
        let running = self.running.lock().await;
        let arr: Vec<_> = known
            .values()
            .map(|e| {
                serde_json::json!({
                    "id": e.id,
                    "name": e.name,
                    "running": running.contains_key(&e.id),
                    // Enabled, but its executable no longer matches what was
                    // consented to — needs re-enabling.
                    "changed": check_consent(e) == Consent::Changed,
                })
            })
            .collect();
        serde_json::to_string(&arr).unwrap_or_else(|_| "[]".into())
    }
}

/// Keep an extension running; restart with exponential backoff on crash.
/// Cancellable: `stop.notified()` triggers a graceful SIGTERM + reap.
async fn supervise(path: PathBuf, name: String, stop: Arc<Notify>) {
    let mut backoff_secs = 2u64;
    loop {
        tracing::debug!("Extension {name}: starting");
        // Put the child in its own process group so we can signal the whole tree
        // — extensions often spawn helper processes (subshells, `pactl subscribe`,
        // etc.) that would otherwise be orphaned when only the leader is killed.
        match Command::new(&path).process_group(0).spawn() {
            Ok(mut child) => {
                tokio::select! {
                    res = child.wait() => match res {
                        Ok(s) if s.success() => {
                            tracing::info!("Extension {name}: exited cleanly — not restarting");
                            return;
                        }
                        Ok(s) => tracing::warn!("Extension {name}: exited {s}, restarting in {backoff_secs}s"),
                        Err(e) => tracing::error!("Extension {name}: wait error: {e}"),
                    },
                    _ = stop.notified() => {
                        tracing::info!("Extension {name}: stopping (SIGTERM)");
                        if let Some(pid) = child.id() {
                            // Negative pid → signal the entire process group.
                            // SAFETY: pid is a live child group leader owned by this task.
                            unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGTERM); }
                        }
                        if tokio::time::timeout(std::time::Duration::from_secs(3), child.wait())
                            .await
                            .is_err()
                        {
                            if let Some(pid) = child.id() {
                                unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGKILL); }
                            }
                        }
                        let _ = child.start_kill();
                        let _ = child.wait().await;
                        return;
                    }
                }
            }
            Err(e) => tracing::error!("Extension {name}: spawn failed: {e}"),
        }
        // Back off before restarting, but let a stop request cancel the wait.
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)) => {}
            _ = stop.notified() => return,
        }
        backoff_secs = (backoff_secs * 2).min(60);
    }
}

/// Parse a manifest folder into a `DaemonExt`, or `None` if it has no daemon part.
fn parse_daemon_ext(dir: &Path) -> Option<DaemonExt> {
    let raw = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;

    let id = v["id"].as_str()?.to_string();
    if id.is_empty() {
        return None;
    }
    let daemon_rel = v["daemon"].as_str().unwrap_or("");
    if daemon_rel.is_empty() {
        return None; // UI-only extension — nothing for the daemon to run.
    }

    // SECURITY: Validate daemon_rel to prevent path traversal and absolute path escapes.
    // Path::join() does NOT sanitize: absolute paths and ".." sequences can escape dir.

    let daemon_path = std::path::Path::new(daemon_rel);

    // Reject absolute paths (e.g., "/usr/bin/rm", "/bin/sh").
    if daemon_path.is_absolute() {
        tracing::warn!(
            extension_id = %id,
            daemon_path = %daemon_rel,
            "Extension daemon path is absolute (security risk); ignoring extension"
        );
        return None;
    }

    // Reject paths with parent directory ("..", RootDir, Prefix) components.
    for component in daemon_path.components() {
        use std::path::Component;
        match component {
            Component::ParentDir => {
                tracing::warn!(
                    extension_id = %id,
                    daemon_path = %daemon_rel,
                    "Extension daemon path contains '..'; ignoring extension"
                );
                return None;
            }
            Component::RootDir | Component::Prefix(_) => {
                tracing::warn!(
                    extension_id = %id,
                    daemon_path = %daemon_rel,
                    "Extension daemon path is absolute or has prefix; ignoring extension"
                );
                return None;
            }
            _ => {}
        }
    }

    // Build the exec path and validate containment via canonicalization.
    let exec = dir.join(daemon_rel);

    // Canonicalize both paths to resolve symlinks and ".." sequences.
    let canon_dir = match std::fs::canonicalize(dir) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                extension_id = %id,
                extension_dir = %dir.display(),
                error = %e,
                "Cannot canonicalize extension directory; ignoring extension"
            );
            return None;
        }
    };

    let canon_exec = match std::fs::canonicalize(&exec) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                extension_id = %id,
                exec_path = %exec.display(),
                error = %e,
                "Cannot canonicalize daemon executable; ignoring extension"
            );
            return None;
        }
    };

    // Enforce that the canonical exec is strictly inside the canonical extension dir.
    if !canon_exec.starts_with(&canon_dir) {
        tracing::warn!(
            extension_id = %id,
            daemon_path = %daemon_rel,
            "Extension daemon path escapes extension directory; ignoring extension"
        );
        return None;
    }

    // Final check: must be an executable file.
    if !is_executable(&exec) {
        tracing::warn!("Extension {id}: daemon '{}' is not an executable file", exec.display());
        return None;
    }

    let name = v["name"].as_str().unwrap_or(&id).to_string();
    Some(DaemonExt { id, name, exec })
}

fn is_executable(path: &Path) -> bool {
    path.is_file()
        && std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

fn extensions_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share"))
        .join("opengg/extensions")
}

// ── Shared enable-state file: ~/.config/opengg/extensions.json ──────────────

fn state_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"))
        .join("opengg/extensions.json")
}

fn load_enabled_map() -> HashMap<String, bool> {
    std::fs::read_to_string(state_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_enabled_map(map: &HashMap<String, bool>) {
    let path = state_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string_pretty(map) {
        if let Err(e) = std::fs::write(&path, s) {
            tracing::warn!("Failed to write extensions state: {e}");
        }
    }
}

/// An extension runs only when the state file explicitly says `true`.
fn is_enabled(map: &HashMap<String, bool>, id: &str) -> bool {
    map.get(id).copied().unwrap_or(false)
}

// ── Consent pinning: ~/.config/opengg/extension-consents.json ───────────────

#[derive(Debug, PartialEq, Eq)]
enum Consent {
    /// The executable is the one the user enabled (or was just pinned).
    Pinned,
    /// The executable changed after the user enabled it.
    Changed,
}

fn consents_path() -> PathBuf {
    state_path().with_file_name("extension-consents.json")
}

fn load_consents(path: &Path) -> HashMap<String, String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_consents(path: &Path, map: &HashMap<String, String>) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string_pretty(map) {
        if let Err(e) = std::fs::write(path, s) {
            tracing::warn!("Failed to write extension consents: {e}");
        }
    }
}

/// SHA-256 of a file as lowercase hex, `None` if it cannot be read.
fn file_sha256(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect())
}

/// Compare the executable against the pinned hash; pin it if none is
/// recorded yet (the user enabled it while the daemon was not running).
fn check_consent_at(path: &Path, ext: &DaemonExt) -> Consent {
    let Some(current) = file_sha256(&ext.exec) else { return Consent::Changed };
    let mut map = load_consents(path);
    match map.get(&ext.id) {
        Some(pinned) if *pinned == current => Consent::Pinned,
        Some(_) => Consent::Changed,
        None => {
            map.insert(ext.id.clone(), current);
            save_consents(path, &map);
            Consent::Pinned
        }
    }
}

fn check_consent(ext: &DaemonExt) -> Consent {
    check_consent_at(&consents_path(), ext)
}

fn record_consent_at(path: &Path, ext: &DaemonExt) {
    if let Some(h) = file_sha256(&ext.exec) {
        let mut map = load_consents(path);
        map.insert(ext.id.clone(), h);
        save_consents(path, &map);
    }
}

fn record_consent(ext: &DaemonExt) {
    record_consent_at(&consents_path(), ext)
}

fn forget_consent(id: &str) {
    let path = consents_path();
    let mut map = load_consents(&path);
    if map.remove(id).is_some() {
        save_consents(&path, &map);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("openggd-ext-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn absent_means_disabled() {
        let mut map = HashMap::new();
        assert!(!is_enabled(&map, "x"));
        map.insert("x".into(), true);
        assert!(is_enabled(&map, "x"));
        map.insert("x".into(), false);
        assert!(!is_enabled(&map, "x"));
    }

    #[test]
    fn consent_pins_the_executable() {
        let d = tmp("pin");
        let exec = d.join("run.sh");
        std::fs::write(&exec, b"#!/bin/sh\necho one\n").unwrap();
        let consents = d.join("consents.json");
        let ext = DaemonExt { id: "demo".into(), name: "Demo".into(), exec: exec.clone() };

        // Enabled while the daemon was down: pinned on first check.
        assert_eq!(check_consent_at(&consents, &ext), Consent::Pinned);
        assert_eq!(check_consent_at(&consents, &ext), Consent::Pinned);

        // The executable changes afterwards: refused until re-enabled.
        std::fs::write(&exec, b"#!/bin/sh\necho two\n").unwrap();
        assert_eq!(check_consent_at(&consents, &ext), Consent::Changed);
        record_consent_at(&consents, &ext);
        assert_eq!(check_consent_at(&consents, &ext), Consent::Pinned);

        // A missing executable is never "pinned".
        std::fs::remove_file(&exec).unwrap();
        assert_eq!(check_consent_at(&consents, &ext), Consent::Changed);
        std::fs::remove_dir_all(&d).ok();
    }
}

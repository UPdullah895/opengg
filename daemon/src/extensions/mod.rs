//! Extensions — manifest-based discovery + supervised daemon subprocesses.
//!
//! Each extension is a folder in `~/.local/share/opengg/extensions/<id>/` with a
//! `manifest.json`. If the manifest declares a `daemon` field (a relative path to
//! an executable), that executable is run as a supervised background subprocess:
//! crashes restart with exponential backoff; a clean exit (code 0) is left alone.
//!
//! Enable-state lives in `~/.config/opengg/extensions.json` (`{ "<id>": bool }`,
//! absent ⇒ enabled) and is shared with the Tauri/frontend layer. The manager can
//! start/stop a daemon part at runtime — via the `org.opengg.Daemon.Extensions`
//! D-Bus interface — without restarting the daemon.
//!
//! Bare executables placed directly in the extensions dir (no manifest) are still
//! supervised, for backward compatibility with the pre-manifest layout.

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
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

            let ext = if path.is_dir() {
                // Manifest-based extension with an optional daemon part.
                parse_daemon_ext(&path)
            } else if is_executable(&path) {
                // Legacy: a bare executable dropped directly into the dir.
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                Some(DaemonExt { id: name.clone(), name, exec: path })
            } else {
                None
            };

            let Some(ext) = ext else { continue };
            found_any = true;
            self.known.lock().await.insert(ext.id.clone(), ext.clone());

            if is_enabled(&enabled, &ext.id) {
                self.spawn(ext).await;
            } else {
                tracing::info!("Extension {}: daemon part disabled — not starting", ext.id);
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
                self.spawn(ext).await;
                tracing::info!("Extension {id}: daemon part enabled");
            } else {
                self.stop(id).await;
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

    let exec = dir.join(daemon_rel);
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

/// An extension is enabled unless the state file explicitly says `false`.
fn is_enabled(map: &HashMap<String, bool>, id: &str) -> bool {
    *map.get(id).unwrap_or(&true)
}

//! Process Watcher
//!
//! Periodically scans /proc to detect game launches and triggers profile
//! switching. Uses the `procfs` crate for safe /proc access.

use anyhow::Result;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Event emitted when a game process is detected or exits.
#[derive(Debug, Clone)]
pub enum ProcessEvent {
    /// A new game executable was detected.
    GameDetected {
        pid: i32,
        exe: String,
        name: String,
    },
    /// A previously detected game process exited.
    GameExited {
        exe: String,
    },
}

/// Watches /proc for game process starts and exits.
pub struct ProcessWatcher {
    /// Set of executable basenames we're tracking
    known_games: Arc<RwLock<HashSet<String>>>,
    /// Currently running game PIDs
    active_pids: Arc<RwLock<HashSet<i32>>>,
}

impl ProcessWatcher {
    pub fn new() -> Self {
        Self {
            known_games: Arc::new(RwLock::new(HashSet::new())),
            active_pids: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Register executable names to watch for.
    pub async fn watch_executables(&self, names: Vec<String>) {
        let mut games = self.known_games.write().await;
        for name in names {
            games.insert(name);
        }
    }

    /// Start the background scan loop. Returns a channel that receives events.
    pub fn start(&self) -> mpsc::UnboundedReceiver<ProcessEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let known = Arc::clone(&self.known_games);
        let active = Arc::clone(&self.active_pids);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));

            loop {
                interval.tick().await;

                let games = known.read().await;
                if games.is_empty() {
                    continue;
                }

                // Scan /proc for matching processes
                let current_pids = match scan_processes(&games) {
                    Ok(pids) => pids,
                    Err(e) => {
                        tracing::debug!("Process scan error: {e}");
                        continue;
                    }
                };

                let mut prev = active.write().await;

                // Detect new games
                for (pid, exe) in &current_pids {
                    if !prev.contains(pid) {
                        let name = exe
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        tracing::info!("Game detected: {name} (PID {pid})");
                        let _ = tx.send(ProcessEvent::GameDetected {
                            pid: *pid,
                            exe: name.clone(),
                            name,
                        });
                    }
                }

                // Detect exited games
                let current_pid_set: HashSet<i32> =
                    current_pids.iter().map(|(pid, _)| *pid).collect();
                let exited: Vec<i32> = prev.difference(&current_pid_set).cloned().collect();
                for pid in exited {
                    tracing::info!("Game exited: PID {pid}");
                    let _ = tx.send(ProcessEvent::GameExited {
                        exe: format!("pid:{pid}"),
                    });
                }

                *prev = current_pid_set;
            }
        });

        rx
    }
}

/// Scan /proc for processes whose exe basename matches any known game.
fn scan_processes(known_games: &HashSet<String>) -> Result<Vec<(i32, PathBuf)>> {
    let mut matches = Vec::new();

    // Use procfs crate for safe /proc access
    for proc_result in procfs::process::all_processes()? {
        let proc = match proc_result {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Read /proc/PID/exe symlink
        let exe = match proc.exe() {
            Ok(e) => e,
            Err(_) => continue,
        };

        let basename = exe
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();

        if known_games.iter().any(|g| basename.contains(&g.to_lowercase())) {
            matches.push((proc.pid, exe));
        }
    }

    Ok(matches)
}

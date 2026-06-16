//! gpu-screen-recorder Wrapper
//!
//! Manages the replay buffer and regular recording via gpu-screen-recorder.
//! Communicates with the process via signals (SIGUSR1 to save, SIGINT to stop).

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use tokio::process::Child;
use tokio::sync::Mutex;

/// Recording mode. The daemon recorder is always `Idle` now (recording is owned
/// by the OpenGG app); the other variants remain part of the D-Bus status surface.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum RecordMode {
    Idle,
    Replay { duration: u32 },
    Recording,
}

/// Daemon-side recorder shell.
///
/// NOTE: live screen/audio recording is owned entirely by the OpenGG app
/// (the Tauri host's `start_gsr_replay`), which captures each enabled channel
/// from its real `OpenGG_<ch>.monitor`. This daemon-side recorder is retained
/// only for its D-Bus surface; it no longer spawns its own gpu-screen-recorder.
/// The old implementation hardcoded `-a OpenGG_Game.monitor -a OpenGG_Chat.monitor
/// -a default_input`, which created a second, conflicting capture and double-wired
/// the raw default mic (`gsr-default_input`) into the graph.
pub struct Recorder {
    process: Arc<Mutex<Option<Child>>>,
    mode: Arc<Mutex<RecordMode>>,
}

impl Recorder {
    pub fn new(output_dir: &Path, _fps: u32, _quality: &str) -> Self {
        std::fs::create_dir_all(output_dir).ok();
        Self {
            process: Arc::new(Mutex::new(None)),
            mode: Arc::new(Mutex::new(RecordMode::Idle)),
        }
    }

    /// Get current recorder state.
    pub async fn status(&self) -> RecordMode {
        let mut mode = self.mode.lock().await;
        // Check if process is still alive
        let mut proc = self.process.lock().await;
        if let Some(ref mut child) = *proc {
            match child.try_wait() {
                Ok(Some(_status)) => {
                    *proc = None;
                    *mode = RecordMode::Idle;
                }
                Ok(None) => {} // still running
                Err(_) => {
                    *proc = None;
                    *mode = RecordMode::Idle;
                }
            }
        }
        *mode
    }

    /// No-op: replay/recording is owned by the OpenGG app (`start_gsr_replay`),
    /// which captures each enabled channel from its real `OpenGG_<ch>.monitor`.
    /// The daemon must not spawn a second gpu-screen-recorder.
    pub async fn start_replay(&self, _duration: u32) -> Result<()> {
        tracing::info!(
            "Daemon start_replay ignored — recording is handled by the OpenGG app"
        );
        Ok(())
    }

    /// No-op for the same reason as `start_replay`.
    pub async fn start_recording(&self) -> Result<()> {
        tracing::info!(
            "Daemon start_recording ignored — recording is handled by the OpenGG app"
        );
        Ok(())
    }

    /// Stop the recorder.
    pub async fn stop(&self) -> Result<()> {
        let mut proc = self.process.lock().await;
        if let Some(ref mut child) = *proc {
            // Send SIGINT for clean shutdown
            let pid = child.id().unwrap_or(0);
            if pid > 0 {
                unsafe {
                    libc::kill(pid as i32, libc::SIGINT);
                }
            }
            // Wait with timeout
            tokio::select! {
                _ = child.wait() => {}
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                    child.kill().await.ok();
                }
            }
        }
        *proc = None;
        *self.mode.lock().await = RecordMode::Idle;
        tracing::info!("Recorder stopped");
        Ok(())
    }

    /// Save the current replay buffer (sends SIGUSR1 to gpu-screen-recorder).
    pub async fn save_replay(&self) -> Result<()> {
        let proc = self.process.lock().await;
        if let Some(ref child) = *proc {
            let pid = child.id().unwrap_or(0);
            if pid > 0 {
                unsafe {
                    libc::kill(pid as i32, libc::SIGUSR1);
                }
                tracing::info!("Replay save triggered (SIGUSR1 → PID {pid})");
                return Ok(());
            }
        }
        anyhow::bail!("Replay buffer not running")
    }
}

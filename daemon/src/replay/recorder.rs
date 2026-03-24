//! gpu-screen-recorder Wrapper
//!
//! Manages the replay buffer and regular recording via gpu-screen-recorder.
//! Communicates with the process via signals (SIGUSR1 to save, SIGINT to stop).

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// Recording mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RecordMode {
    Idle,
    Replay { duration: u32 },
    Recording,
}

/// Manages a gpu-screen-recorder subprocess.
pub struct Recorder {
    process: Arc<Mutex<Option<Child>>>,
    mode: Arc<Mutex<RecordMode>>,
    output_dir: PathBuf,
    fps: u32,
    quality: String,
}

impl Recorder {
    pub fn new(output_dir: &Path, fps: u32, quality: &str) -> Self {
        std::fs::create_dir_all(output_dir).ok();
        Self {
            process: Arc::new(Mutex::new(None)),
            mode: Arc::new(Mutex::new(RecordMode::Idle)),
            output_dir: output_dir.to_owned(),
            fps,
            quality: quality.into(),
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

    /// Start the replay buffer.
    pub async fn start_replay(&self, duration: u32) -> Result<()> {
        if self.status().await != RecordMode::Idle {
            anyhow::bail!("Recorder already running");
        }

        let quality_arg = match self.quality.as_str() {
            "Low" => "medium",
            "Medium" => "high",
            "High" => "very_high",
            "Ultra" => "ultra",
            _ => "very_high",
        };

        let child = Command::new("gpu-screen-recorder")
            .args([
                "-w", "screen",
                "-f", &self.fps.to_string(),
                "-q", quality_arg,
                "-r", &duration.to_string(),
                "-c", "mp4",
                "-a", "OpenGG_Game.monitor",
                "-a", "OpenGG_Chat.monitor",
                "-a", "default_input",
                "-o", self.output_dir.to_str().unwrap_or("/tmp"),
            ])
            .kill_on_drop(true)
            .spawn()
            .context("gpu-screen-recorder not found")?;

        *self.process.lock().await = Some(child);
        *self.mode.lock().await = RecordMode::Replay { duration };

        tracing::info!("Replay buffer started: {duration}s, {fps}fps", fps = self.fps);
        Ok(())
    }

    /// Start regular recording (no replay buffer).
    pub async fn start_recording(&self) -> Result<()> {
        if self.status().await != RecordMode::Idle {
            anyhow::bail!("Recorder already running");
        }

        let quality_arg = match self.quality.as_str() {
            "Low" => "medium",
            "Medium" => "high",
            "High" => "very_high",
            "Ultra" => "ultra",
            _ => "very_high",
        };

        let child = Command::new("gpu-screen-recorder")
            .args([
                "-w", "screen",
                "-f", &self.fps.to_string(),
                "-q", quality_arg,
                "-c", "mp4",
                "-a", "OpenGG_Game.monitor",
                "-a", "OpenGG_Chat.monitor",
                "-a", "default_input",
                "-o", self.output_dir.to_str().unwrap_or("/tmp"),
            ])
            .kill_on_drop(true)
            .spawn()
            .context("gpu-screen-recorder not found")?;

        *self.process.lock().await = Some(child);
        *self.mode.lock().await = RecordMode::Recording;

        tracing::info!("Recording started");
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

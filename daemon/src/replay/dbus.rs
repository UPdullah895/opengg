//! D-Bus interface: org.opengg.Daemon.Replay

use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::interface;

use super::recorder::{RecordMode, Recorder};

pub struct ReplayInterface {
    recorder: Arc<Mutex<Recorder>>,
}

impl ReplayInterface {
    pub fn new(recorder: Recorder) -> Self {
        Self {
            recorder: Arc::new(Mutex::new(recorder)),
        }
    }
}

#[interface(name = "org.opengg.Daemon.Replay")]
impl ReplayInterface {
    /// Get recorder status: "idle", "replay", or "recording".
    async fn get_status(&self) -> String {
        let rec = self.recorder.lock().await;
        match rec.status().await {
            RecordMode::Idle => "idle".into(),
            RecordMode::Replay { duration } => format!("replay:{duration}"),
            RecordMode::Recording => "recording".into(),
        }
    }

    /// Start the replay buffer with a given duration in seconds.
    async fn start_replay(&self, duration_secs: u32) -> zbus::fdo::Result<()> {
        let rec = self.recorder.lock().await;
        rec.start_replay(duration_secs)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Start regular recording.
    async fn start_recording(&self) -> zbus::fdo::Result<()> {
        let rec = self.recorder.lock().await;
        rec.start_recording()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Stop recording or replay.
    async fn stop(&self) -> zbus::fdo::Result<()> {
        let rec = self.recorder.lock().await;
        rec.stop()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Save the current replay buffer.
    async fn save_replay(&self) -> zbus::fdo::Result<()> {
        let rec = self.recorder.lock().await;
        rec.save_replay()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Get list of clips as JSON.
    async fn get_clips(&self, folder: &str) -> String {
        let path = std::path::Path::new(folder);
        match super::clips::scan_clips(path).await {
            Ok(clips) => serde_json::to_string(&clips).unwrap_or_else(|_| "[]".into()),
            Err(_) => "[]".into(),
        }
    }

    /// Trim a clip and export it.
    async fn trim_clip(
        &self,
        input_path: &str,
        output_path: &str,
        start_sec: f64,
        end_sec: f64,
    ) -> zbus::fdo::Result<()> {
        super::clips::trim_clip(
            std::path::Path::new(input_path),
            std::path::Path::new(output_path),
            start_sec,
            end_sec,
        )
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
}

//! D-Bus interface: org.opengg.Daemon.Replay

use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;
use zbus::interface;

use super::hotkey::{Bindings, HotkeyHandle};
use super::recorder::{RecordMode, Recorder};

/// Object path this interface is served at. Shared with `hotkey::emit`, which
/// needs it to build a signal context.
pub const REPLAY_PATH: &str = "/org/opengg/Daemon/Replay";

pub struct ReplayInterface {
    recorder: Arc<Mutex<Recorder>>,
    /// Set once the hotkey listener is up; `None` when this machine has no
    /// readable keyboards, so `SetHotkeys` can say so instead of accepting
    /// bindings that will never fire.
    hotkeys: Arc<StdMutex<Option<HotkeyHandle>>>,
}

impl ReplayInterface {
    pub fn new(recorder: Recorder) -> Self {
        Self {
            recorder: Arc::new(Mutex::new(recorder)),
            hotkeys: Arc::new(StdMutex::new(None)),
        }
    }

    /// A handle to fill in once the listener starts. The interface is built
    /// before the D-Bus connection exists and the listener needs that
    /// connection to emit on, so the two cannot be wired in one step.
    pub fn hotkey_slot(&self) -> Arc<StdMutex<Option<HotkeyHandle>>> {
        self.hotkeys.clone()
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

    /// Replace the global hotkey bindings, in the same `"Alt+F10"` spelling
    /// the Shortcuts panel stores.
    ///
    /// The app pushes these on startup and whenever the user edits them, so
    /// the daemon honours what the UI shows rather than a stale copy in
    /// `daemon.toml`. Returns an error when no listener is running — the
    /// caller should surface that rather than assume the keys now work.
    async fn set_hotkeys(
        &self,
        save_replay: &str,
        toggle_recording: &str,
        screenshot: &str,
    ) -> zbus::fdo::Result<()> {
        let handle = self.hotkeys.lock().unwrap().clone();
        match handle {
            Some(h) => {
                h.replace(Bindings::parse(save_replay, toggle_recording, screenshot));
                Ok(())
            }
            None => Err(zbus::fdo::Error::NotSupported(
                "global hotkeys are not running (no readable keyboard in /dev/input — \
                 is this user in the 'input' group?)"
                    .into(),
            )),
        }
    }

    /// Whether a hotkey listener is actually running, so the UI can flag the
    /// shortcuts as inert instead of letting the user edit dead bindings.
    #[zbus(property)]
    async fn hotkeys_active(&self) -> bool {
        self.hotkeys.lock().unwrap().is_some()
    }

    /// Emitted when a bound combination is pressed. The payload is the
    /// `ui-settings.json` shortcut key — "saveReplay", "toggleRecording" or
    /// "screenshot".
    ///
    /// The daemon signals rather than acts because the recording process
    /// belongs to the app, not to it; see `replay::recorder`.
    #[zbus(signal)]
    pub async fn hotkey_pressed(
        ctxt: &zbus::object_server::SignalContext<'_>,
        action: &str,
    ) -> zbus::Result<()>;

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

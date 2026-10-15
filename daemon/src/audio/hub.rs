//! AudioHub — top-level facade for the Audio module.
//!
//! Construction is instant and touches nothing: sinks are created by a
//! background reconciler (`AudioHub::start`) that waits for pipewire-pulse with
//! a capped backoff, creates whatever OpenGG_* sink is missing, and keeps
//! checking afterwards. That way a daemon that starts before pipewire-pulse is
//! serving (fresh install, fast login, pipewire-pulse restart) still ends up
//! with a working mixer, and D-Bus name acquisition never waits on audio.

use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use super::routing;
use super::sinks::{ChannelInfo, Health, SinkManager, Tick};
use crate::config::AudioConfig;

/// How often a healthy graph is re-checked for vanished sinks (one cheap
/// `pactl list sinks short`).
const RECONCILE_INTERVAL: Duration = Duration::from_secs(15);

/// Waiting for pipewire-pulse: first retry after 0.5 s, doubling to 5 s, kept
/// up indefinitely (it may legitimately appear minutes later).
const WAIT_MIN: Duration = Duration::from_millis(500);
const WAIT_MAX: Duration = Duration::from_secs(5);

/// Capped exponential backoff.
#[derive(Debug, Clone)]
pub struct Backoff {
    min: Duration,
    max: Duration,
    next: Duration,
}

impl Backoff {
    pub fn new(min: Duration, max: Duration) -> Self {
        Self { min, max, next: min }
    }

    /// The delay to sleep now; the following one is doubled up to `max`.
    pub fn next_delay(&mut self) -> Duration {
        let d = self.next;
        self.next = (self.next * 2).min(self.max);
        d
    }

    pub fn reset(&mut self) {
        self.next = self.min;
    }
}

#[derive(Clone)]
pub struct AudioHub {
    sink_mgr: Arc<SinkManager>,
    streams: Arc<RwLock<Vec<routing::StreamInfo>>>,
}

/// Turn a `spawn_blocking` join failure (the closure panicked) into an error the
/// caller can report, instead of letting `.expect` take the whole daemon down.
fn join_err(what: &str, e: tokio::task::JoinError) -> anyhow::Error {
    tracing::error!("audio task '{what}' panicked: {e}");
    anyhow::anyhow!("audio task '{what}' failed internally: {e}")
}

impl AudioHub {
    pub async fn new(config: &AudioConfig) -> Result<Self> {
        // NOTE: We deliberately do NOT re-apply `config.default_volumes` here. Doing so on
        // every daemon (re)start fired a `pactl set-sink-volume` per channel → an OS volume
        // notification on each start and clobbered whatever the user had set. Brand-new
        // null-sinks already default to 100%; the user's saved per-channel levels are
        // restored once at app startup by the Tauri host's `hydrate_audio_routing`, which
        // reads them from ui-settings.json (the UI's source of truth).
        let _ = &config.default_volumes;

        Ok(Self {
            sink_mgr: Arc::new(SinkManager::new()),
            streams: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// Start the background tasks (sink reconciler, stream watcher). Called
    /// after the D-Bus name is owned so a second instance that loses the name
    /// race never touches PipeWire.
    pub fn start(&self) {
        self.start_reconciler();
        self.start_stream_watcher();
    }

    /// Shared handle on the health snapshot for `GetDiagnostics`. A handle to
    /// the snapshot only, NOT to the `SinkManager`: keeping the manager alive in
    /// a global would stop its `Drop` cleanup from ever running at shutdown.
    pub fn health_handle(&self) -> Arc<std::sync::Mutex<Health>> {
        self.sink_mgr.health_handle()
    }

    pub async fn get_channels(&self) -> Vec<ChannelInfo> {
        // Shells out to pactl twice: keep it off the async workers.
        let mgr = Arc::clone(&self.sink_mgr);
        let mut channels = tokio::task::spawn_blocking(move || mgr.get_channels())
            .await
            .unwrap_or_else(|e| {
                tracing::error!("audio task 'get_channels' panicked: {e}");
                Vec::new()
            });
        let streams = self.streams.read().await;
        for ch in &mut channels {
            ch.assigned_apps = routing::apps_for_channel(&streams, &ch.name);
        }
        channels
    }

    pub async fn get_streams(&self) -> Vec<routing::StreamInfo> {
        self.streams.read().await.clone()
    }

    pub async fn set_volume(&self, channel: &str, volume: u32) -> Result<()> {
        let mgr = Arc::clone(&self.sink_mgr);
        let channel = channel.to_string();
        tokio::task::spawn_blocking(move || mgr.set_volume(&channel, volume as f32 / 100.0))
            .await
            .map_err(|e| join_err("set_volume", e))?
    }

    pub async fn set_mute(&self, channel: &str, muted: bool) -> Result<()> {
        let mgr = Arc::clone(&self.sink_mgr);
        let channel = channel.to_string();
        tokio::task::spawn_blocking(move || mgr.set_mute(&channel, muted))
            .await
            .map_err(|e| join_err("set_mute", e))?
    }

    pub async fn route_app(&self, stream_id: u32, channel: &str) -> Result<()> {
        let channel = channel.to_string();
        tokio::task::spawn_blocking(move || routing::route_stream(stream_id, &channel))
            .await
            .map_err(|e| join_err("route_app", e))?
    }

    pub async fn remove_virtual_audio(&self) -> Result<()> {
        let sink_mgr = Arc::clone(&self.sink_mgr);
        tokio::task::spawn_blocking(move || sink_mgr.teardown_all())
            .await
            .map_err(|e| join_err("teardown", e))?
    }

    pub async fn create_virtual_audio(&self) -> Result<()> {
        let sink_mgr = Arc::clone(&self.sink_mgr);
        tokio::task::spawn_blocking(move || sink_mgr.ensure_created())
            .await
            .map_err(|e| join_err("sink creation", e))?
    }

    /// Background reconciler: wait for pipewire-pulse, create what is missing,
    /// keep watching. Logs transitions only (waiting started / reachable again),
    /// never every poll. Never restarts PipeWire.
    fn start_reconciler(&self) {
        let mgr = Arc::clone(&self.sink_mgr);
        tokio::spawn(async move {
            let mut backoff = Backoff::new(WAIT_MIN, WAIT_MAX);
            let mut waiting_since: Option<Instant> = None;
            let mut first = true;
            loop {
                let m = Arc::clone(&mgr);
                let force = first;
                let sleep_for = match tokio::task::spawn_blocking(move || m.tick(force)).await {
                    Ok(Tick::PulseDown(reason)) => {
                        if waiting_since.is_none() {
                            tracing::warn!(
                                "Audio backend not reachable ({reason}); waiting for pipewire-pulse \
                                 and retrying every {}s — virtual sinks are created as soon as it answers \
                                 (is pipewire-pulse installed and running? check `pactl info`, \
                                 `systemctl --user status pipewire-pulse`)",
                                WAIT_MAX.as_secs()
                            );
                            waiting_since = Some(Instant::now());
                        }
                        backoff.next_delay()
                    }
                    Ok(Tick::Ok) => {
                        if let Some(since) = waiting_since.take() {
                            tracing::info!(
                                "Audio backend reachable after {:.1}s",
                                since.elapsed().as_secs_f32()
                            );
                        }
                        backoff.reset();
                        first = false;
                        RECONCILE_INTERVAL
                    }
                    Err(e) => {
                        // A panic inside one pass must not end reconciling.
                        tracing::error!("audio reconcile pass panicked: {e}");
                        RECONCILE_INTERVAL
                    }
                };
                tokio::time::sleep(sleep_for).await;
            }
        });
    }

    fn start_stream_watcher(&self) {
        let streams = Arc::clone(&self.streams);
        tokio::spawn(async move {
            loop {
                match tokio::task::spawn_blocking(routing::list_streams).await {
                    Ok(Ok(new)) => {
                        if crate::statelog::changed("audio.stream-scan", "ok") {
                            tracing::info!("Stream scan working");
                        }
                        *streams.write().await = new;
                    }
                    Ok(Err(e)) => {
                        // First failure at ERROR, identical repeats silent (this
                        // loop runs every 3 s); a different error logs again.
                        if crate::statelog::changed("audio.stream-scan", &format!("{e:#}")) {
                            tracing::error!("Stream scan failing (app list will be empty): {e:#}");
                        }
                        streams.write().await.clear();
                    }
                    Err(e) => { tracing::error!("Stream watcher panic: {e}"); }
                }
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_to_cap_and_resets() {
        let mut b = Backoff::new(Duration::from_millis(500), Duration::from_secs(5));
        let seq: Vec<u128> = (0..6).map(|_| b.next_delay().as_millis()).collect();
        assert_eq!(seq, vec![500, 1000, 2000, 4000, 5000, 5000]);
        b.reset();
        assert_eq!(b.next_delay(), Duration::from_millis(500));
    }
}

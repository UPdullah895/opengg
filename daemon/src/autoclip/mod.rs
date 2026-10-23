//! Auto-clip: save a replay when something worth keeping happens in game.
//!
//! The first event source is Counter-Strike 2's Game State Integration: the
//! game POSTs its state to `127.0.0.1:GSI_PORT` (the cfg that tells it to is
//! written by `core::autoclip`). [`cs2::Tracker`] turns those snapshots into
//! kills, multi-kills, MVPs and deaths; [`coalesce::Coalescer`] folds a
//! streak into one moment; and the moment goes out as the Replay interface's
//! `MomentDetected` signal.
//!
//! Like hotkeys, the daemon signals rather than saves: the replay buffer is
//! the app's `gpu-screen-recorder` child (see `replay::recorder`). The app
//! saves through its normal path and names the clip after the moment.
//!
//! Which events count, and whether auto-clip is on at all, is
//! `settings.autoClip` in `ui-settings.json`, re-read when it changes. The
//! listener runs either way so the UI can show "connected" before the user
//! turns it on, and so turning it on mid-round compares against a baseline.

pub mod coalesce;
pub mod cs2;
pub mod http;

use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// The port the cfg points CS2 at. `core::autoclip` writes the same number.
pub const GSI_PORT: u16 = 37820;

/// What happened, best last: the order is what the coalescer ranks by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Death,
    Kill,
    Headshot,
    Mvp,
    Triple,
    Quad,
    Ace,
}

impl Kind {
    /// The name the user picks in Settings (`settings.autoClip.events`).
    /// Triple and quad kills share one switch.
    pub fn pref_key(self) -> &'static str {
        match self {
            Self::Death => "death",
            Self::Kill => "kill",
            Self::Headshot => "headshot",
            Self::Mvp => "mvp",
            Self::Triple | Self::Quad => "multikill",
            Self::Ace => "ace",
        }
    }
}

/// One highlight, as carried by `MomentDetected` (JSON).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Moment {
    pub game: String,
    pub kind: Kind,
    /// Raw map name, e.g. `de_mirage`.
    pub map: String,
    pub round: i64,
    /// Kills this round when it happened.
    pub kills: u64,
    /// Unix seconds.
    pub at: u64,
}

/// `settings.autoClip`.
#[derive(Debug, Clone, PartialEq)]
pub struct Prefs {
    pub enabled: bool,
    pub events: Vec<String>,
}

pub const DEFAULT_EVENTS: [&str; 3] = ["multikill", "ace", "mvp"];

impl Prefs {
    pub fn from_settings(v: &serde_json::Value) -> Self {
        let a = &v["settings"]["autoClip"];
        Self {
            enabled: a["enabled"].as_bool().unwrap_or(false),
            events: match a["events"].as_array() {
                Some(list) => list.iter().filter_map(|e| e.as_str().map(String::from)).collect(),
                None => DEFAULT_EVENTS.iter().map(|s| s.to_string()).collect(),
            },
        }
    }

    pub fn wants(&self, kind: Kind) -> bool {
        self.enabled && self.events.iter().any(|e| e == kind.pref_key())
    }
}

/// What `GetAutoClipStatus` reports, as JSON.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub listening: bool,
    pub port: u16,
    /// Why the listener is not running, or why the last payload was refused.
    pub error: String,
    /// Unix seconds of the last payload that passed the token check.
    pub last_payload_at: Option<u64>,
    pub moments: u32,
    pub last_moment: Option<Moment>,
}

pub type SharedStatus = Arc<Mutex<Status>>;

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `~/.config/opengg/cs2-gsi.token`, written by `core::autoclip` alongside
/// the cfg. Payloads must carry it as `auth.token`: anything on the machine
/// can reach a loopback port, and a forged "ace" would save a clip.
fn token_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("opengg/cs2-gsi.token"))
}

fn load_token() -> Option<String> {
    let t = std::fs::read_to_string(token_path()?).ok()?;
    let t = t.trim().to_string();
    (!t.is_empty()).then_some(t)
}

/// Check a payload's token against the one on disk.
pub fn authorized(payload: &serde_json::Value, expected: Option<&str>) -> bool {
    match (expected, payload["auth"]["token"].as_str()) {
        (Some(want), Some(got)) => {
            // Length is not secret; compare the rest without an early exit.
            want.len() == got.len()
                && want.bytes().zip(got.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
        }
        _ => false,
    }
}

/// Start the GSI listener and the engine that turns payloads into signals.
pub fn spawn(conn: &zbus::Connection, status: SharedStatus) {
    let (tx, rx) = mpsc::channel::<serde_json::Value>(64);
    tokio::spawn(engine(conn.clone(), rx, status.clone()));
    tokio::spawn(listen(tx, status));
}

async fn listen(tx: mpsc::Sender<serde_json::Value>, status: SharedStatus) {
    let listener = match TcpListener::bind(("127.0.0.1", GSI_PORT)).await {
        Ok(l) => l,
        Err(e) => {
            warn!("auto-clip: cannot listen on 127.0.0.1:{GSI_PORT}: {e}");
            let mut s = status.lock().unwrap();
            s.port = GSI_PORT;
            s.error = format!("port {GSI_PORT} unavailable: {e}");
            return;
        }
    };
    {
        let mut s = status.lock().unwrap();
        s.listening = true;
        s.port = GSI_PORT;
    }
    info!("auto-clip: CS2 Game State Integration on 127.0.0.1:{GSI_PORT}");
    loop {
        let Ok((mut stream, _)) = listener.accept().await else { continue };
        let tx = tx.clone();
        let status = status.clone();
        tokio::spawn(async move {
            let body = match http::read_post(&mut stream).await {
                Ok(Some(b)) => b,
                _ => return,
            };
            let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&body) else {
                http::respond(&mut stream, "400 Bad Request").await;
                return;
            };
            if !authorized(&payload, load_token().as_deref()) {
                status.lock().unwrap().error = "a payload arrived with the wrong token — \
                    reinstall the CS2 integration from Settings"
                    .into();
                http::respond(&mut stream, "403 Forbidden").await;
                return;
            }
            {
                let mut s = status.lock().unwrap();
                s.last_payload_at = Some(unix_now());
                s.error.clear();
            }
            http::respond(&mut stream, "200 OK").await;
            let _ = tx.send(payload).await;
        });
    }
}

/// Re-read prefs only when ui-settings.json changes.
struct PrefsCache {
    mtime: Option<SystemTime>,
    prefs: Prefs,
}

impl PrefsCache {
    fn get(&mut self) -> &Prefs {
        let path = dirs::config_dir().map(|d| d.join("opengg/ui-settings.json"));
        let mtime = path.and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok());
        if mtime != self.mtime {
            self.mtime = mtime;
            self.prefs = Prefs::from_settings(&crate::config::load_ui_settings());
        }
        &self.prefs
    }
}

async fn engine(conn: zbus::Connection, mut rx: mpsc::Receiver<serde_json::Value>, status: SharedStatus) {
    let epoch = Instant::now();
    let ms = || epoch.elapsed().as_millis() as u64;
    let mut tracker = cs2::Tracker::default();
    let mut window = coalesce::Coalescer::default();
    let mut prefs = PrefsCache { mtime: None, prefs: Prefs::from_settings(&serde_json::json!({})) };

    loop {
        let wait = window.deadline().map(|d| d.saturating_sub(ms()));
        tokio::select! {
            msg = rx.recv() => {
                let Some(payload) = msg else { return };
                let prefs = prefs.get();
                for m in tracker.update(&payload, unix_now()) {
                    if prefs.wants(m.kind) {
                        window.push(m, ms());
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(wait.unwrap_or(0))), if wait.is_some() => {}
        }
        if let Some(m) = window.poll(ms()) {
            info!("auto-clip: {:?} on {} (round {})", m.kind, m.map, m.round);
            {
                let mut s = status.lock().unwrap();
                s.moments += 1;
                s.last_moment = Some(m.clone());
            }
            if let Err(e) = emit(&conn, &m).await {
                warn!("auto-clip: MomentDetected not sent: {e}");
            }
        }
    }
}

async fn emit(conn: &zbus::Connection, m: &Moment) -> anyhow::Result<()> {
    let ctxt = zbus::object_server::SignalEmitter::new(conn, crate::replay::REPLAY_PATH)?;
    let json = serde_json::to_string(m)?;
    crate::replay::ReplayInterface::moment_detected(&ctxt, &json).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn prefs_default_to_off_with_the_big_moments_picked() {
        let p = Prefs::from_settings(&json!({}));
        assert!(!p.enabled);
        assert!(!p.wants(Kind::Ace));
        let p = Prefs::from_settings(&json!({ "settings": { "autoClip": { "enabled": true } } }));
        assert!(p.wants(Kind::Ace) && p.wants(Kind::Triple) && p.wants(Kind::Quad) && p.wants(Kind::Mvp));
        assert!(!p.wants(Kind::Kill) && !p.wants(Kind::Death));
        let p = Prefs::from_settings(&json!({ "settings": { "autoClip": { "enabled": true, "events": ["death"] } } }));
        assert!(p.wants(Kind::Death) && !p.wants(Kind::Ace));
    }

    #[test]
    fn the_token_must_match_exactly() {
        let with = |t: &str| json!({ "auth": { "token": t } });
        assert!(authorized(&with("abc123"), Some("abc123")));
        assert!(!authorized(&with("abc124"), Some("abc123")));
        assert!(!authorized(&with("abc12"), Some("abc123")));
        assert!(!authorized(&json!({}), Some("abc123")));
        assert!(!authorized(&with("abc123"), None), "no token on disk means not installed");
    }

    #[test]
    fn ranks_put_the_ace_on_top() {
        assert!(Kind::Ace > Kind::Quad && Kind::Quad > Kind::Triple && Kind::Triple > Kind::Mvp);
        assert!(Kind::Mvp > Kind::Headshot && Kind::Headshot > Kind::Kill && Kind::Kill > Kind::Death);
    }

    #[test]
    fn the_signal_payload_is_stable() {
        let m = Moment { game: "cs2".into(), kind: Kind::Ace, map: "de_mirage".into(), round: 7, kills: 5, at: 1 };
        assert_eq!(
            serde_json::to_value(&m).unwrap(),
            json!({ "game": "cs2", "kind": "ace", "map": "de_mirage", "round": 7, "kills": 5, "at": 1 })
        );
    }
}

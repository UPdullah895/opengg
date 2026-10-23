//! EqController — per-channel 10-band EQ, presentation glue.
//!
//! The engine runs in the daemon (`daemon/src/audio/eq.rs`), so the EQ
//! keeps working with the window closed, survives restarts, and is routed
//! by the same reconciler as everything else. This controller sends the
//! user's state there (`opengg_core::eq::apply`), remembers it in
//! `ui-settings.json`, and reports what the daemon says back.
//!
//! Turning the EQ on or off waits for the daemon's answer, so the toggle
//! reflects what really happened. Band and preamp changes arrive at slider
//! rate; they are coalesced per channel on a worker thread.
//!
//! It also drives the microphone processing chain (noise suppression,
//! gate, compressor), which runs in the daemon too
//! (`daemon/src/audio/micfx.rs`); `micFxCapsJson` says which stages this
//! system has plugins for.

#[cxx_qt::bridge]
pub mod qobject {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, last_error, cxx_name = "lastError")]
        /// Empty when this system can host the EQ, else why it cannot.
        #[qproperty(QString, unavailable_reason, cxx_name = "unavailableReason")]
        /// JSON array of channels whose EQ the daemon reports running.
        #[qproperty(QString, active_channels_json, cxx_name = "activeChannelsJson")]
        /// `{"nr", "gate", "comp"}` booleans: which mic processing stages
        /// this system can run.
        #[qproperty(QString, mic_fx_caps_json, cxx_name = "micFxCapsJson")]
        /// Smart Volume level per output channel (`{"Media": "medium", …}`;
        /// "off" for the rest): the daemon's, else the saved one.
        #[qproperty(QString, smart_json, cxx_name = "smartJson")]
        /// Empty when this system can run Smart Volume (swh-plugins
        /// present), else why it cannot.
        #[qproperty(QString, smart_unavailable_reason, cxx_name = "smartUnavailableReason")]
        type EqController = super::EqControllerRust;

        /// Set a channel's Smart Volume ("off", "light", "medium",
        /// "strong"). Waits for the daemon so the control shows what really
        /// happened; returns false (and sets `lastError`) on failure.
        /// Always remembers the choice.
        #[qinvokable]
        #[cxx_name = "setSmartVolume"]
        fn set_smart_volume(self: Pin<&mut Self>, channel: &QString, level: &QString) -> bool;

        /// Set a channel's EQ. Returns false (and sets `lastError`) if
        /// turning it on failed. Always remembers the state.
        #[qinvokable]
        #[cxx_name = "setEq"]
        fn set_eq(self: Pin<&mut Self>, channel: &QString, enabled: bool, bands_json: &QString, preamp: f64) -> bool;

        /// The saved state for `channel` as JSON `{enabled, bands, preamp}`.
        #[qinvokable]
        #[cxx_name = "savedState"]
        fn saved_state(self: &Self, channel: &QString) -> QString;

        /// Re-read availability and running chains from the daemon.
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>);

        /// Set microphone processing (JSON `{nr, gate, comp}`); saved, and
        /// sent to the daemon off the UI thread.
        #[qinvokable]
        #[cxx_name = "setMicFx"]
        fn set_mic_fx(self: Pin<&mut Self>, json: &QString);

        /// The saved microphone processing settings as JSON.
        #[qinvokable]
        #[cxx_name = "savedMicFx"]
        fn saved_mic_fx(self: &Self) -> QString;
    }

    impl cxx_qt::Threading for EqController {}
}

use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use opengg_core::eq::EqState;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// Latest state per channel waiting to go to the daemon, and the channels
/// with a sender already running.
type Outbox = Arc<Mutex<(HashMap<String, EqState>, HashSet<String>)>>;

pub struct EqControllerRust {
    last_error: QString,
    unavailable_reason: QString,
    active_channels_json: QString,
    mic_fx_caps_json: QString,
    smart_json: QString,
    smart_unavailable_reason: QString,
    /// What the daemon was last told about each channel's on/off state.
    enabled: HashMap<String, bool>,
    outbox: Outbox,
    /// Mic settings waiting to be sent, and whether a sender is running.
    mic_pending: Arc<Mutex<(Option<String>, bool)>>,
}

impl Default for EqControllerRust {
    fn default() -> Self {
        Self {
            last_error: QString::default(),
            unavailable_reason: QString::default(),
            active_channels_json: QString::from("[]"),
            mic_fx_caps_json: QString::from("{}"),
            smart_json: QString::from(
                &serde_json::to_string(&opengg_core::eq::saved_smart()).unwrap_or_else(|_| "{}".into()),
            ),
            smart_unavailable_reason: QString::default(),
            enabled: opengg_core::eq::saved().into_iter().map(|(k, v)| (k, v.enabled)).collect(),
            outbox: Arc::default(),
            mic_pending: Arc::default(),
        }
    }
}

impl qobject::EqController {
    pub fn set_eq(mut self: Pin<&mut Self>, channel: &QString, enabled: bool, bands_json: &QString, preamp: f64) -> bool {
        let channel = channel.to_string();
        let bands: Vec<f32> = match serde_json::from_str(&bands_json.to_string()) {
            Ok(b) => b,
            Err(e) => {
                self.as_mut().set_last_error(QString::from(&format!("bad band data: {e}")));
                return false;
            }
        };
        let state = EqState { enabled, bands, preamp: preamp as f32 };
        opengg_core::eq::save(&channel, &state);

        let was = self.as_ref().rust().enabled.get(&channel).copied().unwrap_or(false);
        if was == enabled {
            // Gains only: coalesce, off the UI thread.
            self.as_mut().queue(channel, state);
            return true;
        }
        // On/off: wait for the answer so the toggle is honest.
        match opengg_core::eq::apply(&channel, &state) {
            Ok(()) => {
                self.as_mut().rust_mut().enabled.insert(channel, enabled);
                self.as_mut().set_last_error(QString::default());
                self.as_mut().refresh();
                true
            }
            Err(e) => {
                self.as_mut().set_last_error(QString::from(&e));
                if enabled {
                    // Remember it as off, so the saved state matches reality.
                    opengg_core::eq::save(&channel, &EqState { enabled: false, ..state });
                }
                self.as_mut().refresh();
                false
            }
        }
    }

    fn queue(self: Pin<&mut Self>, channel: String, state: EqState) {
        let outbox = Arc::clone(&self.as_ref().rust().outbox);
        {
            let mut o = outbox.lock().unwrap();
            o.0.insert(channel.clone(), state);
            if !o.1.insert(channel.clone()) {
                return; // a sender for this channel is already draining
            }
        }
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || loop {
            let next = {
                let mut o = outbox.lock().unwrap();
                match o.0.remove(&channel) {
                    Some(s) => s,
                    None => {
                        o.1.remove(&channel);
                        return;
                    }
                }
            };
            let res = opengg_core::eq::apply(&channel, &next);
            let _ = qt_thread.queue(move |mut c| {
                c.as_mut().set_last_error(QString::from(&res.err().unwrap_or_default()));
            });
        });
    }

    pub fn set_smart_volume(mut self: Pin<&mut Self>, channel: &QString, level: &QString) -> bool {
        let (channel, level) = (channel.to_string(), level.to_string());
        let res = opengg_core::eq::apply_smart(&channel, &level);
        // Remember what is actually in effect: the choice when it applied,
        // "off" when it could not.
        opengg_core::eq::save_smart(&channel, if res.is_ok() { &level } else { "off" });
        self.as_mut().set_last_error(QString::from(&res.clone().err().unwrap_or_default()));
        self.as_mut().refresh();
        res.is_ok()
    }

    pub fn saved_state(&self, channel: &QString) -> QString {
        let st = opengg_core::eq::saved().remove(&channel.to_string()).unwrap_or_default();
        QString::from(&serde_json::to_string(&st).unwrap_or_else(|_| "{}".into()))
    }

    pub fn refresh(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let status = opengg_core::eq::status();
            let (reason, active) = match &status {
                Ok(json) => parse_status(json),
                Err(_) => ("the OpenGG daemon is not running".to_string(), Vec::new()),
            };
            let (smart_reason, smart) = match &status {
                Ok(json) => parse_smart(json),
                Err(_) => (
                    "the OpenGG daemon is not running".to_string(),
                    serde_json::to_string(&opengg_core::eq::saved_smart()).unwrap_or_else(|_| "{}".into()),
                ),
            };
            let active = serde_json::to_string(&active).unwrap_or_else(|_| "[]".into());
            let caps = opengg_core::micfx::status()
                .ok()
                .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
                .map(|v| v["caps"].to_string())
                .unwrap_or_else(|| "{}".into());
            let _ = qt_thread.queue(move |mut c| {
                c.as_mut().set_unavailable_reason(QString::from(&reason));
                c.as_mut().set_active_channels_json(QString::from(&active));
                c.as_mut().set_mic_fx_caps_json(QString::from(&caps));
                c.as_mut().set_smart_unavailable_reason(QString::from(&smart_reason));
                c.as_mut().set_smart_json(QString::from(&smart));
            });
        });
    }

    pub fn set_mic_fx(self: Pin<&mut Self>, json: &QString) {
        let json = json.to_string();
        opengg_core::micfx::save(&json);
        let qt_thread = self.qt_thread();
        let pending = Arc::clone(&self.as_ref().rust().mic_pending);
        // Latest wins; one sender at a time.
        let start = {
            let mut p = pending.lock().unwrap();
            let idle = p.0.is_none() && !p.1;
            p.0 = Some(json);
            if idle {
                p.1 = true;
            }
            idle
        };
        if !start {
            return;
        }
        std::thread::spawn(move || loop {
            let next = {
                let mut p = pending.lock().unwrap();
                match p.0.take() {
                    Some(j) => j,
                    None => {
                        p.1 = false;
                        return;
                    }
                }
            };
            let res = opengg_core::micfx::apply(&next);
            let _ = qt_thread.queue(move |mut c| {
                c.as_mut().set_last_error(QString::from(&res.err().unwrap_or_default()));
            });
        });
    }

    pub fn saved_mic_fx(&self) -> QString {
        QString::from(&opengg_core::micfx::saved())
    }
}

/// (unavailable reason, running channels) from the daemon's GetEq JSON.
fn parse_status(json: &str) -> (String, Vec<String>) {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let reason = if v["available"].as_bool().unwrap_or(false) {
        String::new()
    } else {
        v["reason"].as_str().unwrap_or("unavailable").to_string()
    };
    let mut active: Vec<String> = v["channels"]
        .as_object()
        .map(|m| {
            m.iter()
                .filter(|(_, s)| s["running"].as_bool().unwrap_or(false))
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default();
    active.sort();
    (reason, active)
}

/// (Smart Volume unavailable reason, `{channel: level}` JSON) from GetEq.
fn parse_smart(json: &str) -> (String, String) {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let reason = if v["smartAvailable"].as_bool().unwrap_or(false) {
        String::new()
    } else {
        v["smartReason"].as_str().unwrap_or("unavailable").to_string()
    };
    let levels: serde_json::Map<String, serde_json::Value> = v["channels"]
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, s)| (k.clone(), serde_json::json!(s["smart"].as_str().unwrap_or("off"))))
                .collect()
        })
        .unwrap_or_default();
    (reason, serde_json::Value::Object(levels).to_string())
}

#[cfg(test)]
mod tests {
    use super::{parse_smart, parse_status};

    #[test]
    fn smart_parsing() {
        let (r, l) = parse_smart(
            r#"{"smartAvailable":true,"smartReason":"","channels":{"Media":{"smart":"medium"},"Game":{}}}"#,
        );
        assert_eq!(r, "");
        let l: serde_json::Value = serde_json::from_str(&l).unwrap();
        assert_eq!(l["Media"], "medium");
        assert_eq!(l["Game"], "off");
        let (r, _) = parse_smart(r#"{"smartAvailable":false,"smartReason":"install swh-plugins"}"#);
        assert_eq!(r, "install swh-plugins");
    }

    #[test]
    fn status_parsing() {
        let (r, a) = parse_status(
            r#"{"available":true,"reason":"","channels":{"Game":{"running":true},"Chat":{"running":false}}}"#,
        );
        assert_eq!(r, "");
        assert_eq!(a, vec!["Game"]);
        let (r, a) = parse_status(r#"{"available":false,"reason":"pw-cli is not on PATH","channels":{}}"#);
        assert_eq!(r, "pw-cli is not on PATH");
        assert!(a.is_empty());
        assert_eq!(parse_status("junk").0, "unavailable");
    }
}

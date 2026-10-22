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
//! `applyNoiseGate`/`applyCompressor`/`applyNoiseReduction` remain no-ops:
//! there is still no engine for them. `dspAvailable` reports false so the
//! UI says so rather than implying they work.

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
        /// Whether the noise gate / compressor / noise reduction controls
        /// do anything. They do not.
        #[qproperty(bool, dsp_available, cxx_name = "dspAvailable")]
        type EqController = super::EqControllerRust;

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

        #[qinvokable]
        #[cxx_name = "applyNoiseGate"]
        fn apply_noise_gate(self: &Self, channel: &QString, enabled: bool, threshold: f32, auto_detect: bool);

        #[qinvokable]
        #[cxx_name = "applyCompressor"]
        fn apply_compressor(self: &Self, channel: &QString, enabled: bool, level: f32);

        #[qinvokable]
        #[cxx_name = "applyNoiseReduction"]
        fn apply_noise_reduction(self: &Self, channel: &QString, enabled: bool, intensity: f32);
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
    dsp_available: bool,
    /// What the daemon was last told about each channel's on/off state.
    enabled: HashMap<String, bool>,
    outbox: Outbox,
}

impl Default for EqControllerRust {
    fn default() -> Self {
        Self {
            last_error: QString::default(),
            unavailable_reason: QString::default(),
            active_channels_json: QString::from("[]"),
            dsp_available: false,
            enabled: opengg_core::eq::saved().into_iter().map(|(k, v)| (k, v.enabled)).collect(),
            outbox: Arc::default(),
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

    pub fn saved_state(&self, channel: &QString) -> QString {
        let st = opengg_core::eq::saved().remove(&channel.to_string()).unwrap_or_default();
        QString::from(&serde_json::to_string(&st).unwrap_or_else(|_| "{}".into()))
    }

    pub fn refresh(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let (reason, active) = match opengg_core::eq::status() {
                Ok(json) => parse_status(&json),
                Err(_) => ("the OpenGG daemon is not running".to_string(), Vec::new()),
            };
            let active = serde_json::to_string(&active).unwrap_or_else(|_| "[]".into());
            let _ = qt_thread.queue(move |mut c| {
                c.as_mut().set_unavailable_reason(QString::from(&reason));
                c.as_mut().set_active_channels_json(QString::from(&active));
            });
        });
    }

    pub fn apply_noise_gate(&self, _channel: &QString, _enabled: bool, _threshold: f32, _auto_detect: bool) {}
    pub fn apply_compressor(&self, _channel: &QString, _enabled: bool, _level: f32) {}
    pub fn apply_noise_reduction(&self, _channel: &QString, _enabled: bool, _intensity: f32) {}
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

#[cfg(test)]
mod tests {
    use super::parse_status;

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

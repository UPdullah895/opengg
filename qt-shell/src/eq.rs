//! EqController — per-channel 10-band EQ and voice processing (noise gate,
//! compressor, noise reduction) over `opengg_core::eq` and
//! `opengg_core::voicefx`.
//!
//! The engine is PipeWire's own filter-chain, not an external LV2 host. See
//! `core/src/eq.rs` for why the previous jalv + LSP approach could not work.
//!
//! One engine per channel:
//!
//! * **Output channels** (Game, Chat, …) run one chain between the channel's
//!   sink monitor and its device: voice stages first when any are on, then
//!   the EQ bands. It runs while the EQ or any voice stage is enabled.
//! * **Mic** runs a chain from the hardware mic into `OpenGG_Mic`, replacing
//!   the daemon's dry loopback while it is up (`voicefx::start_mic`).
//!
//! The gate/compressor/NR switches used to be no-ops with `dspAvailable`
//! hard-wired to false. They now drive real stages; `dspCapsJson` reports
//! which ones this machine can run.

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
        /// Lets the UI disable the controls instead of offering an EQ that
        /// silently does nothing.
        #[qproperty(QString, unavailable_reason, cxx_name = "unavailableReason")]
        /// JSON array of channel names with a running engine, so the UI can
        /// reflect real state rather than its own optimistic guess.
        #[qproperty(QString, active_channels_json, cxx_name = "activeChannelsJson")]
        /// Whether the noise gate — the one stage that needs nothing beyond
        /// PipeWire — can run here.
        #[qproperty(bool, dsp_available, cxx_name = "dspAvailable")]
        /// `{"gate": bool, "compressor": bool, "noiseReduction": bool}` —
        /// which voice stages this machine can run. The compressor and noise
        /// reduction need optional LADSPA plugins.
        #[qproperty(QString, dsp_caps_json, cxx_name = "dspCapsJson")]
        /// Result of the latest `measureNoiseFloor`, as
        /// `{"channel": "...", "db": -52.3}` (`db` null if nothing could be
        /// recorded). Empty while none has finished.
        #[qproperty(QString, noise_floor_json, cxx_name = "noiseFloorJson")]
        type EqController = super::EqControllerRust;

        /// Turn the EQ on for `channel`. Returns false and sets `lastError`
        /// if the engine could not be started.
        #[qinvokable]
        #[cxx_name = "startEngine"]
        fn start_engine(self: Pin<&mut Self>, channel: &QString) -> bool;

        /// Turn the EQ off for `channel`. The engine keeps running, with a
        /// flat EQ, while any voice stage is still on there.
        #[qinvokable]
        #[cxx_name = "stopEngine"]
        fn stop_engine(self: Pin<&mut Self>, channel: &QString);

        /// Push a JSON array of band gains in dB to `channel`'s engine.
        /// Remembered, so a restart for a voice-stage change keeps them.
        #[qinvokable]
        #[cxx_name = "applyEq"]
        fn apply_eq(self: Pin<&mut Self>, channel: &QString, bands_json: &QString);

        /// The gains the running engine actually reports, as a JSON array,
        /// or `"[]"`. For verifying that a change landed.
        #[qinvokable]
        #[cxx_name = "readBands"]
        fn read_bands(self: &Self, channel: &QString) -> QString;

        /// Stop every running engine and restore every direct path. Called
        /// on shutdown — a chain left running would keep a channel routed
        /// through a process that no longer has an owner.
        #[qinvokable]
        #[cxx_name = "stopAll"]
        fn stop_all(self: Pin<&mut Self>);

        /// Noise gate. `threshold` is in dB. `auto_detect` is handled by the
        /// UI through `measureNoiseFloor`; the threshold passed here is
        /// whatever that produced.
        #[qinvokable]
        #[cxx_name = "applyNoiseGate"]
        fn apply_noise_gate(
            self: Pin<&mut Self>,
            channel: &QString,
            enabled: bool,
            threshold: f32,
            auto_detect: bool,
        );

        /// Compressor, `level` 0..100.
        #[qinvokable]
        #[cxx_name = "applyCompressor"]
        fn apply_compressor(self: Pin<&mut Self>, channel: &QString, enabled: bool, level: f32);

        /// Noise reduction, `intensity` 0..100.
        #[qinvokable]
        #[cxx_name = "applyNoiseReduction"]
        fn apply_noise_reduction(
            self: Pin<&mut Self>,
            channel: &QString,
            enabled: bool,
            intensity: f32,
        );

        /// Listen to `channel`'s input for a moment and report its background
        /// level in `noiseFloorJson`. Threaded; the user should stay quiet.
        #[qinvokable]
        #[cxx_name = "measureNoiseFloor"]
        fn measure_noise_floor(self: Pin<&mut Self>, channel: &QString);

        /// Re-probe which voice stages can run (after installing a plugin).
        #[qinvokable]
        #[cxx_name = "refreshCaps"]
        fn refresh_caps(self: Pin<&mut Self>);
    }

    impl cxx_qt::Threading for EqController {}
}

use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt::Threading;
use cxx_qt_lib::QString;
use opengg_core::voicefx::{self, FxCaps, FxSettings};
use std::collections::{HashMap, HashSet};
use std::process::Child;

/// Where a running engine sits, so `stop` can put the dry path back.
enum Route {
    /// Between an output channel's sink monitor and its device.
    Output { sink: String, device: String },
    /// Hardware mic → `OpenGG_Mic`, replacing the daemon's loopback.
    Mic,
}

/// A running engine.
struct Engine {
    child: Child,
    route: Route,
    /// The voice stages it was built with; a different set needs a restart,
    /// anything else is a live parameter change.
    stages: (bool, bool, bool),
}

pub struct EqControllerRust {
    last_error: QString,
    unavailable_reason: QString,
    active_channels_json: QString,
    dsp_available: bool,
    dsp_caps_json: QString,
    noise_floor_json: QString,
    caps: FxCaps,
    engines: HashMap<String, Engine>,
    /// Channels whose EQ is switched on.
    eq_on: HashSet<String>,
    /// Last band gains per channel, re-applied after a restart.
    bands: HashMap<String, Vec<f32>>,
    /// Voice-stage settings per channel.
    fx: HashMap<String, FxSettings>,
}

fn caps_json(caps: &FxCaps) -> QString {
    QString::from(&serde_json::to_string(caps).unwrap_or_else(|_| "{}".into()))
}

impl Default for EqControllerRust {
    fn default() -> Self {
        let reason = opengg_core::eq::availability().err().unwrap_or_default();
        let caps = voicefx::capabilities();
        // A chain from a previous run that crashed would have left the mic's
        // dry path cut; putting it back is harmless when it is already there.
        voicefx::relink_loopback();
        Self {
            last_error: QString::default(),
            unavailable_reason: QString::from(&reason),
            active_channels_json: QString::from("[]"),
            dsp_available: caps.gate,
            dsp_caps_json: caps_json(&caps),
            noise_floor_json: QString::default(),
            caps,
            engines: HashMap::new(),
            eq_on: HashSet::new(),
            bands: HashMap::new(),
            fx: HashMap::new(),
        }
    }
}

/// Stop an engine and restore whatever path it replaced.
fn stop_engine_process(engine: Engine) {
    match engine.route {
        Route::Output { sink, device } => opengg_core::eq::stop(engine.child, &sink, &device),
        Route::Mic => voicefx::stop_mic(engine.child),
    }
}

/// The capture-side node name of a channel's engine, for live parameters.
fn capture_node(channel: &str) -> String {
    if channel == "Mic" {
        format!("{}_in", voicefx::MIC_NODE)
    } else {
        format!("{}_in", opengg_core::eq::node_name(channel))
    }
}

impl qobject::EqController {
    /// The virtual sink backing a channel, e.g. "Game" → "OpenGG_Game".
    fn sink_for(channel: &str) -> String {
        format!("OpenGG_{channel}")
    }

    fn publish_active(mut self: Pin<&mut Self>) {
        let mut names: Vec<String> = self.as_ref().rust().engines.keys().cloned().collect();
        names.sort_unstable();
        let json = serde_json::to_string(&names).unwrap_or_else(|_| "[]".into());
        self.as_mut().set_active_channels_json(QString::from(&json));
    }

    /// Bring `channel`'s engine in line with what is switched on: start,
    /// restart, update live, or stop it. Returns whether an engine is now
    /// running when one is wanted.
    fn reconcile(mut self: Pin<&mut Self>, channel: &str) -> bool {
        let fx = self.as_ref().rust().fx.get(channel).cloned().unwrap_or_default();
        let caps = self.as_ref().rust().caps.clone();
        let eq_on = self.as_ref().rust().eq_on.contains(channel);
        let is_mic = channel == "Mic";
        let wanted = if is_mic { fx.any_enabled() } else { eq_on || fx.any_enabled() };

        if !wanted {
            if let Some(engine) = self.as_mut().rust_mut().engines.remove(channel) {
                stop_engine_process(engine);
            }
            self.as_mut().publish_active();
            return true;
        }

        let stages = if fx.any_enabled() {
            voicefx::structure(&fx, &caps)
        } else {
            (false, false, false)
        };
        let running_same = self
            .as_ref()
            .rust()
            .engines
            .get(channel)
            .is_some_and(|e| e.stages == stages);
        if running_same {
            if fx.any_enabled() {
                if let Err(e) = voicefx::set_live(&capture_node(channel), &fx, &caps) {
                    eprintln!("EqController: live update for {channel}: {e}");
                }
            }
            return true;
        }

        // (Re)start. Tear the old one down first, including its routing.
        if let Some(engine) = self.as_mut().rust_mut().engines.remove(channel) {
            stop_engine_process(engine);
        }
        let started = if is_mic {
            voicefx::start_mic(&fx, &caps).map(|child| {
                voicefx::unlink_loopback();
                (child, Route::Mic)
            })
        } else {
            if let Err(reason) = opengg_core::eq::availability() {
                self.as_mut().set_unavailable_reason(QString::from(&reason));
                Err(reason)
            } else {
                let sink = Self::sink_for(channel);
                let device = opengg_core::audio::current_channel_device(channel);
                let with_fx = fx.any_enabled().then_some((&fx, &caps));
                opengg_core::eq::start_with_fx(channel, &sink, &device, with_fx).map(|child| {
                    // Only cut the dry path once the chain is up, or a
                    // failed start would leave the channel silent.
                    opengg_core::eq::unlink_direct(&sink, &device);
                    (child, Route::Output { sink, device })
                })
            }
        };
        match started {
            Ok((child, route)) => {
                self.as_mut()
                    .rust_mut()
                    .engines
                    .insert(channel.to_string(), Engine { child, route, stages });
                self.as_mut().set_last_error(QString::default());
                self.as_mut().publish_active();
                // A fresh chain starts with flat bands; restore the user's
                // once its node is up.
                if eq_on && !is_mic {
                    if let Some(gains) = self.as_ref().rust().bands.get(channel).cloned() {
                        let ch = channel.to_string();
                        std::thread::spawn(move || {
                            for _ in 0..20 {
                                std::thread::sleep(std::time::Duration::from_millis(100));
                                if opengg_core::eq::set_bands(&ch, &gains).is_ok() {
                                    return;
                                }
                            }
                        });
                    }
                }
                true
            }
            Err(e) => {
                eprintln!("EqController: start {channel}: {e}");
                self.as_mut().set_last_error(QString::from(&e));
                self.as_mut().publish_active();
                false
            }
        }
    }

    pub fn start_engine(mut self: Pin<&mut Self>, channel: &QString) -> bool {
        let channel = channel.to_string();
        if let Err(reason) = opengg_core::eq::availability() {
            self.as_mut().set_unavailable_reason(QString::from(&reason));
            self.as_mut().set_last_error(QString::from(&reason));
            return false;
        }
        self.as_mut().rust_mut().eq_on.insert(channel.clone());
        // Restarting is the documented behaviour.
        if let Some(engine) = self.as_mut().rust_mut().engines.remove(&channel) {
            stop_engine_process(engine);
        }
        self.as_mut().reconcile(&channel)
    }

    pub fn stop_engine(mut self: Pin<&mut Self>, channel: &QString) {
        let channel = channel.to_string();
        self.as_mut().rust_mut().eq_on.remove(&channel);
        // Voice stages may still need the chain; flatten the bands for them.
        if self.as_ref().rust().engines.contains_key(&channel) {
            let _ = opengg_core::eq::set_bands(&channel, &[0.0; 10]);
        }
        self.as_mut().reconcile(&channel);
    }

    pub fn apply_eq(mut self: Pin<&mut Self>, channel: &QString, bands_json: &QString) {
        let channel = channel.to_string();
        let bands: Vec<f32> = match serde_json::from_str(&bands_json.to_string()) {
            Ok(b) => b,
            Err(e) => {
                self.as_mut()
                    .set_last_error(QString::from(&format!("bad band data: {e}")));
                return;
            }
        };
        self.as_mut().rust_mut().bands.insert(channel.clone(), bands.clone());
        if !self.as_ref().rust().engines.contains_key(&channel)
            || !self.as_ref().rust().eq_on.contains(&channel)
        {
            return;
        }
        match opengg_core::eq::set_bands(&channel, &bands) {
            Ok(()) => self.as_mut().set_last_error(QString::default()),
            Err(e) => {
                eprintln!("EqController::apply_eq({channel}): {e}");
                self.as_mut().set_last_error(QString::from(&e));
            }
        }
    }

    pub fn read_bands(&self, channel: &QString) -> QString {
        let bands = opengg_core::eq::read_bands(&channel.to_string()).unwrap_or_default();
        QString::from(&serde_json::to_string(&bands).unwrap_or_else(|_| "[]".into()))
    }

    pub fn stop_all(mut self: Pin<&mut Self>) {
        let keys: Vec<String> = self.as_ref().rust().engines.keys().cloned().collect();
        for k in keys {
            if let Some(engine) = self.as_mut().rust_mut().engines.remove(&k) {
                stop_engine_process(engine);
            }
        }
        self.as_mut().publish_active();
    }

    fn update_fx(mut self: Pin<&mut Self>, channel: &QString, f: impl FnOnce(&mut FxSettings)) {
        let channel = channel.to_string();
        f(self.as_mut().rust_mut().fx.entry(channel.clone()).or_default());
        self.as_mut().reconcile(&channel);
    }

    pub fn apply_noise_gate(
        self: Pin<&mut Self>,
        channel: &QString,
        enabled: bool,
        threshold: f32,
        _auto_detect: bool,
    ) {
        self.update_fx(channel, |fx| {
            fx.gate_enabled = enabled;
            fx.gate_threshold_db = threshold;
        });
    }

    pub fn apply_compressor(self: Pin<&mut Self>, channel: &QString, enabled: bool, level: f32) {
        self.update_fx(channel, |fx| {
            fx.comp_enabled = enabled;
            fx.comp_level = level;
        });
    }

    pub fn apply_noise_reduction(
        self: Pin<&mut Self>,
        channel: &QString,
        enabled: bool,
        intensity: f32,
    ) {
        self.update_fx(channel, |fx| {
            fx.nr_enabled = enabled;
            fx.nr_intensity = intensity;
        });
    }

    pub fn measure_noise_floor(mut self: Pin<&mut Self>, channel: &QString) {
        let channel = channel.to_string();
        self.as_mut().set_noise_floor_json(QString::default());
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            // Measure the dry input: the hardware mic for Mic, the channel's
            // own monitor otherwise.
            let source = if channel == "Mic" {
                voicefx::mic_hw_source().unwrap_or_else(|| "OpenGG_Mic.monitor".into())
            } else {
                format!("OpenGG_{channel}.monitor")
            };
            let db = voicefx::measure_noise_floor_db(&source, 1.5);
            let json = serde_json::json!({ "channel": channel, "db": db }).to_string();
            let _ = qt_thread.queue(move |mut c| {
                c.as_mut().set_noise_floor_json(QString::from(&json));
            });
        });
    }

    pub fn refresh_caps(mut self: Pin<&mut Self>) {
        let caps = voicefx::capabilities();
        self.as_mut().set_dsp_available(caps.gate);
        self.as_mut().set_dsp_caps_json(caps_json(&caps));
        self.as_mut().rust_mut().caps = caps;
    }
}

//! EqController — per-channel 10-band EQ over `opengg_core::eq`.
//!
//! The engine is PipeWire's own filter-chain (builtin biquads), not an
//! external LV2 host. See `core/src/eq.rs` for why the previous jalv + LSP
//! approach could not work; in short it depended on plugins that are not
//! installed by default, drove jalv with a flag that makes it ignore its
//! stdin, never linked the result into the audio path, and reported success
//! for a process that had already exited.
//!
//! `applyNoiseGate`/`applyCompressor`/`applyNoiseReduction` remain no-ops:
//! there is still no engine for them. They are kept as real invokables so
//! `DspControls.qml` has a stable call shape, and `dspAvailable` reports
//! false so the UI can say so rather than implying they work.

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
        /// Whether the noise gate / compressor / noise reduction controls
        /// do anything. They do not.
        #[qproperty(bool, dsp_available, cxx_name = "dspAvailable")]
        type EqController = super::EqControllerRust;

        /// Start the EQ for `channel`, inserting it between that channel's
        /// virtual sink and its output device. Returns false and sets
        /// `lastError` if the engine could not be started.
        #[qinvokable]
        #[cxx_name = "startEngine"]
        fn start_engine(self: Pin<&mut Self>, channel: &QString) -> bool;

        /// Stop the EQ for `channel` and restore its direct output path.
        #[qinvokable]
        #[cxx_name = "stopEngine"]
        fn stop_engine(self: Pin<&mut Self>, channel: &QString);

        /// Push a JSON array of band gains in dB to `channel`'s engine.
        /// No-op when no engine is running there.
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
}

use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::collections::HashMap;
use std::process::Child;

/// A running engine, with the routing it displaced so `stop` can put it back.
struct Engine {
    child: Child,
    sink: String,
    device: String,
}

pub struct EqControllerRust {
    last_error: QString,
    unavailable_reason: QString,
    active_channels_json: QString,
    dsp_available: bool,
    engines: HashMap<String, Engine>,
}

impl Default for EqControllerRust {
    fn default() -> Self {
        let reason = opengg_core::eq::availability()
            .err()
            .unwrap_or_default();
        Self {
            last_error: QString::default(),
            unavailable_reason: QString::from(&reason),
            active_channels_json: QString::from("[]"),
            // No engine exists for gate/compressor/NR anywhere in the
            // project. Saying so beats a control that pretends.
            dsp_available: false,
            engines: HashMap::new(),
        }
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

    pub fn start_engine(mut self: Pin<&mut Self>, channel: &QString) -> bool {
        let channel = channel.to_string();
        if let Err(reason) = opengg_core::eq::availability() {
            self.as_mut().set_unavailable_reason(QString::from(&reason));
            self.as_mut().set_last_error(QString::from(&reason));
            return false;
        }

        // Restarting is the documented behaviour, so tear the old one down
        // first — including its routing, or the restore below would run
        // against links the new chain has already replaced.
        self.as_mut().stop_engine(&QString::from(&channel));

        let sink = Self::sink_for(&channel);
        let device = opengg_core::audio::current_channel_device(&channel);

        match opengg_core::eq::start(&channel, &sink, &device) {
            Ok(child) => {
                // Only cut the dry path once the chain is actually up,
                // otherwise a failed start would leave the channel silent.
                opengg_core::eq::unlink_direct(&sink, &device);
                self.as_mut().rust_mut().engines.insert(
                    channel,
                    Engine { child, sink, device },
                );
                self.as_mut().set_last_error(QString::default());
                self.as_mut().publish_active();
                true
            }
            Err(e) => {
                eprintln!("EqController::start_engine({channel}): {e}");
                self.as_mut().set_last_error(QString::from(&e));
                false
            }
        }
    }

    pub fn stop_engine(mut self: Pin<&mut Self>, channel: &QString) {
        let key = channel.to_string();
        if let Some(engine) = self.as_mut().rust_mut().engines.remove(&key) {
            opengg_core::eq::stop(engine.child, &engine.sink, &engine.device);
        }
        self.as_mut().publish_active();
    }

    pub fn apply_eq(mut self: Pin<&mut Self>, channel: &QString, bands_json: &QString) {
        let channel = channel.to_string();
        if !self.as_ref().rust().engines.contains_key(&channel) {
            return;
        }
        let bands: Vec<f32> = match serde_json::from_str(&bands_json.to_string()) {
            Ok(b) => b,
            Err(e) => {
                self.as_mut()
                    .set_last_error(QString::from(&format!("bad band data: {e}")));
                return;
            }
        };
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
                opengg_core::eq::stop(engine.child, &engine.sink, &engine.device);
            }
        }
        self.as_mut().publish_active();
    }

    pub fn apply_noise_gate(&self, _channel: &QString, _enabled: bool, _threshold: f32, _auto_detect: bool) {}
    pub fn apply_compressor(&self, _channel: &QString, _enabled: bool, _level: f32) {}
    pub fn apply_noise_reduction(&self, _channel: &QString, _enabled: bool, _intensity: f32) {}
}

//! EqController — jalv-hosted per-channel LV2 EQ, ported from the Tauri
//! host's `start_eq_engine`/`apply_eq`/`stop_eq_engine` (commands.rs) almost
//! verbatim: one `jalv` subprocess per channel, JACK-auto-connected via
//! WirePlumber, control ports written over its piped stdin.
//!
//! `applyNoiseGate`/`applyCompressor`/`applyNoiseReduction` are no-ops here
//! for the same reason they're no-ops in the Tauri host today (see
//! commands.rs — `apply_noise_gate`/`apply_compressor`/
//! `apply_noise_reduction` all just `Ok(())`): there is no DSP engine wired
//! up for them yet anywhere in this project, Qt shell included. Kept as
//! real qinvokables (not omitted) so `DspControls.qml`'s per-channel state
//! has a stable call shape to target once a real engine lands.
//!
//! State (which channels have an EQ engine running, current band values) is
//! intentionally NOT persisted to `ui-settings.json` — `frontend/src/stores/
//! dsp.ts` is a plain in-memory Pinia store with no persistence wiring
//! either, so this resets on every restart in both apps alike.

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
        type EqController = super::EqControllerRust;

        /// Spawn (or respawn, if one's already running) a jalv EQ host for
        /// `channel`. Sets `lastError` on failure (e.g. jalv not installed).
        #[qinvokable]
        #[cxx_name = "startEngine"]
        fn start_engine(self: Pin<&mut Self>, channel: &QString) -> bool;

        /// Kill the jalv EQ host for `channel`, if one is running.
        #[qinvokable]
        #[cxx_name = "stopEngine"]
        fn stop_engine(self: Pin<&mut Self>, channel: &QString);

        /// Push a 10-element JSON array of band gains (dB, -12..+12) to the
        /// running jalv instance for `channel`. No-op if no engine is
        /// running for that channel.
        #[qinvokable]
        #[cxx_name = "applyEq"]
        fn apply_eq(self: Pin<&mut Self>, channel: &QString, bands_json: &QString);

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
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};

const LSP_EQ_URI: &str = "http://lsp-plug.in/plugins/lv2/para_equalizer_x10_stereo";
const GAIN_PORT_BASE: usize = 15;

#[derive(Default)]
pub struct EqControllerRust {
    last_error: QString,
    procs: HashMap<String, (Child, ChildStdin)>,
}

impl qobject::EqController {
    pub fn start_engine(mut self: Pin<&mut Self>, channel: &QString) -> bool {
        let channel = channel.to_string();
        if let Some((mut child, _stdin)) = self.as_mut().rust_mut().procs.remove(&channel) {
            let _ = child.kill();
        }
        let jack_name = format!("opengg_eq_{}", channel.to_lowercase());
        let spawned = Command::new("jalv")
            .args(["-n", &jack_name, "-i", LSP_EQ_URI])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        match spawned {
            Ok(mut child) => match child.stdin.take() {
                Some(stdin) => {
                    self.as_mut().rust_mut().procs.insert(channel, (child, stdin));
                    self.as_mut().set_last_error(QString::default());
                    true
                }
                None => {
                    let _ = child.kill();
                    self.as_mut().set_last_error(QString::from("jalv started with no stdin"));
                    false
                }
            },
            Err(e) => {
                self.as_mut().set_last_error(QString::from(&format!(
                    "jalv spawn failed: {e}. Install with your distro's LV2/jalv package."
                )));
                false
            }
        }
    }

    pub fn stop_engine(mut self: Pin<&mut Self>, channel: &QString) {
        if let Some((mut child, _stdin)) = self.as_mut().rust_mut().procs.remove(&channel.to_string()) {
            let _ = child.kill();
        }
    }

    pub fn apply_eq(mut self: Pin<&mut Self>, channel: &QString, bands_json: &QString) {
        let bands: Vec<f32> = serde_json::from_str(&bands_json.to_string()).unwrap_or_default();
        if let Some((_child, stdin)) = self.as_mut().rust_mut().procs.get_mut(&channel.to_string()) {
            for (i, gain_db) in bands.iter().enumerate().take(10) {
                let _ = writeln!(stdin, "{} {:.4}", GAIN_PORT_BASE + i, gain_db);
            }
            let _ = stdin.flush();
        }
    }

    pub fn apply_noise_gate(&self, _channel: &QString, _enabled: bool, _threshold: f32, _auto_detect: bool) {}
    pub fn apply_compressor(&self, _channel: &QString, _enabled: bool, _level: f32) {}
    pub fn apply_noise_reduction(&self, _channel: &QString, _enabled: bool, _intensity: f32) {}
}

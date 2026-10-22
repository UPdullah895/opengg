//! Microphone processing: noise suppression, noise gate, compressor.
//!
//! The Mixer's mic cards existed long before any engine did; they were shown
//! disabled until now. This hosts a real chain, built only from plugins the
//! system has:
//!
//! - noise suppression: RNNoise (`librnnoise_ladspa.so`, packaged as
//!   noise-suppression-for-voice);
//! - gate and compressor: swh-plugins (`gate_1410.so`, `sc4m_1916.so`).
//!
//! PipeWire's filter-chain runs them in a `pipewire -c` process (mono
//! plugins are duplicated per channel by the filter-chain itself). The
//! existing hardware-mic loopback stays the tap; with the chain live the
//! reconciler routes `loopback → chain → OpenGG_Mic` instead of
//! `loopback → OpenGG_Mic`, so everything downstream (the virtual mic apps
//! use, recordings) hears the processed voice.
//!
//! Plugins are loaded at runtime by PipeWire, in their own process; OpenGG
//! does not link or ship them.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Child;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::eq::{set_params, spawn_chain};
use super::reconcile::LinkPlan;

pub const INPUT_NODE: &str = "opengg_micfx_in";
pub const OUTPUT_NODE: &str = "opengg_micfx";
/// The hardware-mic loopback's output node (see `sinks::setup_mic_loopback`).
pub const LOOPBACK_OUT: &str = "OpenGG_Mic_Loopback_out";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NoiseReduction {
    pub enabled: bool,
    /// 0–100: how readily non-voice audio is muted.
    pub intensity: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gate {
    pub enabled: bool,
    /// Open threshold in dB, -70..0.
    pub threshold: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Compressor {
    pub enabled: bool,
    /// 0–100: amount of compression.
    pub level: f32,
}

impl Default for NoiseReduction {
    fn default() -> Self {
        NoiseReduction { enabled: false, intensity: 50.0 }
    }
}
impl Default for Gate {
    fn default() -> Self {
        Gate { enabled: false, threshold: -40.0 }
    }
}
impl Default for Compressor {
    fn default() -> Self {
        Compressor { enabled: false, level: 50.0 }
    }
}

/// The mic chain as the user set it (`mixer.micfx`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MicFx {
    pub nr: NoiseReduction,
    pub gate: Gate,
    pub comp: Compressor,
}

/// Which stages this system can run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Caps {
    pub nr: bool,
    pub gate: bool,
    pub comp: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Stage {
    Nr,
    Gate,
    Comp,
}

struct Plugin {
    file: &'static str,
    label: &'static str,
}

const NR: Plugin = Plugin { file: "librnnoise_ladspa.so", label: "noise_suppressor_mono" };
const GATE: Plugin = Plugin { file: "gate_1410.so", label: "gate" };
const COMP: Plugin = Plugin { file: "sc4m_1916.so", label: "sc4m" };

fn plugin(stage: Stage) -> &'static Plugin {
    match stage {
        Stage::Nr => &NR,
        Stage::Gate => &GATE,
        Stage::Comp => &COMP,
    }
}

/// LADSPA directories: `$LADSPA_PATH`, then the usual locations including
/// multiarch ones.
fn ladspa_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var("LADSPA_PATH")
        .unwrap_or_default()
        .split(':')
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .collect();
    for d in ["/usr/lib/ladspa", "/usr/lib64/ladspa", "/usr/local/lib/ladspa"] {
        dirs.push(PathBuf::from(d));
    }
    if let Ok(rd) = std::fs::read_dir("/usr/lib") {
        for e in rd.flatten() {
            let p = e.path().join("ladspa");
            if p.is_dir() {
                dirs.push(p);
            }
        }
    }
    dirs
}

fn find_plugin(file: &str) -> Option<PathBuf> {
    ladspa_dirs().into_iter().map(|d| d.join(file)).find(|p| p.is_file())
}

pub fn caps() -> Caps {
    if !subprocess_ok() {
        return Caps::default();
    }
    Caps {
        nr: find_plugin(NR.file).is_some(),
        gate: find_plugin(GATE.file).is_some(),
        comp: find_plugin(COMP.file).is_some(),
    }
}

fn subprocess_ok() -> bool {
    super::eq::availability().is_ok()
}

impl MicFx {
    /// Values clamped to their ranges.
    pub fn normalized(&self) -> MicFx {
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        MicFx {
            nr: NoiseReduction { enabled: self.nr.enabled, intensity: f(self.nr.intensity, 0.0, 100.0, 50.0) },
            gate: Gate { enabled: self.gate.enabled, threshold: f(self.gate.threshold, -70.0, 0.0, -40.0) },
            comp: Compressor { enabled: self.comp.enabled, level: f(self.comp.level, 0.0, 100.0, 50.0) },
        }
    }

    /// The stages to run, in order, given what is installed.
    fn stages(&self, caps: Caps) -> Vec<Stage> {
        let mut v = Vec::new();
        if self.nr.enabled && caps.nr {
            v.push(Stage::Nr);
        }
        if self.gate.enabled && caps.gate {
            v.push(Stage::Gate);
        }
        if self.comp.enabled && caps.comp {
            v.push(Stage::Comp);
        }
        v
    }
}

/// Control values for one stage, as (port name, value).
fn controls(stage: Stage, fx: &MicFx) -> Vec<(&'static str, f32)> {
    match stage {
        // RNNoise always denoises; the VAD threshold decides how readily
        // frames without voice are muted outright.
        Stage::Nr => vec![
            ("VAD Threshold (%)", (fx.nr.intensity * 0.95).round()),
            ("VAD Grace Period (ms)", 200.0),
            ("Retroactive VAD Grace (ms)", 0.0),
        ],
        Stage::Gate => vec![
            ("Threshold (dB)", fx.gate.threshold),
            ("Attack (ms)", 5.0),
            ("Hold (ms)", 250.0),
            ("Decay (ms)", 150.0),
            ("Range (dB)", -60.0),
        ],
        Stage::Comp => {
            let amount = fx.comp.level / 100.0;
            let threshold = -24.0 * amount;
            let ratio = 1.0 + 7.0 * amount;
            // Make up roughly half the gain the compressor takes away at
            // the threshold, so turning it up doesn't just make you quieter.
            let makeup = (-threshold) * (1.0 - 1.0 / ratio) * 0.5;
            vec![
                ("RMS/peak", 0.5),
                ("Attack time (ms)", 10.0),
                ("Release time (ms)", 150.0),
                ("Threshold level (dB)", threshold),
                ("Ratio (1:n)", ratio),
                ("Knee radius (dB)", 6.0),
                ("Makeup gain (dB)", makeup),
            ]
        }
    }
}

fn node_name(stage: Stage) -> &'static str {
    match stage {
        Stage::Nr => "nr",
        Stage::Gate => "gate",
        Stage::Comp => "comp",
    }
}

/// The filter-chain config for `stages`, with plugin paths resolved by
/// `resolve` (a lookup in tests, the LADSPA search in the daemon).
fn build_config(fx: &MicFx, stages: &[Stage], resolve: impl Fn(&str) -> Option<PathBuf>) -> Option<String> {
    let mut nodes = String::new();
    for st in stages {
        let p = plugin(*st);
        let path = resolve(p.file)?;
        let ctl: Vec<String> = controls(*st, fx).iter().map(|(k, v)| format!("\"{k}\" = {v:.3}")).collect();
        nodes.push_str(&format!(
            "          {{ type = ladspa name = {} plugin = \"{}\" label = {} control = {{ {} }} }}\n",
            node_name(*st),
            path.display(),
            p.label,
            ctl.join(" ")
        ));
    }
    let links: String = stages
        .windows(2)
        .map(|w| {
            format!(
                "          {{ output = \"{}:Output\" input = \"{}:Input\" }}\n",
                node_name(w[0]),
                node_name(w[1])
            )
        })
        .collect();
    Some(format!(
        r#"# Generated by openggd — microphone processing. Do not edit.
context.properties = {{ log.level = 0 }}
context.spa-libs = {{
  audio.convert.* = audioconvert/libspa-audioconvert
  support.*       = support/libspa-support
}}
context.modules = [
  {{ name = libpipewire-module-rt flags = [ ifexists nofail ] }}
  {{ name = libpipewire-module-protocol-native }}
  {{ name = libpipewire-module-client-node }}
  {{ name = libpipewire-module-adapter }}
  {{ name = libpipewire-module-filter-chain
    args = {{
      node.description = "OpenGG Mic Processing"
      media.name       = "OpenGG Mic Processing"
      filter.graph = {{
        nodes = [
{nodes}        ]
        links = [
{links}        ]
      }}
      audio.channels = 2
      audio.position = [ FL FR ]
      capture.props  = {{ node.name = "{INPUT_NODE}" node.autoconnect = false media.class = Stream/Input/Audio }}
      playback.props = {{ node.name = "{OUTPUT_NODE}" node.autoconnect = false media.class = Stream/Output/Audio }}
    }}
  }}
]
"#
    ))
}

/// `pw-cli set-param` payload updating every control of `stages`.
fn params_payload(fx: &MicFx, stages: &[Stage]) -> String {
    let parts: Vec<String> = stages
        .iter()
        .flat_map(|st| controls(*st, fx).into_iter().map(move |(k, v)| format!("\"{}:{k}\" {v:.3}", node_name(*st))))
        .collect();
    format!("{{ params = [ {} ] }}", parts.join(" "))
}

/// Links so the mic flows through the chain when it is live, and straight
/// into OpenGG_Mic when it is not. Nothing is planned while the loopback is
/// absent (no microphone).
pub fn plan_mic_links(links: &HashSet<(String, String)>, ports: &HashSet<String>, live: bool) -> LinkPlan {
    let mut plan = LinkPlan::default();
    for p in ["FL", "FR"] {
        let tap = format!("{LOOPBACK_OUT}:output_{p}");
        if !ports.contains(&tap) {
            continue;
        }
        let mic = format!("OpenGG_Mic:playback_{p}");
        let fx_in = format!("{INPUT_NODE}:input_{p}");
        let fx_out = format!("{OUTPUT_NODE}:output_{p}");
        let want: Vec<(String, String)> =
            if live { vec![(tap.clone(), fx_in), (fx_out, mic.clone())] } else { vec![(tap.clone(), mic.clone())] };
        if live && links.contains(&(tap.clone(), mic.clone())) {
            plan.remove.push((tap.clone(), mic));
        }
        for l in want {
            if !links.contains(&l) {
                plan.add.push(l);
            }
        }
    }
    plan
}

struct Running {
    child: Child,
    stages: Vec<Stage>,
}

/// Owns the mic chain process.
#[derive(Default)]
pub struct MicFxHost {
    desired: Mutex<Option<MicFx>>,
    running: Mutex<Option<Running>>,
}

impl MicFxHost {
    /// Seed from saved settings, once (a later `set` wins).
    pub fn seed(&self, saved: Option<&MicFx>) {
        let mut d = self.desired.lock().unwrap();
        if d.is_none() {
            *d = Some(saved.cloned().unwrap_or_default().normalized());
        }
    }

    /// Apply a user change. Parameter changes go to the running chain in
    /// place; adding or removing a stage restarts it.
    pub fn set(&self, fx: &MicFx) -> Result<(), String> {
        let fx = fx.normalized();
        *self.desired.lock().unwrap() = Some(fx.clone());
        let stages = fx.stages(caps());
        let same_shape = self.running.lock().unwrap().as_ref().map(|r| r.stages == stages).unwrap_or(false);
        if same_shape && self.is_running() {
            return set_params(INPUT_NODE, &params_payload(&fx, &stages));
        }
        self.ensure().map(|_| ())
    }

    /// Make the process match the desired state. Returns whether a chain
    /// is running.
    pub fn ensure(&self) -> Result<bool, String> {
        let fx = self.desired.lock().unwrap().clone().unwrap_or_default();
        let stages = fx.stages(caps());
        let shape_ok = self.running.lock().unwrap().as_ref().map(|r| r.stages == stages).unwrap_or(stages.is_empty());
        if shape_ok && (stages.is_empty() || self.is_running()) {
            return Ok(!stages.is_empty());
        }
        self.stop();
        if stages.is_empty() {
            return Ok(false);
        }
        let conf = build_config(&fx, &stages, find_plugin).ok_or("a mic processing plugin went missing")?;
        let child = spawn_chain("micfx", &conf)?;
        tracing::info!("Mic processing started (pid {}): {:?}", child.id(), stages);
        *self.running.lock().unwrap() = Some(Running { child, stages });
        Ok(true)
    }

    pub fn is_running(&self) -> bool {
        let mut r = self.running.lock().unwrap();
        let alive = r.as_mut().map(|r| matches!(r.child.try_wait(), Ok(None))).unwrap_or(false);
        if !alive && r.is_some() {
            *r = None;
            tracing::warn!("Mic processing chain exited");
        }
        alive
    }

    /// True if a chain should be running and is not.
    pub fn needs_attention(&self) -> bool {
        let want = self.desired.lock().unwrap().as_ref().map(|f| !f.stages(caps()).is_empty()).unwrap_or(false);
        want && !self.is_running()
    }

    fn stop(&self) {
        if let Some(mut r) = self.running.lock().unwrap().take() {
            let _ = r.child.kill();
            let _ = r.child.wait();
            tracing::info!("Mic processing stopped");
        }
    }

    /// `{"caps": {nr, gate, comp}, "state": MicFx, "running": bool}`
    pub fn status_json(&self) -> String {
        serde_json::json!({
            "caps": caps(),
            "state": self.desired.lock().unwrap().clone().unwrap_or_default(),
            "running": self.is_running(),
        })
        .to_string()
    }
}

impl Drop for MicFxHost {
    fn drop(&mut self) {
        self.stop();
    }
}

/// `mixer.micfx` from ui-settings.json.
pub fn parse_saved(v: &serde_json::Value) -> Option<MicFx> {
    serde_json::from_value::<MicFx>(v.clone()).ok().filter(|_| v.is_object()).map(|f| f.normalized())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(file: &str) -> Option<PathBuf> {
        Some(PathBuf::from("/lib/ladspa").join(file))
    }

    #[test]
    fn only_enabled_and_installed_stages_run_in_order() {
        let fx = MicFx {
            nr: NoiseReduction { enabled: true, intensity: 80.0 },
            gate: Gate { enabled: true, threshold: -30.0 },
            comp: Compressor { enabled: true, level: 50.0 },
        };
        let all = Caps { nr: true, gate: true, comp: true };
        assert_eq!(fx.stages(all), vec![Stage::Nr, Stage::Gate, Stage::Comp]);
        assert_eq!(fx.stages(Caps { nr: false, ..all }), vec![Stage::Gate, Stage::Comp]);
        assert!(MicFx::default().stages(all).is_empty());
    }

    #[test]
    fn config_chains_the_stages() {
        let fx = MicFx { nr: NoiseReduction { enabled: true, intensity: 100.0 }, ..Default::default() };
        let cfg = build_config(&fx, &[Stage::Nr, Stage::Comp], fake).unwrap();
        assert!(cfg.contains("plugin = \"/lib/ladspa/librnnoise_ladspa.so\" label = noise_suppressor_mono"));
        assert!(cfg.contains("\"VAD Threshold (%)\" = 95.000"));
        assert!(cfg.contains("{ output = \"nr:Output\" input = \"comp:Input\" }"));
        assert_eq!(cfg.matches("node.autoconnect = false").count(), 2);
        // A missing plugin means no config, not a broken one.
        assert!(build_config(&fx, &[Stage::Gate], |_| None).is_none());
    }

    #[test]
    fn compressor_amount_maps_to_sane_settings() {
        let at = |level| {
            let fx = MicFx { comp: Compressor { enabled: true, level }, ..Default::default() };
            controls(Stage::Comp, &fx).into_iter().collect::<std::collections::HashMap<_, _>>()
        };
        assert_eq!(at(0.0)["Ratio (1:n)"], 1.0);
        assert_eq!(at(0.0)["Makeup gain (dB)"], 0.0);
        let full = at(100.0);
        assert_eq!(full["Ratio (1:n)"], 8.0);
        assert_eq!(full["Threshold level (dB)"], -24.0);
        assert!(full["Makeup gain (dB)"] > 0.0 && full["Makeup gain (dB)"] <= 24.0);
    }

    #[test]
    fn payload_addresses_each_stage() {
        let fx = MicFx { gate: Gate { enabled: true, threshold: -33.0 }, ..Default::default() };
        let p = params_payload(&fx, &[Stage::Gate]);
        assert!(p.contains("\"gate:Threshold (dB)\" -33.000"));
        assert!(!p.contains("nr:"));
    }

    #[test]
    fn values_are_clamped() {
        let fx = MicFx {
            nr: NoiseReduction { enabled: true, intensity: 500.0 },
            gate: Gate { enabled: true, threshold: f32::NAN },
            comp: Compressor { enabled: true, level: -5.0 },
        }
        .normalized();
        assert_eq!(fx.nr.intensity, 100.0);
        assert_eq!(fx.gate.threshold, -40.0);
        assert_eq!(fx.comp.level, 0.0);
    }

    #[test]
    fn mic_links_route_through_the_chain_only_when_live() {
        let ports: HashSet<String> =
            ["OpenGG_Mic_Loopback_out:output_FL", "OpenGG_Mic_Loopback_out:output_FR"].iter().map(|s| s.to_string()).collect();
        let direct: HashSet<(String, String)> = ["FL", "FR"]
            .iter()
            .map(|p| (format!("OpenGG_Mic_Loopback_out:output_{p}"), format!("OpenGG_Mic:playback_{p}")))
            .collect();
        assert_eq!(plan_mic_links(&direct, &ports, false), LinkPlan::default());
        let live = plan_mic_links(&direct, &ports, true);
        assert_eq!(live.remove.len(), 2);
        assert_eq!(live.add.len(), 4);
        assert!(live.add.contains(&("opengg_micfx:output_FR".into(), "OpenGG_Mic:playback_FR".into())));
        // Back off: the direct path returns.
        let off = plan_mic_links(&HashSet::new(), &ports, false);
        assert_eq!(off.add.len(), 2);
        // No microphone (no loopback): nothing to do.
        assert_eq!(plan_mic_links(&direct, &HashSet::new(), true), LinkPlan::default());
    }

    #[test]
    fn saved_state_parses() {
        let fx = parse_saved(&serde_json::json!({"nr": {"enabled": true, "intensity": 70}, "gate": {"enabled": true}}))
            .unwrap();
        assert!(fx.nr.enabled && fx.nr.intensity == 70.0);
        assert_eq!(fx.gate.threshold, -40.0);
        assert!(parse_saved(&serde_json::json!(5)).is_none());
        assert!(parse_saved(&serde_json::Value::Null).is_none());
    }
}

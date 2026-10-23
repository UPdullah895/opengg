//! Per-channel FX chain — 10-band EQ and Smart Volume — hosted by the daemon.
//!
//! The EQ used to live in the UI process: closing the window stopped it,
//! nothing remembered it, and it cut the channel's direct output link,
//! which the reconciler (rightly) put back, so the EQ'd and dry signal
//! played together. Now the daemon owns it:
//!
//! - one `pipewire -c <conf>` process per channel that needs one (EQ on or
//!   Smart Volume on), started with the saved settings and killed with the
//!   daemon (`PR_SET_PDEATHSIG`);
//! - its ports are not auto-connected; the reconciler links
//!   `OpenGG_<Ch>:monitor → chain → device`, so device changes and restarts
//!   are handled in one place;
//! - parameter changes go to the running chain with `pw-cli set-param`, by
//!   node name, without restarting it; only adding or removing the Smart
//!   Volume stage changes the graph and restarts the chain;
//! - EQ state persists as `mixer.eq` and Smart Volume as
//!   `mixer.smartVolume` in `ui-settings.json`, written by the UI; the daemon
//!   seeds from them on its first pass.
//!
//! The graph is explicit stereo — `pre → eq0..9 → [smart] → fader` per side,
//! with the Smart Volume compressor (swh SC4, stereo-linked) taking both
//! sides — so the compressor never pumps one side of the image alone:
//!
//! ```text
//!   L: preL → eqL0 … eqL9 ─┐            ┌─ faderL → out FL
//!                          ├─ smart ───┤
//!   R: preR → eqR0 … eqR9 ─┘  (SC4)     └─ faderR → out FR
//! ```
//!
//! The fader is the channel's Personal volume. While a chain runs, the
//! reconciler makes the channel's monitor pre-fader and mirrors the sink's
//! volume/mute onto `fader*:Mult`: Smart Volume must see the signal before
//! the user's fader, or it would compress away part of every fader move.

use std::collections::HashMap;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::reconcile::OUTPUT_CHANNELS;
use crate::subprocess;

/// ISO octave centres. Band 0 is a low shelf and band 9 a high shelf; the
/// eight between are peaking filters.
pub const EQ_FREQS: [f32; 10] = [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];
pub const GAIN_MIN_DB: f32 = -12.0;
pub const GAIN_MAX_DB: f32 = 12.0;

/// Smart Volume strength: an RMS compressor with makeup gain that pulls
/// quiet and loud content toward one level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SmartLevel {
    #[default]
    Off,
    Light,
    Medium,
    Strong,
}

/// swh-plugins' stereo SC4 (the mic chain uses its mono sibling).
pub const SMART_PLUGIN: &str = "sc4_1882.so";

/// SC4 controls for a level, by port name. SC4's threshold bottoms out at
/// -30 dB, which bounds how strong "Strong" can be.
pub fn smart_controls(level: SmartLevel) -> Option<[(&'static str, f32); 7]> {
    let (thr, ratio, attack, release, knee, makeup) = match level {
        SmartLevel::Off => return None,
        SmartLevel::Light => (-18.0, 2.5, 20.0, 400.0, 6.0, 4.0),
        SmartLevel::Medium => (-24.0, 4.0, 10.0, 300.0, 6.0, 8.0),
        SmartLevel::Strong => (-30.0, 8.0, 5.0, 250.0, 8.0, 13.0),
    };
    Some([
        ("RMS/peak", 0.0),
        ("Attack time (ms)", attack),
        ("Release time (ms)", release),
        ("Threshold level (dB)", thr),
        ("Ratio (1:n)", ratio),
        ("Knee radius (dB)", knee),
        ("Makeup gain (dB)", makeup),
    ])
}

/// One channel's EQ as the user set it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EqState {
    pub enabled: bool,
    /// Gain per band in dB; missing bands are 0, extras ignored.
    pub bands: Vec<f32>,
    /// Gain before the bands, in dB.
    pub preamp: f32,
}

impl Default for EqState {
    fn default() -> Self {
        EqState { enabled: false, bands: vec![0.0; EQ_FREQS.len()], preamp: 0.0 }
    }
}

impl EqState {
    /// Bands padded/truncated to 10 and every gain clamped to the range.
    pub fn normalized(&self) -> EqState {
        let mut bands: Vec<f32> = self.bands.iter().take(EQ_FREQS.len()).map(|g| clamp(*g)).collect();
        bands.resize(EQ_FREQS.len(), 0.0);
        EqState { enabled: self.enabled, bands, preamp: clamp(self.preamp) }
    }
}

fn clamp(g: f32) -> f32 {
    if g.is_finite() {
        g.clamp(GAIN_MIN_DB, GAIN_MAX_DB)
    } else {
        0.0
    }
}

/// The node whose input ports take the channel's monitor (and which holds
/// the filter controls).
pub fn input_node(channel: &str) -> String {
    format!("opengg_eq_{}_in", channel.to_lowercase())
}

/// The node whose output ports play to the device.
pub fn output_node(channel: &str) -> String {
    format!("opengg_eq_{}", channel.to_lowercase())
}

/// Linear gain for a dB value.
fn db_to_mult(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Linear gain for a pactl volume (0.0–1.5, where 1.0 is 100%): PipeWire's
/// pulse layer maps percentages cubically (50% = -18 dB, measured), so the
/// fader has to as well or the Personal level would jump when a chain
/// takes over the path.
pub fn fader_mult(volume: f32, muted: bool) -> f32 {
    if muted {
        0.0
    } else {
        volume.clamp(0.0, 1.5).powi(3)
    }
}

/// The EQ gains actually applied: the user's, or flat while the EQ is off
/// and the chain runs only for Smart Volume.
fn effective(state: &EqState) -> EqState {
    let s = state.normalized();
    if s.enabled {
        s
    } else {
        EqState { enabled: false, bands: vec![0.0; EQ_FREQS.len()], preamp: 0.0 }
    }
}

/// Build the filter-chain config for one channel. `smart_plugin` is the
/// SC4 path when Smart Volume is on (`None` leaves the stage out);
/// `fader` is the initial Personal level as a linear gain.
pub fn build_config(
    channel: &str,
    state: &EqState,
    smart: SmartLevel,
    smart_plugin: Option<&std::path::Path>,
    fader: f32,
) -> String {
    let s = effective(state);
    let mut nodes = String::new();
    let mut links = String::new();
    for side in ["L", "R"] {
        nodes.push_str(&format!(
            "          {{ type = builtin name = pre{side} label = linear control = {{ \"Mult\" = {:.6} \"Add\" = 0.0 }} }}\n",
            db_to_mult(s.preamp)
        ));
        links.push_str(&format!("          {{ output = \"pre{side}:Out\" input = \"eq{side}0:In\" }}\n"));
        for (i, freq) in EQ_FREQS.iter().enumerate() {
            let label = match i {
                0 => "bq_lowshelf",
                9 => "bq_highshelf",
                _ => "bq_peaking",
            };
            nodes.push_str(&format!(
                "          {{ type = builtin name = eq{side}{i} label = {label} \
                 control = {{ \"Freq\" = {freq:.1} \"Q\" = 1.0 \"Gain\" = {:.4} }} }}\n",
                s.bands[i]
            ));
            if i > 0 {
                links.push_str(&format!(
                    "          {{ output = \"eq{side}{}:Out\" input = \"eq{side}{i}:In\" }}\n",
                    i - 1
                ));
            }
        }
        nodes.push_str(&format!(
            "          {{ type = builtin name = fader{side} label = linear control = {{ \"Mult\" = {fader:.6} \"Add\" = 0.0 }} }}\n"
        ));
    }
    match (smart_controls(smart), smart_plugin) {
        (Some(ctl), Some(path)) => {
            let ctl: Vec<String> = ctl.iter().map(|(k, v)| format!("\"{k}\" = {v:.3}")).collect();
            nodes.push_str(&format!(
                "          {{ type = ladspa name = smart plugin = \"{}\" label = sc4 control = {{ {} }} }}\n",
                path.display(),
                ctl.join(" ")
            ));
            links.push_str(concat!(
                "          { output = \"eqL9:Out\" input = \"smart:Left input\" }\n",
                "          { output = \"eqR9:Out\" input = \"smart:Right input\" }\n",
                "          { output = \"smart:Left output\" input = \"faderL:In\" }\n",
                "          { output = \"smart:Right output\" input = \"faderR:In\" }\n",
            ));
        }
        _ => {
            links.push_str("          { output = \"eqL9:Out\" input = \"faderL:In\" }\n");
            links.push_str("          { output = \"eqR9:Out\" input = \"faderR:In\" }\n");
        }
    }
    let (inp, out) = (input_node(channel), output_node(channel));
    format!(
        r#"# Generated by openggd — FX chain (EQ, Smart Volume, Personal fader) for the {channel} channel. Do not edit.
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
      node.description = "OpenGG FX {channel}"
      media.name       = "OpenGG FX {channel}"
      filter.graph = {{
        nodes = [
{nodes}        ]
        links = [
{links}        ]
        inputs  = [ "preL:In" "preR:In" ]
        outputs = [ "faderL:Out" "faderR:Out" ]
      }}
      audio.channels = 2
      audio.position = [ FL FR ]
      capture.props  = {{ node.name = "{inp}" node.autoconnect = false media.class = Stream/Input/Audio }}
      playback.props = {{ node.name = "{out}" node.autoconnect = false media.class = Stream/Output/Audio }}
    }}
  }}
]
"#
    )
}

/// The `pw-cli set-param` Props payloads for a state: one per EQ side, plus
/// one for the Smart Volume controls when that stage is present. Split
/// because pw-cli builds the parameter object in a fixed ~1 KiB buffer: the
/// whole set in one payload (measured ~1.3 KiB) overflows it, and pw-cli
/// then sends an *empty* object and still exits 0.
pub fn props_payloads(state: &EqState, smart: SmartLevel) -> Vec<String> {
    let s = effective(state);
    let mut out = Vec::new();
    for side in ["L", "R"] {
        let mut parts = vec![format!("\"pre{side}:Mult\" {:.6}", db_to_mult(s.preamp))];
        for (i, g) in s.bands.iter().enumerate() {
            parts.push(format!("\"eq{side}{i}:Gain\" {g:.4}"));
        }
        out.push(format!("{{ params = [ {} ] }}", parts.join(" ")));
    }
    if let Some(ctl) = smart_controls(smart) {
        let parts: Vec<String> = ctl.iter().map(|(k, v)| format!("\"smart:{k}\" {v:.3}")).collect();
        out.push(format!("{{ params = [ {} ] }}", parts.join(" ")));
    }
    out
}

/// The Props payload that sets the Personal fader.
pub fn fader_payload(mult: f32) -> String {
    format!("{{ params = [ \"faderL:Mult\" {mult:.6} \"faderR:Mult\" {mult:.6} ] }}")
}

/// Where the Smart Volume plugin is, or why it can't be used.
pub fn smart_availability() -> Result<std::path::PathBuf, String> {
    availability()?;
    super::micfx::find_plugin(SMART_PLUGIN)
        .ok_or_else(|| "Smart Volume needs swh-plugins (sc4_1882.so) — install swh-plugins".to_string())
}

/// Whether this system can host the EQ: `Ok` or a reason to show the user.
pub fn availability() -> Result<(), String> {
    if !subprocess::is_available("pipewire") {
        return Err("pipewire is not on PATH".into());
    }
    if !subprocess::is_available("pw-cli") {
        return Err("pw-cli is not on PATH (install the PipeWire tools package)".into());
    }
    if module_dirs().iter().any(|d| d.join("libpipewire-module-filter-chain.so").exists()) {
        Ok(())
    } else {
        Err("PipeWire's filter-chain module is missing".into())
    }
}

/// Where PipeWire modules can live, including Debian/Ubuntu's multiarch
/// directories (`/usr/lib/x86_64-linux-gnu/pipewire-0.3`), which the old
/// check missed — it reported the module missing on those systems.
fn module_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = ["/usr/lib", "/usr/lib64", "/usr/local/lib"]
        .iter()
        .map(|d| PathBuf::from(d).join("pipewire-0.3"))
        .collect();
    if let Ok(rd) = std::fs::read_dir("/usr/lib") {
        for e in rd.flatten() {
            let p = e.path().join("pipewire-0.3");
            if p.is_dir() {
                dirs.push(p);
            }
        }
    }
    dirs
}

/// Write `conf` and run it in its own `pipewire -c` process, which dies
/// with the daemon. Shared by the EQ and the mic chain.
pub(crate) fn spawn_chain(name: &str, conf: &str) -> Result<Child, String> {
    let dir = dirs::runtime_dir().unwrap_or_else(std::env::temp_dir).join("opengg");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{name}.conf"));
    std::fs::write(&path, conf).map_err(|e| format!("write {}: {e}", path.display()))?;
    let mut cmd = Command::new("pipewire");
    cmd.arg("-c").arg(&path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // SAFETY: prctl is async-signal-safe; this runs in the forked child
    // before exec and only asks the kernel to SIGTERM it when the daemon
    // dies, so a crashed daemon never leaves orphan chains whose node names
    // collide with the next daemon's.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    cmd.spawn().map_err(|e| format!("spawn pipewire: {e}"))
}

struct Running {
    child: Child,
    /// Whether the graph has the Smart Volume stage (changing that needs a
    /// restart; everything else is a live parameter change).
    smart_stage: bool,
}

/// Owns the FX chain processes. `desired`/`smart` are the user's intent;
/// `running` what exists. [`EqHost::ensure`] makes the second match the
/// first.
#[derive(Default)]
pub struct EqHost {
    desired: Mutex<HashMap<String, EqState>>,
    smart: Mutex<HashMap<String, SmartLevel>>,
    running: Mutex<HashMap<String, Running>>,
    seeded: Mutex<bool>,
}

impl EqHost {
    /// Seed from saved settings, once. Later changes come through
    /// [`EqHost::set`] / [`EqHost::set_smart`], so a settings file the UI
    /// has not flushed yet can never undo them.
    pub fn seed(&self, saved: &HashMap<String, EqState>, smart: &HashMap<String, SmartLevel>) {
        let mut seeded = self.seeded.lock().unwrap();
        if *seeded {
            return;
        }
        *seeded = true;
        let mut d = self.desired.lock().unwrap();
        for (ch, st) in saved {
            if OUTPUT_CHANNELS.contains(&ch.as_str()) {
                d.entry(ch.clone()).or_insert_with(|| st.normalized());
            }
        }
        let mut sm = self.smart.lock().unwrap();
        for (ch, lvl) in smart {
            if OUTPUT_CHANNELS.contains(&ch.as_str()) {
                sm.entry(ch.clone()).or_insert(*lvl);
            }
        }
    }

    fn eq_of(&self, channel: &str) -> EqState {
        self.desired.lock().unwrap().get(channel).cloned().unwrap_or_default()
    }

    pub fn smart_of(&self, channel: &str) -> SmartLevel {
        self.smart.lock().unwrap().get(channel).copied().unwrap_or_default()
    }

    /// True when the channel should have a chain: EQ on or Smart Volume on.
    pub fn wants_chain(&self, channel: &str) -> bool {
        self.eq_of(channel).enabled || self.smart_of(channel) != SmartLevel::Off
    }

    /// Apply a user EQ change. A running chain gets the new gains in place;
    /// the chain starts or stops as the channel starts or stops needing one.
    pub fn set(&self, channel: &str, state: &EqState) -> Result<(), String> {
        if !OUTPUT_CHANNELS.contains(&channel) {
            return Err(format!("no EQ for channel {channel:?}"));
        }
        *self.seeded.lock().unwrap() = true;
        let state = state.normalized();
        if state.enabled {
            availability()?;
        }
        self.desired.lock().unwrap().insert(channel.to_string(), state);
        self.converge(channel)
    }

    /// Apply a user Smart Volume change.
    pub fn set_smart(&self, channel: &str, level: SmartLevel) -> Result<(), String> {
        if !OUTPUT_CHANNELS.contains(&channel) {
            return Err(format!("no Smart Volume for channel {channel:?}"));
        }
        *self.seeded.lock().unwrap() = true;
        if level != SmartLevel::Off {
            smart_availability()?;
        }
        self.smart.lock().unwrap().insert(channel.to_string(), level);
        self.converge(channel)
    }

    /// Bring one channel's chain in line with its desired state.
    fn converge(&self, channel: &str) -> Result<(), String> {
        if !self.wants_chain(channel) {
            self.stop(channel);
            return Ok(());
        }
        let smart = self.smart_of(channel);
        let stage = smart != SmartLevel::Off;
        let running_stage = self.running.lock().unwrap().get(channel).map(|r| r.smart_stage);
        match running_stage {
            Some(had) if had == stage && self.is_running(channel) => {
                for payload in props_payloads(&self.eq_of(channel), smart) {
                    set_params(&input_node(channel), &payload)?;
                }
                Ok(())
            }
            Some(_) => {
                self.stop(channel);
                self.start(channel)
            }
            None => self.start(channel),
        }
    }

    /// Start chains that should run and are not running (never started, or
    /// exited — e.g. PipeWire itself restarted), and stop chains no longer
    /// wanted. Returns the channels with a live chain.
    pub fn ensure(&self) -> Vec<String> {
        for ch in OUTPUT_CHANNELS {
            if self.wants_chain(ch) && !self.is_running(ch) {
                if let Err(e) = availability().and_then(|_| self.start(ch)) {
                    tracing::warn!("FX chain for {ch} could not start: {e}");
                }
            }
        }
        let mut live: Vec<String> = self.running.lock().unwrap().keys().cloned().collect();
        live.sort();
        live
    }

    /// True if a chain process for `channel` is alive (reaping it if not).
    pub fn is_running(&self, channel: &str) -> bool {
        let mut running = self.running.lock().unwrap();
        let alive = match running.get_mut(channel) {
            Some(r) => matches!(r.child.try_wait(), Ok(None)),
            None => return false,
        };
        if !alive {
            running.remove(channel);
            tracing::warn!("FX chain for {channel} exited");
        }
        alive
    }

    /// True if any chain that should be running is not.
    pub fn needs_attention(&self) -> bool {
        OUTPUT_CHANNELS.iter().any(|c| self.wants_chain(c) && !self.is_running(c))
    }

    /// Set a running chain's Personal fader (linear gain).
    pub fn set_fader(&self, channel: &str, mult: f32) -> Result<(), String> {
        set_params(&input_node(channel), &fader_payload(mult))
    }

    fn start(&self, channel: &str) -> Result<(), String> {
        let smart = self.smart_of(channel);
        let plugin = if smart == SmartLevel::Off {
            None
        } else {
            match smart_availability() {
                Ok(p) => Some(p),
                Err(e) => {
                    tracing::warn!("Smart Volume for {channel} unavailable: {e}");
                    None
                }
            }
        };
        // Start at the channel's current level so taking over the path from
        // the sink's own volume never jumps.
        let (vol, muted) = super::sinks::read_live_sink_state()
            .get(&format!("OpenGG_{channel}"))
            .copied()
            .unwrap_or((1.0, false));
        let conf = build_config(channel, &self.eq_of(channel), smart, plugin.as_deref(), fader_mult(vol, muted));
        let child = spawn_chain(&format!("fx-{}", channel.to_lowercase()), &conf)?;
        tracing::info!("FX chain for {channel} started (pid {}, smart volume {smart:?})", child.id());
        self.running
            .lock()
            .unwrap()
            .insert(channel.to_string(), Running { child, smart_stage: plugin.is_some() });
        Ok(())
    }

    fn stop(&self, channel: &str) {
        if let Some(mut r) = self.running.lock().unwrap().remove(channel) {
            let _ = r.child.kill();
            let _ = r.child.wait();
            tracing::info!("FX chain for {channel} stopped");
        }
    }

    /// `{"available", "reason", "smartAvailable", "smartReason", "channels":
    /// {ch: {enabled, bands, preamp, smart, running}}}`
    pub fn status_json(&self) -> String {
        let desired = self.desired.lock().unwrap().clone();
        let mut channels = serde_json::Map::new();
        for ch in OUTPUT_CHANNELS {
            let st = desired.get(*ch).cloned().unwrap_or_default();
            let mut v = serde_json::to_value(&st).unwrap_or_default();
            v["smart"] = serde_json::json!(self.smart_of(ch));
            v["running"] = serde_json::json!(self.is_running(ch));
            channels.insert(ch.to_string(), v);
        }
        let avail = availability();
        let smart = smart_availability();
        serde_json::json!({
            "available": avail.is_ok(),
            "reason": avail.err().unwrap_or_default(),
            "smartAvailable": smart.is_ok(),
            "smartReason": smart.err().unwrap_or_default(),
            "channels": channels,
        })
        .to_string()
    }
}

impl Drop for EqHost {
    fn drop(&mut self) {
        if let Ok(mut running) = self.running.lock() {
            for (_, mut r) in running.drain() {
                let _ = r.child.kill();
                let _ = r.child.wait();
            }
        }
    }
}

/// `pw-cli set-param <node> Props <payload>`, by node name. A chain just
/// started takes a moment to appear, so a miss is retried briefly.
pub(crate) fn set_params(node: &str, payload: &str) -> Result<(), String> {
    let mut last = String::new();
    for _ in 0..10 {
        match subprocess::command("pw-cli").args(["set-param", node, "Props", payload]).output() {
            // An 8-byte object is an empty one: pw-cli could not build the
            // payload (see props_payloads) yet exits 0 — a silent no-op.
            Ok(o) if o.status.success() && String::from_utf8_lossy(&o.stdout).contains("Object: size 8,") => {
                return Err(format!("pw-cli could not build the parameters for {node} (payload too large?)"));
            }
            Ok(o) if o.status.success() && !String::from_utf8_lossy(&o.stderr).contains("unknown") => return Ok(()),
            Ok(o) => last = String::from_utf8_lossy(&o.stderr).trim().to_string(),
            Err(e) => last = e.to_string(),
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err(format!("could not set parameters on {node}: {last}"))
}

/// `mixer.eq` from ui-settings.json.
pub fn parse_saved(v: &serde_json::Value) -> HashMap<String, EqState> {
    v.as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), serde_json::from_value::<EqState>(v.clone()).ok()?.normalized())))
                .collect()
        })
        .unwrap_or_default()
}

/// `mixer.smartVolume` from ui-settings.json: `{channel: "off"|"light"|…}`.
pub fn parse_saved_smart(v: &serde_json::Value) -> HashMap<String, SmartLevel> {
    v.as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), serde_json::from_value::<SmartLevel>(v.clone()).ok()?)))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn config_is_an_explicit_stereo_chain_with_preamp_fader_and_no_autoconnect() {
        let st = EqState { enabled: true, bands: vec![1.0, 2.0, 3.0], preamp: -6.0 };
        let cfg = build_config("Game", &st, SmartLevel::Off, None, 0.5);
        for side in ["L", "R"] {
            for i in 0..10 {
                assert!(cfg.contains(&format!("name = eq{side}{i} ")), "missing band {side}{i}");
            }
            assert!(cfg.contains(&format!("name = fader{side} label = linear control = {{ \"Mult\" = 0.500000")));
            assert!(cfg.contains(&format!("{{ output = \"eq{side}9:Out\" input = \"fader{side}:In\" }}")));
        }
        assert_eq!(cfg.matches("bq_peaking").count(), 16);
        // pre→eq0 + 9 band links + eq9→fader, per side
        assert_eq!(cfg.matches("output = \"").count(), 22);
        assert!(cfg.contains("\"Gain\" = 3.0000"));
        assert!(cfg.contains("\"Mult\" = 0.501187"), "-6 dB preamp");
        assert!(cfg.contains("inputs  = [ \"preL:In\" \"preR:In\" ]"));
        assert!(cfg.contains("outputs = [ \"faderL:Out\" \"faderR:Out\" ]"));
        assert_eq!(cfg.matches("node.autoconnect = false").count(), 2);
        assert!(cfg.contains("node.name = \"opengg_eq_game_in\""));
        assert!(cfg.contains("node.name = \"opengg_eq_game\""));
        assert!(!cfg.contains("ladspa"));
    }

    #[test]
    fn smart_volume_inserts_one_stereo_sc4_between_eq_and_fader() {
        let st = EqState::default(); // EQ off: bands flat
        let cfg = build_config("Media", &st, SmartLevel::Medium, Some(Path::new("/lib/ladspa/sc4_1882.so")), 1.0);
        assert_eq!(cfg.matches("type = ladspa").count(), 1);
        assert!(cfg.contains("plugin = \"/lib/ladspa/sc4_1882.so\" label = sc4"));
        assert!(cfg.contains("\"Threshold level (dB)\" = -24.000"));
        assert!(cfg.contains("{ output = \"eqL9:Out\" input = \"smart:Left input\" }"));
        assert!(cfg.contains("{ output = \"smart:Right output\" input = \"faderR:In\" }"));
        assert!(!cfg.contains("input = \"faderL:In\" }\n          { output = \"eqL9"), "no bypass link");
        assert!(!cfg.contains("{ output = \"eqL9:Out\" input = \"faderL:In\" }"));
        // EQ off means flat bands even if gains were saved.
        let saved = EqState { enabled: false, bands: vec![9.0; 10], preamp: 6.0 };
        let cfg = build_config("Media", &saved, SmartLevel::Light, Some(Path::new("/x.so")), 1.0);
        assert!(!cfg.contains("\"Gain\" = 9.0000"));
        // Smart requested but plugin missing: no stage, direct link.
        let cfg = build_config("Media", &st, SmartLevel::Strong, None, 1.0);
        assert!(!cfg.contains("ladspa"));
        assert!(cfg.contains("{ output = \"eqL9:Out\" input = \"faderL:In\" }"));
    }

    #[test]
    fn smart_levels_stay_inside_sc4_ranges() {
        assert!(smart_controls(SmartLevel::Off).is_none());
        for lvl in [SmartLevel::Light, SmartLevel::Medium, SmartLevel::Strong] {
            let c: HashMap<&str, f32> = smart_controls(lvl).unwrap().into_iter().collect();
            assert!((-30.0..=0.0).contains(&c["Threshold level (dB)"]));
            assert!((1.0..=20.0).contains(&c["Ratio (1:n)"]));
            assert!((1.5..=400.0).contains(&c["Attack time (ms)"]));
            assert!((2.0..=800.0).contains(&c["Release time (ms)"]));
            assert!((1.0..=10.0).contains(&c["Knee radius (dB)"]));
            assert!((0.0..=24.0).contains(&c["Makeup gain (dB)"]));
        }
        let s: SmartLevel = serde_json::from_str("\"medium\"").unwrap();
        assert_eq!(s, SmartLevel::Medium);
    }

    #[test]
    fn fader_follows_pactls_cubic_volume() {
        assert_eq!(fader_mult(1.0, false), 1.0);
        assert!((fader_mult(0.5, false) - 0.125).abs() < 1e-6, "50% = -18 dB");
        assert_eq!(fader_mult(0.8, true), 0.0);
        assert!((fader_mult(9.0, false) - 3.375).abs() < 1e-6, "capped at 150%");
        assert_eq!(fader_payload(0.25), "{ params = [ \"faderL:Mult\" 0.250000 \"faderR:Mult\" 0.250000 ] }");
    }

    #[test]
    fn state_is_normalized() {
        let st = EqState { enabled: true, bands: vec![99.0, -99.0, f32::NAN], preamp: 40.0 }.normalized();
        assert_eq!(st.bands.len(), 10);
        assert_eq!(&st.bands[..3], &[12.0, -12.0, 0.0]);
        assert_eq!(st.preamp, 12.0);
        let long = EqState { bands: vec![1.0; 14], ..Default::default() }.normalized();
        assert_eq!(long.bands.len(), 10);
    }

    #[test]
    fn payloads_set_every_band_on_both_sides_and_smart_controls() {
        let p = props_payloads(&EqState { enabled: true, bands: vec![0.5; 10], preamp: 0.0 }, SmartLevel::Off);
        assert_eq!(p.len(), 2);
        assert!(p[0].starts_with("{ params = [ \"preL:Mult\" 1.000000"));
        assert!(p[1].starts_with("{ params = [ \"preR:Mult\" 1.000000"));
        assert_eq!(p.iter().map(|x| x.matches(":Gain\" 0.5000").count()).sum::<usize>(), 20);
        let p = props_payloads(&EqState::default(), SmartLevel::Strong);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].matches(":Gain\" 0.0000").count(), 10, "EQ off is flat");
        assert!(p[2].contains("\"smart:Makeup gain (dB)\" 13.000"));
    }

    #[test]
    fn every_payload_fits_pw_clis_buffer() {
        // pw-cli silently sends an empty object past ~1 KiB of pod; each
        // text payload must stay well under that (pod size ≈ 1.1× text).
        let st = EqState { enabled: true, bands: vec![-11.5; 10], preamp: -11.5 };
        for p in props_payloads(&st, SmartLevel::Strong) {
            assert!(p.len() < 600, "payload too large for pw-cli: {} bytes", p.len());
        }
    }

    #[test]
    fn saved_settings_parse_and_ignore_junk() {
        let v = serde_json::json!({
            "Game": {"enabled": true, "bands": [3, 2], "preamp": -2},
            "Chat": {"enabled": false},
            "Bad": 5
        });
        let m = parse_saved(&v);
        assert_eq!(m.len(), 2);
        assert!(m["Game"].enabled && m["Game"].bands[0] == 3.0 && m["Game"].bands.len() == 10);
        assert!(!m["Chat"].enabled);
        let sm = parse_saved_smart(&serde_json::json!({"Media": "strong", "Chat": "loud", "Aux": 3}));
        assert_eq!(sm.len(), 1);
        assert_eq!(sm["Media"], SmartLevel::Strong);
    }

    #[test]
    fn host_rejects_unknown_channels_and_seeds_once() {
        let h = EqHost::default();
        assert!(h.set("Mic", &EqState::default()).is_err());
        assert!(h.set_smart("Mic", SmartLevel::Light).is_err());
        let mut saved = HashMap::new();
        saved.insert("Game".to_string(), EqState { bands: vec![4.0], ..Default::default() });
        saved.insert("Nope".to_string(), EqState::default());
        let mut smart = HashMap::new();
        smart.insert("Chat".to_string(), SmartLevel::Light);
        h.seed(&saved, &smart);
        h.seed(&HashMap::new(), &HashMap::new());
        let s: serde_json::Value = serde_json::from_str(&h.status_json()).unwrap();
        assert_eq!(s["channels"]["Game"]["bands"][0], 4.0);
        assert_eq!(s["channels"]["Chat"]["smart"], "light");
        assert_eq!(s["channels"]["Game"]["smart"], "off");
        assert!(s["channels"].get("Nope").is_none());
        assert!(h.wants_chain("Chat"));
        assert!(!h.wants_chain("Game"), "EQ saved but off");
    }
}

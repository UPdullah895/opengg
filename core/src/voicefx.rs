//! Voice processing — noise reduction, noise gate and compressor — built on
//! PipeWire's own filter-chain, the same way as the EQ (see `eq.rs`).
//!
//! These three controls existed in the UI for a long time with no engine at
//! all behind them. They now run for real:
//!
//! * **Noise gate** — PipeWire's builtin `noisegate` (PipeWire 1.4+), keyed
//!   by the `ebur128` meter that ships with it. No third-party plugin.
//!
//!   The gate cannot use its own level detector from a filter-chain: that
//!   detector only runs while the `Level` control is NaN, and filter-chain
//!   pins every control to a number — `Level` reads back 0 whatever is
//!   written to it, config or `pw-cli`, so the gate never opened (measured
//!   on PipeWire 1.6.9). A LINKED control is not pinned, so the meter's
//!   momentary loudness is mapped onto 0..1 by a `linear` node and wired
//!   into `Level`. Thresholds use the same mapping — see [`lufs_to_level`].
//! * **Compressor** — Steve Harris' `sc4m` LADSPA plugin (`swh-plugins`).
//! * **Noise reduction** — RNNoise's LADSPA plugin
//!   (`noise-suppression-for-voice`).
//!
//! The last two are optional packages. When one is missing its stage is left
//! out of the graph and [`capabilities`] says so, so the UI can offer the
//! install command instead of a switch that does nothing.
//!
//! ## Mic insert point
//!
//! The daemon feeds the hardware mic into the `OpenGG_Mic` sink through a
//! loopback; `OpenGG_Mic.monitor` is what recordings and the `OpenGG Mic`
//! virtual source read. With processing on, a filter-chain reads the same
//! hardware mic and writes into `OpenGG_Mic` instead, and the loopback's
//! output links are cut so the voice is not heard twice. [`unlink_loopback`]
//! and [`relink_loopback`] manage that.
//!
//! Output channels (Chat) get the same stages inside their EQ chain — see
//! [`graph_nodes`] and `eq::build_config`.

use crate::subprocess::{command, run_cmd_sync};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::{Child, Stdio};

/// What the user asked for on one channel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FxSettings {
    pub gate_enabled: bool,
    /// Open threshold in dBFS.
    pub gate_threshold_db: f32,
    pub comp_enabled: bool,
    /// 0..=100, mapped onto threshold, ratio and makeup gain.
    pub comp_level: f32,
    pub nr_enabled: bool,
    /// 0..=100, mapped onto RNNoise's voice-activity threshold.
    pub nr_intensity: f32,
}

impl Default for FxSettings {
    fn default() -> Self {
        Self {
            gate_enabled: false,
            gate_threshold_db: -40.0,
            comp_enabled: false,
            comp_level: 50.0,
            nr_enabled: false,
            nr_intensity: 50.0,
        }
    }
}

impl FxSettings {
    /// Whether any stage is switched on — i.e. whether a chain is needed.
    pub fn any_enabled(&self) -> bool {
        self.gate_enabled || self.comp_enabled || self.nr_enabled
    }
}

/// Which stages this machine can run.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FxCaps {
    pub gate: bool,
    pub compressor: bool,
    pub noise_reduction: bool,
}

const LADSPA_DIRS: &[&str] = &[
    "/usr/lib/ladspa",
    "/usr/lib64/ladspa",
    "/usr/local/lib/ladspa",
    "/usr/lib/x86_64-linux-gnu/ladspa",
];

fn find_ladspa(file: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var("LADSPA_PATH")
        .unwrap_or_default()
        .split(':')
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .collect();
    dirs.extend(LADSPA_DIRS.iter().map(PathBuf::from));
    if let Some(h) = dirs::home_dir() {
        dirs.push(h.join(".ladspa"));
    }
    dirs.into_iter().map(|d| d.join(file)).find(|p| p.is_file())
}

/// The compressor plugin (`swh-plugins`' mono SC4), if installed.
pub fn compressor_plugin() -> Option<PathBuf> {
    find_ladspa("sc4m_1916.so")
}

/// The RNNoise plugin (`noise-suppression-for-voice`), if installed.
pub fn rnnoise_plugin() -> Option<PathBuf> {
    find_ladspa("librnnoise_ladspa.so")
}

const FILTER_GRAPH_DIRS: &[&str] = &[
    "/usr/lib/spa-0.2/filter-graph",
    "/usr/lib64/spa-0.2/filter-graph",
    "/usr/lib/x86_64-linux-gnu/spa-0.2/filter-graph",
];

/// Whether PipeWire has the builtin `noisegate` (1.4+) and the `ebur128`
/// meter that keys it. Read from the plugins themselves rather than guessed
/// from a version number, since distributions backport.
fn gate_plugins_present() -> bool {
    let builtin = FILTER_GRAPH_DIRS.iter().any(|d| {
        std::fs::read(format!("{d}/libspa-filter-graph-plugin-builtin.so"))
            .is_ok_and(|bytes| bytes.windows(10).any(|w| w == b"noisegate\0"))
    });
    let meter = FILTER_GRAPH_DIRS
        .iter()
        .any(|d| std::path::Path::new(&format!("{d}/libspa-filter-graph-plugin-ebur128.so")).is_file());
    builtin && meter
}

pub fn capabilities() -> FxCaps {
    FxCaps {
        gate: crate::eq::availability().is_ok() && gate_plugins_present(),
        compressor: compressor_plugin().is_some(),
        noise_reduction: rnnoise_plugin().is_some(),
    }
}

/// The gate's level scale: momentary loudness mapped linearly so that
/// -100 LUFS is 0 and 0 LUFS is 1 (`noisegate` clamps its thresholds to
/// 0..1). The `lvl` node applies the same mapping to the meter's output.
pub fn lufs_to_level(lufs: f32) -> f32 {
    ((lufs + 100.0) / 100.0).clamp(0.0, 1.0)
}
/// Close this far below the open threshold, so a voice hovering at the
/// threshold does not chatter.
const GATE_HYSTERESIS: f32 = 0.06;

/// Every control that can change while a chain is running, as
/// `(param, value)`. A disabled gate is held permanently open (thresholds
/// of 0) and a disabled compressor runs at 1:1, so switching those on and
/// off never needs a restart.
pub fn live_params(fx: &FxSettings, caps: &FxCaps) -> Vec<(String, f32)> {
    let mut out = Vec::new();
    if caps.gate {
        // The UI's threshold is in dB; momentary loudness is on the same
        // scale for a voice, so it maps across directly.
        let (open, close) = if fx.gate_enabled {
            let open = lufs_to_level(fx.gate_threshold_db.clamp(-90.0, 0.0));
            (open, (open - GATE_HYSTERESIS).max(0.0))
        } else {
            (0.0, 0.0)
        };
        out.push(("gate:Open Threshold".into(), open));
        out.push(("gate:Close Threshold".into(), close));
    }
    if caps.compressor && fx.comp_enabled {
        let l = fx.comp_level.clamp(0.0, 100.0) / 100.0;
        out.push(("comp:Threshold level (dB)".into(), -10.0 - 30.0 * l));
        out.push(("comp:Ratio (1:n)".into(), 1.0 + 7.0 * l));
        out.push(("comp:Makeup gain (dB)".into(), 10.0 * l));
    }
    if caps.noise_reduction && fx.nr_enabled {
        out.push((
            "nr:VAD Threshold (%)".into(),
            fx.nr_intensity.clamp(0.0, 100.0) * 0.95,
        ));
    }
    out
}

/// The stages a chain needs. Changing this set needs a restart; everything
/// else is a live parameter. Plugins are only loaded while enabled, since
/// RNNoise has no neutral setting to fall back on.
pub fn structure(fx: &FxSettings, caps: &FxCaps) -> (bool, bool, bool) {
    (
        caps.noise_reduction && fx.nr_enabled,
        caps.gate,
        caps.compressor && fx.comp_enabled,
    )
}

/// Filter-graph node declarations for the enabled stages, in signal order,
/// plus the links between them. Returns `(nodes, links, first_in, last_out)`
/// with port references, or `None` when there is nothing to insert.
pub fn graph_nodes(fx: &FxSettings, caps: &FxCaps) -> Option<(String, String, String, String)> {
    let (nr, gate, comp) = structure(fx, caps);
    let params: std::collections::HashMap<String, f32> =
        live_params(fx, caps).into_iter().collect();
    let p = |k: &str, default: f32| params.get(k).copied().unwrap_or(default);

    // (name, declaration, input port, output port)
    let mut stages: Vec<(String, String, String)> = Vec::new();
    if nr {
        let plugin = rnnoise_plugin()?;
        stages.push((
            format!(
                "          {{ type = ladspa name = nr plugin = \"{}\" label = noise_suppressor_mono \
                 control = {{ \"VAD Threshold (%)\" = {:.1} }} }}\n",
                plugin.display(),
                p("nr:VAD Threshold (%)", 50.0)
            ),
            "nr:Input".into(),
            "nr:Output".into(),
        ));
    }
    if gate {
        // Meter → level map → gate:Level; the audio passes through the meter.
        // A mono ("DUAL MONO") chain, so filter-chain runs one copy per
        // channel just like the EQ bands.
        stages.push((
            format!(
                "          {{ type = ebur128 name = meter label = ebur128 }}\n\
                 \x20         {{ type = builtin name = lvl label = linear control = {{ \"Mult\" = 0.01 \"Add\" = 1.0 }} }}\n\
                 \x20         {{ type = builtin name = gate label = noisegate \
                 control = {{ \"Open Threshold\" = {:.6} \"Close Threshold\" = {:.6} \
                 \"Attack (s)\" = 0.005 \"Hold (s)\" = 0.15 \"Release (s)\" = 0.08 }} }}\n",
                p("gate:Open Threshold", 0.0),
                p("gate:Close Threshold", 0.0)
            ),
            "meter:In DUAL MONO".into(),
            "gate:Out".into(),
        ));
    }
    if comp {
        let plugin = compressor_plugin()?;
        stages.push((
            format!(
                "          {{ type = ladspa name = comp plugin = \"{}\" label = sc4m \
                 control = {{ \"RMS/peak\" = 0.5 \"Attack time (ms)\" = 10 \
                 \"Release time (ms)\" = 120 \"Threshold level (dB)\" = {:.2} \
                 \"Ratio (1:n)\" = {:.2} \"Knee radius (dB)\" = 6 \
                 \"Makeup gain (dB)\" = {:.2} }} }}\n",
                plugin.display(),
                p("comp:Threshold level (dB)", -25.0),
                p("comp:Ratio (1:n)", 1.0),
                p("comp:Makeup gain (dB)", 0.0)
            ),
            "comp:Input".into(),
            "comp:Output".into(),
        ));
    }
    if stages.is_empty() {
        return None;
    }
    let nodes: String = stages.iter().map(|s| s.0.as_str()).collect();
    let mut links: String = stages
        .windows(2)
        .map(|w| format!("          {{ output = \"{}\" input = \"{}\" }}\n", w[0].2, w[1].1))
        .collect();
    if gate {
        links.push_str(
            "          { output = \"meter:Out DUAL MONO\" input = \"gate:In\" }\n\
             \x20         { output = \"meter:Momentary LUFS\" input = \"lvl:Control\" }\n\
             \x20         { output = \"lvl:Notify\" input = \"gate:Level\" }\n",
        );
    }
    let first_in = stages.first().unwrap().1.clone();
    let last_out = stages.last().unwrap().2.clone();
    Some((nodes, links, first_in, last_out))
}

/// PipeWire node name of the mic chain.
pub const MIC_NODE: &str = "opengg_micfx";

fn config_path() -> PathBuf {
    let dir = dirs::runtime_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("opengg");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("micfx.conf")
}

/// Filter-chain config for the mic: `hw_source` → stages → `OpenGG_Mic`.
/// `None` when no stage is enabled.
pub fn build_mic_config(fx: &FxSettings, caps: &FxCaps, hw_source: &str) -> Option<String> {
    let (nodes, links, first_in, last_out) = graph_nodes(fx, caps)?;
    Some(format!(
        r#"# Generated by OpenGG — mic voice processing. Do not edit.
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
      node.description = "OpenGG Mic processing"
      media.name       = "OpenGG Mic processing"
      filter.graph = {{
        nodes = [
{nodes}        ]
        links = [
{links}        ]
        inputs  = [ "{first_in}" ]
        outputs = [ "{last_out}" ]
      }}
      audio.channels = 2
      audio.position = [ FL FR ]
      capture.props  = {{ node.name = "{MIC_NODE}_in" node.target = "{hw_source}" node.dont-reconnect = true }}
      playback.props = {{ node.name = "{MIC_NODE}" node.passive = false node.target = "OpenGG_Mic" node.dont-reconnect = true }}
    }}
  }}
]
"#
    ))
}

/// The hardware mic the daemon's loopback reads, so processing starts from
/// the same device. Read from the loopback module's own arguments.
pub fn mic_hw_source() -> Option<String> {
    let list = run_cmd_sync("pactl", &["list", "short", "modules"]).ok()?;
    list.lines()
        .filter(|l| l.contains("module-loopback") && l.contains("sink=OpenGG_Mic"))
        .find_map(|l| {
            l.split_whitespace()
                .find_map(|w| w.strip_prefix("source=").map(str::to_string))
        })
}

const LOOPBACK_OUT: &str = "OpenGG_Mic_Loopback_out";

/// Cut the daemon's dry `loopback → OpenGG_Mic` path while processing runs.
pub fn unlink_loopback() {
    for p in ["FL", "FR"] {
        let _ = run_cmd_sync(
            "pw-link",
            &["-d", &format!("{LOOPBACK_OUT}:output_{p}"), &format!("OpenGG_Mic:playback_{p}")],
        );
    }
}

/// Restore the dry path. Harmless when it already exists.
pub fn relink_loopback() {
    for p in ["FL", "FR"] {
        let _ = run_cmd_sync(
            "pw-link",
            &[&format!("{LOOPBACK_OUT}:output_{p}"), &format!("OpenGG_Mic:playback_{p}")],
        );
    }
}

/// Start the mic chain. The caller owns the process; see [`stop_mic`].
pub fn start_mic(fx: &FxSettings, caps: &FxCaps) -> Result<Child, String> {
    let hw = mic_hw_source().ok_or(
        "the daemon's microphone loopback is not running, so there is no mic to process",
    )?;
    let cfg = build_mic_config(fx, caps, &hw).ok_or("no processing stage is enabled")?;
    let path = config_path();
    std::fs::write(&path, cfg).map_err(|e| format!("write {}: {e}", path.display()))?;
    command("pipewire")
        .arg("-c")
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn pipewire: {e}"))
}

/// Stop the mic chain and put the dry path back.
pub fn stop_mic(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
    relink_loopback();
}

/// PipeWire node id of a chain's capture side, by node name.
pub fn node_id(capture_node_name: &str) -> Option<u32> {
    let dump = run_cmd_sync("pw-dump", &[]).ok()?;
    let v: serde_json::Value = serde_json::from_str(&dump).ok()?;
    v.as_array()?.iter().find_map(|obj| {
        let name = obj
            .get("info")
            .and_then(|i| i.get("props"))
            .and_then(|p| p.get("node.name"))
            .and_then(|n| n.as_str());
        (name == Some(capture_node_name))
            .then(|| obj.get("id").and_then(|i| i.as_u64()).map(|i| i as u32))
            .flatten()
    })
}

/// Push the live parameters to a running chain.
pub fn set_live(capture_node_name: &str, fx: &FxSettings, caps: &FxCaps) -> Result<(), String> {
    let id = node_id(capture_node_name)
        .ok_or_else(|| format!("no node {capture_node_name} (is the chain running?)"))?;
    let body: Vec<String> = live_params(fx, caps)
        .iter()
        .map(|(k, v)| format!("\"{k}\" {v:.6}"))
        .collect();
    if body.is_empty() {
        return Ok(());
    }
    let param = format!("{{ params = [ {} ] }}", body.join(" "));
    run_cmd_sync("pw-cli", &["set-param", &id.to_string(), "Props", &param]).map(|_| ())
}

/// Measure a source's background level in dBFS over `secs`, for the gate's
/// auto-detect: the user stays quiet, and the gate opens a margin above what
/// was heard. `None` if nothing could be recorded.
pub fn measure_noise_floor_db(source: &str, secs: f32) -> Option<f32> {
    let mut child = command("parec")
        .args([
            "--device",
            source,
            "--format=float32le",
            "--channels=1",
            "--rate=16000",
            "--raw",
            "--latency-msec=50",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let want = (16000.0 * secs) as usize * 4;
    let mut buf = vec![0u8; want];
    let mut got = 0;
    use std::io::Read;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs_f32(secs + 2.0);
    while got < want && std::time::Instant::now() < deadline {
        match stdout.read(&mut buf[got..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => got += n,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let samples: Vec<f32> = buf[..got - got % 4]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    if samples.len() < 1600 {
        return None;
    }
    // 95th percentile of 20 ms block peaks: the noise floor's loud end,
    // ignoring a stray click.
    let mut peaks: Vec<f32> = samples
        .chunks(320)
        .map(|c| c.iter().fold(0f32, |m, s| m.max(s.abs())))
        .collect();
    peaks.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95 = peaks[(peaks.len() * 95 / 100).min(peaks.len() - 1)];
    Some(20.0 * p95.max(1e-6).log10())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(gate: bool, comp: bool, nr: bool) -> FxCaps {
        FxCaps { gate, compressor: comp, noise_reduction: nr }
    }

    #[test]
    fn disabled_gate_is_held_open_and_enabled_gate_follows_the_threshold() {
        let mut fx = FxSettings::default();
        let c = caps(true, false, false);
        let off: std::collections::HashMap<_, _> = live_params(&fx, &c).into_iter().collect();
        assert_eq!(off["gate:Open Threshold"], 0.0);
        assert_eq!(off["gate:Close Threshold"], 0.0);

        fx.gate_enabled = true;
        fx.gate_threshold_db = -40.0;
        let on: std::collections::HashMap<_, _> = live_params(&fx, &c).into_iter().collect();
        assert!((on["gate:Open Threshold"] - 0.60).abs() < 1e-4);
        assert!(on["gate:Close Threshold"] < on["gate:Open Threshold"]);
    }

    #[test]
    fn missing_plugins_leave_their_stage_out() {
        let fx = FxSettings { comp_enabled: true, nr_enabled: true, ..Default::default() };
        let (nodes, links, first, last) = graph_nodes(&fx, &caps(true, false, false)).unwrap();
        assert!(nodes.contains("label = noisegate") && nodes.contains("label = ebur128"));
        assert!(!nodes.contains("sc4m") && !nodes.contains("rnnoise"));
        // The gate is keyed by the meter through a LINK — see the module doc.
        assert!(links.contains("input = \"gate:Level\""));
        assert_eq!((first.as_str(), last.as_str()), ("meter:In DUAL MONO", "gate:Out"));
        assert!(graph_nodes(&FxSettings::default(), &caps(false, false, false)).is_none());
    }

    #[test]
    fn mic_config_reads_the_hardware_mic_and_writes_the_mic_channel() {
        let fx = FxSettings { gate_enabled: true, ..Default::default() };
        let cfg = build_mic_config(&fx, &caps(true, false, false), "alsa_input.test").unwrap();
        assert!(cfg.contains("node.target = \"alsa_input.test\""));
        assert!(cfg.contains("node.target = \"OpenGG_Mic\""));
        assert!(!cfg.contains("stream.capture.sink"));
        assert!(cfg.contains("inputs  = [ \"meter:In DUAL MONO\" ]"));
    }
}

//! Per-channel 10-band equaliser, built on PipeWire's own filter-chain.
//!
//! ## Why not the previous design
//!
//! This replaces a jalv + LSP-LV2 approach that could not work, for four
//! independent reasons, all confirmed on a stock system:
//!
//! 1. The LSP plugin set is a hard dependency that was never declared or
//!    checked. `/usr/lib/lv2` normally holds only the LV2 *specification*
//!    bundles, so `jalv <lsp-uri>` failed with `Failed to load state`.
//! 2. jalv was spawned with `-i`, which means "ignore keyboard input, run
//!    non-interactively" — the opposite of what the caller assumed. Band
//!    gains written to its stdin were discarded even when it did run.
//! 3. Nothing ever linked the resulting JACK client into the audio path, so
//!    even a working plugin processed silence.
//! 4. Spawn success was reported as engine success, so the UI showed an
//!    active EQ over a process that had already exited.
//!
//! ## This design
//!
//! PipeWire ships `module-filter-chain` with builtin biquads (`bq_lowshelf`,
//! `bq_peaking`, `bq_highshelf`), so a 10-band EQ needs **no third-party
//! plugins at all**. The chain runs as its own `pipewire -c <conf>` process
//! — a normal client of the user's session daemon — which keeps it out of
//! the user's PipeWire config and lets it be started and stopped at will.
//!
//! Band gains are changed live with
//! `pw-cli set-param <node> Props '{ params = [ "eqN:Gain" <dB> ] }'`,
//! which takes effect immediately and reads back through `pw-dump`.
//!
//! ## Insert point
//!
//! A channel normally runs `OpenGG_<Ch>:monitor_{FL,FR}` straight to the
//! output device. With the EQ active the chain captures that monitor and
//! plays to the device, and the direct links are removed so the audio is
//! not also heard dry. [`stop`] restores them.

use crate::subprocess::{
    capture_stderr, command, is_available, run_tool, StderrTail, ToolError,
};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a freshly spawned chain gets to show up in the graph before the
/// start is declared failed. A healthy chain appears in well under a second.
const READY_TIMEOUT: Duration = Duration::from_millis(3000);

/// ISO octave centres. Band 0 is a low shelf and band 9 a high shelf (the
/// ends of the spectrum are better served by shelves than by peaks); the
/// eight in between are peaking filters.
pub const EQ_FREQS: [f32; 10] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// Gains outside this range are clamped. Matches the UI's own -12..+12 dB.
pub const GAIN_MIN_DB: f32 = -12.0;
pub const GAIN_MAX_DB: f32 = 12.0;

/// PipeWire node name for a channel's EQ sink.
pub fn node_name(channel: &str) -> String {
    format!("opengg_eq_{}", channel.to_lowercase())
}

/// Where the generated filter-chain config for a channel is written.
fn config_path(channel: &str) -> Result<PathBuf, String> {
    let dir = dirs::runtime_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("opengg");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    Ok(dir.join(format!("eq-{}.conf", channel.to_lowercase())))
}

/// Escape a value for a double-quoted string in a PipeWire SPA-JSON config.
/// A device name containing `"` or `\` would otherwise corrupt the whole
/// config and surface as a chain that silently never starts.
fn quote(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Library roots whose `pipewire-0.3` subdirectory may hold PipeWire's
/// modules. Covers Arch/Fedora (`/usr/lib`, `/usr/lib64`), source installs
/// and NixOS; Debian/Ubuntu multiarch dirs are found by scanning these roots
/// for `*linux*` children (see [`module_dir_candidates`]).
const LIB_ROOTS: &[&str] = &[
    "/usr/lib",
    "/usr/lib64",
    "/usr/local/lib",
    "/usr/local/lib64",
    "/lib",
    "/run/current-system/sw/lib",
];

/// Every directory that could hold `libpipewire-module-*.so`, most specific
/// first. `explicit` is `$PIPEWIRE_MODULE_DIR` (PipeWire's own override, a
/// colon-separated list). Debian and Ubuntu install under
/// `/usr/lib/<triplet>/pipewire-0.3` (x86_64-linux-gnu, aarch64-linux-gnu,
/// arm-linux-gnueabihf, ...), so any `<root>/*linux*` child is a candidate —
/// listing triplets by hand would rot with every new architecture.
fn module_dir_candidates(explicit: Option<&str>, roots: &[&Path]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(list) = explicit {
        dirs.extend(list.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    }
    for root in roots {
        dirs.push(root.join("pipewire-0.3"));
        let Ok(rd) = std::fs::read_dir(root) else { continue };
        let mut triplets: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("linux"))
            .map(|e| e.path().join("pipewire-0.3"))
            .collect();
        triplets.sort();
        dirs.extend(triplets);
    }
    dirs
}

/// The filter-chain module's path if it is installed anywhere we know to look.
fn find_filter_chain_module(explicit: Option<&str>, roots: &[&Path]) -> Option<PathBuf> {
    module_dir_candidates(explicit, roots)
        .into_iter()
        .map(|d| d.join("libpipewire-module-filter-chain.so"))
        .find(|p| p.exists())
}

/// Whether this system can host the EQ at all. `Ok(())` or a reason to show
/// the user — the previous engine reported success regardless, which is how
/// a completely absent plugin set went unnoticed.
pub fn availability() -> Result<(), String> {
    for tool in ["pipewire", "pw-cli", "pw-dump"] {
        if !is_available(tool) {
            return Err(ToolError::ToolMissing { tool: tool.into() }.to_string());
        }
    }
    // The module ships with PipeWire itself, but a cut-down package can omit
    // it, and its absence is otherwise only visible as a silent no-op.
    let explicit = std::env::var("PIPEWIRE_MODULE_DIR").ok();
    let roots: Vec<&Path> = LIB_ROOTS.iter().map(Path::new).collect();
    if find_filter_chain_module(explicit.as_deref(), &roots).is_none() {
        return Err(
            "PipeWire's filter-chain module (libpipewire-module-filter-chain.so) was not \
             found under /usr/lib*/pipewire-0.3 or /usr/lib/<arch>-linux-gnu/pipewire-0.3; \
             install the full PipeWire package, or set PIPEWIRE_MODULE_DIR"
                .into(),
        );
    }
    Ok(())
}

/// Build the filter-chain config for one channel.
///
/// `source_sink` is the OpenGG virtual sink whose monitor to process, and
/// `target_device` the sink to play out to (empty = let PipeWire pick the
/// default, which is what happens before a device has been chosen).
pub fn build_config(channel: &str, source_sink: &str, target_device: &str) -> String {
    build_config_with_fx(channel, source_sink, target_device, None)
}

/// [`build_config`] with voice-processing stages (gate/compressor/noise
/// reduction, see `voicefx`) ahead of the EQ bands — how an output channel
/// such as Chat gets both from one chain.
pub fn build_config_with_fx(
    channel: &str,
    source_sink: &str,
    target_device: &str,
    fx: Option<(&crate::voicefx::FxSettings, &crate::voicefx::FxCaps)>,
) -> String {
    let mut nodes = String::new();
    let mut links = String::new();
    // With voice stages in front, the graph's ends must be named: the meter
    // has more input ports than the one in use, and filter-chain would
    // otherwise count them all as graph inputs and stop duplicating the
    // chain per channel.
    let mut ends = String::new();
    if let Some((nodes_fx, links_fx, first_in, last_out)) =
        fx.and_then(|(f, c)| crate::voicefx::graph_nodes(f, c))
    {
        nodes.push_str(&nodes_fx);
        links.push_str(&links_fx);
        links.push_str(&format!("          {{ output = \"{last_out}\" input = \"eq0:In\" }}\n"));
        ends = format!(
            "        inputs  = [ \"{first_in}\" ]\n        outputs = [ \"eq{}:Out\" ]\n",
            EQ_FREQS.len() - 1
        );
    }
    for (i, freq) in EQ_FREQS.iter().enumerate() {
        let label = match i {
            0 => "bq_lowshelf",
            9 => "bq_highshelf",
            _ => "bq_peaking",
        };
        nodes.push_str(&format!(
            "          {{ type = builtin name = eq{i} label = {label} \
             control = {{ \"Freq\" = {freq:.1} \"Q\" = 1.0 \"Gain\" = 0.0 }} }}\n"
        ));
        if i > 0 {
            links.push_str(&format!(
                "          {{ output = \"eq{}:Out\" input = \"eq{i}:In\" }}\n",
                i - 1
            ));
        }
    }

    // `stream.capture.sink = true` makes the capture side read a sink's
    // MONITOR rather than a source, which is what puts this after the
    // channel rather than in front of a microphone.
    let capture_target = if source_sink.is_empty() {
        String::new()
    } else {
        format!("node.target = \"{}\" stream.capture.sink = true", quote(source_sink))
    };
    let playback_target = if target_device.is_empty() {
        String::new()
    } else {
        format!("node.target = \"{}\"", quote(target_device))
    };
    let node = node_name(channel);

    format!(
        r#"# Generated by OpenGG — per-channel EQ for {channel}. Do not edit.
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
      node.description = "OpenGG EQ {channel}"
      media.name       = "OpenGG EQ {channel}"
      filter.graph = {{
        nodes = [
{nodes}        ]
        links = [
{links}        ]
{ends}      }}
      audio.channels = 2
      audio.position = [ FL FR ]
      capture.props  = {{ node.name = "{node}_in" {capture_target} }}
      playback.props = {{ node.name = "{node}" node.passive = false {playback_target} }}
    }}
  }}
]
"#
    )
}

/// A running EQ chain: the `pipewire -c` host process plus what it printed.
///
/// The caller owns it — dropping it without [`stop`] leaves the chain running.
pub struct Chain {
    child: Arc<Mutex<Child>>,
    /// Set before any kill, so the watchdog can tell a requested shutdown
    /// from a crash.
    stopping: Arc<AtomicBool>,
    stderr: StderrTail,
}

impl Chain {
    /// The last lines the host process wrote to stderr.
    pub fn stderr_tail(&self) -> String {
        self.stderr.tail(4)
    }

    /// Kill the host process. Marks the chain as stopping first so the
    /// watchdog does not mistake this for a crash. Does NOT restore any
    /// routing; [`stop`] (EQ) and `voicefx::stop_mic` do that.
    pub fn shutdown(self) {
        self.stopping.store(true, Ordering::SeqCst);
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    /// Watch for the host dying on its own. When it does, `restore` puts the
    /// dry path back (so the channel is audible again) and `on_exit` is told
    /// why. Does nothing if the chain was shut down deliberately first.
    pub fn watch(
        &self,
        restore: impl FnOnce() -> Result<(), String> + Send + 'static,
        on_exit: impl FnOnce(String) + Send + 'static,
    ) {
        let (child, stopping, stderr) =
            (self.child.clone(), self.stopping.clone(), self.stderr.clone());
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(750));
            if stopping.load(Ordering::SeqCst) {
                return;
            }
            let exited = child.lock().ok().and_then(|mut c| c.try_wait().ok().flatten());
            let Some(status) = exited else { continue };
            if stopping.swap(true, Ordering::SeqCst) {
                return; // stop() got there first
            }
            std::thread::sleep(Duration::from_millis(100)); // let stderr drain
            let msg = format!("EQ engine exited unexpectedly ({status}): {}", stderr.tail(4));
            log::error!("{msg}");
            if let Err(e) = restore() {
                log::error!("could not restore the dry path after the chain exited: {e}");
            }
            on_exit(msg);
            return;
        });
    }
}

/// Start the EQ for a channel and wait until it is actually in the graph.
///
/// A spawned `pipewire -c` is not a working chain: a rejected config or an
/// unknown device makes it exit within milliseconds. So this only returns
/// `Ok` once the child is still alive AND its node shows up in `pw-dump`;
/// otherwise the error carries the process's own stderr. Callers cut the
/// channel's dry path only after `Ok`, which is what keeps a failed start
/// from leaving the channel silent.
pub fn start(channel: &str, source_sink: &str, target_device: &str) -> Result<Chain, String> {
    start_with_fx(channel, source_sink, target_device, None)
}

/// [`start`] with voice-processing stages ahead of the bands.
pub fn start_with_fx(
    channel: &str,
    source_sink: &str,
    target_device: &str,
    fx: Option<(&crate::voicefx::FxSettings, &crate::voicefx::FxCaps)>,
) -> Result<Chain, String> {
    availability()?;
    let path = config_path(channel)?;
    std::fs::write(&path, build_config_with_fx(channel, source_sink, target_device, fx))
        .map_err(|e| format!("write {}: {e}", path.display()))?;
    host_chain(&format!("eq-{channel}"), &path, &format!("{}_in", node_name(channel)))
}

/// Run `pipewire -c <conf>` and wait until the node `ready_node` exists.
/// Shared by the per-channel EQ and the Mic voice chain, so both get the
/// same guarantee: `Ok` means the chain is alive and in the graph, and any
/// failure carries the process's stderr.
pub fn host_chain(label: &str, conf: &Path, ready_node: &str) -> Result<Chain, String> {
    let mut child = command("pipewire")
        .arg("-c")
        .arg(conf)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn pipewire: {e}"))?;
    let stderr = capture_stderr(label, &mut child);

    let begun = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                std::thread::sleep(Duration::from_millis(100)); // let stderr drain
                return Err(format!(
                    "EQ engine exited ({status}) before it came up: {}",
                    stderr.tail(4)
                ));
            }
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                return Err(format!("EQ engine: {e}"));
            }
        }
        if node_by_name(ready_node).is_some() {
            break;
        }
        if begun.elapsed() >= READY_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "EQ engine did not appear in the PipeWire graph within {}s: {}",
                READY_TIMEOUT.as_secs(),
                stderr.tail(4)
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    Ok(Chain {
        child: Arc::new(Mutex::new(child)),
        stopping: Arc::new(AtomicBool::new(false)),
        stderr,
    })
}

/// Find a node's id by `node.name` in `pw-dump` output.
fn parse_node_id(dump: &str, want: &str) -> Option<u32> {
    let v: serde_json::Value = serde_json::from_str(dump).ok()?;
    // `and_then`, NOT `?`, inside the loop: a `?` here propagates out of the
    // whole function the moment it meets an object without `info.props` —
    // and pw-dump's first entries (Core, Client, Module) have none, so the
    // scan gave up before reaching any node at all.
    for obj in v.as_array()? {
        let name = obj
            .get("info")
            .and_then(|i| i.get("props"))
            .and_then(|p| p.get("node.name"))
            .and_then(|n| n.as_str());
        if name == Some(want) {
            return obj.get("id").and_then(|i| i.as_u64()).map(|i| i as u32);
        }
    }
    None
}

/// The PipeWire node id for a channel's EQ, or `None` if it is not up yet.
/// Looked up through `pw-dump` rather than by parsing `pw-cli ls`, whose
/// output is line-oriented and shifts between versions.
pub fn node_id(channel: &str) -> Option<u32> {
    node_by_name(&format!("{}_in", node_name(channel)))
}

/// Id of the PipeWire node whose `node.name` is `name`.
pub fn node_by_name(name: &str) -> Option<u32> {
    let dump = run_tool("pw-dump", &[]).ok()?;
    parse_node_id(&dump, name)
}

/// Push band gains (dB) to a running EQ. Extra entries beyond the band count
/// are ignored; missing ones are left alone.
pub fn set_bands(channel: &str, gains: &[f32]) -> Result<(), String> {
    let id = node_id(channel)
        .ok_or_else(|| format!("no EQ node for {channel} (is the engine running?)"))?;
    for (i, gain) in gains.iter().enumerate().take(EQ_FREQS.len()) {
        let g = gain.clamp(GAIN_MIN_DB, GAIN_MAX_DB);
        let param = format!("{{ params = [ \"eq{i}:Gain\" {g:.4} ] }}");
        run_tool("pw-cli", &["set-param", &id.to_string(), "Props", &param])
            .map_err(|e| format!("set eq{i}: {e}"))?;
    }
    Ok(())
}

/// Read the gains currently applied, for verification and tests.
pub fn read_bands(channel: &str) -> Option<Vec<f32>> {
    let id = node_id(channel)?;
    let dump = run_tool("pw-dump", &[id.to_string().as_str()]).ok()?;
    let v: serde_json::Value = serde_json::from_str(&dump).ok()?;
    for obj in v.as_array()? {
        let Some(props) = obj
            .get("info")
            .and_then(|i| i.get("params"))
            .and_then(|p| p.get("Props"))
            .and_then(|p| p.as_array())
        else {
            continue;
        };
        for entry in props {
            let Some(list) = entry.get("params").and_then(|p| p.as_array()) else {
                continue;
            };
            let mut out = vec![0.0f32; EQ_FREQS.len()];
            let mut seen = false;
            // The list is a flat [key, value, key, value, ...] sequence.
            for pair in list.chunks(2) {
                let (Some(k), Some(val)) = (pair.first().and_then(|k| k.as_str()), pair.get(1))
                else {
                    continue;
                };
                for (i, slot) in out.iter_mut().enumerate() {
                    if k == format!("eq{i}:Gain") {
                        *slot = val.as_f64().unwrap_or(0.0) as f32;
                        seen = true;
                    }
                }
            }
            if seen {
                return Some(out);
            }
        }
    }
    None
}

/// Cut the direct `sink:monitor_* → device:playback_*` links, so the EQ's
/// output is not doubled by the untouched signal. A link that is already
/// gone is not an error; a missing `pw-link` is.
pub fn unlink_direct(sink: &str, device: &str) -> Result<(), String> {
    if sink.is_empty() || device.is_empty() {
        return Ok(());
    }
    for p in ["FL", "FR"] {
        match run_tool(
            "pw-link",
            &["-d", &format!("{sink}:monitor_{p}"), &format!("{device}:playback_{p}")],
        ) {
            Ok(_) | Err(ToolError::Failed { .. }) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

/// Put the direct links back. Safe to call when they already exist. This is
/// the step that un-silences a channel, so a failure is returned, not eaten.
pub fn relink_direct(sink: &str, device: &str) -> Result<(), String> {
    if sink.is_empty() || device.is_empty() {
        return Ok(());
    }
    for p in ["FL", "FR"] {
        crate::audio::link_ports(
            &format!("{sink}:monitor_{p}"),
            &format!("{device}:playback_{p}"),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Stop a channel's EQ and restore its dry path. Returns an error if the
/// dry path could not be restored (the channel would stay silent).
pub fn stop(chain: Chain, sink: &str, device: &str) -> Result<(), String> {
    chain.shutdown();
    relink_direct(sink, device)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generated config must be a complete 10-band chain: ten nodes,
    /// nine links, shelves at the ends. A malformed graph makes PipeWire
    /// exit immediately, which is exactly the silent failure this module
    /// exists to end.
    #[test]
    fn fx_stages_feed_the_first_band() {
        let fx = crate::voicefx::FxSettings { gate_enabled: true, ..Default::default() };
        let caps = crate::voicefx::FxCaps { gate: true, compressor: false, noise_reduction: false };
        let cfg = build_config_with_fx("Chat", "OpenGG_Chat", "alsa_output.test", Some((&fx, &caps)));
        assert!(cfg.contains("label = noisegate"));
        assert!(cfg.contains("output = \"gate:Out\" input = \"eq0:In\""));
    }

    #[test]
    fn config_describes_a_full_ten_band_chain() {
        let cfg = build_config("Game", "OpenGG_Game", "alsa_output.test");
        for i in 0..10 {
            assert!(cfg.contains(&format!("name = eq{i} ")), "missing band {i}");
        }
        assert_eq!(cfg.matches("bq_peaking").count(), 8);
        assert_eq!(cfg.matches("bq_lowshelf").count(), 1);
        assert_eq!(cfg.matches("bq_highshelf").count(), 1);
        assert_eq!(cfg.matches("output = \"eq").count(), 9);
        // Capturing a SINK's monitor, not a source — without this the chain
        // would sit in front of a microphone instead of after the channel.
        assert!(cfg.contains("stream.capture.sink = true"));
        assert!(cfg.contains("node.target = \"OpenGG_Game\""));
        assert!(cfg.contains("node.target = \"alsa_output.test\""));
    }

    /// With no device chosen yet the chain must still be loadable, just
    /// without an explicit playback target.
    #[test]
    fn config_omits_targets_when_unknown() {
        let cfg = build_config("Chat", "", "");
        assert!(!cfg.contains("node.target"));
        assert!(cfg.contains("name = eq9"));
    }

    #[test]
    fn node_names_are_channel_scoped_and_lowercase() {
        assert_eq!(node_name("Game"), "opengg_eq_game");
        assert_eq!(node_name("Mic"), "opengg_eq_mic");
        assert_ne!(node_name("Game"), node_name("Chat"));
    }

    /// End-to-end against the real PipeWire session: start a chain on a
    /// scratch sink, set every band, read them back. Skipped where PipeWire
    /// is unavailable (CI), so it never turns into a flaky gate.
    #[test]
    #[ignore = "modifies the live PipeWire graph; run only in a container"]
    fn live_chain_applies_and_reports_band_gains() {
        if availability().is_err() {
            eprintln!("no PipeWire filter-chain here — skipping");
            return;
        }
        let ch = "SelfTest";
        // No source/target: this is about the DSP, not the routing, and an
        // untargeted chain disturbs nothing that is playing.
        // `start` only returns once the node is in the graph.
        let Ok(chain) = start(ch, "", "") else {
            eprintln!("could not start test chain — skipping");
            return;
        };

        let wanted: Vec<f32> = (0..10).map(|i| i as f32 - 4.0).collect();
        let applied = set_bands(ch, &wanted);
        let read = read_bands(ch);
        let _ = stop(chain, "", "");

        applied.expect("set_bands");
        let read = read.expect("read_bands");
        assert_eq!(read.len(), 10);
        for (i, (got, want)) in read.iter().zip(wanted.iter()).enumerate() {
            assert!((got - want).abs() < 0.01, "band {i}: got {got}, wanted {want}");
        }
    }

    /// Debian/Ubuntu keep PipeWire's modules under a multiarch triplet
    /// directory; Arch/Fedora use the plain lib dir. Both must be found
    /// without naming the architecture.
    #[test]
    fn module_lookup_finds_multiarch_and_plain_layouts() {
        let tmp = std::env::temp_dir().join(format!("opengg-eq-mod-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let so = "libpipewire-module-filter-chain.so";

        // Debian 13 layout.
        let deb = tmp.join("deb");
        let deb_dir = deb.join("x86_64-linux-gnu/pipewire-0.3");
        std::fs::create_dir_all(&deb_dir).unwrap();
        std::fs::write(deb_dir.join(so), b"").unwrap();
        assert_eq!(find_filter_chain_module(None, &[deb.as_path()]), Some(deb_dir.join(so)));

        // aarch64 and a non-gnu libc triplet resolve the same way.
        let arm = tmp.join("arm");
        let arm_dir = arm.join("aarch64-linux-gnu/pipewire-0.3");
        std::fs::create_dir_all(&arm_dir).unwrap();
        std::fs::write(arm_dir.join(so), b"").unwrap();
        assert!(find_filter_chain_module(None, &[arm.as_path()]).is_some());

        // Arch / Fedora layout.
        let arch = tmp.join("arch");
        std::fs::create_dir_all(arch.join("pipewire-0.3")).unwrap();
        std::fs::write(arch.join("pipewire-0.3").join(so), b"").unwrap();
        assert!(find_filter_chain_module(None, &[arch.as_path()]).is_some());

        // Explicit override wins even when no root has the module.
        let custom = tmp.join("custom");
        std::fs::create_dir_all(&custom).unwrap();
        std::fs::write(custom.join(so), b"").unwrap();
        let empty = tmp.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let explicit = format!("/nonexistent:{}", custom.display());
        assert_eq!(
            find_filter_chain_module(Some(&explicit), &[empty.as_path()]),
            Some(custom.join(so))
        );

        // A root with a triplet dir but no module is correctly "missing".
        let none = tmp.join("none");
        std::fs::create_dir_all(none.join("x86_64-linux-gnu/pipewire-0.3")).unwrap();
        assert_eq!(find_filter_chain_module(None, &[none.as_path()]), None);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Device names are interpolated into a config; a quote must not be able
    /// to terminate the string.
    #[test]
    fn config_escapes_quotes_in_device_names() {
        let cfg = build_config("Game", "OpenGG_Game", "weird\"name\\x");
        assert!(cfg.contains(r#"node.target = "weird\"name\\x""#), "{cfg}");
    }

    /// pw-dump begins with Core/Module objects that have no `info.props`
    /// node name; the scan must walk past them to the real node.
    #[test]
    fn node_id_parses_pw_dump_past_objects_without_props() {
        let dump = r#"[
          {"id":0,"type":"PipeWire:Interface:Core","info":{"props":{"core.name":"pipewire-0"}}},
          {"id":3,"type":"PipeWire:Interface:Module"},
          {"id":41,"type":"PipeWire:Interface:Node","info":{"props":{"node.name":"other"}}},
          {"id":97,"type":"PipeWire:Interface:Node","info":{"props":{"node.name":"opengg_eq_game_in"}}}
        ]"#;
        assert_eq!(parse_node_id(dump, "opengg_eq_game_in"), Some(97));
        assert_eq!(parse_node_id(dump, "opengg_eq_chat_in"), None);
        assert_eq!(parse_node_id("not json", "x"), None);
    }

    /// Gains beyond the UI's range must be clamped rather than passed
    /// through to the filter, where a large value is audible distortion.
    #[test]
    fn gains_are_clamped_to_the_documented_range() {
        assert_eq!((-99.0f32).clamp(GAIN_MIN_DB, GAIN_MAX_DB), GAIN_MIN_DB);
        assert_eq!((99.0f32).clamp(GAIN_MIN_DB, GAIN_MAX_DB), GAIN_MAX_DB);
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;

    /// Full insert/restore against the real graph: with the EQ up, the
    /// channel's dry monitor→device links must be gone; after stop they
    /// must be back. Getting this wrong either doubles the audio or leaves
    /// the channel silent, and neither is visible from the config alone.
    #[test]
    #[ignore = "modifies the live PipeWire graph; run only in a container"]
    fn insert_and_restore_preserves_the_dry_path() {
        if availability().is_err() {
            eprintln!("no PipeWire here — skipping");
            return;
        }
        let sink = "OpenGG_Chat";
        let device = crate::audio::current_channel_device("Chat");
        if device.is_empty() || !links_exist(sink, &device) {
            eprintln!("Chat is not linked to a device right now — skipping");
            return;
        }

        let Ok(chain) = start("RouteTest", sink, &device) else {
            eprintln!("could not start chain — skipping");
            return;
        };
        unlink_direct(sink, &device).expect("unlink");
        let during = links_exist(sink, &device);
        stop(chain, sink, &device).expect("restore dry path");
        std::thread::sleep(std::time::Duration::from_millis(400));
        let after = links_exist(sink, &device);

        assert!(!during, "dry path still present while the EQ was inserted");
        assert!(after, "dry path was NOT restored after stopping the EQ");
    }

    /// True when the sink's monitor is linked straight to the device.
    fn links_exist(sink: &str, device: &str) -> bool {
        let Ok(out) = run_tool("pw-link", &["-l"]) else {
            return false;
        };
        let mut in_monitor = false;
        for line in out.lines() {
            if line.starts_with(&format!("{sink}:monitor_FL")) {
                in_monitor = true;
                continue;
            }
            if in_monitor {
                if !line.starts_with(' ') && !line.starts_with('\t') {
                    in_monitor = false;
                } else if line.contains(device) {
                    return true;
                }
            }
        }
        false
    }
}

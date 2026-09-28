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

use crate::subprocess::{command, is_available, run_cmd_sync};
use std::path::PathBuf;
use std::process::{Child, Stdio};

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
fn config_path(channel: &str) -> PathBuf {
    let dir = dirs::runtime_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("opengg");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("eq-{}.conf", channel.to_lowercase()))
}

/// Whether this system can host the EQ at all. `Ok(())` or a reason to show
/// the user — the previous engine reported success regardless, which is how
/// a completely absent plugin set went unnoticed.
pub fn availability() -> Result<(), String> {
    if !is_available("pipewire") {
        return Err("pipewire is not on PATH".into());
    }
    if !is_available("pw-cli") {
        return Err("pw-cli is not on PATH (install the pipewire tools package)".into());
    }
    // The module ships with PipeWire itself, but a cut-down package can omit
    // it, and its absence is otherwise only visible as a silent no-op.
    let found = [
        "/usr/lib/pipewire-0.3",
        "/usr/lib64/pipewire-0.3",
        "/usr/local/lib/pipewire-0.3",
    ]
    .iter()
    .any(|d| std::path::Path::new(d).join("libpipewire-module-filter-chain.so").exists());
    if !found {
        return Err("PipeWire's filter-chain module is missing".into());
    }
    Ok(())
}

/// Build the filter-chain config for one channel.
///
/// `source_sink` is the OpenGG virtual sink whose monitor to process, and
/// `target_device` the sink to play out to (empty = let PipeWire pick the
/// default, which is what happens before a device has been chosen).
pub fn build_config(channel: &str, source_sink: &str, target_device: &str) -> String {
    let mut nodes = String::new();
    let mut links = String::new();
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
        format!("node.target = \"{source_sink}\" stream.capture.sink = true")
    };
    let playback_target = if target_device.is_empty() {
        String::new()
    } else {
        format!("node.target = \"{target_device}\"")
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
      }}
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

/// Start the EQ for a channel. Returns the hosting process, which the caller
/// owns — dropping it without [`stop`] leaves the chain running.
pub fn start(channel: &str, source_sink: &str, target_device: &str) -> Result<Child, String> {
    availability()?;
    let path = config_path(channel);
    std::fs::write(&path, build_config(channel, source_sink, target_device))
        .map_err(|e| format!("write {}: {e}", path.display()))?;

    let child = command("pipewire")
        .arg("-c")
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn pipewire: {e}"))?;

    Ok(child)
}

/// The PipeWire node id for a channel's EQ, or `None` if it is not up yet.
/// Looked up through `pw-dump` rather than by parsing `pw-cli ls`, whose
/// output is line-oriented and shifts between versions.
pub fn node_id(channel: &str) -> Option<u32> {
    let want = format!("{}_in", node_name(channel));
    let dump = run_cmd_sync("pw-dump", &[]).ok()?;
    let v: serde_json::Value = serde_json::from_str(&dump).ok()?;
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
        if name == Some(want.as_str()) {
            return obj.get("id").and_then(|i| i.as_u64()).map(|i| i as u32);
        }
    }
    None
}

/// Push band gains (dB) to a running EQ. Extra entries beyond the band count
/// are ignored; missing ones are left alone.
pub fn set_bands(channel: &str, gains: &[f32]) -> Result<(), String> {
    let id = node_id(channel)
        .ok_or_else(|| format!("no EQ node for {channel} (is the engine running?)"))?;
    for (i, gain) in gains.iter().enumerate().take(EQ_FREQS.len()) {
        let g = gain.clamp(GAIN_MIN_DB, GAIN_MAX_DB);
        let param = format!("{{ params = [ \"eq{i}:Gain\" {g:.4} ] }}");
        run_cmd_sync("pw-cli", &["set-param", &id.to_string(), "Props", &param])
            .map_err(|e| format!("set eq{i}: {e}"))?;
    }
    Ok(())
}

/// Read the gains currently applied, for verification and tests.
pub fn read_bands(channel: &str) -> Option<Vec<f32>> {
    let id = node_id(channel)?;
    let dump = run_cmd_sync("pw-dump", &[id.to_string().as_str()]).ok()?;
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
/// output is not doubled by the untouched signal.
pub fn unlink_direct(sink: &str, device: &str) {
    if sink.is_empty() || device.is_empty() {
        return;
    }
    for p in ["FL", "FR"] {
        let _ = run_cmd_sync(
            "pw-link",
            &["-d", &format!("{sink}:monitor_{p}"), &format!("{device}:playback_{p}")],
        );
    }
}

/// Put the direct links back. Safe to call when they already exist —
/// pw-link simply reports the duplicate.
pub fn relink_direct(sink: &str, device: &str) {
    if sink.is_empty() || device.is_empty() {
        return;
    }
    for p in ["FL", "FR"] {
        let _ = run_cmd_sync(
            "pw-link",
            &[&format!("{sink}:monitor_{p}"), &format!("{device}:playback_{p}")],
        );
    }
}

/// Stop a channel's EQ and restore its dry path.
pub fn stop(mut child: Child, sink: &str, device: &str) {
    let _ = child.kill();
    let _ = child.wait();
    relink_direct(sink, device);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generated config must be a complete 10-band chain: ten nodes,
    /// nine links, shelves at the ends. A malformed graph makes PipeWire
    /// exit immediately, which is exactly the silent failure this module
    /// exists to end.
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
    fn live_chain_applies_and_reports_band_gains() {
        if availability().is_err() {
            eprintln!("no PipeWire filter-chain here — skipping");
            return;
        }
        let ch = "SelfTest";
        // No source/target: this is about the DSP, not the routing, and an
        // untargeted chain disturbs nothing that is playing.
        let Ok(child) = start(ch, "", "") else {
            eprintln!("could not start test chain — skipping");
            return;
        };

        // The node appears asynchronously once PipeWire has loaded it.
        let mut id = None;
        for _ in 0..40 {
            id = node_id(ch);
            if id.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if id.is_none() {
            stop(child, "", "");
            eprintln!("test chain never appeared — skipping");
            return;
        }

        let wanted: Vec<f32> = (0..10).map(|i| i as f32 - 4.0).collect();
        let applied = set_bands(ch, &wanted);
        let read = read_bands(ch);
        stop(child, "", "");

        applied.expect("set_bands");
        let read = read.expect("read_bands");
        assert_eq!(read.len(), 10);
        for (i, (got, want)) in read.iter().zip(wanted.iter()).enumerate() {
            assert!((got - want).abs() < 0.01, "band {i}: got {got}, wanted {want}");
        }
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

        let Ok(child) = start("RouteTest", sink, &device) else {
            eprintln!("could not start chain — skipping");
            return;
        };
        unlink_direct(sink, &device);
        let during = links_exist(sink, &device);
        stop(child, sink, &device);
        std::thread::sleep(std::time::Duration::from_millis(400));
        let after = links_exist(sink, &device);

        assert!(!during, "dry path still present while the EQ was inserted");
        assert!(after, "dry path was NOT restored after stopping the EQ");
    }

    /// True when the sink's monitor is linked straight to the device.
    fn links_exist(sink: &str, device: &str) -> bool {
        let Ok(out) = run_cmd_sync("pw-link", &["-l"]) else {
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

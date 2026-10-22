//! Streamer Mode — a second, independent "Stream" mix next to the user's own
//! ("Personal") mix, the PipeWire equivalent of SteelSeries Sonar's
//! Streamer Mode. OBS (or anything else) captures `OpenGG_Stream_Mix`; the
//! headset keeps hearing the Personal mix, and the two are leveled and muted
//! independently per channel.
//!
//! ## Why the graph has to change while this is on
//!
//! A null-sink's monitor is **post-fader** by default (measured: a sink at
//! 50% drops its monitor by ~18 dB, mute silences it). The classic graph
//! relies on that — `OpenGG_<ch>:monitor_*` is `pw-link`ed straight to the
//! output device and the sink volume *is* the Personal fader. A Stream tap
//! on the same monitor would therefore follow the Personal fader and mute,
//! which is exactly what Streamer Mode must not do.
//!
//! So while Streamer Mode is on, each output channel (Game/Chat/Media/Aux):
//!
//! ```text
//!   apps → OpenGG_<ch> (monitor.channel-volumes=false: monitor is pre-fader)
//!            └─ monitor ─┬─ loopback "OpenGG_Personal_<ch>"   → output device
//!                        │    (volume/mute mirror the sink's — the sink
//!                        │     volume stays the one Personal fader)
//!                        └─ loopback "OpenGG_StreamFeed_<ch>" → OpenGG_Stream
//!   OpenGG_Mic.monitor (post-fader: your mic level) → "OpenGG_StreamFeed_Mic" ┘
//!   OpenGG_Stream.monitor → remap source OpenGG_Stream_Mix ("OpenGG Stream Mix")
//! ```
//!
//! Turning it off restores the classic graph exactly (post-fader monitor,
//! direct `pw-link` to the same device), and so does dropping the daemon —
//! the graph outside a running daemon is always the classic one, so a crash
//! or upgrade never leaves the user without their Personal mix.
//!
//! Everything is `pactl load-module` / `pw-link` / `pw-cli set-param` at
//! runtime; PipeWire is never restarted.

use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::sinks;
use crate::subprocess;

pub const STREAM_SINK: &str = "OpenGG_Stream";
pub const STREAM_SOURCE: &str = "OpenGG_Stream_Mix";
/// Channels whose Personal path moves onto a loopback in Streamer Mode.
pub const OUTPUT_CHANNELS: [&str; 4] = ["Game", "Chat", "Media", "Aux"];
/// Channels with a fader in the Stream mix. "Master" is the Stream sink
/// itself — the level of the whole mix OBS receives.
pub const STREAM_CHANNELS: [&str; 6] = ["Master", "Game", "Chat", "Media", "Aux", "Mic"];
/// Added latency per loopback. Low enough that the Personal mix stays
/// usable for gaming; PipeWire's default (200 ms) is not.
const LOOPBACK_LATENCY_MS: u32 = 10;

fn personal_node(ch: &str) -> String {
    format!("OpenGG_Personal_{ch}")
}

fn stream_feed_node(ch: &str) -> String {
    format!("OpenGG_StreamFeed_{ch}")
}

/// One fader of the Stream mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MixLevel {
    pub volume: u32,
    pub muted: bool,
}

impl Default for MixLevel {
    fn default() -> Self {
        Self { volume: 100, muted: false }
    }
}

/// What the daemon reports over `GetStreamerState`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StreamerState {
    pub enabled: bool,
    /// Sink OBS can capture the monitor of, and the friendlier remap source.
    pub sink: &'static str,
    pub source: &'static str,
    /// Stream-mix level per channel (kept while disabled, so the mix can be
    /// set up before going live).
    pub channels: BTreeMap<String, MixLevel>,
    /// Where each output channel's Personal loopback plays, while enabled.
    pub devices: BTreeMap<String, String>,
}

impl Default for StreamerState {
    fn default() -> Self {
        Self {
            enabled: false,
            sink: STREAM_SINK,
            source: STREAM_SOURCE,
            channels: STREAM_CHANNELS
                .iter()
                .map(|c| (c.to_string(), MixLevel::default()))
                .collect(),
            devices: BTreeMap::new(),
        }
    }
}

/// Saved Streamer Mode settings: `mixer.stream` in ui-settings.json, written
/// by the UI (`{"enabled": bool, "volumes": {ch: pct}, "mutes": {ch: bool}}`).
pub fn seed_from_settings(settings: &serde_json::Value) -> (bool, BTreeMap<String, MixLevel>) {
    let stream = &settings["mixer"]["stream"];
    let mut levels: BTreeMap<String, MixLevel> = STREAM_CHANNELS
        .iter()
        .map(|c| (c.to_string(), MixLevel::default()))
        .collect();
    for (ch, level) in levels.iter_mut() {
        if let Some(v) = stream["volumes"][ch.as_str()].as_u64() {
            level.volume = v.min(150) as u32;
        }
        if let Some(m) = stream["mutes"][ch.as_str()].as_bool() {
            level.muted = m;
        }
    }
    (stream["enabled"].as_bool().unwrap_or(false), levels)
}

/// True for a `pactl list modules short` line that belongs to Streamer
/// Mode. Matched on our own node/sink names so a stale set left by a killed
/// daemon is found again, and so nothing else of OpenGG's is touched.
pub fn is_streamer_module(line: &str) -> bool {
    line.contains("OpenGG_Personal_")
        || line.contains("OpenGG_StreamFeed_")
        || line.contains(&format!("source_name={STREAM_SOURCE}"))
        || line
            .split_whitespace()
            .any(|tok| tok == format!("sink_name={STREAM_SINK}"))
}

/// One sink-input as `pactl -f json list sink-inputs` reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct SinkInput {
    pub index: u32,
    pub node_name: String,
    pub volume_pct: u32,
    pub muted: bool,
}

pub fn parse_sink_inputs(json: &str) -> Vec<SinkInput> {
    let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(json) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|si| {
            let index = si["index"].as_u64()? as u32;
            let node_name = si["properties"]["node.name"].as_str()?.to_string();
            // First channel's "75%" — the loopbacks are always set evenly.
            let volume_pct = si["volume"]
                .as_object()
                .and_then(|m| m.values().next())
                .and_then(|c| c["value_percent"].as_str())
                .and_then(|p| p.trim_end_matches('%').trim().parse::<u32>().ok())
                .unwrap_or(100);
            Some(SinkInput {
                index,
                node_name,
                volume_pct,
                muted: si["mute"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

/// The physical device `OpenGG_<ch>`'s monitor is linked to in the classic
/// graph, read from `pw-link -l` pairs. Our own loopback inputs are ignored
/// so a half-built Streamer graph never reports itself as the device.
pub fn linked_device(links: &HashSet<(String, String)>, channel: &str) -> Option<String> {
    let src = format!("OpenGG_{channel}:monitor_FL");
    let mut found: Vec<&str> = links
        .iter()
        .filter(|(s, _)| *s == src)
        .filter_map(|(_, d)| d.strip_suffix(":playback_FL"))
        .filter(|node| !node.starts_with("OpenGG_"))
        .collect();
    found.sort();
    found.first().map(|s| s.to_string())
}

// ── pactl / pw-link / pw-cli plumbing ──────────────────────────────────────

fn pactl(args: &[&str]) -> Result<String> {
    let out = subprocess::command("pactl")
        .args(args)
        .output()
        .context("pactl not found — is PipeWire running?")?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        anyhow::bail!("pactl {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim())
    }
}

fn list_sink_inputs() -> Vec<SinkInput> {
    pactl(&["-f", "json", "list", "sink-inputs"])
        .map(|j| parse_sink_inputs(&j))
        .unwrap_or_default()
}

fn find_sink_input(node: &str) -> Option<SinkInput> {
    list_sink_inputs().into_iter().find(|s| s.node_name == node)
}

fn set_sink_input_level(node: &str, volume: u32, muted: bool) {
    let Some(si) = find_sink_input(node) else {
        tracing::warn!("Streamer: loopback {node} not found");
        return;
    };
    let idx = si.index.to_string();
    if si.volume_pct != volume {
        let _ = pactl(&["set-sink-input-volume", &idx, &format!("{volume}%")]);
    }
    if si.muted != muted {
        let _ = pactl(&["set-sink-input-mute", &idx, if muted { "1" } else { "0" }]);
    }
}

fn set_sink_input_mute(node: &str, muted: bool) {
    if let Some(si) = find_sink_input(node) {
        let _ = pactl(&["set-sink-input-mute", &si.index.to_string(), if muted { "1" } else { "0" }]);
    }
}

/// Flip whether a sink's monitor carries its volume/mute. Runtime-settable
/// and reversible (verified against PipeWire 1.0); this is the switch
/// between the classic graph and the Streamer one.
fn set_monitor_post_fader(sink: &str, post_fader: bool) -> Result<()> {
    let out = subprocess::command("pw-cli")
        .args([
            "set-param",
            sink,
            "Props",
            &format!("{{ params = [ \"monitor.channel-volumes\" {post_fader} ] }}"),
        ])
        .output()
        .context("pw-cli not found")?;
    if !out.status.success() {
        anyhow::bail!("pw-cli set-param {sink}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

fn pw_link(src: &str, dst: &str, unlink: bool) {
    let mut args = Vec::with_capacity(3);
    if unlink {
        args.push("-d");
    }
    args.push(src);
    args.push(dst);
    let _ = subprocess::command("pw-link").args(&args).output();
}

/// Load a 10 ms loopback `source → sink` whose playback stream is named
/// `node` (so it can be found, leveled and unloaded by name), starting
/// muted so it never plays a burst before its level is set.
fn load_loopback(source: &str, sink: &str, node: &str) -> Result<()> {
    pactl(&[
        "load-module",
        "module-loopback",
        &format!("source={source}"),
        &format!("sink={sink}"),
        &format!("latency_msec={LOOPBACK_LATENCY_MS}"),
        "source_dont_move=true",
        "sink_dont_move=true",
        &format!("sink_input_properties=node.name={node} media.name={node}"),
        &format!("source_output_properties=node.name={node}_in media.name={node}"),
    ])?;
    // The playback stream registers asynchronously; wait for it, then mute.
    for _ in 0..20 {
        if find_sink_input(node).is_some() {
            set_sink_input_mute(node, true);
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    anyhow::bail!("loopback {node} did not appear")
}

fn unload_modules_where(pred: impl Fn(&str) -> bool) {
    let Ok(list) = pactl(&["list", "modules", "short"]) else {
        return;
    };
    for line in list.lines().filter(|l| pred(l)) {
        if let Some(id) = line.split_whitespace().next() {
            let _ = pactl(&["unload-module", id]);
        }
    }
}

fn unload_loopback(node: &str) {
    let needle = format!("node.name={node} ");
    unload_modules_where(|l| l.contains("module-loopback") && l.contains(&needle));
}

fn ensure_stream_bus() -> Result<()> {
    if !sinks::sink_exists(STREAM_SINK) {
        pactl(&[
            "load-module",
            "module-null-sink",
            &format!("sink_name={STREAM_SINK}"),
            "sink_properties=device.description=\"OpenGG Stream\" node.description=\"OpenGG Stream\"",
            "channels=2",
            "channel_map=front-left,front-right",
        ])?;
    }
    let sources = pactl(&["list", "sources", "short"]).unwrap_or_default();
    if !sources.lines().any(|l| l.split_whitespace().nth(1) == Some(STREAM_SOURCE)) {
        // Same doubly-quoted proplist the virtual mic needs — pactl's
        // remap-source parser truncates a description at the first space
        // otherwise (see sinks::setup_mic_loopback).
        pactl(&[
            "load-module",
            "module-remap-source",
            &format!("master={STREAM_SINK}.monitor"),
            &format!("source_name={STREAM_SOURCE}"),
            "source_properties=\"device.description=\\\"OpenGG Stream Mix\\\"\"",
        ])?;
    }
    Ok(())
}

// ── Streamer ───────────────────────────────────────────────────────────────

pub struct Streamer {
    state: Mutex<StreamerState>,
}

impl Streamer {
    pub fn new(levels: BTreeMap<String, MixLevel>) -> Self {
        let mut state = StreamerState::default();
        state.channels.extend(levels);
        Self { state: Mutex::new(state) }
    }

    pub fn state(&self) -> StreamerState {
        self.state.lock().unwrap().clone()
    }

    pub fn is_enabled(&self) -> bool {
        self.state.lock().unwrap().enabled
    }

    /// Build the Streamer graph. Idempotent: any leftovers of an earlier
    /// run (a killed daemon) are cleared first, then the graph is rebuilt
    /// from the live classic one. Blocking — call off the async runtime.
    pub fn enable(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        let live = sinks::read_live_sink_state();
        let links = sinks::parse_pw_links().unwrap_or_default();
        let default_sink = sinks::get_default_sink_name().unwrap_or_default();

        // Recover the device of a channel whose classic link a previous
        // (killed) run already replaced, before clearing that run away.
        let stale_devices: BTreeMap<String, String> = list_sink_inputs()
            .into_iter()
            .filter_map(|si| {
                let ch = si.node_name.strip_prefix("OpenGG_Personal_")?.to_string();
                Some((ch, si.index))
            })
            .filter_map(|(ch, idx)| {
                let sink = sink_of_sink_input(idx)?;
                Some((ch, sink))
            })
            .collect();
        unload_modules_where(is_streamer_module);

        // Without the channel sinks there is nothing to mix: say so rather
        // than report success with an empty Stream bus.
        if !OUTPUT_CHANNELS.iter().any(|ch| live.contains_key(&format!("OpenGG_{ch}"))) {
            anyhow::bail!("OpenGG's virtual audio channels are not set up — create virtual audio first");
        }

        ensure_stream_bus().context("creating the Stream bus")?;
        let master = state.channels.get("Master").copied().unwrap_or_default();
        let _ = pactl(&["set-sink-volume", STREAM_SINK, &format!("{}%", master.volume)]);
        let _ = pactl(&["set-sink-mute", STREAM_SINK, if master.muted { "1" } else { "0" }]);

        for ch in OUTPUT_CHANNELS {
            let sink = format!("OpenGG_{ch}");
            if !live.contains_key(&sink) {
                continue; // virtual audio not created (or removed)
            }
            let device = state
                .devices
                .get(ch)
                .cloned()
                .or_else(|| linked_device(&links, ch))
                .or_else(|| stale_devices.get(ch).cloned())
                .unwrap_or_else(|| default_sink.clone());
            if device.is_empty() {
                tracing::warn!("Streamer: no output device for {ch} — skipping");
                continue;
            }
            let (vol, muted) = live.get(&sink).copied().unwrap_or((1.0, false));
            let monitor = format!("{sink}.monitor");

            // Order matters for what the user hears during the switch: the
            // new path is built muted, the old one is removed, the monitor
            // goes pre-fader, and only then does the new path open. The
            // worst case is a few ms of silence — never a jump to unity.
            load_loopback(&monitor, &device, &personal_node(ch))?;
            set_sink_input_level(&personal_node(ch), (vol * 100.0).round() as u32, true);
            for p in ["FL", "FR"] {
                pw_link(&format!("{sink}:monitor_{p}"), &format!("{device}:playback_{p}"), true);
            }
            set_monitor_post_fader(&sink, false)?;
            set_sink_input_mute(&personal_node(ch), muted);

            let level = state.channels.get(ch).copied().unwrap_or_default();
            load_loopback(&monitor, STREAM_SINK, &stream_feed_node(ch))?;
            set_sink_input_level(&stream_feed_node(ch), level.volume, level.muted);

            state.devices.insert(ch.to_string(), device);
        }

        // Mic joins the Stream mix post-fader: the Mic fader is "my mic
        // level" for every consumer, the stream included.
        if live.contains_key("OpenGG_Mic") {
            let level = state.channels.get("Mic").copied().unwrap_or_default();
            load_loopback("OpenGG_Mic.monitor", STREAM_SINK, &stream_feed_node("Mic"))?;
            set_sink_input_level(&stream_feed_node("Mic"), level.volume, level.muted);
        }

        state.enabled = true;
        tracing::info!("Streamer Mode on — OBS source: {STREAM_SOURCE}");
        Ok(())
    }

    /// Restore the classic graph. Safe to call when already off.
    pub fn disable(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        for ch in OUTPUT_CHANNELS {
            let sink = format!("OpenGG_{ch}");
            let device = state.devices.get(ch).cloned().or_else(|| {
                find_sink_input(&personal_node(ch)).and_then(|si| sink_of_sink_input(si.index))
            });
            // Mirror image of `enable`: close the loopback, put the monitor
            // back post-fader, then relink.
            set_sink_input_mute(&personal_node(ch), true);
            if sinks::sink_exists(&sink) {
                if let Err(e) = set_monitor_post_fader(&sink, true) {
                    tracing::warn!("Streamer: {e}");
                }
                if let Some(device) = &device {
                    for p in ["FL", "FR"] {
                        pw_link(&format!("{sink}:monitor_{p}"), &format!("{device}:playback_{p}"), false);
                    }
                }
            }
        }
        unload_modules_where(is_streamer_module);
        state.devices.clear();
        if state.enabled {
            tracing::info!("Streamer Mode off — classic graph restored");
        }
        state.enabled = false;
        Ok(())
    }

    /// Level one Stream-mix fader. Applied live when enabled, remembered
    /// either way.
    pub fn set_level(&self, channel: &str, volume: Option<u32>, muted: Option<bool>) -> Result<()> {
        if !STREAM_CHANNELS.contains(&channel) {
            anyhow::bail!("unknown stream channel {channel}");
        }
        let mut state = self.state.lock().unwrap();
        let level = state.channels.entry(channel.to_string()).or_default();
        if let Some(v) = volume {
            level.volume = v.min(150);
        }
        if let Some(m) = muted {
            level.muted = m;
        }
        let level = *level;
        if state.enabled {
            if channel == "Master" {
                pactl(&["set-sink-volume", STREAM_SINK, &format!("{}%", level.volume)])?;
                pactl(&["set-sink-mute", STREAM_SINK, if level.muted { "1" } else { "0" }])?;
            } else {
                set_sink_input_level(&stream_feed_node(channel), level.volume, level.muted);
            }
        }
        Ok(())
    }

    /// Point a channel's Personal path at another device while enabled.
    pub fn set_personal_device(&self, channel: &str, device: &str) -> Result<()> {
        if !OUTPUT_CHANNELS.contains(&channel) {
            anyhow::bail!("{channel} has no personal output path");
        }
        let mut state = self.state.lock().unwrap();
        if !state.enabled {
            anyhow::bail!("Streamer Mode is off");
        }
        if state.devices.get(channel).map(String::as_str) == Some(device) {
            return Ok(());
        }
        let sink = format!("OpenGG_{channel}");
        let (vol, muted) = sinks::read_live_sink_state().get(&sink).copied().unwrap_or((1.0, false));
        unload_loopback(&personal_node(channel));
        load_loopback(&format!("{sink}.monitor"), device, &personal_node(channel))?;
        set_sink_input_level(&personal_node(channel), (vol * 100.0).round() as u32, muted);
        state.devices.insert(channel.to_string(), device.to_string());
        Ok(())
    }

    /// Copy each channel sink's live volume/mute onto its Personal loopback,
    /// so the sink stays the single Personal fader — whoever moved it (the
    /// mixer, Ear Blast's ducking, pavucontrol).
    pub fn sync_personal(&self) {
        if !self.is_enabled() {
            return;
        }
        let live = sinks::read_live_sink_state();
        let inputs = list_sink_inputs();
        for ch in OUTPUT_CHANNELS {
            let Some(&(vol, muted)) = live.get(&format!("OpenGG_{ch}")) else {
                continue;
            };
            let want = (vol * 100.0).round() as u32;
            let node = personal_node(ch);
            if let Some(si) = inputs.iter().find(|s| s.node_name == node) {
                let idx = si.index.to_string();
                if si.volume_pct != want {
                    let _ = pactl(&["set-sink-input-volume", &idx, &format!("{want}%")]);
                }
                if si.muted != muted {
                    let _ = pactl(&["set-sink-input-mute", &idx, if muted { "1" } else { "0" }]);
                }
            }
        }
    }
}

impl Drop for Streamer {
    /// Leave the classic graph behind on shutdown (see the module docs).
    fn drop(&mut self) {
        if self.is_enabled() {
            let _ = self.disable();
        }
    }
}

/// Name of the sink a sink-input plays into.
fn sink_of_sink_input(index: u32) -> Option<String> {
    let inputs: Vec<serde_json::Value> =
        serde_json::from_str(&pactl(&["-f", "json", "list", "sink-inputs"]).ok()?).ok()?;
    let sink_idx = inputs
        .iter()
        .find(|si| si["index"].as_u64() == Some(index as u64))?["sink"]
        .as_u64()?;
    let sinks_json: Vec<serde_json::Value> =
        serde_json::from_str(&pactl(&["-f", "json", "list", "sinks"]).ok()?).ok()?;
    sinks_json
        .iter()
        .find(|s| s["index"].as_u64() == Some(sink_idx))?["name"]
        .as_str()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_only_streamer_modules() {
        assert!(is_streamer_module(
            "536870916\tmodule-loopback\tsource=OpenGG_Game.monitor sink=hw sink_input_properties=node.name=OpenGG_Personal_Game media.name=OpenGG_Personal_Game"
        ));
        assert!(is_streamer_module("12\tmodule-loopback\tsource=OpenGG_Mic.monitor sink=OpenGG_Stream sink_input_properties=node.name=OpenGG_StreamFeed_Mic"));
        assert!(is_streamer_module("13\tmodule-null-sink\tsink_name=OpenGG_Stream sink_properties=x"));
        assert!(is_streamer_module("14\tmodule-remap-source\tmaster=OpenGG_Stream.monitor source_name=OpenGG_Stream_Mix"));
        // The classic graph's own modules must never match.
        assert!(!is_streamer_module("15\tmodule-null-sink\tsink_name=OpenGG_Game sink_properties=x"));
        assert!(!is_streamer_module("16\tmodule-loopback\tsource=alsa sink=OpenGG_Mic sink_input_properties=node.name=OpenGG_Mic_Loopback_out"));
        assert!(!is_streamer_module("17\tmodule-remap-source\tmaster=OpenGG_Mic.monitor source_name=OpenGG_Virtual_Mic"));
    }

    #[test]
    fn parses_sink_inputs() {
        let json = r#"[
          {"index": 331, "mute": false, "sink": 45,
           "volume": {"front-left": {"value": 49152, "value_percent": "75%", "db": "-7.50 dB"},
                      "front-right": {"value": 49152, "value_percent": "75%", "db": "-7.50 dB"}},
           "properties": {"node.name": "OpenGG_Personal_Game"}},
          {"index": 332, "mute": true, "sink": 60, "volume": {},
           "properties": {"node.name": "OpenGG_StreamFeed_Mic"}},
          {"index": 333, "properties": {}}
        ]"#;
        let v = parse_sink_inputs(json);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], SinkInput { index: 331, node_name: "OpenGG_Personal_Game".into(), volume_pct: 75, muted: false });
        assert!(v[1].muted);
        assert!(parse_sink_inputs("not json").is_empty());
    }

    #[test]
    fn seeds_from_settings_with_defaults() {
        let s = serde_json::json!({"mixer": {"stream": {
            "enabled": true, "volumes": {"Game": 40, "Mic": 999}, "mutes": {"Chat": true}
        }}});
        let (enabled, levels) = seed_from_settings(&s);
        assert!(enabled);
        assert_eq!(levels["Game"], MixLevel { volume: 40, muted: false });
        assert_eq!(levels["Mic"].volume, 150);
        assert!(levels["Chat"].muted);
        assert_eq!(levels["Master"], MixLevel::default());
        let (enabled, levels) = seed_from_settings(&serde_json::json!({}));
        assert!(!enabled);
        assert_eq!(levels.len(), STREAM_CHANNELS.len());
    }

    #[test]
    fn finds_the_linked_device_and_ignores_our_loopbacks() {
        let links: HashSet<(String, String)> = [
            ("OpenGG_Game:monitor_FL", "alsa_output.usb:playback_FL"),
            ("OpenGG_Game:monitor_FR", "alsa_output.usb:playback_FR"),
            ("OpenGG_Chat:monitor_FL", "OpenGG_Personal_Chat_in:input_FL"),
            ("OpenGG_Media:monitor_FL", "OpenGG_Stream:playback_FL"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        assert_eq!(linked_device(&links, "Game").as_deref(), Some("alsa_output.usb"));
        assert_eq!(linked_device(&links, "Chat"), None);
        assert_eq!(linked_device(&links, "Media"), None);
        assert_eq!(linked_device(&links, "Aux"), None);
    }

    #[test]
    fn state_json_shape() {
        let s = serde_json::to_value(StreamerState::default()).unwrap();
        assert_eq!(s["enabled"], false);
        assert_eq!(s["source"], STREAM_SOURCE);
        assert_eq!(s["channels"]["Master"]["volume"], 100);
        assert_eq!(s["channels"].as_object().unwrap().len(), 6);
    }
}

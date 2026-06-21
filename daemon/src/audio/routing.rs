//! Audio App Routing — discovers streams and moves them between sinks.

use anyhow::{Context, Result};
use std::collections::HashMap;

use super::sinks::AppInfo;
use crate::subprocess;

#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub index: u32,
    pub sink: String,
    pub app_name: String,
    pub binary: String,
    pub icon: String,
    pub channel: String,
}

/// Build a sink-index → sink-name map from `pactl list sinks`.
/// Used to resolve the integer `sink` field in sink-input records.
fn build_sink_map() -> HashMap<u32, String> {
    let Ok(out) = subprocess::command("pactl").args(["-f", "json", "list", "sinks"]).output() else {
        return HashMap::new();
    };
    let Ok(sinks) = serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout) else {
        return HashMap::new();
    };
    sinks.iter().filter_map(|s| {
        let idx = s["index"].as_u64()? as u32;
        let name = s["name"].as_str()?.to_string();
        Some((idx, name))
    }).collect()
}

fn normalized_stream_name(app_name: Option<&str>, media_name: Option<&str>, binary_name: Option<&str>) -> String {
    let app = app_name.map(str::trim).filter(|v| !v.is_empty());
    let media = media_name.map(str::trim).filter(|v| !v.is_empty());
    let binary = binary_name.map(str::trim).filter(|v| !v.is_empty());

    match app {
        Some(name) if !name.eq_ignore_ascii_case("opengg") => name.to_string(),
        _ => media.or(binary).unwrap_or("Unknown").to_string(),
    }
}

fn is_internal_stream(app_name: Option<&str>, media_name: Option<&str>, binary_name: Option<&str>) -> bool {
    let matches_internal = |value: Option<&str>| {
        value
            .map(str::to_lowercase)
            .map(|raw| {
                raw.contains("opengg")
                    || raw.contains("wireplumber")
                    || raw.contains("pipewire")
                    || raw.contains("peak detect")
                    || raw.contains("monitor")
                    // OpenGG-internal helpers that must never show as user apps:
                    || raw.contains("pw-cat")          // native VU-meter readers
                    || raw.contains("gsr-")            // gpu-screen-recorder captures
                    || raw.contains("gpu-screen-recorder")
                    || raw.contains("loopback")        // mic loopback helper streams
            })
            .unwrap_or(false)
    };
    matches_internal(app_name) || matches_internal(media_name) || matches_internal(binary_name)
}

pub fn list_streams() -> Result<Vec<StreamInfo>> {
    let output = subprocess::command("pactl")
        .args(["-f", "json", "list", "sink-inputs"])
        .output()
        .context("pactl not found")?;

    if !output.status.success() {
        return list_streams_text();
    }

    let raw: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("Failed to parse pactl JSON")?;

    // ★ FIX: `sink` field in pactl JSON is an integer index, not a name string.
    // Build a lookup map once and resolve each sink-input's sink name from it.
    let sink_map = build_sink_map();

    let empty = vec![];
    let inputs = raw.as_array().unwrap_or(&empty);
    let mut streams = Vec::new();

    for input in inputs {
        let props = &input["properties"];
        let index = input["index"].as_u64().unwrap_or(0) as u32;
        if is_internal_stream(
            props["application.name"].as_str(),
            props["media.name"].as_str(),
            props["application.process.binary"].as_str(),
        ) {
            continue;
        }

        // ★ FIX: was input["sink"].as_str() — always None since it's a u32
        let sink_idx = input["sink"].as_u64().unwrap_or(u64::MAX) as u32;
        let sink_name = sink_map.get(&sink_idx).cloned().unwrap_or_default();

        let binary = props["application.process.binary"].as_str().unwrap_or("").to_string();
        let app_name = normalized_stream_name(
            props["application.name"].as_str(),
            props["media.name"].as_str(),
            Some(binary.as_str()),
        );
        let icon = props["application.icon_name"].as_str().unwrap_or("").to_string();

        let channel = super::sinks::CHANNEL_NAMES
            .iter()
            .find(|&&ch| sink_name.contains(&format!("OpenGG_{ch}")))
            .map(|s| s.to_string())
            .unwrap_or_default();

        streams.push(StreamInfo { index, sink: sink_name, app_name, binary, icon, channel });
    }
    streams.extend(list_source_outputs());
    Ok(streams)
}

/// Scan mic-capturing apps (source-outputs) so they appear under the Mic track.
/// IDs are offset by 90000 to avoid colliding with sink-input indices.
fn list_source_outputs() -> Vec<StreamInfo> {
    let Ok(out) = subprocess::command("pactl")
        .args(["-f", "json", "list", "source-outputs"])
        .output()
    else {
        return Vec::new();
    };
    let Ok(sos) = serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout) else {
        return Vec::new();
    };
    let mut streams = Vec::new();
    for (idx, so) in sos.iter().enumerate() {
        let props = &so["properties"];
        let binary = props["application.process.binary"].as_str().unwrap_or("").to_string();
        let app_name = normalized_stream_name(
            props["application.name"].as_str(),
            props["media.name"].as_str(),
            Some(binary.as_str()),
        );
        let icon = props["application.icon_name"].as_str().unwrap_or("").to_string();
        if is_internal_stream(Some(app_name.as_str()), None, Some(binary.as_str())) {
            continue;
        }
        streams.push(StreamInfo {
            index: 90000 + idx as u32,
            sink: String::new(),
            app_name,
            binary,
            icon,
            channel: "Mic".into(),
        });
    }
    streams
}

fn list_streams_text() -> Result<Vec<StreamInfo>> {
    let output = subprocess::command("pactl").args(["list", "sink-inputs"]).output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut streams = Vec::new();
    let mut current: Option<StreamInfo> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Sink Input #") {
            if let Some(s) = current.take() { streams.push(s); }
            let id = trimmed.trim_start_matches("Sink Input #").parse::<u32>().unwrap_or(0);
            current = Some(StreamInfo {
                index: id, sink: String::new(), app_name: "Unknown".into(),
                binary: String::new(), icon: String::new(), channel: String::new(),
            });
        } else if let Some(ref mut s) = current {
            if trimmed.starts_with("Sink:") {
                s.sink = trimmed.split_once(':').map(|x| x.1.trim().to_string()).unwrap_or_default();
            } else if trimmed.contains("application.name") {
                s.app_name = extract_val(trimmed);
            } else if trimmed.contains("application.process.binary") {
                s.binary = extract_val(trimmed);
            } else if trimmed.contains("application.icon_name") {
                s.icon = extract_val(trimmed);
            }
        }
    }
    if let Some(s) = current { streams.push(s); }

    streams.retain(|s| !is_internal_stream(Some(s.app_name.as_str()), None, Some(s.binary.as_str())));

    for s in &mut streams {
        s.channel = super::sinks::CHANNEL_NAMES
            .iter()
            .find(|&&ch| s.sink.contains(&format!("OpenGG_{ch}")))
            .map(|x| x.to_string())
            .unwrap_or_default();
    }
    streams.extend(list_source_outputs());
    Ok(streams)
}

fn extract_val(line: &str) -> String {
    line.split_once('=').map(|(_, v)| v.trim().trim_matches('"').to_string()).unwrap_or_default()
}

/// Resolve an incoming stream id to a pactl sink-input INDEX.
///
/// The id may already be a pactl sink-input index, or a PipeWire node.id (object.id).
/// `pactl move-sink-input` requires the index, so passing a node.id yields
/// "No such entity". Translating here keeps the whole route path in one id space.
/// Returns `None` for ids that are not sink-inputs (e.g. source-outputs / mic captures,
/// which cannot be moved to a playback sink).
fn resolve_sink_input_index(stream_id: u32) -> Option<u32> {
    let out = subprocess::command("pactl")
        .args(["-f", "json", "list", "sink-inputs"])
        .output()
        .ok()?;
    let sis: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).ok()?;
    // Direct index match.
    if sis.iter().any(|si| si["index"].as_u64() == Some(stream_id as u64)) {
        return Some(stream_id);
    }
    // Otherwise treat stream_id as a PW node.id (object.id) and find its sink-input.
    for si in &sis {
        let p = &si["properties"];
        for key in ["object.id", "node.id", "object.serial"] {
            if p[key].as_str().and_then(|s| s.parse::<u32>().ok()) == Some(stream_id) {
                return si["index"].as_u64().map(|i| i as u32);
            }
        }
    }
    None
}

pub fn route_stream(stream_id: u32, channel: &str) -> Result<()> {
    let target = if channel == "default" {
        let o = subprocess::command("pactl").args(["get-default-sink"]).output()?;
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    } else {
        format!("OpenGG_{channel}")
    };

    // ★ Single id space: translate node.id → pactl sink-input index before moving.
    let si_index = resolve_sink_input_index(stream_id).ok_or_else(|| {
        anyhow::anyhow!(
            "stream {stream_id} is not a movable sink-input (no pactl index nor node.id match) → {target}"
        )
    })?;

    // ★ check exit code — pactl returns non-zero on failure (wrong index, sink not
    // found, etc.) but .output() only errors if the binary isn't found.
    let out = subprocess::command("pactl")
        .args(["move-sink-input", &si_index.to_string(), &target])
        .output()
        .context(format!("pactl not found while routing {si_index} → {target}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("pactl move-sink-input {si_index} {target} failed: {err}");
    }
    tracing::info!("Routed stream {stream_id} (si #{si_index}) → {target}");
    Ok(())
}

pub fn apps_for_channel(streams: &[StreamInfo], channel: &str) -> Vec<AppInfo> {
    streams.iter()
        .filter(|s| s.channel == channel)
        .map(|s| AppInfo { id: s.index, name: s.app_name.clone(), binary: s.binary.clone() })
        .collect()
}

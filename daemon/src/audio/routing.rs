//! Audio App Routing — discovers streams and moves them between sinks.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::process::Command;

use super::sinks::AppInfo;

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
    let Ok(out) = Command::new("pactl").args(["-f", "json", "list", "sinks"]).output() else {
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

pub fn list_streams() -> Result<Vec<StreamInfo>> {
    let output = Command::new("pactl")
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

        // ★ FIX: was input["sink"].as_str() — always None since it's a u32
        let sink_idx = input["sink"].as_u64().unwrap_or(u64::MAX) as u32;
        let sink_name = sink_map.get(&sink_idx).cloned().unwrap_or_default();

        let app_name = props["application.name"]
            .as_str()
            .or_else(|| props["media.name"].as_str())
            .unwrap_or("Unknown")
            .to_string();
        let binary = props["application.process.binary"].as_str().unwrap_or("").to_string();
        let icon = props["application.icon_name"].as_str().unwrap_or("").to_string();

        let channel = super::sinks::CHANNEL_NAMES
            .iter()
            .find(|&&ch| sink_name.contains(&format!("OpenGG_{ch}")))
            .map(|s| s.to_string())
            .unwrap_or_default();

        streams.push(StreamInfo { index, sink: sink_name, app_name, binary, icon, channel });
    }
    Ok(streams)
}

fn list_streams_text() -> Result<Vec<StreamInfo>> {
    let output = Command::new("pactl").args(["list", "sink-inputs"]).output()?;
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

    for s in &mut streams {
        s.channel = super::sinks::CHANNEL_NAMES
            .iter()
            .find(|&&ch| s.sink.contains(&format!("OpenGG_{ch}")))
            .map(|x| x.to_string())
            .unwrap_or_default();
    }
    Ok(streams)
}

fn extract_val(line: &str) -> String {
    line.split_once('=').map(|(_, v)| v.trim().trim_matches('"').to_string()).unwrap_or_default()
}

pub fn route_stream(stream_id: u32, channel: &str) -> Result<()> {
    let target = if channel == "default" {
        let o = Command::new("pactl").args(["get-default-sink"]).output()?;
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    } else {
        format!("OpenGG_{channel}")
    };
    // ★ FIX: check exit code — pactl returns non-zero on failure (wrong index,
    // sink not found, etc.) but .output() only errors if the binary isn't found.
    let out = Command::new("pactl")
        .args(["move-sink-input", &stream_id.to_string(), &target])
        .output()
        .context(format!("pactl not found while routing {stream_id} → {target}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("pactl move-sink-input {stream_id} {target} failed: {err}");
    }
    tracing::info!("Routed stream {stream_id} → {target}");
    Ok(())
}

pub fn apps_for_channel(streams: &[StreamInfo], channel: &str) -> Vec<AppInfo> {
    streams.iter()
        .filter(|s| s.channel == channel)
        .map(|s| AppInfo { id: s.index, name: s.app_name.clone(), binary: s.binary.clone() })
        .collect()
}

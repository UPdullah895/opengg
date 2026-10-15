//! Version and health reporting.
//!
//! One JSON document answers "who are you, and what is broken?" for three
//! consumers: the `org.opengg.Daemon.Core.GetDiagnostics` D-Bus method (the
//! app's "copy diagnostics" / version-mismatch banner), the `openggd --diagnose`
//! command line (works with no service running), and anyone pasting it into a
//! bug report.
//!
//! Collection is blocking (it spawns `pactl`) and read-only: it never creates
//! sinks, never quarantines a config file, never touches the bus.

use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::audio::sinks::Health;

/// Bumped when the D-Bus surface changes in a way a client must know about.
/// Additive changes do not bump it.
pub const API_VERSION: u32 = 1;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Tools the daemon shells out to, and what they are for.
const TOOLS: &[(&str, &str)] = &[
    ("pactl", "audio mixer (required)"),
    ("pw-link", "audio routing (required)"),
    ("pw-cli", "sink teardown"),
    ("pw-dump", "EQ / graph verification"),
    ("ffmpeg", "clip export"),
    ("ffprobe", "clip metadata"),
    ("gpu-screen-recorder", "replay / clipping"),
    ("headsetcontrol", "headset control (optional)"),
];

static STARTED: OnceLock<Instant> = OnceLock::new();

#[derive(Default)]
struct Runtime {
    config_problem: Option<String>,
    /// module name -> "disabled" | "running" | "failed: <reason>"
    modules: Vec<(&'static str, String)>,
    hotkeys_active: bool,
    audio: Option<std::sync::Arc<Mutex<Health>>>,
}

static RUNTIME: OnceLock<Mutex<Runtime>> = OnceLock::new();

fn runtime() -> std::sync::MutexGuard<'static, Runtime> {
    let m = RUNTIME.get_or_init(|| Mutex::new(Runtime::default()));
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

pub fn mark_started() {
    let _ = STARTED.set(Instant::now());
}

pub fn set_config_problem(problem: Option<String>) {
    runtime().config_problem = problem;
}

pub fn set_module(name: &'static str, state: impl Into<String>) {
    let state = state.into();
    let mut rt = runtime();
    match rt.modules.iter_mut().find(|(n, _)| *n == name) {
        Some(slot) => slot.1 = state,
        None => rt.modules.push((name, state)),
    }
}

pub fn set_hotkeys_active(active: bool) {
    runtime().hotkeys_active = active;
}

pub fn set_audio_health(h: std::sync::Arc<Mutex<Health>>) {
    runtime().audio = Some(h);
}

/// `/dev/input/event*` as the current user sees it: how many exist, how many can
/// be opened, how many are refused. Distinguishes "not in the `input` group"
/// from "no keyboard", which the hotkey listener's own log cannot.
fn input_devices() -> Value {
    let (mut total, mut readable, mut denied) = (0u32, 0u32, 0u32);
    if let Ok(rd) = std::fs::read_dir("/dev/input") {
        for e in rd.flatten() {
            if !e.file_name().to_string_lossy().starts_with("event") {
                continue;
            }
            total += 1;
            match std::fs::File::open(e.path()) {
                Ok(_) => readable += 1,
                Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => denied += 1,
                Err(_) => {}
            }
        }
    }
    json!({ "event_devices": total, "readable": readable, "permission_denied": denied })
}

/// Build the diagnostics document. Blocking.
pub fn collect() -> Value {
    let (config_problem, modules, hotkeys_active, audio) = {
        let rt = runtime();
        (
            rt.config_problem.clone(),
            rt.modules.clone(),
            rt.hotkeys_active,
            rt.audio.clone(),
        )
    };
    let running = STARTED.get().is_some();

    let mut tools = serde_json::Map::new();
    let mut missing_tools = Vec::new();
    for (bin, purpose) in TOOLS {
        let path = crate::subprocess::which(bin);
        if path.is_none() {
            missing_tools.push(json!({
                "tool": bin,
                "purpose": purpose,
                "hint": crate::subprocess::install_hint(bin),
            }));
        }
        tools.insert((*bin).to_string(), json!({ "path": path, "purpose": purpose }));
    }

    // Fresh probe (also tells us whether pipewire-pulse is answering right now).
    let (pulse_reachable, pulse_error, present, missing) = match crate::audio::sinks::list_sink_names() {
        Ok(names) => {
            let missing = crate::audio::sinks::missing_sinks(&names);
            let present: Vec<String> = names.into_iter().filter(|n| n.starts_with("OpenGG_")).collect();
            (true, None, present, missing)
        }
        Err(e) => (false, Some(e), Vec::new(), crate::audio::sinks::missing_sinks(&[])),
    };

    let config = match crate::config::peek() {
        Ok(_) => Value::Null,
        Err(e) => Value::String(e),
    };

    let mut doc = json!({
        "version": version(),
        "api_version": API_VERSION,
        "mode": if running { "daemon" } else { "cli" },
        "uptime_secs": STARTED.get().map(|t| t.elapsed().as_secs()),
        "config": {
            "path": crate::config::config_path(),
            // Startup problem (file quarantined) or, in CLI mode, a current parse error.
            "problem": config_problem.map(Value::String).unwrap_or(config),
        },
        "modules": modules.into_iter().map(|(n, s)| (n.to_string(), Value::String(s))).collect::<serde_json::Map<_, _>>(),
        "tools": tools,
        "missing_tools": missing_tools,
        "audio": {
            "pulse_reachable": pulse_reachable,
            "pulse_error": pulse_error,
            "sinks_present": present,
            "sinks_missing": missing,
        },
        "hotkeys": {
            "active": hotkeys_active,
            "input": input_devices(),
        },
    });

    if let Some(h) = audio {
        doc["audio"]["health"] = serde_json::to_value(match h.lock() { Ok(g) => g.clone(), Err(p) => p.into_inner().clone() }).unwrap_or(Value::Null);
    }
    doc
}

pub fn collect_pretty() -> String {
    serde_json::to_string_pretty(&collect()).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_document_has_the_documented_shape() {
        let d = collect();
        assert_eq!(d["version"], version());
        assert_eq!(d["api_version"], API_VERSION);
        assert!(d["tools"]["pactl"].is_object());
        assert!(d["audio"]["pulse_reachable"].is_boolean());
        assert!(d["hotkeys"]["input"]["event_devices"].is_number());
        assert!(d["missing_tools"].is_array());
    }
}

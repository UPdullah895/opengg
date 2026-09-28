//! Native PipeWire VU metering via `pw-cat` subprocess.
//!
//! Ported from `frontend/src-tauri/src/vu_native.rs`. Deliberately drops that
//! file's libpulse fallback path (`libpulse_binding`, used only when `pw-cat`
//! is missing from `$PATH`) — PipeWire is this project's whole audio
//! backbone (see root CLAUDE.md), so a host without `pw-cat` has no virtual
//! sinks either and metering silence is an acceptable degrade. Revisit if
//! that assumption turns out wrong in practice.
//!
//! Each channel's reader thread:
//! 1. Spawns `pw-cat` with the appropriate target/format args, stdout piped.
//! 2. Reads F32LE mono PCM samples from stdout in 1 KB chunks (256 samples).
//! 3. Computes RMS over each chunk, applies attack/decay smoothing.
//! 4. Sends `(channel_name, db_level)` to the shared `mpsc` channel.
//! 5. Checks the generation counter each loop to exit stale threads gracefully.
//! 6. Kills the `pw-cat` subprocess on exit.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

/// Spawns a `pw-cat`-backed reader thread for one channel. No-op (silently
/// sends a single 0.0 sample) if `pw-cat` isn't on `$PATH`.
pub fn spawn_channel_reader(
    name: String,
    target: String,
    is_source: bool,
    tx: mpsc::Sender<(String, f32)>,
    running: Arc<AtomicBool>,
    gen: Arc<AtomicU64>,
    my_gen: u64,
) {
    let has_pw_cat = Command::new("which")
        .arg("pw-cat")
        .output()
        .ok()
        .map(|out| out.status.success())
        .unwrap_or(false);

    if !has_pw_cat {
        let _ = tx.send((name, -60.0));
        return;
    }

    std::thread::spawn(move || {
        if let Err(e) = run_reader(&name, &target, is_source, &tx, &running, &gen, my_gen) {
            eprintln!("vu reader {name} error: {e}");
        }
    });
}

fn run_reader(
    name: &str,
    target: &str,
    is_source: bool,
    tx: &mpsc::Sender<(String, f32)>,
    running: &Arc<AtomicBool>,
    gen: &Arc<AtomicU64>,
    my_gen: u64,
) -> Result<(), String> {
    let mut cmd = Command::new("pw-cat");
    cmd.args(["--format", "f32"])
        .args(["--channels", "1"])
        .args(["--rate", "48000"])
        .args(["--latency", "256"])
        .arg("--record");

    // Targeting: PA-style ".monitor" source names come in for sink channels
    // (needed by the caller's existence guard), but pw-cat's
    // `stream.capture.sink = true` wants the bare sink node name.
    if is_source {
        cmd.args(["--target", target]);
    } else {
        let sink_name = target.strip_suffix(".monitor").unwrap_or(target);
        cmd.args(["--target", sink_name]);
        cmd.args(["-P", "{ stream.capture.sink = true }"]);
    }
    cmd.arg("-"); // stream PCM to stdout; must be the last arg

    cmd.stdout(Stdio::piped()).stderr(Stdio::null()).stdin(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| format!("pw-cat spawn failed: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("failed to open pw-cat stdout")?;

    let mut buf = vec![0u8; 256 * 4];
    let mut prev = 0.0f32;
    let mut read_errors = 0u32;
    const MAX_READ_ERRORS: u32 = 3;

    loop {
        if !running.load(Ordering::Relaxed) || gen.load(Ordering::Relaxed) != my_gen {
            break;
        }

        match stdout.read(&mut buf) {
            Ok(0) => {
                read_errors += 1;
                if read_errors >= MAX_READ_ERRORS {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Ok(n) => {
                read_errors = 0;
                let whole = n - (n % 4);
                if whole == 0 {
                    continue;
                }
                let mut sum_sq = 0.0f32;
                for chunk in buf[..whole].chunks_exact(4) {
                    let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    sum_sq += sample * sample;
                }
                let rms = (sum_sq / (whole / 4) as f32).sqrt().min(1.0);
                let smoothed = if rms > prev { rms * 0.9 + prev * 0.1 } else { rms * 0.3 + prev * 0.7 };
                prev = smoothed;
                let db = (20.0f32 * smoothed.max(1e-9f32).log10()).max(-60.0f32);
                let _ = tx.send((name.to_string(), db));
            }
            Err(_) => {
                read_errors += 1;
                if read_errors >= MAX_READ_ERRORS {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}

/// One (channel name, PulseAudio-style monitor/source target) pair for the
/// fixed 6-channel layout this app always meters.
pub fn channel_targets(mic_source: String) -> Vec<(&'static str, String, bool)> {
    vec![
        ("Master", format!("{}.monitor", default_sink_monitor_base()), false),
        ("Game", "OpenGG_Game.monitor".into(), false),
        ("Chat", "OpenGG_Chat.monitor".into(), false),
        ("Media", "OpenGG_Media.monitor".into(), false),
        ("Aux", "OpenGG_Aux.monitor".into(), false),
        ("Mic", mic_source, true),
    ]
}

fn default_sink_monitor_base() -> String {
    crate::subprocess::run_cmd_sync("pactl", &["get-default-sink"])
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Mic source: prefers `OpenGG_Virtual_Mic` when it exists, else the
/// hardware default source.
pub fn resolve_mic_source() -> String {
    let mut mic = crate::subprocess::run_cmd_sync("pactl", &["get-default-source"])
        .unwrap_or_default()
        .trim()
        .to_string();
    if let Ok(list) = crate::subprocess::run_cmd_sync("pactl", &["list", "sources", "short"]) {
        if list.contains("OpenGG_Virtual_Mic") {
            mic = "OpenGG_Virtual_Mic".to_string();
        }
    }
    mic
}

/// Sources currently known to PipeWire/PulseAudio, for the existence guard.
pub fn known_sources() -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    if let Ok(json) = crate::subprocess::run_cmd_sync("pactl", &["-f", "json", "list", "sources"]) {
        if let Ok(sources) = serde_json::from_str::<Vec<serde_json::Value>>(&json) {
            for s in &sources {
                if let Some(name) = s["name"].as_str() {
                    set.insert(name.to_string());
                }
            }
        }
    }
    set
}

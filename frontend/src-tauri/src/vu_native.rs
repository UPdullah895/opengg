//! Native PipeWire VU metering via pw-cat subprocess.
//!
//! Uses pw-cat (part of pipewire-tools) to capture audio streams directly from
//! PipeWire sinks/sources with minimal latency. If pw-cat is not available,
//! falls back to libpulse.
//!
//! Stream targeting:
//! - Master: capture from default sink monitor via `--record --target <sink>.monitor`
//! - Game/Chat/Media/Aux: capture from sink monitors via `--record --target <sink> -P '{ stream.capture.sink = true }'`
//! - Mic: capture from source via `--record --target <source>`
//!
//! Each channel's reader thread:
//! 1. Spawns `pw-cat` with appropriate target, format args, and stdin/stdout setup
//! 2. Reads F32LE mono PCM samples from stdout in 1KB chunks (256 samples)
//! 3. Computes RMS over each chunk, applies attack/decay smoothing
//! 4. Sends (channel_name, db_level) to the shared tokio::sync::mpsc channel
//! 5. Checks generation counter to exit stale threads gracefully
//! 6. Kills pw-cat subprocess on exit (SIGKILL via child.kill())

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::process::{Command, Stdio};
use std::io::Read;

/// Attempts to spawn a native PipeWire reader thread for a single channel.
/// Returns `Ok(())` if the thread was spawned, `Err(String)` if PipeWire not available.
/// On error, the caller should fall back to libpulse for that channel.
pub fn spawn_channel_reader(
    name: String,
    target: String,
    is_source: bool,
    tx: tokio::sync::mpsc::UnboundedSender<(String, f32)>,
    running: Arc<AtomicBool>,
    gen: Arc<AtomicU64>,
    my_gen: u64,
) -> Result<(), String> {
    // Check if pw-cat is available
    Command::new("which")
        .arg("pw-cat")
        .output()
        .ok()
        .and_then(|out| if out.status.success() { Some(()) } else { None })
        .ok_or_else(|| "pw-cat not found on $PATH".to_string())?;

    tokio::task::spawn_blocking(move || {
        let res = run_native_vu_reader(&name, &target, is_source, &tx, &running, &gen, my_gen);
        if let Err(e) = res {
            eprintln!("native VU reader {name} error: {e}");
        }
    });

    Ok(())
}

/// Runs the native PipeWire capture via pw-cat.
/// On error, returns an Err string that describes the failure point.
fn run_native_vu_reader(
    name: &str,
    target: &str,
    is_source: bool,
    tx: &tokio::sync::mpsc::UnboundedSender<(String, f32)>,
    running: &Arc<AtomicBool>,
    gen: &Arc<AtomicU64>,
    my_gen: u64,
) -> Result<(), String> {
    // Build pw-cat command
    let mut cmd = Command::new("pw-cat");

    // Common format args
    cmd.args(["--format", "f32"])
        .args(["--channels", "1"]) // mono
        .args(["--rate", "48000"]) // 48kHz
        .args(["--latency", "256"]); // small quantum for live metering

    // For capturing (both sinks and sources), use --record mode
    // and read from stdout via pipes.
    cmd.arg("--record");

    // Targeting. The caller passes PA source names (".monitor" for sink
    // monitors — that namespace is what the existence guard and the
    // libpulse fallback need), but pw-cat's `stream.capture.sink = true`
    // wants the SINK node name: ".monitor" is not a PipeWire node, and a
    // failed target lookup silently falls back to the DEFAULT device —
    // which made every channel meter the master output.
    if is_source {
        cmd.args(["--target", target]);
    } else {
        let sink_name = target.strip_suffix(".monitor").unwrap_or(target);
        cmd.args(["--target", sink_name]);
        cmd.args(["-P", "{ stream.capture.sink = true }"]);
    }

    // pw-cat REQUIRES a file argument; "-" streams PCM to stdout (without
    // it pw-cat exits immediately with a usage error and readers see
    // instant EOF). It must come LAST: options placed after the positional
    // can be dropped by argument parsing, silently losing --target.
    cmd.arg("-");

    // Set up pipes: stdout gets the audio data, stderr is suppressed
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null());

    // Spawn the subprocess
    let mut child = cmd.spawn()
        .map_err(|e| format!("pw-cat spawn failed: {e}"))?;

    let mut stdout = child.stdout.take()
        .ok_or("failed to open pw-cat stdout")?;

    eprintln!("native VU reader {name} → '{target}' (is_source={is_source}, gen={my_gen}) streaming via pw-cat");

    // Buffer for F32LE samples: read in chunks of 256 samples (1 KB at F32)
    let mut buf = vec![0u8; 256 * 4]; // 256 f32 samples
    let mut prev = 0.0f32;
    let mut read_error_count = 0;
    const MAX_READ_ERRORS: u32 = 3;

    loop {
        // Check generation counter and running flag
        if !running.load(Ordering::Relaxed) || gen.load(Ordering::Relaxed) != my_gen {
            eprintln!(
                "native VU reader {name} exiting (running={}, gen_match={})",
                running.load(Ordering::Relaxed),
                gen.load(Ordering::Relaxed) == my_gen
            );
            break;
        }

        // Read samples from pw-cat stdout. Use read() and tolerate partial
        // chunks: the quantum doesn't have to align with our buffer size, and
        // a short read is normal — only EOF (Ok(0)) or hard errors count
        // against the error budget.
        match stdout.read(&mut buf) {
            Ok(0) => {
                read_error_count += 1;
                eprintln!("native VU reader {name} EOF #{read_error_count} (pw-cat exited?)");
                if read_error_count >= MAX_READ_ERRORS {
                    eprintln!("native VU reader {name} stream ended, exiting");
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Ok(n) => {
                // Reset read error counter on successful read
                read_error_count = 0;

                // Interpret the bytes we actually got as f32LE samples (drop
                // any trailing partial sample) and compute RMS
                let whole = n - (n % 4);
                if whole == 0 {
                    continue;
                }
                let mut sum_sq: f32 = 0.0;
                for chunk in buf[..whole].chunks_exact(4) {
                    let sample_bytes = [chunk[0], chunk[1], chunk[2], chunk[3]];
                    let sample = f32::from_le_bytes(sample_bytes);
                    sum_sq += sample * sample;
                }

                let rms = (sum_sq / (whole / 4) as f32).sqrt().min(1.0);

                // Apply attack/decay smoothing (same as libpulse version)
                let smoothed = if rms > prev {
                    rms * 0.9 + prev * 0.1  // Fast attack
                } else {
                    rms * 0.3 + prev * 0.7  // Slow decay
                };
                prev = smoothed;

                let db = (20.0_f32 * smoothed.max(1e-9_f32).log10()).max(-60.0_f32);
                let _ = tx.send((name.to_string(), db));
            }
            Err(e) => {
                read_error_count += 1;
                eprintln!("native VU reader {name} read error #{read_error_count}: {e}");

                // If pw-cat died unexpectedly, bail out
                if read_error_count >= MAX_READ_ERRORS {
                    eprintln!("native VU reader {name} exceeded {MAX_READ_ERRORS} read errors, exiting");
                    break;
                }

                // Small backoff before retry
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }

    // Kill pw-cat subprocess on exit
    let _ = child.kill();
    let _ = child.wait();

    eprintln!("native VU reader {name} cleaned up and exited");

    Ok(())
}

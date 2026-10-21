//! GPU Screen Recorder (GSR) subprocess management — direct control, replay buffer.
//!
//! Moved from frontend/src-tauri/src/commands.rs (start_gsr_replay, save_gsr_replay, etc.).
//! Process state is held in a shared `static OnceLock<Mutex<Option<GsrState>>>` so both
//! status checks and start/stop/save operations work on the same live process handle.
//!
//! All functions here are synchronous/blocking (no async, no Tauri, no events).
//! The Tauri wrapper layer handles event emission based on results.

use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;

use crate::clips::organizer;
use crate::clips::retention;
use crate::clips::save_watch::{self, buffer_bytes, write_limit, LowDiskWarner, SaveGate, SaveRequest, SaveVerdict};
use crate::paths::shexp;

/// Parameters for spawning GPU Screen Recorder (passed once at start, used for restarts).
#[derive(Clone, Debug)]
pub struct GsrSpawnParams {
    pub output_dir: String,
    pub replay_secs: u32,
    pub fps: u32,
    pub quality: String,
    pub bitrate_kbps: Option<u32>,
    pub monitor_target: String,
    pub audio_sources: Vec<String>,
    /// The resolved, ordered capture targets actually passed to GSR via `-a` (after
    /// existence-filtering / default fallback). The muxed file's audio stream order matches
    /// this, so it's used to title each audio track with a friendly name on save.
    pub audio_targets: Vec<String>,
}

/// Managed state for the GPU Screen Recorder child process.
/// Stores the child, original spawn params, and a shared stderr buffer so we can
/// diagnose crashes and surface actionable errors to the user.
pub struct GsrState {
    pub child: std::process::Child,
    pub params: GsrSpawnParams,
    pub stderr_log: Arc<Mutex<Vec<String>>>,
}

/// Global GSR process state — wrapped in Option so we can take() and replace atomically.
static GSR_PROCESS: OnceLock<Mutex<Option<GsrState>>> = OnceLock::new();

/// Get or initialize the global GSR process state mutex.
fn gsr_process() -> &'static Mutex<Option<GsrState>> {
    GSR_PROCESS.get_or_init(|| Mutex::new(None))
}

/// Information returned when GSR crashes unexpectedly.
#[derive(Debug, Clone)]
pub struct GsrCrashInfo {
    pub status_code: Option<i32>,
    pub stderr_tail: String,
}

/// Start gpu-screen-recorder in replay-buffer mode.
/// Quality is passed directly as a GSR preset string: cbr | medium | high | very_high | ultra.
/// When quality is "cbr", `bitrate_kbps` sets the target bitrate (e.g. 8000 = 8 Mbps).
/// `monitor_target` is passed to `-w` (e.g. "screen", "DP-1", "HDMI-1").
/// Audio sources are PipeWire sink names without the "OpenGG_" prefix (e.g. ["Game","Chat","Mic"]).
pub fn start_gsr_replay(
    output_dir: String,
    replay_secs: u32,
    fps: u32,
    quality: String,
    bitrate_kbps: Option<u32>,
    monitor_target: String,
    audio_sources: Vec<String>,
) -> Result<(), String> {
    let mut lock = gsr_process().lock().unwrap();
    if lock.is_some() {
        return Err("gpu-screen-recorder is already running".into());
    }
    let expanded = shexp(&output_dir);
    std::fs::create_dir_all(&expanded)
        .map_err(|e| format!("Cannot create output dir '{expanded}': {e}"))?;

    // ★ The virtual audio engine is OPTIONAL — never hard-block recording.
    // Resolve each requested source to the capture node GSR expects, keep the ones that
    // actually exist as live PipeWire sources, and fall back to the real default devices
    // (desktop + mic) when none are available.
    let sources_short = std::process::Command::new("pactl")
        .args(["list", "sources", "short"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    // Map a requested source to the capture node name GSR records with (shared resolver).
    let resolve_capture = resolve_capture_source;
    let source_exists = |name: &str| {
        sources_short
            .lines()
            .any(|l| l.split('\t').nth(1) == Some(name))
    };
    let mut audio_targets: Vec<String> = Vec::new();
    for src in &audio_sources {
        let cap = resolve_capture(src);
        // default_* are resolved by GSR itself; everything else must be a live source.
        if cap.contains("default") || source_exists(&cap) {
            if !audio_targets.contains(&cap) {
                audio_targets.push(cap);
            }
        } else {
            log::warn!("capture source {cap:?} not present — skipping (virtual engine absent?)");
        }
    }
    if audio_targets.is_empty() {
        log::info!(
            "No OpenGG capture sources available — falling back to default_output + default_input"
        );
        audio_targets.push("default_output".to_string());
        audio_targets.push("default_input".to_string());
    }

    // Guard against stale settings that contain EDID model names or resolution strings
    // (e.g. "1920x1080", "BenQ GW2780") left over from the old Tauri monitor API.
    // GSR only accepts connector names ("screen", "DP-1", "HDMI-A-1", "focused").
    // A valid connector name never starts with a digit and never contains a space.
    let monitor_target = {
        // Legacy installs stored a "connector|resolution" composite (e.g. "DP-1|1920x1080").
        // GSR's -w only accepts the bare connector — keep the part before '|'.
        let connector = monitor_target.split('|').next().unwrap_or("").trim().to_string();
        let looks_invalid = connector.is_empty()
            || connector
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
            || connector.contains(' ');
        if looks_invalid {
            log::warn!(
                "gsrMonitorTarget {:?} is not a valid GSR connector name — resetting to 'screen'",
                monitor_target
            );
            "screen".to_string()
        } else {
            connector
        }
    };

    // For "focused" target: resolve to an actual monitor name using xdotool + xrandr.
    // This fixes multi-monitor setups where `-w focused` captures the wrong display.
    // Falls back to "focused" if detection fails (Wayland, missing tools, or parse error).
    let target = match monitor_target.as_str() {
        "focused" => get_focused_window_monitor().unwrap_or_else(|| {
            log::warn!("Could not detect monitor for focused window; falling back to -w focused");
            "focused".to_string()
        }),
        "" | "screen" => "screen".to_string(),
        other => other.to_string(),
    };
    let fps_str = fps.to_string();
    let secs_str = replay_secs.to_string();

    // "-w focused" (fallback only) requires an explicit resolution; detect primary monitor
    let focused_resolution = if target == "focused" {
        Some(detect_primary_resolution())
    } else {
        None
    };

    // ★ Map target for Wayland: "screen"/"DP-*"/"HDMI-*" → "portal" on Wayland
    // X11 behavior preserved: all targets pass through unchanged
    let target = map_gsr_target_for_wayland(&target);

    let mut cmd = std::process::Command::new("gpu-screen-recorder");
    cmd.args([
        "-w", &target, "-f", &fps_str, "-r", &secs_str, "-c", "mp4", "-o", &expanded,
    ]);

    // CBR mode: GSR requires an integer -q index even when -bm cbr is set. 0 = lowest quality
    // index, which is effectively ignored when the bitrate is set via -ffmpeg-opts.
    if quality == "cbr" {
        cmd.args(["-bm", "cbr", "-q", "0"]);
        if let Some(kbps) = bitrate_kbps {
            let bv = format!("-b:v {}k", kbps);
            cmd.args(["-ffmpeg-opts", &bv]);
        }
    } else {
        cmd.args(["-q", quality.as_str()]);
    }

    // Append resolution when capturing focused window
    if let Some(ref res) = focused_resolution {
        cmd.args(["-s", res]);
    }

    // Audio capture targets resolved above (existing sources, or default fallback).
    for monitor in &audio_targets {
        cmd.args(["-a", monitor]);
    }

    // ★ Pipe stderr so we can diagnose immediate failures (missing groups, bad GPU, etc.)
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start gpu-screen-recorder: {e}"))?;

    let stderr = child.stderr.take().ok_or("Failed to capture GSR stderr")?;
    let stderr_log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let stderr_log_clone = Arc::clone(&stderr_log);
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader};
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            log::warn!("[GSR stderr] {line}");
            stderr_log_clone.lock().unwrap().push(line);
        }
    });

    // ★ Warmup check: give GSR 1 s to crash immediately after spawn.
    // This catches missing permissions, invalid targets, encoder failures, etc.
    std::thread::sleep(std::time::Duration::from_millis(1000));
    match child.try_wait() {
        Ok(Some(status)) => {
            let tail: String = {
                let log = stderr_log.lock().unwrap();
                log.iter().rev().take(12).rev().cloned().collect::<Vec<_>>().join("\n")
            };
            let reason = if tail.is_empty() {
                format!("gpu-screen-recorder exited immediately with status {status:?}.")
            } else {
                format!(
                    "gpu-screen-recorder exited immediately with status {status:?}. Details:\n{tail}",
                )
            };
            log::error!("{reason}");
            return Err(reason);
        }
        Ok(None) => {
            // Still running — good
        }
        Err(e) => {
            return Err(format!("Failed to check gpu-screen-recorder status after spawn: {e}"));
        }
    }

    log::info!(
        "GSR started (pid {}) replay={}s fps={fps} quality={quality} bitrate={bitrate_kbps:?}kbps target={target} dir={expanded} audio={:?}",
        child.id(), replay_secs, audio_sources
    );
    save_gate().lock().unwrap().buffer_started(std::time::Instant::now());
    *lock = Some(GsrState {
        child,
        params: GsrSpawnParams {
            output_dir,
            replay_secs,
            fps,
            quality,
            bitrate_kbps,
            monitor_target,
            audio_sources,
            audio_targets,
        },
        stderr_log,
    });
    Ok(())
}

/// Check if GSR is currently running.
/// Returns true if running, false if not.
/// If the process crashed unexpectedly, returns the crash info in the error.
/// Note: The Tauri wrapper should emit the crash event based on this result.
pub fn is_gsr_running() -> Result<bool, GsrCrashInfo> {
    let mut lock = gsr_process().lock().unwrap();
    match lock.as_mut() {
        Some(gsr_state) => match gsr_state.child.try_wait() {
            Ok(None) => Ok(true),
            Ok(Some(status)) => {
                let tail: String = {
                    let log = gsr_state.stderr_log.lock().unwrap();
                    log.iter().rev().take(12).rev().cloned().collect::<Vec<_>>().join("\n")
                };
                log::warn!(
                    "GSR crashed (status={:?}). Last stderr:\n{tail}",
                    status.code()
                );
                lock.take();
                Err(GsrCrashInfo {
                    status_code: status.code(),
                    stderr_tail: tail,
                })
            }
            Err(e) => {
                log::warn!("GSR try_wait error: {e}");
                lock.take();
                Ok(false)
            }
        },
        None => Ok(false),
    }
}

/// Get the current GSR recorder status as a string:
/// - "replay:<secs>" if running (where <secs> is the replay buffer duration)
/// - "idle" if not running
pub fn get_recorder_status() -> String {
    let mut lock = gsr_process().lock().unwrap();
    if let Some(state) = lock.as_mut() {
        match state.child.try_wait() {
            Ok(None) => {
                // Process still running — return replay:<secs>
                let secs = state.params.replay_secs;
                return format!("replay:{secs}");
            }
            Ok(Some(status)) => {
                // Exited unexpectedly — log stderr tail and clean up
                let tail: String = {
                    let log = state.stderr_log.lock().unwrap();
                    log.iter().rev().take(10).rev().cloned().collect::<Vec<_>>().join("\n")
                };
                log::warn!(
                    "GSR exited unexpectedly (status={:?}) during get_recorder_status. Last stderr:\n{tail}",
                    status.code()
                );
                lock.take();
            }
            Err(e) => {
                log::warn!("GSR try_wait error: {e}");
                lock.take();
            }
        }
    }
    drop(lock);
    "idle".into()
}

/// Result of a replay save that produced a clip.
#[derive(Debug, Clone, Serialize)]
pub struct SaveReplayResult {
    pub game_title: String,
    pub filename: String,
    /// Full path of the saved clip (after any per-game folder / " (n)").
    pub filepath: String,
    pub filesize_mb: f64,
    pub success: bool,
    /// Free space left after the save when it is below the low-disk
    /// threshold and a warning is due (rate-limited), else `None`.
    pub low_disk_free_bytes: Option<u64>,
}

/// Why a save produced no clip. Each variant has a stable [`SaveFailure::id`]
/// so the UI can show a translated reason instead of a raw error string.
#[derive(Debug, Clone, PartialEq)]
pub enum SaveFailure {
    /// The request was refused before anything was signalled.
    Refused(SaveRequest),
    /// The recorder never finished writing within the write limit.
    Timeout,
    /// The saved file is empty.
    Empty,
    /// The disk is (nearly) full; the clip is likely truncated.
    DiskFull,
    /// Anything else (rename failed, process vanished, …).
    Other(String),
}

impl SaveFailure {
    pub fn id(&self) -> &'static str {
        match self {
            Self::Refused(r) => r.id(),
            Self::Timeout => "timeout",
            Self::Empty => "empty",
            Self::DiskFull => "diskFull",
            Self::Other(_) => "error",
        }
    }
}

impl std::fmt::Display for SaveFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(SaveRequest::Off) => write!(f, "Clip not saved — the replay buffer is off"),
            Self::Refused(SaveRequest::Starting) => write!(f, "Clip not saved — the replay buffer is still starting"),
            Self::Refused(_) => write!(f, "A clip is already being saved"),
            Self::Timeout => write!(f, "Clip not saved — the recorder did not finish writing it"),
            Self::Empty => write!(f, "Clip not saved — the file could not be written"),
            Self::DiskFull => write!(f, "Clip not saved — the disk is full"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

/// Where and how saved clips are filed (from ui-settings.json).
#[derive(Debug, Clone, Default)]
pub struct SaveOptions {
    /// File each clip under `<clips>/<Game>/`.
    pub per_game_folder: bool,
    /// Folder size limit in GB for OpenGG's own clips (0 = off).
    pub max_folder_gb: u64,
}

fn save_gate() -> &'static Mutex<SaveGate> {
    static GATE: OnceLock<Mutex<SaveGate>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(SaveGate::default()))
}

fn low_disk_warner() -> &'static Mutex<LowDiskWarner> {
    static W: OnceLock<Mutex<LowDiskWarner>> = OnceLock::new();
    W.get_or_init(|| Mutex::new(LowDiskWarner::default()))
}

/// Save the current replay buffer.
///
/// Safe to call from a worker thread: the GSR lock is held only while
/// signalling. The flow, in order:
///
/// 1. name the game in front *before* signalling (focus may move after);
/// 2. ask the [`SaveGate`] — refuse when the buffer is off, younger than
///    2 s, or already saving;
/// 3. SIGUSR1, then wait for the file GSR writes to be *finished*: GSR no
///    longer holds it open and its size has stopped changing, within a
///    write limit sized to the buffer (the old fixed 5 s poll could pick up
///    a half-written file, or give up on a long one);
/// 4. remux with per-track titles into a hidden part file beside the
///    target, then move it into place under a collision-free name;
/// 5. judge the result (empty file / full disk), record it as OpenGG's for
///    the folder size limit, and enforce that limit in the background.
///
/// If `restart_on_save` is true, GSR is also respawned so the next save
/// only holds footage recorded after this one.
pub fn save_gsr_replay(restart_on_save: bool, opts: &SaveOptions) -> Result<SaveReplayResult, SaveFailure> {
    let game_title = crate::foreground::current_game_name();
    let pre_save_time = std::time::SystemTime::now();

    let (pid, output_dir_exp, restart_params, audio_targets_ordered, limit) = {
        let lock = gsr_process().lock().unwrap();
        let running = lock.is_some();
        let buffer = lock
            .as_ref()
            .map(|s| buffer_bytes(u64::from(s.params.bitrate_kbps.unwrap_or(ASSUMED_KBPS)), u64::from(s.params.replay_secs)))
            .unwrap_or(0);
        let req = save_gate().lock().unwrap().request(std::time::Instant::now(), running, buffer);
        if req != SaveRequest::Started {
            log::info!("replay save refused: {}", req.id());
            return Err(SaveFailure::Refused(req));
        }
        let gsr_state = lock.as_ref().expect("running checked above");
        let pid = gsr_state.child.id();
        #[cfg(unix)]
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGUSR1);
        }
        log::info!("GSR SIGUSR1 → pid {pid}");
        let rp = restart_on_save.then(|| gsr_state.params.clone());
        (
            pid,
            shexp(&gsr_state.params.output_dir),
            rp,
            gsr_state.params.audio_targets.clone(),
            write_limit(buffer),
        )
    }; // lock dropped here

    let outcome = finish_save(pid, &output_dir_exp, pre_save_time, limit, &game_title, &audio_targets_ordered, opts);
    save_gate().lock().unwrap().finished();
    let result = outcome?;

    if let Some(params) = restart_params {
        {
            let mut lock = gsr_process().lock().unwrap();
            if let Some(mut gsr_state) = lock.take() {
                gsr_kill_graceful(&mut gsr_state.child);
                log::info!("GSR stopped for restart-on-save (SIGINT + wait)");
            }
        }
        start_gsr_replay(
            params.output_dir,
            params.replay_secs,
            params.fps,
            params.quality,
            params.bitrate_kbps,
            params.monitor_target,
            params.audio_sources,
        )
        .map_err(SaveFailure::Other)?;
        log::info!("GSR restarted (restart_on_save=true)");
    }
    Ok(result)
}

/// Bitrate assumed for sizing the save timeout when the quality preset
/// gives none: on the high side, so a slow disk is never cut short.
const ASSUMED_KBPS: u32 = 50_000;

/// Steps 3–5 of [`save_gsr_replay`], after the signal.
fn finish_save(
    pid: u32,
    output_dir: &str,
    after: std::time::SystemTime,
    limit: std::time::Duration,
    game_title: &str,
    audio_targets: &[String],
    opts: &SaveOptions,
) -> Result<SaveReplayResult, SaveFailure> {
    let src_path = wait_for_finished_clip(pid, output_dir, after, limit).ok_or(SaveFailure::Timeout)?;
    let root = std::path::Path::new(output_dir);
    let stamp = organizer::local_stamp(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
    );
    let target = organizer::target_path(root, game_title, &stamp, "mp4", opts.per_game_folder);
    let target_dir = target.parent().unwrap_or(root).to_path_buf();
    let _ = std::fs::create_dir_all(&target_dir);

    // Remux with friendly per-track titles ("Game"/"Chat"/"Mic" instead of
    // "OpenGG_*.monitor"; stream copy, no re-encode) into a hidden part file
    // so neither the library nor the watcher sees it half-written; a plain
    // move is the fallback when ffmpeg is missing or fails.
    let part = target_dir.join(format!(".{}.{}.part.mp4", stamp, std::process::id()));
    let staged = if remux_with_track_titles(&src_path, &part, audio_targets) {
        let _ = std::fs::remove_file(&src_path);
        part
    } else {
        let _ = std::fs::remove_file(&part);
        src_path
    };
    let dest = organizer::move_into_place(&staged, &target)
        .map_err(|e| SaveFailure::Other(format!("Could not move the clip into place: {e}")))?;

    let bytes = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    let free = save_watch::free_bytes(&target_dir);
    match save_watch::verdict(bytes, free) {
        SaveVerdict::Ok => {}
        SaveVerdict::Empty => return Err(SaveFailure::Empty),
        SaveVerdict::DiskFull => return Err(SaveFailure::DiskFull),
    }

    if let Ok(db) = crate::clips::open_db() {
        let _ = retention::record_owned(&db, &dest);
    }
    if opts.max_folder_gb > 0 {
        let gb = opts.max_folder_gb;
        std::thread::spawn(move || {
            retention::enforce(gb);
        });
    }
    let low_disk_free_bytes = low_disk_warner()
        .lock()
        .unwrap()
        .should_warn(std::time::Instant::now(), free)
        .then_some(free)
        .flatten();

    let filename = dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    log::info!("GSR clip saved as {dest:?}");
    Ok(SaveReplayResult {
        game_title: game_title.to_string(),
        filename,
        filepath: dest.to_string_lossy().to_string(),
        filesize_mb: (bytes as f64 / 100_000.0).round() / 10.0,
        success: true,
        low_disk_free_bytes,
    })
}

/// Gracefully kill and immediately respawn GSR with updated settings (hot-reload).
pub fn restart_gsr_replay(
    output_dir: String,
    replay_secs: u32,
    fps: u32,
    quality: String,
    bitrate_kbps: Option<u32>,
    monitor_target: String,
    audio_sources: Vec<String>,
) -> Result<(), String> {
    {
        let mut lock = gsr_process().lock().unwrap();
        if let Some(mut gsr_state) = lock.take() {
            gsr_kill_graceful(&mut gsr_state.child);
            log::info!("GSR stopped for restart (SIGINT + wait)");
        }
    }
    start_gsr_replay(
        output_dir,
        replay_secs,
        fps,
        quality,
        bitrate_kbps,
        monitor_target,
        audio_sources,
    )
}

/// Stop the GSR process gracefully (SIGINT → SIGKILL fallback).
pub fn stop_gsr_replay() -> Result<(), String> {
    save_gate().lock().unwrap().buffer_stopped();
    let mut lock = gsr_process().lock().unwrap();
    if let Some(mut gsr_state) = lock.take() {
        gsr_kill_graceful(&mut gsr_state.child);
        log::info!("GSR stopped (SIGINT + wait)");
    }
    Ok(())
}

// ═══ Helper Functions ═══

/// Map a GSR capture target (e.g. "OpenGG_Game.monitor", "default_output") to a friendly
/// audio-track title shown in players. Mirrors the frontend mapping in `utils/audio.ts`.
fn friendly_track_name(target: &str) -> String {
    let t = target.strip_prefix("device:").unwrap_or(target);
    if let Some(ch) = t.strip_prefix("OpenGG_").and_then(|s| s.strip_suffix(".monitor")) {
        return ch.to_string(); // Game / Chat / Media / Aux / Mic
    }
    match t {
        "default_output" => "Desktop".to_string(),
        "default_input" => "Mic".to_string(),
        _ if t.starts_with("alsa_input.") => "Mic".to_string(),
        _ if t.ends_with(".monitor") => "Output".to_string(),
        other => other.to_string(),
    }
}

/// Remux `src` → `dest` with friendly per-audio-track titles (stream-copy, no re-encode).
/// `targets` is the ordered capture list; audio stream N is titled `friendly_track_name(targets[N])`.
/// Returns true only if ffmpeg succeeded and `dest` exists. Any failure → false (caller falls
/// back to a plain rename so saving never regresses).
fn remux_with_track_titles(
    src: &std::path::Path,
    dest: &std::path::Path,
    targets: &[String],
) -> bool {
    if targets.is_empty() {
        return false; // nothing to title — plain rename is fine
    }
    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.args(["-y", "-i"]);
    cmd.arg(src);
    cmd.args(["-map", "0", "-c", "copy", "-movflags", "+faststart"]);
    for (i, target) in targets.iter().enumerate() {
        cmd.arg(format!("-metadata:s:a:{i}"));
        cmd.arg(format!("title={}", friendly_track_name(target)));
    }
    cmd.arg(dest);
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    match cmd.status() {
        Ok(s) if s.success() && dest.exists() => true,
        Ok(s) => {
            log::warn!("ffmpeg remux for track titles exited {s:?}; falling back to rename");
            false
        }
        Err(e) => {
            log::warn!("ffmpeg not available for track titles ({e}); falling back to rename");
            false
        }
    }
}

/// Wait for the clip GSR writes after a save signal to be *complete*.
///
/// The candidate is the newest visible video in `dir` modified after
/// `after_time`. It counts as finished once GSR (`pid`) no longer holds it
/// open and its size is non-zero and unchanged across two polls — or, where
/// `/proc` cannot tell (another user's process), once the size has been
/// stable for a full second. Gives up after `limit`.
fn wait_for_finished_clip(
    pid: u32,
    dir: &str,
    after_time: std::time::SystemTime,
    limit: std::time::Duration,
) -> Option<std::path::PathBuf> {
    const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "webm", "avi", "mov", "ts", "flv"];
    let deadline = std::time::Instant::now() + limit;
    let mut last: Option<(std::path::PathBuf, u64)> = None;
    let mut stable_polls = 0u32;
    loop {
        let found = std::fs::read_dir(dir).ok().and_then(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| !organizer::is_hidden(&e.file_name().to_string_lossy()))
                .filter(|e| {
                    e.path()
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|x| VIDEO_EXTS.contains(&x.to_lowercase().as_str()))
                        .unwrap_or(false)
                })
                .filter(|e| {
                    e.metadata()
                        .and_then(|m| m.modified())
                        .map(|t| t > after_time)
                        .unwrap_or(false)
                })
                .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
                .map(|e| e.path())
        });
        if let Some(path) = found {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if last.as_ref().is_some_and(|(p, s)| *p == path && *s == size) && size > 0 {
                stable_polls += 1;
            } else {
                stable_polls = 0;
            }
            let open = holds_open(pid, &path);
            let done = match open {
                Some(false) => stable_polls >= 1,
                Some(true) => false,
                None => stable_polls >= 5,
            };
            if done {
                return Some(path);
            }
            last = Some((path, size));
        }
        if std::time::Instant::now() >= deadline {
            log::warn!("wait_for_finished_clip: no finished clip in {dir} within {limit:?}");
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Whether process `pid` has `path` open, via `/proc/<pid>/fd`. `None` when
/// that cannot be read (process gone, or not ours).
fn holds_open(pid: u32, path: &std::path::Path) -> Option<bool> {
    let rd = std::fs::read_dir(format!("/proc/{pid}/fd")).ok()?;
    let target = std::fs::canonicalize(path).ok()?;
    Some(rd.flatten().any(|fd| std::fs::read_link(fd.path()).is_ok_and(|l| l == target)))
}

/// Send SIGINT to let GSR flush cleanly, wait up to 2 s, then SIGKILL as fallback.
/// Reaping with `.wait()` prevents zombie PIDs.
/// Also kills `gsr-kms-server` — a helper daemon spawned by GSR that survives
/// the parent process and must be cleaned up explicitly.
fn gsr_kill_graceful(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as libc::pid_t;
        unsafe {
            libc::kill(pid, libc::SIGINT);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => {
                    // Exited cleanly — still need to reap gsr-kms-server
                    kill_kms_server();
                    return;
                }
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                _ => break, // timeout — fall through to SIGKILL
            }
        }
    }
    let _ = child.kill(); // SIGKILL — last resort
    let _ = child.wait(); // reap zombie
    kill_kms_server();
}

/// Kill the `gsr-kms-server` helper daemon that gpu-screen-recorder spawns.
/// It does not exit when the parent process is killed, so we must clean it up
/// explicitly to prevent dangling GPU capture sessions.
fn kill_kms_server() {
    let _ = std::process::Command::new("pkill")
        .args(["-9", "gsr-kms-server"])
        .output();
}

/// Map a requested capture source to the PipeWire node name GSR records from.
///
/// Shared by the recorder (`start_gsr_replay`) and the diagnostic validator
/// (`gsr_diagnostics`) so both agree on what a source resolves to. GSR special names
/// (`default_output`/`default_input`), real monitor sources (`*.monitor`), and hardware
/// capture inputs (`alsa_input.*`) are already valid capture nodes and pass through
/// unchanged. A bare channel name ("Game") maps to its OpenGG monitor; a sink name
/// ("OpenGG_Game") gets its `.monitor` suffix.
pub fn resolve_capture_source(src: &str) -> String {
    if src.contains("default") || src.ends_with(".monitor") || src.starts_with("alsa_input.") {
        src.to_string()
    } else if !src.contains('_') && !src.contains('.') && !src.contains('-') {
        format!("OpenGG_{src}.monitor") // bare channel name e.g. "Game"
    } else {
        format!("{src}.monitor") // a sink name e.g. "OpenGG_Game"
    }
}

/// Detect which monitor output (e.g. "DP-1") the currently focused window is on.
/// Uses xdotool to find the window position, then xrandr to match it to a monitor.
/// Returns None on Wayland, if tools are unavailable, or if detection fails.
fn get_focused_window_monitor() -> Option<String> {
    // Wayland: xdotool is unreliable — bail early
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        log::info!("Wayland detected — skipping xdotool monitor detection");
        return None;
    }

    // Step 1: get the focused window ID (decimal)
    let win_id_out = std::process::Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .ok()?;
    let win_id = String::from_utf8_lossy(&win_id_out.stdout)
        .trim()
        .to_string();
    if win_id.is_empty() {
        return None;
    }

    // Step 2: get the window's absolute position on the desktop
    // xdotool getwindowgeometry output: "Window NNN\n  Position: X,Y (screen N)\n  Geometry: WxH"
    let geom_out = std::process::Command::new("xdotool")
        .args(["getwindowgeometry", "--shell", &win_id])
        .output()
        .ok()?;
    let geom_text = String::from_utf8_lossy(&geom_out.stdout).into_owned();
    // --shell format: "X=123\nY=456\nWIDTH=...\nHEIGHT=..."
    let win_x: i64 = geom_text
        .lines()
        .find(|l| l.starts_with("X="))
        .and_then(|l| l[2..].parse().ok())?;
    let win_y: i64 = geom_text
        .lines()
        .find(|l| l.starts_with("Y="))
        .and_then(|l| l[2..].parse().ok())?;

    // Step 3: parse `xrandr --listmonitors` to get monitor names + offsets
    // Output format per monitor line: "  N: +*DP-1 1920/527x1080/296+0+0  ..."
    let xrandr_out = std::process::Command::new("xrandr")
        .arg("--listmonitors")
        .output()
        .ok()?;
    let xrandr_text = String::from_utf8_lossy(&xrandr_out.stdout).into_owned();

    // Each monitor line: "  0: +*DP-1 1920/527x1080/296+0+0   0"
    // Geometry token: "<w>/<mm>x<h>/<mm>+<ox>+<oy>"
    let mut best: Option<String> = None;
    for line in xrandr_text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        // parts[1] is monitor name (may have leading '+' or '*' flags)
        let name = parts[1].trim_start_matches('+').trim_start_matches('*');
        // parts[2] is geometry token
        let geom_tok = parts[2];
        // Parse "WxH+OX+OY" or "W/mmxH/mm+OX+OY"
        let geom_clean = geom_tok.split('+').collect::<Vec<_>>();
        if geom_clean.len() < 3 {
            continue;
        }
        let ox: i64 = geom_clean[1].parse().ok()?;
        let oy: i64 = geom_clean[2].parse().ok()?;
        // Width/height may be "1920/527" or plain "1920"
        let wh = geom_clean[0];
        let (w_part, h_part) = wh.split_once('x')?;
        let w: i64 = w_part.split('/').next()?.parse().ok()?;
        let h: i64 = h_part.split('/').next()?.parse().ok()?;
        // Check if window origin falls inside this monitor's rectangle
        if win_x >= ox && win_x < ox + w && win_y >= oy && win_y < oy + h {
            best = Some(name.to_string());
            // Prefer primary (marked with '*')
            if parts[1].contains('*') {
                break;
            }
        }
    }
    if let Some(ref mon) = best {
        log::info!("Focused window (id={win_id}) at ({win_x},{win_y}) → monitor {mon}");
    }
    best
}

/// Detect the primary monitor's resolution using xrandr.
fn detect_primary_resolution() -> String {
    if let Ok(o) = std::process::Command::new("xrandr")
        .arg("--current")
        .output()
    {
        let text = String::from_utf8_lossy(&o.stdout);
        let mut any_connected: Option<String> = None;
        for line in text.lines() {
            if line.contains(" connected") && !line.contains(" disconnected") {
                for word in line.split_whitespace() {
                    // Resolution tokens look like "1920x1080+0+0"
                    if word.contains('x')
                        && word.chars().next().is_some_and(|c| c.is_ascii_digit())
                    {
                        let res = word.split('+').next().unwrap_or(word).to_string();
                        if res.split('x').count() == 2 {
                            if line.contains("primary") {
                                return res; // prefer explicitly marked primary
                            }
                            any_connected.get_or_insert(res);
                        }
                    }
                }
            }
        }
        if let Some(r) = any_connected {
            return r;
        }
    }
    log::warn!(
        "detect_primary_resolution: xrandr failed or no connected display; defaulting to 1920x1080"
    );
    "1920x1080".to_string()
}

/// Map a GSR capture target for Wayland compatibility.
/// On Wayland, X11-only targets like "screen" and "focused" must be mapped to "portal".
/// Connector names are passed through unchanged for direct KMS capture.
pub fn map_gsr_target_for_wayland(target: &str) -> String {
    let on_wayland = std::env::var_os("XDG_SESSION_TYPE")
        .map(|s| s == "wayland")
        .unwrap_or(false);

    if !on_wayland {
        // X11: pass through unchanged
        return target.to_string();
    }

    // Wayland: only map X11-only/ambiguous values; pass through connector names
    match target {
        // X11-only or ambiguous → use portal
        "screen" | "focused" | "" => "portal".to_string(),
        // portal is idempotent
        "portal" => "portal".to_string(),
        // Connector names: pass through unchanged for direct KMS capture
        // Pattern: DP-*, HDMI-*, eDP-*, LVDS-*, etc. (any alphanumeric-hyphenated connector)
        other => other.to_string(),
    }
}

/// A connected monitor, as reported by `gpu-screen-recorder --list-monitors`.
#[derive(Serialize, Clone, Debug)]
pub struct MonitorInfo {
    pub name: String,
    pub label: String,
}

/// Returns a list of connected monitors via gpu-screen-recorder's own enumeration —
/// NOT Tauri's/Qt's windowing-toolkit monitor APIs, which return EDID model names
/// ("BenQ GW2780", "MASI251K03") that are NOT valid GSR `-w` targets and cause an
/// immediate GSR crash if passed. `--list-monitors` returns the exact X11/Wayland
/// connector names (e.g. "DP-1", "HDMI-A-1", "screen") its `-w` flag accepts.
pub fn list_monitors() -> Vec<MonitorInfo> {
    let output = std::process::Command::new("gpu-screen-recorder")
        .arg("--list-monitors")
        .output();

    let stdout = match output {
        Ok(o) if !o.stdout.is_empty() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => String::new(),
    };

    let mut monitors: Vec<MonitorInfo> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|name| {
            // "screen" is GSR's "capture all outputs / entire desktop" pseudo-target.
            let label = if name == "screen" {
                "Entire Desktop".to_string()
            } else {
                name.to_string()
            };
            MonitorInfo {
                name: name.to_string(),
                label,
            }
        })
        .collect();

    if monitors.is_empty() {
        // GSR not installed, not in PATH, or no displays detected — safe fallback.
        monitors.push(MonitorInfo {
            name: "screen".into(),
            label: "Entire Desktop".into(),
        });
    }

    monitors
}

/// Resolve distro family from ID and ID_LIKE fields (GSR's own distro-family
/// resolver — a slightly wider Arch-derivative list than
/// `crate::system::get_dependency_status`'s install-hint resolver, since GSR's
/// driver-install hints benefit from covering popular Arch derivatives directly).
fn resolve_distro_family(id: &str, id_like: &str) -> &'static str {
    match id {
        "arch" | "manjaro" | "endeavouros" | "garuda" | "cachyos" => return "arch",
        "debian" | "ubuntu" => return "debian",
        "fedora" | "rhel" | "centos" => return "fedora",
        _ => {}
    }
    let id_like_lower = id_like.to_lowercase();
    if id_like_lower.contains("arch") {
        return "arch";
    }
    if id_like_lower.contains("debian") || id_like_lower.contains("ubuntu") {
        return "debian";
    }
    if id_like_lower.contains("fedora") || id_like_lower.contains("rhel") || id_like_lower.contains("centos") {
        return "fedora";
    }
    "unknown"
}

/// Detect the host Linux distribution by reading /etc/os-release.
fn detect_distro() -> &'static str {
    let info = crate::system::get_distro_info();
    resolve_distro_family(&info.id, &info.id_like)
}

/// A single actionable fix that the UI can present with a copy button.
#[derive(Serialize, Clone, Debug)]
pub struct DiagnosticFix {
    pub command: String,
    pub description: String,
}

/// A single diagnostic item with optional fix guidance.
#[derive(Serialize, Clone, Debug)]
pub struct DiagnosticItem {
    pub message: String,
    pub severity: String,
    pub fix: Option<DiagnosticFix>,
}

/// Structured result from the GSR pre-flight diagnostic suite.
#[derive(Serialize, Clone, Debug)]
pub struct GsrDiagnosticResult {
    pub ok: bool,
    pub gsr_installed: bool,
    pub gsr_version: Option<String>,
    pub in_render_group: bool,
    pub in_video_group: bool,
    pub gpu_encoder_available: bool,
    pub audio_sources_ok: bool,
    pub missing_audio_sources: Vec<String>,
    pub items: Vec<DiagnosticItem>,
    pub report: String,
}

/// Run a comprehensive pre-flight check before attempting to start GSR.
/// Returns a structured report the UI can turn into actionable messages.
pub fn gsr_diagnostics(audio_sources: Vec<String>, monitor_target: String) -> GsrDiagnosticResult {
    let distro = detect_distro();

    let mut result = GsrDiagnosticResult {
        ok: true,
        gsr_installed: false,
        gsr_version: None,
        in_render_group: false,
        in_video_group: false,
        gpu_encoder_available: false,
        audio_sources_ok: true,
        missing_audio_sources: Vec::new(),
        items: Vec::new(),
        report: String::new(),
    };

    // Distro-specific install commands
    // For arch family: prefer pacman; AUR -git variant available for testing
    // For debian/fedora/unknown: use Flatpak (universal, PPA is deprecated)
    let (gsr_install_cmd, install_desc) = match distro {
        "arch" => (
            "sudo pacman -S gpu-screen-recorder",
            "gpu-screen-recorder is available in the Arch repos. If not available, try the AUR: yay -S gpu-screen-recorder-git",
        ),
        "fedora" => (
            "flatpak install flathub com.dec05eba.gpu_screen_recorder",
            "Use Flatpak for a reliable cross-distro installation.",
        ),
        _ => (
            "flatpak install flathub com.dec05eba.gpu_screen_recorder",
            "Use Flatpak for a reliable cross-distro installation.",
        ),
    };
    let driver_hint = match distro {
        "arch" => "Install proprietary GPU drivers (e.g. sudo pacman -S nvidia-utils or mesa-va-drivers) for hardware encoding.",
        "fedora" => "Install proprietary GPU drivers (e.g. sudo dnf install mesa-va-drivers) for hardware encoding.",
        _ => "Install proprietary GPU drivers (e.g. sudo apt install mesa-va-drivers) for hardware encoding.",
    };

    // 1. Binary presence + version
    // Prefer --version (plain output) but fall back to --help if it doesn't work
    let gsr_version_output = std::process::Command::new("gpu-screen-recorder")
        .arg("--version")
        .output();

    match gsr_version_output {
        Ok(o) if o.status.success() => {
            result.gsr_installed = true;
            // --version output is plain: "5.13.9"
            let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
            // Extract version matching ^\d+\.\d+ pattern
            if let Some(cap) = text.split('\n').next() {
                if cap.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    result.gsr_version = Some(cap.to_string());
                }
            }
        }
        _ => {
            // --version failed; try --help as fallback (less reliable)
            if let Ok(o) = std::process::Command::new("gpu-screen-recorder")
                .arg("--help")
                .output()
            {
                if o.status.success() {
                    result.gsr_installed = true;
                    // Help output starts with "usage:" — no version in first line
                    let text = String::from_utf8_lossy(&o.stdout);
                    for line in text.lines() {
                        if line.contains("version") || line.contains("Version") {
                            if let Some(ver) = line.split_whitespace().find_map(|w| {
                                if w.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
                                    && w.contains('.')
                                {
                                    Some(w.to_string())
                                } else {
                                    None
                                }
                            }) {
                                result.gsr_version = Some(ver);
                                break;
                            }
                        }
                    }
                }
            } else {
                result.ok = false;
                result.items.push(DiagnosticItem {
                    message: "gpu-screen-recorder not found in PATH. Install it via your package manager.".into(),
                    severity: "error".into(),
                    fix: Some(DiagnosticFix {
                        command: gsr_install_cmd.into(),
                        description: install_desc.into(),
                    }),
                });
            }
        }
    }

    // 2. Group membership
    if let Ok(o) = std::process::Command::new("id").args(["-Gn"]).output() {
        let group_str = String::from_utf8_lossy(&o.stdout);
        result.in_render_group = group_str.contains("render");
        result.in_video_group = group_str.contains("video");
    }
    if !result.in_render_group {
        result.ok = false;
        result.items.push(DiagnosticItem {
            message: "Your user is not in the 'render' group. Run: sudo usermod -aG render,video $USER — then re-login.".into(),
            severity: "error".into(),
            fix: Some(DiagnosticFix {
                command: "sudo usermod -aG render,video $USER".into(),
                description: "Add your user to the required groups, then re-login.".into(),
            }),
        });
    }
    if !result.in_video_group {
        result.items.push(DiagnosticItem {
            message: "Your user is not in the 'video' group. Some capture modes may fail. Run: sudo usermod -aG video $USER — then re-login.".into(),
            severity: "warning".into(),
            fix: Some(DiagnosticFix {
                command: "sudo usermod -aG video $USER".into(),
                description: "Add your user to the video group, then re-login.".into(),
            }),
        });
    }

    // 3. GPU encoder probe (ffmpeg -encoders is the most portable check)
    if let Ok(o) = std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
    {
        let enc = String::from_utf8_lossy(&o.stdout);
        result.gpu_encoder_available =
            enc.contains("h264_vaapi") || enc.contains("h264_nvenc") || enc.contains("h264_amf");
    }
    if !result.gpu_encoder_available {
        result.items.push(DiagnosticItem {
            message: "No GPU encoder (h264_vaapi / h264_nvenc / h264_amf) detected. GSR may fall back to software encoding or fail.".into(),
            severity: "warning".into(),
            fix: Some(DiagnosticFix {
                command: "".into(),
                description: driver_hint.into(),
            }),
        });
    }

    // 4. Audio monitor sources
    let sources_short = std::process::Command::new("pactl")
        .args(["list", "sources", "short"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    for src in &audio_sources {
        // Use the SAME resolver as the recorder so hardware inputs (alsa_input.*) and
        // output monitors (*.monitor) validate identically and aren't falsely flagged.
        let cap = resolve_capture_source(src);
        // GSR resolves default_output/default_input itself — always treat as present.
        if cap.contains("default") {
            continue;
        }
        // pactl `list sources short` is tab-separated: "<id>\t<name>\t..." — match the name
        // column exactly so a substring of a longer node name can't give a false positive.
        let present = sources_short
            .lines()
            .any(|l| l.split('\t').nth(1) == Some(cap.as_str()));
        if !present {
            result.audio_sources_ok = false;
            result.missing_audio_sources.push(cap);
        }
    }
    if !result.audio_sources_ok {
        // The virtual engine is optional — missing channel monitors is informational, not a
        // blocker. Recording falls back to the real default devices (default_output/input).
        let missing = result.missing_audio_sources.join(", ");
        result.items.push(DiagnosticItem {
            message: format!("Virtual audio sources not found ({missing}). Recording will use your default output + microphone. Create the Virtual Audio Engine for per-channel capture."),
            severity: "info".into(),
            fix: None,
        });
    }

    // ── Build the copyable diagnostic report (session, monitors, resolved target,
    //    and a REAL test capture's stderr) so failures on other machines are visible. ──
    result.report = build_gsr_report(&result, &monitor_target);

    result
}

/// Normalize a saved gsrMonitorTarget the same way `start_gsr_replay` does, then apply the
/// Wayland mapping — so the diagnostic tests exactly what a real recording would use.
fn resolve_gsr_target(monitor_target: &str) -> String {
    let connector = monitor_target.split('|').next().unwrap_or("").trim().to_string();
    let looks_invalid = connector.is_empty()
        || connector.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
        || connector.contains(' ');
    let base = if looks_invalid { "screen".to_string() } else { connector };
    let base = match base.as_str() {
        "focused" => get_focused_window_monitor().unwrap_or_else(|| "focused".to_string()),
        "" => "screen".to_string(),
        other => other.to_string(),
    };
    map_gsr_target_for_wayland(&base)
}

/// Spawn gpu-screen-recorder for ~1.3s against `target`, then SIGINT it, capturing the real
/// stderr + exit status. This reproduces the exact failure a recording would hit.
fn gsr_test_capture(target: &str) -> String {
    let tmp = std::env::temp_dir().join("opengg_gsr_diag_test.mp4");
    let mut cmd = std::process::Command::new("gpu-screen-recorder");
    cmd.args(["-w", target, "-f", "30", "-c", "mp4", "-o"]);
    cmd.arg(&tmp);
    cmd.stderr(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::null());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return format!("test capture failed to spawn gpu-screen-recorder: {e}"),
    };
    let stderr_buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    if let Some(stderr) = child.stderr.take() {
        let buf = Arc::clone(&stderr_buf);
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                buf.lock().unwrap().push(line);
            }
        });
    }
    // Give it time to either crash or reach a steady recording state.
    std::thread::sleep(std::time::Duration::from_millis(1300));
    let outcome = match child.try_wait() {
        Ok(Some(status)) => format!("EXITED EARLY with {status:?} (failure)"),
        Ok(None) => {
            // Still running → capture works. Stop it cleanly.
            let _ = std::process::Command::new("kill")
                .args(["-SIGINT", &child.id().to_string()])
                .output();
            std::thread::sleep(std::time::Duration::from_millis(400));
            let _ = child.kill();
            let _ = child.wait();
            "OK — capture ran for ~1.3s without crashing".to_string()
        }
        Err(e) => format!("could not poll test process: {e}"),
    };
    let _ = std::fs::remove_file(&tmp);
    let tail = {
        let b = stderr_buf.lock().unwrap();
        b.iter().rev().take(20).rev().cloned().collect::<Vec<_>>().join("\n")
    };
    if tail.is_empty() {
        format!("Result: {outcome}\n(stderr was empty)")
    } else {
        format!("Result: {outcome}\nstderr:\n{tail}")
    }
}

/// Assemble the full copyable diagnostic dump.
fn build_gsr_report(result: &GsrDiagnosticResult, monitor_target: &str) -> String {
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "(unset)".into());
    let wayland = if std::env::var_os("WAYLAND_DISPLAY").is_some() { "yes" } else { "no" };
    let xdg_desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "(unset)".into());
    let monitors = list_monitors_raw();
    let resolved = resolve_gsr_target(monitor_target);
    let test = gsr_test_capture(&resolved);

    format!(
        "=== OpenGG GPU Screen Recorder Diagnostics ===\n\
         distro: {distro}\n\
         session type: {session}   wayland: {wayland}   desktop: {xdg_desktop}\n\
         gpu-screen-recorder installed: {installed}  version: {version}\n\
         groups: render={render} video={video}\n\
         gpu encoder (vaapi/nvenc/amf) available: {enc}\n\
         configured monitor target: {target:?}\n\
         resolved capture target: {resolved:?}\n\
         detected monitors (gpu-screen-recorder --list-monitors):\n{monitors}\n\
         audio sources requested: {audio:?}  (missing: {missing:?})\n\
         --- real test capture ---\n{test}\n",
        distro = detect_distro(),
        session = session,
        wayland = wayland,
        xdg_desktop = xdg_desktop,
        installed = result.gsr_installed,
        version = result.gsr_version.clone().unwrap_or_else(|| "(unknown)".into()),
        render = result.in_render_group,
        video = result.in_video_group,
        enc = result.gpu_encoder_available,
        target = monitor_target,
        resolved = resolved,
        monitors = monitors,
        audio = "(see settings)",
        missing = result.missing_audio_sources,
        test = test,
    )
}

/// Raw `gpu-screen-recorder --list-monitors` output (newline-joined), for the report.
fn list_monitors_raw() -> String {
    match std::process::Command::new("gpu-screen-recorder").arg("--list-monitors").output() {
        Ok(o) if !o.stdout.is_empty() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Ok(o) => format!("(none; exit={:?})", o.status.code()),
        Err(e) => format!("(failed to run: {e})"),
    }
}

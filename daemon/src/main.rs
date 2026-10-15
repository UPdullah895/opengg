use anyhow::Result;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

mod audio;
mod config;
mod device;
mod diag;
mod extensions;
mod ipc;
mod platform;
mod replay;
mod statelog;
mod subprocess;

const USAGE: &str = "\
openggd - OpenGG background daemon

USAGE:
    openggd              run the daemon (normally started by systemd / D-Bus activation)
    openggd --version    print the version and exit
    openggd --diagnose   print a JSON health report (tools, PipeWire, sinks, hotkey
                         permissions, config) and exit; does not need a running daemon
    openggd --help       show this text

The running daemon serves the same report over D-Bus:
    busctl --user call org.opengg.Daemon /org/opengg/Daemon org.opengg.Daemon.Core GetDiagnostics";

/// Handle `--version` / `--diagnose` / `--help` before any service setup.
/// Returns true when the process should exit instead of running the daemon.
fn handle_cli() -> bool {
    let mut args = std::env::args().skip(1);
    let Some(arg) = args.next() else { return false };
    match arg.as_str() {
        "--version" | "-V" => println!("openggd {}", diag::version()),
        "--diagnose" => println!("{}", diag::collect_pretty()),
        "--help" | "-h" => println!("{USAGE}"),
        other => {
            eprintln!("openggd: unknown argument {other:?}\n\n{USAGE}");
            std::process::exit(2);
        }
    }
    true
}

/// Logging setup. ANSI colour only when stdout is a terminal (journald and
/// pasted logs were full of escape codes), a malformed `RUST_LOG` is reported
/// instead of silently replaced, and panics go through tracing so they land in
/// the journal with a location instead of only on a stderr nobody reads.
fn init_logging() {
    use std::io::IsTerminal;

    let mut filter_problem = None;
    let filter = match std::env::var("RUST_LOG") {
        Ok(v) => match EnvFilter::try_new(&v) {
            Ok(f) => f,
            Err(e) => {
                filter_problem = Some(format!("ignoring invalid RUST_LOG={v:?} ({e}); using \"info\""));
                EnvFilter::new("info")
            }
        },
        Err(_) => EnvFilter::new("info"),
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(std::io::stdout().is_terminal())
        .init();

    if let Some(p) = filter_problem {
        warn!("{p}");
    }

    std::panic::set_hook(Box::new(|info| {
        let thread = std::thread::current();
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(non-string panic payload)".into());
        let loc = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".into());
        error!(
            "PANIC in thread '{}' at {loc}: {msg}",
            thread.name().unwrap_or("<unnamed>")
        );
    }));
}

#[tokio::main]
async fn main() -> Result<()> {
    if handle_cli() {
        return Ok(());
    }

    init_logging();
    diag::mark_started();

    info!("openggd v{} starting", env!("CARGO_PKG_VERSION"));

    // A corrupt daemon.toml no longer aborts startup (it used to crash-loop
    // under Restart=on-failure): the bad file is kept aside, an ERROR names the
    // parse error, and the daemon runs on defaults.
    let loaded = config::load();
    diag::set_config_problem(loaded.problem);
    let cfg = loaded.config;
    info!("Config loaded from {:?}", config::config_path());

    // ── Probe runtime dependencies ──────────────────────────────
    let required_bins = vec![
        ("pactl", "audio mixer"),
        ("pw-link", "audio routing"),
        ("gpu-screen-recorder", "replay/clipping"),
        ("headsetcontrol", "headset control"),
        ("ffmpeg", "clip export"),
    ];
    for (bin, feature) in required_bins {
        if !subprocess::is_available(bin) {
            warn!("{} not found — {} disabled ({})", bin, feature, subprocess::install_hint(bin));
        }
    }

    // ── Module 1: Audio Hub ─────────────────────────────────────
    // Construction is instant: sink creation happens in the hub's background
    // reconciler (started after the bus name is owned), so a slow or absent
    // pipewire-pulse can never keep the daemon off the bus.
    let audio_module = if cfg.modules.audio_enabled {
        info!("Audio Hub module: ENABLED");
        match audio::AudioHub::new(&cfg.audio).await {
            Ok(hub) => {
                diag::set_module("audio", "running");
                diag::set_audio_health(hub.health_handle());
                Some(hub)
            }
            Err(e) => {
                error!("Audio Hub init failed - audio module disabled: {e:#}");
                diag::set_module("audio", format!("failed: {e:#}"));
                None
            }
        }
    } else {
        diag::set_module("audio", "disabled");
        None
    };
    let audio_handle = audio_module.clone();

    // ── Module 2: Device Manager ────────────────────────────────
    let device_module = if cfg.modules.device_enabled {
        info!("Device Manager module: ENABLED");
        device::profiles::ProfileManager::create_example_profile(&cfg.device.profiles_dir).ok();
        diag::set_module("device", "running");
        Some(device::DeviceInterface::new(cfg.device.profiles_dir.clone()).await)
    } else {
        diag::set_module("device", "disabled");
        None
    };

    // ── Module 3: Replay & Clipping ─────────────────────────────
    let replay_module = if cfg.modules.replay_enabled {
        info!("Replay module: ENABLED");
        let recorder = replay::Recorder::new(
            &cfg.replay.clips_dir, cfg.replay.fps, &cfg.replay.quality,
        );
        diag::set_module("replay", "running");
        Some(replay::ReplayInterface::new(recorder))
    } else {
        diag::set_module("replay", "disabled");
        None
    };
    // Taken before the interface moves into `serve`. The listener needs the
    // D-Bus connection to emit on and `serve` is what creates it, so the
    // handle is posted back into this slot afterwards.
    let hotkey_slot = replay_module.as_ref().map(|r| r.hotkey_slot());

    // ── Extensions manager ──────────────────────────────────────
    let extensions = std::sync::Arc::new(extensions::ExtensionManager::new());

    // ── Register D-Bus ──────────────────────────────────────────
    let conn = match ipc::serve(audio_module, device_module, replay_module, extensions.clone()).await {
        Ok(c) => c,
        Err(e) => {
            error!(
                "Cannot publish org.opengg.Daemon on the session bus: {e:#} \
                 (is another openggd already running? check `systemctl --user status openggd`)"
            );
            return Err(e);
        }
    };
    info!("D-Bus service: org.opengg.Daemon — ready");

    // The bus name is ours: now (and only now) touch PipeWire.
    if let Some(hub) = &audio_handle {
        hub.start();
    }

    // ── Global hotkeys ──────────────────────────────────────────
    // Started from daemon.toml's bindings; the app replaces them over
    // `SetHotkeys` as soon as it connects, so what the user edits in
    // Settings → Shortcuts is what actually fires.
    if let Some(slot) = hotkey_slot {
        let sc = &cfg.replay.shortcuts;
        let bindings = replay::hotkey::Bindings::parse(
            &sc.save_replay, &sc.toggle_recording, &sc.screenshot,
        );
        let handle = replay::hotkey::spawn(&conn, bindings);
        diag::set_hotkeys_active(handle.is_some());
        *slot.lock().unwrap_or_else(|p| p.into_inner()) = handle;
    }

    // ── Discover + start enabled daemon extensions ──────────────
    extensions.start_all().await;

    // ── Process watcher for auto-profile ────────────────────────
    if cfg.modules.device_enabled && !cfg.device.game_profiles.is_empty() {
        let watcher = device::ProcessWatcher::new();
        let exe_names: Vec<String> = cfg.device.game_profiles.keys().cloned().collect();
        watcher.watch_executables(exe_names).await;
        let mut rx = watcher.start();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                info!("Process event: {event:?}");
            }
        });
    }

    // Wait for either SIGINT (Ctrl-C) or SIGTERM (systemctl stop / session
    // logout). Without the SIGTERM arm, `systemctl --user stop openggd` and
    // most desktop-session shutdowns kill the process without ever reaching
    // this `Ok(())` return, so `SinkManager`'s `Drop`-based cleanup never
    // runs and its pactl-loaded sink modules leak until PipeWire itself
    // restarts. SIGKILL still can't be handled — that's a hard OS limit —
    // but SIGTERM is the far more common real-world shutdown signal.
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        res = tokio::signal::ctrl_c() => { res?; info!("Received SIGINT"); }
        _ = sigterm.recv() => { info!("Received SIGTERM"); }
    }
    info!("Shutting down…");
    Ok(())
}

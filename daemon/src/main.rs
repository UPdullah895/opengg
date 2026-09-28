use anyhow::Result;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

mod audio;
mod config;
mod device;
mod extensions;
mod ipc;
mod platform;
mod replay;
mod subprocess;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("openggd v{} starting", env!("CARGO_PKG_VERSION"));

    let cfg = config::load()?;
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
            warn!("{} not found — {} disabled", bin, feature);
        }
    }

    // ── Module 1: Audio Hub ─────────────────────────────────────
    let audio_module = if cfg.modules.audio_enabled {
        info!("Audio Hub module: ENABLED");
        match audio::AudioHub::new(&cfg.audio).await {
            Ok(hub) => Some(hub),
            Err(e) => { warn!("Audio Hub init failed: {e}"); None }
        }
    } else { None };

    // ── Module 2: Device Manager ────────────────────────────────
    let device_module = if cfg.modules.device_enabled {
        info!("Device Manager module: ENABLED");
        device::profiles::ProfileManager::create_example_profile(&cfg.device.profiles_dir).ok();
        Some(device::DeviceInterface::new(cfg.device.profiles_dir.clone()).await)
    } else { None };

    // ── Module 3: Replay & Clipping ─────────────────────────────
    let replay_module = if cfg.modules.replay_enabled {
        info!("Replay module: ENABLED");
        let recorder = replay::Recorder::new(
            &cfg.replay.clips_dir, cfg.replay.fps, &cfg.replay.quality,
        );
        Some(replay::ReplayInterface::new(recorder))
    } else { None };
    // Taken before the interface moves into `serve`. The listener needs the
    // D-Bus connection to emit on and `serve` is what creates it, so the
    // handle is posted back into this slot afterwards.
    let hotkey_slot = replay_module.as_ref().map(|r| r.hotkey_slot());

    // ── Extensions manager ──────────────────────────────────────
    let extensions = std::sync::Arc::new(extensions::ExtensionManager::new());

    // ── Register D-Bus ──────────────────────────────────────────
    let conn = ipc::serve(audio_module, device_module, replay_module, extensions.clone()).await?;
    info!("D-Bus service: org.opengg.Daemon — ready");

    // ── Global hotkeys ──────────────────────────────────────────
    // Started from daemon.toml's bindings; the app replaces them over
    // `SetHotkeys` as soon as it connects, so what the user edits in
    // Settings → Shortcuts is what actually fires.
    if let Some(slot) = hotkey_slot {
        let sc = &cfg.replay.shortcuts;
        let bindings = replay::hotkey::Bindings::parse(
            &sc.save_replay, &sc.toggle_recording, &sc.screenshot,
        );
        *slot.lock().unwrap() = replay::hotkey::spawn(&conn, bindings);
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

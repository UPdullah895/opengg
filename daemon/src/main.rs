use anyhow::Result;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

mod audio;
mod config;
mod device;
mod ipc;
mod replay;

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
        Some(device::DeviceInterface::new().await)
    } else { None };

    // ── Module 3: Replay & Clipping ─────────────────────────────
    let replay_module = if cfg.modules.replay_enabled {
        info!("Replay module: ENABLED");
        let recorder = replay::Recorder::new(
            &cfg.replay.clips_dir, cfg.replay.fps, &cfg.replay.quality,
        );
        Some(replay::ReplayInterface::new(recorder))
    } else { None };

    // ── Register D-Bus ──────────────────────────────────────────
    let _conn = ipc::serve(audio_module, device_module, replay_module).await?;
    info!("D-Bus service: org.opengg.Daemon — ready");

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

    tokio::signal::ctrl_c().await?;
    info!("Shutting down…");
    Ok(())
}

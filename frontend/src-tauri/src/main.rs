#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod media_server;
mod subprocess;
mod vu_native;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, WindowEvent,
};

const VIDEO_WATCH_EXTS: &[&str] = &["mp4", "mkv", "webm", "avi", "mov", "ts", "flv"];

/// GSR auto-start parameters: (output_dir, replay_secs, fps, quality, bitrate_kbps, monitor_target, audio_sources)
type GsrAutoParams = (String, u32, u32, String, Option<u32>, String, Vec<String>);

/// Read all directories to watch from the saved settings JSON.
fn get_watch_dirs() -> Vec<std::path::PathBuf> {
    let sp = commands::settings_path_pub();
    let mut dirs: Vec<std::path::PathBuf> = vec![];
    if sp.exists() {
        if let Ok(j) = std::fs::read_to_string(&sp) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&j) {
                if let Some(arr) = v["settings"]["clip_directories"].as_array() {
                    for s in arr {
                        if let Some(p) = s.as_str() {
                            let pb = std::path::PathBuf::from(commands::shexp(p));
                            if !dirs.contains(&pb) {
                                dirs.push(pb);
                            }
                        }
                    }
                }
            }
        }
    }
    if dirs.is_empty() {
        dirs.push(commands::default_clips_dir());
    }
    dirs
}

// ══════════════════════════════════════════════════════
//  ★ EPIC 2: File-based crash / info logger
// ══════════════════════════════════════════════════════

/// How many log files to retain before pruning the oldest.
const MAX_LOG_FILES: usize = 10;

/// Returns the runtime log directory (`$XDG_DATA_HOME/opengg/logs`, fallback `/tmp/opengg/logs`).
pub fn logs_dir() -> std::path::PathBuf {
    let p = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("opengg/logs");
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Delete all but the `MAX_LOG_FILES` most recent `opengg_*.log` files in the
/// log directory. Called once at startup so a new session counts against the cap.
fn prune_old_logs(dir: &std::path::Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::path::PathBuf, std::time::SystemTime)> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_string_lossy().to_string();
            if !p.is_file() {
                return None;
            }
            if !name.starts_with("opengg_") || !name.ends_with(".log") {
                return None;
            }
            let mtime = e.metadata().ok()?.modified().ok()?;
            Some((p, mtime))
        })
        .collect();
    // Newest first
    files.sort_by_key(|b| std::cmp::Reverse(b.1));
    for (p, _) in files.into_iter().skip(MAX_LOG_FILES.saturating_sub(1)) {
        // Leave (MAX_LOG_FILES - 1) existing files + the one we're about to create
        let _ = std::fs::remove_file(p);
    }
}

/// Local-time "YYYY-MM-DD_HH-MM-SS" for log filenames. Uses libc::localtime_r
/// on unix so we don't need a chrono dep just for the log filename.
#[cfg(unix)]
fn local_ts_filename() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        format!(
            "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

#[cfg(not(unix))]
fn local_ts_filename() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

fn init_logging() {
    use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};
    use std::fs::OpenOptions;

    let log_dir = logs_dir();
    prune_old_logs(&log_dir);

    // One log file per session, timestamped. Session boundaries are visible
    // and we never reopen a giant append-only file.
    let log_name = format!("opengg_{}.log", local_ts_filename());
    let log_file = log_dir.join(&log_name);

    // Silence noisy transitive crates (zbus/zvariant/tao/wry/tracing plumbing).
    // Info-level logging from these crates was producing hundreds of lines per
    // DBus call and measurably slowing down the main thread via log-write syscalls.
    let config = ConfigBuilder::new()
        .add_filter_ignore_str("zbus")
        .add_filter_ignore_str("zvariant")
        .add_filter_ignore_str("zbus_names")
        .add_filter_ignore_str("tracing")
        .add_filter_ignore_str("tao")
        .add_filter_ignore_str("wry")
        .add_filter_ignore_str("webkit2gtk")
        .add_filter_ignore_str("hyper")
        .add_filter_ignore_str("reqwest")
        .add_filter_ignore_str("tokio")
        .add_filter_ignore_str("mio")
        .add_filter_ignore_str("polling")
        .add_filter_ignore_str("async_io")
        .set_time_format_rfc3339()
        .build();

    if let Ok(file) = OpenOptions::new().create(true).append(true).open(&log_file) {
        let _ = WriteLogger::init(LevelFilter::Info, config, file);
    }

    // Catch panics and write them to the same log file
    let lf = log_file.clone();
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!("{info}");
        eprintln!("[PANIC] {msg}");
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&lf) {
            use std::io::Write;
            let _ = writeln!(f, "[PANIC] {msg}");
        }
    }));

    log::info!("=== OpenGG started → {} ===", log_file.display());
}

fn main() {
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .invoke_handler(tauri::generate_handler![
            // Audio
            commands::get_channels,
            commands::set_volume,
            commands::set_mute,
            commands::get_apps,
            commands::route_app,
            commands::get_audio_devices,
            commands::set_channel_device,
            commands::set_app_volume,
            commands::unmute_media_streams,
            commands::start_vu_stream,
            commands::stop_vu_stream,
            // Replay
            commands::get_recorder_status,
            commands::start_replay,
            commands::stop_recorder,
            commands::save_replay,
            // Clips
            commands::get_clips,
            commands::get_clips_fast,
            commands::probe_clips,
            commands::generate_thumbnail,
            commands::generate_thumbnails_batch,
            commands::set_clip_meta,
            commands::get_clip_meta,
            commands::take_screenshot,
            commands::clear_thumbnail_cache,
            commands::delete_clip,
            commands::trim_clip,
            commands::export_clip_sized,
            commands::save_trim_state,
            commands::get_trim_state,
            commands::open_file_location,
            // ★ Power User: single-file fetch for watcher
            commands::get_clip_by_path,
            // Editor
            commands::analyze_media,
            commands::rename_clip,
            commands::export_timeline,
            commands::calc_export_settings,
            commands::export_clip_with_filters,
            commands::generate_waveform,
            commands::export_with_progress,
            commands::cancel_export,
            commands::write_clipboard,
            // Recording
            commands::start_screen_recording,
            commands::stop_screen_recording,
            // Theme + Settings
            commands::load_theme,
            commands::save_theme,
            commands::get_media_server_port,
            commands::get_media_server_token,
            commands::save_ui_settings,
            commands::load_ui_settings,
            commands::get_storage_info,
            commands::open_locales_folder,
            commands::list_user_locales,
            commands::scan_extensions,
            commands::set_extension_enabled,
            commands::open_extensions_folder,
            commands::fetch_extension_registry,
            // ★ Epic 2: Crash log
            commands::open_crash_logs_folder,
            // ★ Epic 4: Background daemon + autostart
            commands::quit_app,
            commands::set_run_in_background,
            commands::get_autostart,
            commands::set_autostart,
            // ★ Session 3: Home page clip count
            commands::get_clips_count,
            // ★ Virtual audio onboarding + factory reset + routing hydration
            commands::check_virtual_audio_status,
            commands::create_virtual_audio,
            commands::remove_virtual_audio,
            commands::hydrate_audio_routing,
            // ★ Job #3: Optimization & Features
            commands::scan_folder_recursive,
            commands::get_steam_games,
            // ★ Epic 1C: Global OS shortcuts
            commands::register_global_shortcuts,
            // ★ GPU Screen Recorder
            commands::check_gsr_installed,
            commands::gsr_diagnostics,
            commands::start_gsr_replay,
            commands::save_gsr_replay,
            commands::stop_gsr_replay,
            commands::is_gsr_running,
            commands::restart_gsr_replay,
            commands::get_active_window_title,
            commands::list_audio_sinks,
            commands::list_capture_sources,
            commands::get_session_type,
            commands::list_monitors,
            commands::show_clip_notification,
            // ★ DSP: EQ engine + effect stubs
            commands::apply_eq,
            commands::apply_noise_gate,
            commands::apply_compressor,
            commands::apply_noise_reduction,
            commands::start_eq_engine,
            commands::stop_eq_engine,
            // ★ Ear blast protection
            commands::set_ear_blast_enabled,
            commands::set_ear_blast_channels,
            commands::set_ear_blast_threshold,
            commands::set_ear_blast_target,
            commands::get_ear_blast_state,
            // ★ Live watcher directory sync
            commands::update_watch_dirs,
            // ★ Epic 5: Devices
            commands::get_devices,
            commands::set_mouse_dpi, commands::set_mouse_polling_rate,
            commands::set_headset_sidetone, commands::set_headset_chatmix,
            commands::set_headset_inactive_time, commands::set_headset_mic_volume,
            commands::set_headset_mic_mute_led, commands::set_headset_volume_limiter,
            commands::set_headset_bt_powered_on, commands::set_headset_bt_call_volume,
            commands::set_headset_eq_preset, commands::set_headset_eq_curve,
            // ★ Dependency probing
            commands::get_dependency_status,
            commands::get_distro_info,
            // ★ Device access status (groups, ratbagd, udev)
            commands::get_device_access_status,
        ])
        .setup(|app| {
            // ── Managed states ──
            app.manage(VuState(
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicU64::new(0)),
            ));
            app.manage(EarBlastState::default());
            app.manage(ExportProcess::default());
            app.manage(GsrProcess(Mutex::new(None)));
            app.manage(JalvProcesses(Mutex::new(std::collections::HashMap::new())));
            app.manage(RouteState::new());

            // ★ Epic 4: RunInBackground defaults true; overridden from saved settings below
            let run_bg_flag = Arc::new(AtomicBool::new(true));
            app.manage(RunInBackground(Arc::clone(&run_bg_flag)));

            // DB init
            if let Err(e) = commands::init_clips_db() {
                log::error!("DB init failed: {e}");
            }

            // Media server — pass clip directories for path containment checks
            let clip_dirs = get_watch_dirs();
            let steam_roots = commands::steam_artwork_roots();
            if !steam_roots.is_empty() {
                log::info!("Steam artwork roots available: {}", steam_roots.len());
                for root in &steam_roots {
                    log::debug!("  → {}", root.display());
                }
            }
            let (port, token) = media_server::spawn_media_server(clip_dirs, steam_roots);
            app.manage(MediaServerPort(port));
            app.manage(MediaServerToken(token.clone()));
            log::info!("Media server on http://localhost:{port}");

            // ★ Epic 3 + 4: Read saved settings at startup
            let settings_path = dirs::config_dir()
                .unwrap_or_default()
                .join("opengg/ui-settings.json");

            // Parameters for optional GSR auto-start (read before spawning threads)
            let mut gsr_auto_params: Option<GsrAutoParams> = None;

            if let Ok(json) = std::fs::read_to_string(&settings_path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                    // Restore run-in-background preference before the first close event
                    if let Some(run_bg) = v["settings"]["runInBackground"].as_bool() {
                        run_bg_flag.store(run_bg, Ordering::Relaxed);
                    }

                    // ★ Initialize ear-blast protection from saved settings
                    if let Some(eb) = v.get("mixer").and_then(|m| m.get("earBlast")) {
                        let ear_blast = app.state::<EarBlastState>();
                        if let Some(enabled) = eb.get("enabled").and_then(|e| e.as_bool()) {
                            ear_blast.enabled.store(enabled, Ordering::Relaxed);
                        }
                        if let Some(chs) = eb.get("channels").and_then(|c| c.as_array()) {
                            let mut set = std::collections::HashSet::new();
                            for c in chs {
                                if let Some(s) = c.as_str() {
                                    set.insert(s.to_string());
                                }
                            }
                            *ear_blast.channels.lock().unwrap() = set;
                        }
                        if let Some(t) = eb.get("threshold").and_then(|t| t.as_u64()) {
                            ear_blast.threshold_percent.store(t as u32, Ordering::Relaxed);
                        }
                        if let Some(t) = eb.get("target").and_then(|t| t.as_u64()) {
                            ear_blast.target_percent.store(t as u32, Ordering::Relaxed);
                        }
                    }

                    // ★ Auto-start GSR if enabled and gsrAutoStart is true
                    let gsr_enabled = v["settings"]["gsrEnabled"].as_bool().unwrap_or(false);
                    let gsr_auto = v["settings"]["gsrAutoStart"].as_bool().unwrap_or(true);
                    if gsr_enabled && gsr_auto {
                        let output_dir = v["settings"]["clip_directories"][0]
                            .as_str()
                            .unwrap_or("~/Videos/OpenGG")
                            .to_string();
                        let replay_secs =
                            v["settings"]["gsrReplaySecs"].as_u64().unwrap_or(30) as u32;
                        let fps = v["settings"]["gsrFps"].as_u64().unwrap_or(60) as u32;
                        let quality = v["settings"]["gsrQuality"]
                            .as_str()
                            .unwrap_or("cbr")
                            .to_string();
                        let bitrate_kbps = if quality == "cbr" {
                            v["settings"]["gsrCbrBitrate"].as_u64().map(|b| b as u32)
                        } else {
                            None
                        };
                        let monitor_target = v["settings"]["gsrMonitorTarget"]
                            .as_str()
                            .unwrap_or("screen")
                            .to_string();
                        let audio_sources: Vec<String> = v["settings"]["captureTracks"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|t| t["source"].as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_else(|| vec!["Game".into(), "Chat".into(), "Mic".into()]);
                        gsr_auto_params = Some((
                            output_dir,
                            replay_secs,
                            fps,
                            quality,
                            bitrate_kbps,
                            monitor_target,
                            audio_sources,
                        ));
                    }
                }
            }

            // ★ Epic 3: Re-apply saved pw-link routing in background (1.5 s delay lets PA settle)
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(1500));
                commands::hydrate_audio_routing();
            });

            // ★ Auto-start GSR replay buffer — poll until virtual sinks are ready
            // instead of a blind sleep. Prevents the race where GSR starts before
            // PipeWire null-sinks exist (common on slower systems).
            if let Some((
                output_dir,
                replay_secs,
                fps,
                quality,
                bitrate_kbps,
                monitor_target,
                audio_sources,
            )) = gsr_auto_params
            {
                let gsr_app = app.handle().clone();
                std::thread::spawn(move || {
                    let deadline = std::time::Instant::now()
                        + std::time::Duration::from_secs(10);
                    loop {
                        if let Ok(o) = std::process::Command::new("pactl")
                            .args(["list", "sinks", "short"])
                            .output()
                        {
                            let text = String::from_utf8_lossy(&o.stdout);
                            if text.contains("OpenGG_Game")
                                && text.contains("OpenGG_Chat")
                            {
                                break;
                            }
                        }
                        if std::time::Instant::now() >= deadline {
                            log::warn!(
                                "GSR auto-start aborted: virtual sinks did not appear within 10 s"
                            );
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    match commands::start_gsr_replay(
                        gsr_app,
                        output_dir,
                        replay_secs,
                        fps,
                        quality,
                        bitrate_kbps,
                        monitor_target,
                        audio_sources,
                    ) {
                        Ok(()) => log::info!("GSR auto-started on launch"),
                        Err(e) => log::warn!("GSR auto-start failed: {e}"),
                    }
                });
            }

            // ★ Power User: Live file-system watcher — emits `clip_added` / `clip_removed`
            // Also watches `~/.local/share/opengg/extensions/` and emits `plugins-changed`
            // when a manifest.json is created, modified, or removed.
            {
                use notify::event::{CreateKind, EventKind, ModifyKind, RemoveKind};
                use notify::{Config, RecursiveMode, Watcher};

                let watch_handle = app.handle().clone();
                let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();

                match notify::RecommendedWatcher::new(
                    move |res| {
                        let _ = tx.send(res);
                    },
                    Config::default(),
                ) {
                    Ok(mut watcher) => {
                        let dirs = get_watch_dirs();
                        for dir in &dirs {
                            let _ = std::fs::create_dir_all(dir);
                            if let Err(e) = watcher.watch(dir, RecursiveMode::Recursive) {
                                log::warn!("Watcher: cannot watch {:?}: {e}", dir);
                            } else {
                                log::info!("Watcher: watching {:?}", dir);
                            }
                        }

                        // Also watch the extensions directory (recursive — detects manifest.json
                        // inside any extension subdirectory).
                        let extensions_dir = commands::extensions_dir_pub();
                        let _ = std::fs::create_dir_all(&extensions_dir);
                        if let Err(e) = watcher.watch(&extensions_dir, RecursiveMode::Recursive) {
                            log::warn!(
                                "Watcher: cannot watch extensions dir {:?}: {e}",
                                extensions_dir
                            );
                        } else {
                            log::info!("Watcher: watching extensions {:?}", extensions_dir);
                        }

                        // Keep watcher alive for the app lifetime.
                        app.manage(WatcherHandle(Mutex::new(Some(watcher))));
                        app.manage(WatchedDirs(Mutex::new(dirs.clone())));

                        // Drain events in a background thread.
                        // plugins-changed is debounced: we track the last emit time and
                        // suppress duplicate events within a 500ms window.
                        // Note: event name kept as `plugins-changed` for frontend compatibility.
                        std::thread::spawn(move || {
                            let mut last_plugin_emit = std::time::Instant::now()
                                .checked_sub(std::time::Duration::from_secs(10))
                                .unwrap_or(std::time::Instant::now());
                            for res in rx {
                                let event = match res {
                                    Ok(e) => e,
                                    Err(_) => continue,
                                };
                                for path in &event.paths {
                                    let filename =
                                        path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                                    // Extension manifest events — debounced 500ms
                                    if filename == "manifest.json" {
                                        let is_manifest_event = matches!(
                                            &event.kind,
                                            EventKind::Create(_)
                                                | EventKind::Remove(_)
                                                | EventKind::Modify(ModifyKind::Data(_))
                                                | EventKind::Modify(ModifyKind::Name(_))
                                                | EventKind::Modify(_)
                                        );
                                        if is_manifest_event
                                            && last_plugin_emit.elapsed()
                                                > std::time::Duration::from_millis(500)
                                        {
                                            log::info!(
                                                "Watcher: extensions-changed (manifest: {:?})",
                                                path
                                            );
                                            let _ = watch_handle.emit("plugins-changed", ());
                                            last_plugin_emit = std::time::Instant::now();
                                        }
                                        continue;
                                    }

                                    // Clip file events
                                    let ext = path
                                        .extension()
                                        .and_then(|e| e.to_str())
                                        .unwrap_or("")
                                        .to_lowercase();
                                    if !VIDEO_WATCH_EXTS.contains(&ext.as_str()) {
                                        continue;
                                    }
                                    let fp = path.to_string_lossy().to_string();
                                    match &event.kind {
                                        EventKind::Create(CreateKind::File)
                                        | EventKind::Create(_) => {
                                            log::info!("Watcher: clip_added {fp}");
                                            let _ = watch_handle.emit("clip_added", &fp);
                                        }
                                        EventKind::Remove(RemoveKind::File)
                                        | EventKind::Remove(_) => {
                                            log::info!("Watcher: clip_removed {fp}");
                                            let _ = watch_handle.emit("clip_removed", &fp);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => {
                        log::error!("Watcher init failed: {e}");
                        // App still works; just no live updates.
                        app.manage(WatcherHandle(Mutex::new(None)));
                        app.manage(WatchedDirs(Mutex::new(vec![])));
                    }
                }
            }

            // Device-changed watcher: polls D-Bus every 3 s, emits only on change.
            // Reuses a single D-Bus connection instead of reconnecting every tick.
            {
                use std::time::Duration;
                let dv_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let mut last: String = String::new();
                    let mut interval = tokio::time::interval(Duration::from_millis(3000));
                    let conn = match zbus::Connection::session().await {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("device-watcher: D-Bus connection failed: {e}");
                            return;
                        }
                    };
                    loop {
                        interval.tick().await;
                        if let Ok(reply) = conn.call_method(
                            Some("org.opengg.Daemon"),
                            "/org/opengg/Daemon/Device",
                            Some("org.opengg.Daemon.Device"),
                            "GetDevices",
                            &(),
                        ).await {
                            if let Ok(json) = reply.body().deserialize::<String>() {
                                if json != last {
                                    last = json.clone();
                                    let _ = dv_handle.emit("device-changed", json);
                                }
                            }
                        }
                    }
                });
            }

            // Sleep/wake: emit system-resume after logind PrepareForSleep(false)
            {
                use futures_util::StreamExt;
                let sl_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    async fn listen_sleep(handle: tauri::AppHandle) -> zbus::Result<()> {
                        let conn = zbus::Connection::system().await?;
                        let rule = zbus::MatchRule::builder()
                            .msg_type(zbus::message::Type::Signal)
                            .sender("org.freedesktop.login1")?
                            .interface("org.freedesktop.login1.Manager")?
                            .member("PrepareForSleep")?
                            .build();
                        let mut stream = zbus::MessageStream::for_match_rule(
                            rule, &conn, None,
                        ).await?;
                        while let Some(Ok(msg)) = stream.next().await {
                            if let Ok((going_to_sleep,)) = msg.body().deserialize::<(bool,)>() {
                                if !going_to_sleep {
                                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                    let _ = handle.emit("system-resume", ());
                                }
                            }
                        }
                        Ok(())
                    }
                    let _ = listen_sleep(sl_handle).await;
                });
            }

            // ★ Epic 4: System Tray — "Show OpenGG" + "Quit"
            let show_i = MenuItem::with_id(app, "show", "Show OpenGG", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit OpenGG", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("app icon missing").clone())
                .menu(&menu)
                .tooltip("OpenGG")
                .on_menu_event(
                    |app: &tauri::AppHandle, event: tauri::menu::MenuEvent| match event.id.as_ref()
                    {
                        "show" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "quit" => {
                            log::info!("OpenGG: quit from tray");
                            app.exit(0);
                        }
                        _ => {}
                    },
                )
                .build(app)?;

            Ok(())
        })
        // ★ Epic 4: Conditional close-to-tray
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let run_bg = window
                    .app_handle()
                    .try_state::<RunInBackground>()
                    .map(|s| s.0.load(Ordering::Relaxed))
                    .unwrap_or(false);

                if run_bg {
                    api.prevent_close();
                    let _ = window.hide();
                    log::info!("OpenGG: window hidden (running in background)");
                } else {
                    log::info!("OpenGG: window closed (exiting)");
                    tauri::async_runtime::block_on(async {
                        let _ = commands::remove_virtual_audio().await;
                    });
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("opengg failed");
}

// ── Managed-state types ──────────────────────────────────────────────────────
/// VU stream state: running flag + generation counter.
/// The generation counter prevents stale reader threads (leaked from a previous
/// `start_vu_stream` call) from re-attaching after `stop_vu_stream` returns —
/// each thread checks `my_gen == current_gen` on every iteration.
pub struct VuState(pub Arc<AtomicBool>, pub Arc<AtomicU64>);
pub struct RunInBackground(pub Arc<AtomicBool>);
pub struct MediaServerPort(pub u16);

/// Ear-blast protection state: enabled flag, protected channels, threshold/target
/// percentages, per-channel active limiting state, and cached original volumes.
pub struct EarBlastState {
    pub enabled: AtomicBool,
    pub channels: Mutex<HashSet<String>>,
    pub threshold_percent: AtomicU32,
    pub target_percent: AtomicU32,
    pub active: Mutex<HashMap<String, bool>>,
    pub original_volumes: Mutex<HashMap<String, u32>>,
    pub last_trigger_ms: Mutex<HashMap<String, u64>>,
}

impl Default for EarBlastState {
    fn default() -> Self {
        let mut channels = HashSet::new();
        channels.insert("Game".to_string());
        Self {
            enabled: AtomicBool::new(false),
            channels: Mutex::new(channels),
            threshold_percent: AtomicU32::new(85),
            target_percent: AtomicU32::new(60),
            active: Mutex::new(HashMap::new()),
            original_volumes: Mutex::new(HashMap::new()),
            last_trigger_ms: Mutex::new(HashMap::new()),
        }
    }
}

/// Managed state for a cancellable FFmpeg export child process.
#[derive(Default)]
pub struct ExportProcess {
    pub child: Mutex<Option<(std::process::Child, String)>>,
}

/// Keeps the notify watcher alive for the full app lifetime.
/// Wrapped in Mutex<Option<…>> so it can be taken on shutdown if needed.
pub struct WatcherHandle(pub Mutex<Option<notify::RecommendedWatcher>>);

/// Per-session authentication token for media server requests.
pub struct MediaServerToken(pub String);

/// Spawn parameters retained so restart-on-save and hot-reload can respawn identically.
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

pub struct GsrProcess(pub Mutex<Option<GsrState>>);

/// jalv LV2-host subprocesses keyed by channel name (e.g. "Game", "Chat").
/// Each entry is (Child, ChildStdin) — stdin is kept open for runtime parameter updates.
pub struct JalvProcesses(
    pub Mutex<std::collections::HashMap<String, (std::process::Child, std::process::ChildStdin)>>,
);

/// Tracks which directories the watcher is currently watching, so
/// `update_watch_dirs` can diff current vs desired and call watch/unwatch.
pub struct WatchedDirs(pub Mutex<Vec<std::path::PathBuf>>);

// ══════════════════════════════════════════════════════════════════════
//  ★ RouteState: Thread-safe audio routing deduplication + blacklist
// ══════════════════════════════════════════════════════════════════════

const ROUTE_COOLDOWN_SECS: u64 = 5;
const ROUTE_BLACKLIST: &[&str] = &[
    "plasmashell", "kwin_wayland", "kwin_x11", "swaync", "sway",
    "xdg-desktop-portal", "xdg-desktop-portal-gnome", "xdg-desktop-portal-kde",
    "wireplumber", "pipewire", "pipewire-pulse", "opengg", "peak detect",
];
const FAIL_THRESHOLD: u32 = 3;
const FAIL_WINDOW_SECS: u64 = 30;
const FAIL_COOLDOWN_SECS: u64 = 30;

/// Tracks routing attempts, successes, and failures per PID to prevent
/// the infinite re-routing loop that spawns pactl/pw-metadata thousands
/// of times per second and eventually OOM-kills the system.
pub struct RouteState {
    /// routing-key → last routing attempt time. Prevents retry during cooldown.
    /// KEY = stable app identity (binary, lowercased) when known, else the stream id.
    /// Keying by binary (not the volatile PipeWire object.serial / sink-input index)
    /// is what makes these guards actually work: a flood of short-lived streams from
    /// one app shares ONE key, so the cooldown + circuit breaker can finally engage.
    pub cooldown: Mutex<HashMap<String, SystemTime>>,
    /// routing-key → (channel, success_time). Tracks successfully routed apps.
    pub routed: Mutex<HashMap<String, (String, SystemTime)>>,
    /// routing-key → (fail_count, first_fail_time). Circuit breaker for repeated failures.
    pub fail_counts: Mutex<HashMap<String, (u32, SystemTime)>>,
    /// Binary names that must never be routed.
    pub blacklist: HashSet<&'static str>,
}

impl Default for RouteState {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteState {
    pub fn new() -> Self {
        let mut blacklist = HashSet::new();
        for &name in ROUTE_BLACKLIST {
            blacklist.insert(name);
        }
        Self {
            cooldown: Mutex::new(HashMap::new()),
            routed: Mutex::new(HashMap::new()),
            fail_counts: Mutex::new(HashMap::new()),
            blacklist,
        }
    }

    pub fn is_blacklisted(&self, binary: &str) -> bool {
        self.blacklist.contains(binary.to_lowercase().as_str())
    }

    pub fn is_on_cooldown(&self, key: &str) -> bool {
        let map = self.cooldown.lock().unwrap();
        map.get(key).is_some_and(|t| {
            SystemTime::now().duration_since(*t).unwrap_or(Duration::MAX)
                < Duration::from_secs(ROUTE_COOLDOWN_SECS)
        })
    }

    pub fn record_attempt(&self, key: &str) {
        self.cooldown.lock().unwrap().insert(key.to_string(), SystemTime::now());
    }

    pub fn record_success(&self, key: &str, channel: String) {
        self.routed.lock().unwrap().insert(key.to_string(), (channel, SystemTime::now()));
        // Clear failure count on success
        self.fail_counts.lock().unwrap().remove(key);
    }

    pub fn is_already_routed(&self, key: &str, channel: &str) -> bool {
        let map = self.routed.lock().unwrap();
        map.get(key).is_some_and(|(ch, _)| ch == channel)
    }

    #[allow(dead_code)]
    pub fn clear_key(&self, key: &str) {
        self.cooldown.lock().unwrap().remove(key);
        self.routed.lock().unwrap().remove(key);
        self.fail_counts.lock().unwrap().remove(key);
    }

    /// Records a failure and returns `true` if the circuit breaker is now
    /// open (this app should be blocked from further attempts).
    pub fn record_failure(&self, key: &str) -> bool {
        let mut map = self.fail_counts.lock().unwrap();
        let now = SystemTime::now();
        let entry = map.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(entry.1).unwrap_or(Duration::MAX)
            > Duration::from_secs(FAIL_WINDOW_SECS)
        {
            *entry = (1, now);
        } else {
            entry.0 += 1;
        }
        if entry.0 >= FAIL_THRESHOLD {
            // Also put on extended cooldown
            self.cooldown.lock().unwrap().insert(
                key.to_string(),
                now + Duration::from_secs(FAIL_COOLDOWN_SECS),
            );
            true
        } else {
            false
        }
    }

    pub fn is_circuit_open(&self, key: &str) -> bool {
        let map = self.fail_counts.lock().unwrap();
        let now = SystemTime::now();
        map.get(key).is_some_and(|(count, first)| {
            *count >= FAIL_THRESHOLD
                && now.duration_since(*first).unwrap_or(Duration::MAX)
                    <= Duration::from_secs(FAIL_COOLDOWN_SECS)
        })
    }
}

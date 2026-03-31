#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod media_server;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, WindowEvent,
};

const VIDEO_WATCH_EXTS: &[&str] = &["mp4", "mkv", "webm", "avi", "mov", "ts", "flv"];

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
                            if !dirs.contains(&pb) { dirs.push(pb); }
                        }
                    }
                }
            }
        }
    }
    if dirs.is_empty() { dirs.push(commands::default_clips_dir()); }
    dirs
}

// ══════════════════════════════════════════════════════
//  ★ EPIC 2: File-based crash / info logger
// ══════════════════════════════════════════════════════

fn init_logging() {
    use simplelog::{Config, LevelFilter, WriteLogger};
    use std::fs::OpenOptions;

    let log_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("opengg");
    let _ = std::fs::create_dir_all(&log_dir);
    let log_file = log_dir.join("opengg_crash.log");

    // Append to existing log so successive runs accumulate
    if let Ok(file) = OpenOptions::new().create(true).append(true).open(&log_file) {
        let _ = WriteLogger::init(LevelFilter::Info, Config::default(), file);
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

    log::info!("=== OpenGG started ===");
}

fn main() {
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            // Audio
            commands::get_channels, commands::set_volume, commands::set_mute,
            commands::get_apps, commands::route_app,
            commands::get_audio_devices, commands::set_channel_device,
            commands::set_app_volume,
            commands::start_vu_stream, commands::stop_vu_stream,
            // Replay
            commands::get_recorder_status, commands::start_replay,
            commands::stop_recorder, commands::save_replay,
            // Clips
            commands::get_clips, commands::generate_thumbnail,
            commands::set_clip_meta, commands::get_clip_meta,
            commands::take_screenshot,
            commands::clear_thumbnail_cache, commands::delete_clip,
            commands::trim_clip, commands::export_clip_sized,
            commands::save_trim_state, commands::get_trim_state,
            commands::open_file_location,
            // ★ Power User: single-file fetch for watcher
            commands::get_clip_by_path,
            // Editor
            commands::analyze_media, commands::rename_clip, commands::export_timeline,
            commands::calc_export_settings, commands::export_clip_with_filters,
            commands::generate_waveform, commands::export_with_progress,
            commands::cancel_export,
            // Recording
            commands::start_screen_recording, commands::stop_screen_recording,
            // Theme + Settings
            commands::load_theme, commands::save_theme,
            commands::get_media_server_port,
            commands::save_ui_settings, commands::load_ui_settings,
            commands::get_storage_info, commands::open_locales_folder,
            commands::list_user_locales,
            commands::scan_extensions, commands::open_extensions_folder,
            // ★ Epic 2: Crash log
            commands::open_crash_logs_folder,
            // ★ Epic 4: Background daemon + autostart
            commands::quit_app,
            commands::set_run_in_background,
            commands::get_autostart, commands::set_autostart,
            // ★ Session 3: Home page clip count
            commands::get_clips_count,
            // ★ Virtual audio onboarding + factory reset + routing hydration
            commands::check_virtual_audio_status,
            commands::create_virtual_audio,
            commands::remove_virtual_audio,
            commands::hydrate_audio_routing,
            // ★ Epic 1C: Global OS shortcuts
            commands::register_global_shortcuts,
            // ★ GPU Screen Recorder
            commands::start_gsr_replay, commands::save_gsr_replay,
            commands::stop_gsr_replay, commands::is_gsr_running,
            commands::get_active_window_title,
        ])
        .setup(|app| {
            // ── Managed states ──
            app.manage(VuState(Arc::new(AtomicBool::new(false))));
            app.manage(ExportProcess::default());
            app.manage(GsrProcess(Mutex::new(None)));

            // ★ Epic 4: RunInBackground defaults true; overridden from saved settings below
            let run_bg_flag = Arc::new(AtomicBool::new(true));
            app.manage(RunInBackground(Arc::clone(&run_bg_flag)));

            // DB init
            if let Err(e) = commands::init_clips_db() {
                log::error!("DB init failed: {e}");
            }

            // Media server
            let port = media_server::spawn_media_server();
            app.manage(MediaServerPort(port));
            log::info!("Media server on http://localhost:{port}");

            // ★ Epic 3 + 4: Read saved settings at startup
            let settings_path = dirs::config_dir()
                .unwrap_or_default()
                .join("opengg/ui-settings.json");
            if let Ok(json) = std::fs::read_to_string(&settings_path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                    // Restore run-in-background preference before the first close event
                    if let Some(run_bg) = v["settings"]["runInBackground"].as_bool() {
                        run_bg_flag.store(run_bg, Ordering::Relaxed);
                    }
                }
            }

            // ★ Epic 3: Re-apply saved pw-link routing in background (1.5 s delay lets PA settle)
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(1500));
                commands::hydrate_audio_routing();
            });

            // ★ Power User: Live file-system watcher — emits `clip_added` / `clip_removed`
            {
                use notify::{Config, RecursiveMode, Watcher};
                use notify::event::{CreateKind, RemoveKind, EventKind};

                let watch_handle = app.handle().clone();
                let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();

                match notify::RecommendedWatcher::new(
                    move |res| { let _ = tx.send(res); },
                    Config::default(),
                ) {
                    Ok(mut watcher) => {
                        let dirs = get_watch_dirs();
                        for dir in &dirs {
                            let _ = std::fs::create_dir_all(dir);
                            if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
                                log::warn!("Watcher: cannot watch {:?}: {e}", dir);
                            } else {
                                log::info!("Watcher: watching {:?}", dir);
                            }
                        }
                        // Keep watcher alive for the app lifetime.
                        app.manage(WatcherHandle(Mutex::new(Some(watcher))));

                        // Drain events in a background thread.
                        std::thread::spawn(move || {
                            for res in rx {
                                let event = match res { Ok(e) => e, Err(_) => continue };
                                for path in &event.paths {
                                    let ext = path.extension()
                                        .and_then(|e| e.to_str())
                                        .unwrap_or("")
                                        .to_lowercase();
                                    if !VIDEO_WATCH_EXTS.contains(&ext.as_str()) { continue; }
                                    let fp = path.to_string_lossy().to_string();
                                    match &event.kind {
                                        EventKind::Create(CreateKind::File) | EventKind::Create(_) => {
                                            log::info!("Watcher: clip_added {fp}");
                                            let _ = watch_handle.emit("clip_added", &fp);
                                        }
                                        EventKind::Remove(RemoveKind::File) | EventKind::Remove(_) => {
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
                    }
                }
            }

            // ★ Epic 4: System Tray — "Show OpenGG" + "Quit"
            let show_i = MenuItem::with_id(app, "show", "Show OpenGG",  true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit OpenGG",  true, None::<&str>)?;
            let menu   = Menu::with_items(app, &[&show_i, &quit_i])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("app icon missing").clone())
                .menu(&menu)
                .tooltip("OpenGG")
                .on_menu_event(|app: &tauri::AppHandle, event: tauri::menu::MenuEvent| {
                    match event.id.as_ref() {
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
                    }
                })
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
                    .unwrap_or(true);

                if run_bg {
                    api.prevent_close();
                    let _ = window.hide();
                    log::info!("OpenGG: window hidden (running in background)");
                } else {
                    log::info!("OpenGG: window closed (exiting)");
                    // Allow the default close, which will exit the app on the last window
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("opengg failed");
}

// ── Managed-state types ──────────────────────────────────────────────────────
pub struct VuState(pub Arc<AtomicBool>);
pub struct RunInBackground(pub Arc<AtomicBool>);
pub struct MediaServerPort(pub u16);

/// Managed state for a cancellable FFmpeg export child process.
#[derive(Default)]
pub struct ExportProcess {
    pub child: Mutex<Option<(std::process::Child, String)>>,
}

/// Keeps the notify watcher alive for the full app lifetime.
/// Wrapped in Mutex<Option<…>> so it can be taken on shutdown if needed.
pub struct WatcherHandle(pub Mutex<Option<notify::RecommendedWatcher>>);

/// Managed state for the GPU Screen Recorder (gpu-screen-recorder) child process.
pub struct GsrProcess(pub Mutex<Option<std::process::Child>>);

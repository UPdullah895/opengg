#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod media_server;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tauri::{Manager, WindowEvent};

fn main() {
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
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
            // ★ Epic 4: Quit command (for menu/tray "Exit")
            commands::quit_app,
        ])
        .setup(|app| {
            app.manage(VuState(Arc::new(AtomicBool::new(false))));
            app.manage(ExportProcess::default());
            if let Err(e) = commands::init_clips_db() { eprintln!("DB: {e}"); }
            let port = media_server::spawn_media_server();
            app.manage(MediaServerPort(port));
            eprintln!("OpenGG media server on http://localhost:{port}");
            Ok(())
        })
        // ★ Epic 4: Close-to-background — hiding instead of quitting
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Prevent the window from actually closing
                api.prevent_close();
                // Hide the window instead
                let _ = window.hide();
                eprintln!("OpenGG: window hidden (still running in background)");
            }
        })
        .run(tauri::generate_context!())
        .expect("opengg failed");
}

pub struct VuState(pub Arc<AtomicBool>);
pub struct MediaServerPort(pub u16);

/// Managed state for cancellable FFmpeg export
#[derive(Default)]
pub struct ExportProcess {
    pub child: Mutex<Option<(std::process::Child, String)>>,
}

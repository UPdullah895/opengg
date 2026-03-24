#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod media_server;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::Manager;

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
            commands::take_screenshot, commands::delete_clip,
            commands::trim_clip, commands::export_clip_sized,
            commands::save_trim_state, commands::get_trim_state,
            commands::open_file_location,
            // Editor
            commands::analyze_media, commands::rename_clip, commands::export_timeline,
            commands::calc_export_settings, commands::export_clip_with_filters,
            commands::generate_waveform,
            commands::export_with_progress, commands::generate_waveform,
            // Recording
            commands::start_screen_recording, commands::stop_screen_recording,
            // Theme
            commands::load_theme, commands::save_theme,
            // Media server
            commands::get_media_server_port,
            // Persistence
            commands::save_ui_settings, commands::load_ui_settings,
        ])
        .setup(|app| {
            app.manage(VuState(Arc::new(AtomicBool::new(false))));
            if let Err(e) = commands::init_clips_db() { eprintln!("DB: {e}"); }

            // ★ Spawn local HTTP media server on a random port
            let port = media_server::spawn_media_server();
            app.manage(MediaServerPort(port));
            eprintln!("Media server started on http://localhost:{port}");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running opengg");
}

pub struct VuState(pub Arc<AtomicBool>);
pub struct MediaServerPort(pub u16);

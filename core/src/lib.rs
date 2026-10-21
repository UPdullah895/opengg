//! opengg-core — shared, Tauri-free business logic for OpenGG.
//!
//! Code here is **moved verbatim** out of `frontend/src-tauri` (plan §2.1): the
//! Tauri host keeps thin `#[tauri::command]` wrappers that delegate to these
//! plain functions, and the Qt/QML shell (`qt-shell`) calls the same functions
//! through its cxx-qt QObject bridge. Nothing in this crate depends on Tauri,
//! Qt, or any UI framework.
//!
//! Boundary rule (plan §2.3): deleting `qt-shell/` must not remove any
//! capability from here, and `qt-shell` must not depend on ffmpeg/sqlite/
//! pipewire/gstreamer/zbus except through this crate.

pub mod audio;
pub mod button_hotspots;
pub mod clips;
pub mod daemon;
pub mod device;
pub mod dialogs;
pub mod device_assets;
pub mod desktop_shot;
pub mod device_display;
pub mod ear_blast;
pub mod edit;
pub mod eq;
pub mod extensions;
pub mod foreground;
pub mod gsr;
pub mod hotkeys;
pub mod media;
pub mod notify;
pub mod paths;
pub mod recording;
pub mod settings;
pub mod share;
pub mod steam;
pub mod storage;
pub mod subprocess;
pub mod supervisor;
pub mod system;
pub mod vu;
pub mod watcher;

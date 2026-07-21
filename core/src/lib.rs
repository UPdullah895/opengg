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

pub mod clips;
pub mod media;
pub mod paths;
pub mod settings;
pub mod steam;
pub mod storage;

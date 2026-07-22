//! ClipsController — a QML singleton exposing the local clip library from
//! `opengg_core::clips` (SQLite metadata + filesystem scan; the "fast" lister,
//! no ffprobe). Read-only for now; per-clip metadata mutation, the async
//! thumbnail image provider, and the video player land in later Phase 2
//! increments.
//!
//! Boundary (plan §2.2/§2.3): all clip DB + path resolution lives in
//! `opengg_core`; this is pure presentation glue over
//! `opengg_core::clips::get_clips_fast`. Clips are handed to QML as a JSON
//! string (parsed with `JSON.parse`); the `QAbstractListModel` upgrade is a
//! later Phase 2/3 step (plan C-model note §2.2).

#[cxx_qt::bridge]
pub mod qobject {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // Raw clips JSON (Vec<ClipInfo> serialized); parsed in QML with
        // JSON.parse. A QAbstractListModel is the Phase 3 upgrade (plan §2.2).
        #[qproperty(QString, clips_json, cxx_name = "clipsJson")]
        #[qproperty(bool, loading)]
        #[qproperty(QString, error)]
        #[qproperty(i32, count)]
        type ClipsController = super::ClipsControllerRust;

        /// Rescan the clip library (fast path: DB + filesystem, no ffprobe).
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>);

        /// Toggle a clip's favorite flag (clip_meta), then rescan.
        #[qinvokable]
        #[cxx_name = "setFavorite"]
        fn set_favorite(self: Pin<&mut Self>, filepath: &QString, favorite: bool);

        /// Set a clip's custom display name (clip_meta), then rescan. An empty
        /// name is ignored by the DB layer (keeps the existing name).
        #[qinvokable]
        #[cxx_name = "setCustomName"]
        fn set_custom_name(self: Pin<&mut Self>, filepath: &QString, name: &QString);

        /// Delete a clip from disk + its metadata, then rescan.
        #[qinvokable]
        #[cxx_name = "deleteClip"]
        fn delete_clip(self: Pin<&mut Self>, filepath: &QString);
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;
use opengg_core::clips::ClipMetaUpdate;

#[derive(Default)]
pub struct ClipsControllerRust {
    clips_json: QString,
    loading: bool,
    error: QString,
    count: i32,
}

impl qobject::ClipsController {
    /// Rescan the library and republish clipsJson/count/error. Shared by
    /// refresh() and every mutation invokable.
    fn reload(mut self: Pin<&mut Self>) {
        self.as_mut().set_loading(true);
        // Empty folder → core resolves configured clip_directories or the
        // default (~/Videos/OpenGG) via resolve_clips_dir/get_all_clip_dirs.
        match opengg_core::clips::get_clips_fast(String::new()) {
            Ok(clips) => {
                let count = clips.len() as i32;
                let json = serde_json::to_string(&clips).unwrap_or_else(|_| "[]".into());
                self.as_mut().set_clips_json(QString::from(&json));
                self.as_mut().set_count(count);
                self.as_mut().set_error(QString::default());
            }
            Err(e) => {
                self.as_mut().set_clips_json(QString::from("[]"));
                self.as_mut().set_count(0);
                self.as_mut().set_error(QString::from(&e));
            }
        }
        self.as_mut().set_loading(false);
    }

    pub fn refresh(self: Pin<&mut Self>) {
        self.reload();
    }

    fn meta_update(filepath: String) -> ClipMetaUpdate {
        ClipMetaUpdate {
            filepath,
            custom_name: None,
            favorite: None,
            game_tag: None,
            notes: None,
        }
    }

    /// Current favorite flag for a clip (false if unknown). Needed because
    /// core::set_clip_meta always writes `favorite`, so a custom-name update
    /// must re-send the existing value or it would reset to false.
    fn current_favorite(filepath: &str) -> bool {
        opengg_core::clips::get_clip_meta(filepath)
            .ok()
            .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
            .and_then(|v| v.get("favorite").and_then(|f| f.as_bool()))
            .unwrap_or(false)
    }

    pub fn set_favorite(self: Pin<&mut Self>, filepath: &QString, favorite: bool) {
        let update = ClipMetaUpdate {
            favorite: Some(favorite),
            ..Self::meta_update(filepath.to_string())
        };
        if let Err(e) = opengg_core::clips::set_clip_meta(update) {
            eprintln!("set_favorite: {e}");
        }
        self.reload();
    }

    pub fn set_custom_name(self: Pin<&mut Self>, filepath: &QString, name: &QString) {
        let fp = filepath.to_string();
        let favorite = Some(Self::current_favorite(&fp)); // preserve, see above
        let update = ClipMetaUpdate {
            custom_name: Some(name.to_string()),
            favorite,
            ..Self::meta_update(fp)
        };
        if let Err(e) = opengg_core::clips::set_clip_meta(update) {
            eprintln!("set_custom_name: {e}");
        }
        self.reload();
    }

    pub fn delete_clip(self: Pin<&mut Self>, filepath: &QString) {
        if let Err(e) = opengg_core::clips::delete_clip(&filepath.to_string()) {
            eprintln!("delete_clip: {e}");
        }
        self.reload();
    }
}

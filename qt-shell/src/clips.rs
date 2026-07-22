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
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct ClipsControllerRust {
    clips_json: QString,
    loading: bool,
    error: QString,
    count: i32,
}

impl qobject::ClipsController {
    pub fn refresh(mut self: Pin<&mut Self>) {
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
}

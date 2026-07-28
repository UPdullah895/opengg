//! StorageController — disk-usage stats + thumbnail-cache clearing for the
//! Settings → Storage panel, over `opengg_core::storage`.

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
        #[qproperty(QString, storage_json, cxx_name = "storageJson")]
        #[qproperty(bool, loading)]
        type StorageController = super::StorageControllerRust;

        /// Recompute stats for the given clip directories (a JSON string array).
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>, clip_directories_json: QString);

        /// Delete all cached clip thumbnails; returns the number removed.
        #[qinvokable]
        #[cxx_name = "clearThumbnailCache"]
        fn clear_thumbnail_cache(self: &Self) -> i32;
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct StorageControllerRust {
    storage_json: QString,
    loading: bool,
}

impl qobject::StorageController {
    pub fn refresh(mut self: Pin<&mut Self>, clip_directories_json: QString) {
        self.as_mut().set_loading(true);

        let dirs: Vec<String> =
            serde_json::from_str(&clip_directories_json.to_string()).unwrap_or_default();
        let info = opengg_core::storage::get_storage_info(&dirs);
        let json = serde_json::to_string(&info).unwrap_or_else(|_| "{}".into());

        self.as_mut().set_storage_json(QString::from(&json));
        self.as_mut().set_loading(false);
    }

    pub fn clear_thumbnail_cache(&self) -> i32 {
        opengg_core::storage::clear_thumbnail_cache().unwrap_or(0) as i32
    }
}

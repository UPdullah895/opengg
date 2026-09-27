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
        /// Result of the most recent `pickFolder`, as JSON. See that method.
        #[qproperty(QString, picked_json, cxx_name = "pickedJson")]
        type StorageController = super::StorageControllerRust;

        /// Recompute stats for the given clip directories (a JSON string array).
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>, clip_directories_json: QString);

        /// Delete all cached clip thumbnails; returns the number removed.
        #[qinvokable]
        #[cxx_name = "clearThumbnailCache"]
        fn clear_thumbnail_cache(self: &Self) -> i32;

        /// Whether the desktop can show its own folder picker. False means
        /// QML must open the in-app dialog instead — see
        /// `opengg_core::dialogs` for why this shell cannot use
        /// Qt.labs.platform's.
        #[qinvokable]
        #[cxx_name = "nativePickerAvailable"]
        fn native_picker_available(self: &Self) -> bool;

        /// Open the desktop's folder picker. Threaded: the dialog blocks
        /// until the user answers. The result lands in `pickedJson` as
        /// `{"tag": "...", "path": "..."}`, with `tag` echoed back so one
        /// property can serve several buttons; a cancel publishes an empty
        /// path.
        #[qinvokable]
        #[cxx_name = "pickFolder"]
        fn pick_folder(self: Pin<&mut Self>, tag: QString, title: QString, start_dir: QString);
    }

    impl cxx_qt::Threading for StorageController {}
}

use core::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct StorageControllerRust {
    storage_json: QString,
    loading: bool,
    picked_json: QString,
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

    pub fn native_picker_available(&self) -> bool {
        opengg_core::dialogs::native_picker().is_some()
    }

    pub fn pick_folder(
        mut self: Pin<&mut Self>,
        tag: QString,
        title: QString,
        start_dir: QString,
    ) {
        self.as_mut().set_picked_json(QString::default());
        let (tag, title, start) = (tag.to_string(), title.to_string(), start_dir.to_string());
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let path = match opengg_core::dialogs::pick_folder(&title, &start) {
                Ok(Some(p)) => p,
                Ok(None) => String::new(),
                Err(e) => {
                    eprintln!("StorageController::pick_folder: {e}");
                    String::new()
                }
            };
            let json = serde_json::json!({ "tag": tag, "path": path }).to_string();
            let _ = qt_thread.queue(move |mut c| {
                c.as_mut().set_picked_json(QString::from(&json));
            });
        });
    }
}

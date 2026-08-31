//! DeviceController — a QML singleton exposing live device data from the openggd
//! daemon (`org.opengg.Daemon.Device`). Read-only for now (does not write to the
//! user's real hardware); setters land with the full Devices UI in a later phase.
//!
//! The daemon D-Bus client lives in `opengg_core::device` (plan §2.2/§2.3); this
//! controller is pure presentation glue over `opengg_core::device::get_devices()`.

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
        #[qproperty(QString, devices_json, cxx_name = "devicesJson")]
        #[qproperty(bool, connected)]
        // Set on a failed setDpi/setPollingRate write, cleared on the next
        // successful one. `lastErrorDeviceId` lets a specific device card
        // show the error next to the control that caused it rather than as
        // an ambiguous page-wide banner — two devices' cards can otherwise
        // both be mid-write at once.
        #[qproperty(QString, last_error, cxx_name = "lastError")]
        #[qproperty(QString, last_error_device_id, cxx_name = "lastErrorDeviceId")]
        // Devices roadmap Phase 5 (button-mapping editor).
        #[qproperty(QString, button_mappings_json, cxx_name = "buttonMappingsJson")]
        #[qproperty(QString, button_action_catalog_json, cxx_name = "buttonActionCatalogJson")]
        // Bumped by `saveDevicePhoto` on every successful save. `imagePath`
        // resolves to the same `file://` URL before and after a re-upload
        // (same vid:pid, likely same extension), so QML's `Image.source`
        // binding wouldn't detect any change and would keep the stale
        // pixmap on screen — every card appends this to the URL as a
        // cache-busting query param so a change here forces every open
        // card (List/Grid/Carousel can all have one for the same device at
        // once) to reload, not just whichever one triggered the upload.
        #[qproperty(i32, photo_revision, cxx_name = "photoRevision")]
        type DeviceController = super::DeviceControllerRust;

        /// Fetch the current device list from the daemon.
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>);

        /// Set a mouse's DPI (must be one of that device's `dpiOptions`) and
        /// refresh. Runs off the Qt thread — a D-Bus round trip to ratbagd.
        #[qinvokable]
        #[cxx_name = "setDpi"]
        fn set_dpi(self: Pin<&mut Self>, device_id: &QString, dpi: i32);

        /// Set a mouse's polling rate in Hz (must be one of that device's
        /// `pollingRateOptions`) and refresh.
        #[qinvokable]
        #[cxx_name = "setPollingRate"]
        fn set_polling_rate(self: Pin<&mut Self>, device_id: &QString, rate: i32);

        /// Resolve the image to show for a device — Devices roadmap Phase 3
        /// (bundled generic silhouettes only, see `opengg_core::device_assets`
        /// for the resolution order and why there's no real per-model photo
        /// tier yet). `vid`/`pid`/`device_type` are the same fields already
        /// present in every entry of `devicesJson` (`.vid`, `.pid`,
        /// `.deviceType`) — no new D-Bus round trip needed. Returns a
        /// `file://` URL for QML's `Image.source`, or an empty string when
        /// there's genuinely nothing to show (never a broken-image path).
        /// Synchronous — just a couple of filesystem `stat` calls.
        #[qinvokable]
        #[cxx_name = "imagePath"]
        fn image_path(self: &Self, vid: i32, pid: i32, device_type: &QString) -> QString;

        // ── Devices roadmap Phase 5: button-mapping editor ──────────────

        /// Fetch a mouse's per-button mappings into `buttonMappingsJson`.
        /// Backgrounded — a D-Bus round trip, same reasoning as `refresh`.
        #[qinvokable]
        #[cxx_name = "refreshButtonMappings"]
        fn refresh_button_mappings(self: Pin<&mut Self>, device_id: &QString);

        /// Set one button's action (`{"type":"key","name":"e"}` etc.) and
        /// refresh both the device list and its button mappings. Applies
        /// immediately, no separate "save" — same convention as `setDpi`.
        #[qinvokable]
        #[cxx_name = "setButtonAction"]
        fn set_button_action(
            self: Pin<&mut Self>,
            device_id: &QString,
            button_index: i32,
            action_json: &QString,
        );

        /// Fetch the static action catalog into `buttonActionCatalogJson`.
        /// Backgrounded for the same reason as `refresh` even though the
        /// daemon answers this one instantly — it's still a session-bus
        /// round trip, and this codebase's rule is "no D-Bus call runs on
        /// the Qt thread," not "only slow ones."
        #[qinvokable]
        #[cxx_name = "refreshButtonActionCatalog"]
        fn refresh_button_action_catalog(self: Pin<&mut Self>);

        /// Save a user-picked local photo for this device into the same
        /// per-user image cache `imagePath` already reads from — never
        /// bundled, never synced, specific to this one install. `source_url`
        /// is whatever QML's `FileDialog.currentFile` hands back (a
        /// `file://` URL). Returns `""` on success or a human-readable
        /// error. Synchronous — a single local file copy, no D-Bus.
        #[qinvokable]
        #[cxx_name = "saveDevicePhoto"]
        fn save_device_photo(self: Pin<&mut Self>, vid: i32, pid: i32, source_url: &QString) -> QString;

        /// Whether this device already has a user-uploaded photo saved —
        /// distinct from `imagePath` returning *something*, since that also
        /// resolves to the generic bundled silhouette when no real photo
        /// exists yet. The button-mapping editor uses this to decide
        /// whether to open straight to hotspot editing or prompt for a
        /// photo first.
        #[qinvokable]
        #[cxx_name = "hasDevicePhoto"]
        fn has_device_photo(self: &Self, vid: i32, pid: i32) -> bool;

        /// This device's saved hotspots, as a JSON array of
        /// `{buttonIndex, x, y}`. `""`/no saved entry yet → `"[]"`.
        /// Synchronous — local file read, no D-Bus.
        #[qinvokable]
        #[cxx_name = "deviceHotspots"]
        fn device_hotspots(self: &Self, vid: i32, pid: i32) -> QString;

        /// Replace this device's saved hotspot set. `linked_ids_json` is
        /// the device's own `linkedIds` field (or `"[]"` for an unmerged
        /// device) — every member gets the same saved set, so a lookup by
        /// whichever link is "primary" later still finds it (see
        /// `opengg_core::button_hotspots`'s module doc comment for why).
        /// Returns `""` on success or a human-readable validation error.
        /// Synchronous — local file read+write, no D-Bus.
        #[qinvokable]
        #[cxx_name = "saveDeviceHotspots"]
        fn save_device_hotspots(
            self: &Self,
            vid: i32,
            pid: i32,
            button_count: i32,
            linked_ids_json: &QString,
            hotspots_json: &QString,
        ) -> QString;

        /// A built-in preset's hotspot coordinates for this vendor:product
        /// pair, as JSON, or `""` if none exists (see
        /// `opengg_core::button_hotspots::BUILTIN_PRESETS`). Synchronous —
        /// a static in-memory table lookup.
        #[qinvokable]
        #[cxx_name = "findDevicePreset"]
        fn find_device_preset(self: &Self, vid: i32, pid: i32) -> QString;
    }

    impl cxx_qt::Threading for DeviceController {}
}

use core::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct DeviceControllerRust {
    devices_json: QString,
    connected: bool,
    last_error: QString,
    last_error_device_id: QString,
    button_mappings_json: QString,
    button_action_catalog_json: QString,
    photo_revision: i32,
}

impl qobject::DeviceController {
    /// Reload the device list.
    ///
    /// DevicesPage polls this on a 3s repeating Timer, and `get_devices` is a
    /// blocking D-Bus round trip, so running it on the Qt thread stalled the
    /// render loop every 3 seconds for as long as the daemon took to answer.
    /// The I/O runs on a worker; only the result comes back to the Qt thread.
    pub fn refresh(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let devices = opengg_core::device::get_devices();
            let _ = qt_thread.queue(move |mut controller| match devices {
                Ok(j) => {
                    controller.as_mut().set_devices_json(QString::from(&j));
                    controller.as_mut().set_connected(true);
                }
                Err(_) => controller.as_mut().set_connected(false),
            });
        });
    }

    /// Set a mouse's DPI and refresh. Always refreshes afterward — on
    /// success so the UI reflects what the device actually reports back
    /// (not just what we asked for), and on failure so a stale slider
    /// position doesn't linger looking like it "took."
    pub fn set_dpi(self: Pin<&mut Self>, device_id: &QString, dpi: i32) {
        let device_id = device_id.to_string();
        let dpi = dpi.max(0) as u32;
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = opengg_core::device::set_mouse_dpi(device_id.clone(), dpi);
            let _ = qt_thread.queue(move |mut controller| {
                match result {
                    Ok(()) => {
                        controller.as_mut().set_last_error(QString::from(""));
                        controller.as_mut().set_last_error_device_id(QString::from(""));
                    }
                    Err(e) => {
                        eprintln!("setDpi: {e}");
                        controller.as_mut().set_last_error(QString::from(&e));
                        controller
                            .as_mut()
                            .set_last_error_device_id(QString::from(&device_id));
                    }
                }
                controller.as_mut().refresh();
            });
        });
    }

    /// Set a mouse's polling rate and refresh. Same always-refresh reasoning
    /// as `set_dpi`.
    pub fn set_polling_rate(self: Pin<&mut Self>, device_id: &QString, rate: i32) {
        let device_id = device_id.to_string();
        let rate = rate.max(0) as u32;
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = opengg_core::device::set_mouse_polling_rate(device_id.clone(), rate);
            let _ = qt_thread.queue(move |mut controller| {
                match result {
                    Ok(()) => {
                        controller.as_mut().set_last_error(QString::from(""));
                        controller.as_mut().set_last_error_device_id(QString::from(""));
                    }
                    Err(e) => {
                        eprintln!("setPollingRate: {e}");
                        controller.as_mut().set_last_error(QString::from(&e));
                        controller
                            .as_mut()
                            .set_last_error_device_id(QString::from(&device_id));
                    }
                }
                controller.as_mut().refresh();
            });
        });
    }

    pub fn image_path(&self, vid: i32, pid: i32, device_type: &QString) -> QString {
        let kind = opengg_core::device_assets::DeviceKind::from_device_type(&device_type.to_string());
        let resolved = opengg_core::device_assets::resolve_device_image(
            vid.max(0) as u16,
            pid.max(0) as u16,
            kind,
        );
        match resolved {
            Some(path) => QString::from(&format!("file://{}", path.display())),
            None => QString::from(""),
        }
    }

    // ── Devices roadmap Phase 5: button-mapping editor ──────────────────

    pub fn refresh_button_mappings(self: Pin<&mut Self>, device_id: &QString) {
        let device_id = device_id.to_string();
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = opengg_core::device::get_button_mappings(device_id);
            let _ = qt_thread.queue(move |mut controller| {
                let json = result.unwrap_or_else(|_| "[]".to_string());
                controller.as_mut().set_button_mappings_json(QString::from(&json));
            });
        });
    }

    pub fn set_button_action(
        self: Pin<&mut Self>,
        device_id: &QString,
        button_index: i32,
        action_json: &QString,
    ) {
        let device_id = device_id.to_string();
        let button_index = button_index.max(0) as u32;
        let action_json = action_json.to_string();
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = opengg_core::device::set_button_action(
                device_id.clone(),
                button_index,
                action_json,
            );
            let _ = qt_thread.queue(move |mut controller| {
                match result {
                    Ok(()) => {
                        controller.as_mut().set_last_error(QString::from(""));
                        controller.as_mut().set_last_error_device_id(QString::from(""));
                    }
                    Err(e) => {
                        eprintln!("setButtonAction: {e}");
                        controller.as_mut().set_last_error(QString::from(&e));
                        controller
                            .as_mut()
                            .set_last_error_device_id(QString::from(&device_id));
                    }
                }
                controller.as_mut().refresh();
                controller.as_mut().refresh_button_mappings(&QString::from(&device_id));
            });
        });
    }

    pub fn refresh_button_action_catalog(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = opengg_core::device::get_button_action_catalog();
            let _ = qt_thread.queue(move |mut controller| {
                let json = result.unwrap_or_else(|_| r#"{"special":[],"key":[]}"#.to_string());
                controller.as_mut().set_button_action_catalog_json(QString::from(&json));
            });
        });
    }

    pub fn save_device_photo(mut self: Pin<&mut Self>, vid: i32, pid: i32, source_url: &QString) -> QString {
        let source = source_url.to_string();
        let source_path = source.strip_prefix("file://").unwrap_or(&source);
        let source_path = std::path::Path::new(source_path);

        let ext = source_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let allowed = opengg_core::device_assets::CACHE_EXTENSIONS;
        if !allowed.contains(&ext.as_str()) {
            return QString::from(&format!(
                "unsupported image type \".{ext}\" (use one of: {})",
                allowed.join(", ")
            ));
        }

        let dest_dir = opengg_core::paths::device_images_cache_dir();
        if let Err(e) = std::fs::create_dir_all(&dest_dir) {
            return QString::from(&format!("couldn't create image cache directory: {e}"));
        }
        let dest = dest_dir.join(format!("{:04x}-{:04x}.{ext}", vid.max(0) as u16, pid.max(0) as u16));

        // The photo tier checks each extension in turn
        // (`opengg_core::device_assets::resolve_device_image`) — remove any
        // stale copy under a *different* extension first so a user who
        // re-uploads in a new format doesn't end up with both an old and a
        // new file and an ambiguous "which one wins" resolution order.
        for other_ext in allowed {
            if *other_ext != ext {
                let _ = std::fs::remove_file(
                    dest_dir.join(format!("{:04x}-{:04x}.{other_ext}", vid.max(0) as u16, pid.max(0) as u16)),
                );
            }
        }

        match std::fs::copy(source_path, &dest) {
            Ok(_) => {
                let next = self.photo_revision() + 1;
                self.as_mut().set_photo_revision(next);
                QString::from("")
            }
            Err(e) => QString::from(&format!("couldn't save photo: {e}")),
        }
    }

    pub fn has_device_photo(&self, vid: i32, pid: i32) -> bool {
        let dest_dir = opengg_core::paths::device_images_cache_dir();
        let (vid, pid) = (vid.max(0) as u16, pid.max(0) as u16);
        opengg_core::device_assets::CACHE_EXTENSIONS
            .iter()
            .any(|ext| dest_dir.join(format!("{vid:04x}-{pid:04x}.{ext}")).is_file())
    }

    pub fn device_hotspots(&self, vid: i32, pid: i32) -> QString {
        let hotspots =
            opengg_core::button_hotspots::get_hotspots(vid.max(0) as u16, pid.max(0) as u16);
        QString::from(&serde_json::to_string(&hotspots).unwrap_or_else(|_| "[]".to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn save_device_hotspots(
        &self,
        vid: i32,
        pid: i32,
        button_count: i32,
        linked_ids_json: &QString,
        hotspots_json: &QString,
    ) -> QString {
        let hotspots: Vec<opengg_core::button_hotspots::Hotspot> =
            match serde_json::from_str(&hotspots_json.to_string()) {
                Ok(h) => h,
                Err(e) => return QString::from(&format!("bad hotspots JSON: {e}")),
            };

        // Every member of a merged device (see `linkedIds` on `DeviceInfo`)
        // gets the same saved set, plus this device's own vid:pid as a
        // fallback for an unmerged device or in case `linkedIds` was empty.
        let mut aliases: Vec<(u16, u16)> = vec![(vid.max(0) as u16, pid.max(0) as u16)];
        if let Ok(linked) = serde_json::from_str::<Vec<String>>(&linked_ids_json.to_string()) {
            for id in linked {
                if let Some((v, p)) = id.split_once(':') {
                    if let (Ok(v), Ok(p)) =
                        (u16::from_str_radix(v, 16), u16::from_str_radix(p, 16))
                    {
                        aliases.push((v, p));
                    }
                }
            }
        }
        aliases.sort_unstable();
        aliases.dedup();

        match opengg_core::button_hotspots::save_hotspots(
            &aliases,
            button_count.max(0) as u32,
            hotspots,
        ) {
            Ok(()) => QString::from(""),
            Err(e) => QString::from(&e),
        }
    }

    pub fn find_device_preset(&self, vid: i32, pid: i32) -> QString {
        match opengg_core::button_hotspots::find_preset(vid.max(0) as u16, pid.max(0) as u16) {
            Some(preset) => {
                let hotspots: Vec<_> = preset
                    .hotspots
                    .iter()
                    .map(|(idx, x, y, label)| {
                        serde_json::json!({"buttonIndex": idx, "x": x, "y": y, "label": label})
                    })
                    .collect();
                QString::from(
                    &serde_json::json!({
                        "modelName": preset.model_name,
                        "hotspots": hotspots,
                    })
                    .to_string(),
                )
            }
            None => QString::from(""),
        }
    }
}

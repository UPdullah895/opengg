//! ShareController — presentation glue for the Share sheet.
//!
//! All planning, encoding and clipboard work lives in `opengg_core::share`;
//! this bridge only holds the probed source and the current plan, runs the
//! slow parts (ffprobe, the encode) off the Qt thread, and publishes state as
//! whole-JSON properties (AGENTS.md landmine #8).
//!
//! Flow: `open(path)` probes once → QML calls `replan(requestJson)` on every
//! control change (pure, instant) → `render()` encodes the current plan into
//! the share cache → the result can be copied to the clipboard as a file or
//! handed to the platform's upload page.

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
        /// The current `SharePlan` as JSON (empty until the probe lands).
        #[qproperty(QString, plan_json, cxx_name = "planJson")]
        /// True while the source is being probed.
        #[qproperty(bool, probing)]
        #[qproperty(bool, running)]
        /// Encode progress, 0..1.
        #[qproperty(f64, progress)]
        #[qproperty(QString, error)]
        /// Path of the finished share file, empty until one is ready.
        #[qproperty(QString, result_path, cxx_name = "resultPath")]
        type ShareController = super::ShareControllerRust;

        /// Probe `filepath` on a worker thread, then publish a first plan
        /// built from `request_json` (a `SharePlanRequest` without `input`).
        #[qinvokable]
        fn open(self: Pin<&mut Self>, filepath: QString, request_json: QString);

        /// Re-plan with new choices. Instant — pure arithmetic on the cached
        /// probe. Ignored until `open` has finished probing.
        #[qinvokable]
        fn replan(self: Pin<&mut Self>, request_json: QString);

        /// Encode the current plan into the share cache.
        #[qinvokable]
        fn render(self: Pin<&mut Self>);

        /// Stop a running encode; its partial file is removed.
        #[qinvokable]
        fn cancel(self: Pin<&mut Self>);

        /// Put `path` on the clipboard as a file (paste into Discord, a chat,
        /// a file manager). Returns an error message, or "" on success.
        #[qinvokable]
        #[cxx_name = "copyFile"]
        fn copy_file(self: &Self, path: QString) -> QString;

        /// Open the platform's upload page (X: a prefilled post) in the
        /// browser. Returns an error message, or "" on success.
        #[qinvokable]
        #[cxx_name = "openUploadPage"]
        fn open_upload_page(self: &Self, platform: QString, caption: QString) -> QString;

        /// Default caption: the game's hashtag plus the user's saved tags.
        #[qinvokable]
        fn caption(self: &Self, game: QString, saved_tags: QString) -> QString;

        /// The tags worth remembering from an edited caption (game tag
        /// removed), for the `shareTags` setting.
        #[qinvokable]
        #[cxx_name = "rememberTags"]
        fn remember_tags(self: &Self, caption: QString, game: QString) -> QString;

        /// `[{id, label, vertical}]` for every platform, in display order.
        #[qinvokable]
        #[cxx_name = "platformsJson"]
        fn platforms_json(self: &Self) -> QString;
    }

    impl cxx_qt::Threading for ShareController {}
}

use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use opengg_core::share::{self, Platform, SharePlan, SharePlanRequest, SourceInfo};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub struct ShareControllerRust {
    plan_json: QString,
    probing: bool,
    running: bool,
    progress: f64,
    error: QString,
    result_path: QString,
    input: String,
    source: Option<SourceInfo>,
    plan: Option<SharePlan>,
    cancel: Arc<AtomicBool>,
    /// Bumped by `open`/`replan`/`render`; a render result carrying an older
    /// value belongs to a clip or plan the sheet no longer shows.
    generation: u64,
}

/// Parse a request from QML, forcing the input to the opened clip.
fn request(input: &str, json: &str) -> SharePlanRequest {
    let mut v: serde_json::Value = serde_json::from_str(json).unwrap_or_else(|_| serde_json::json!({}));
    v["input"] = serde_json::json!(input);
    serde_json::from_value(v).unwrap_or(SharePlanRequest {
        input: input.into(),
        start: None,
        end: None,
        platforms: Vec::new(),
        short_side: None,
        vertical: false,
        crop_pos: 0.5,
        tracks: Vec::new(),
    })
}

impl qobject::ShareController {
    fn publish(mut self: Pin<&mut Self>, plan: SharePlan) {
        let json = serde_json::to_string(&plan).unwrap_or_default();
        self.as_mut().rust_mut().plan = Some(plan);
        self.as_mut().set_plan_json(QString::from(&json));
    }

    pub fn open(mut self: Pin<&mut Self>, filepath: QString, request_json: QString) {
        let input = filepath.to_string();
        {
            let mut r = self.as_mut().rust_mut();
            r.input = input.clone();
            r.source = None;
            r.plan = None;
            r.generation += 1;
            r.cancel.store(true, Ordering::Relaxed);
        }
        self.as_mut().set_running(false);
        self.as_mut().set_plan_json(QString::default());
        self.as_mut().set_result_path(QString::default());
        self.as_mut().set_error(QString::default());
        self.as_mut().set_progress(0.0);
        self.as_mut().set_probing(true);
        let req_json = request_json.to_string();
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let probed = share::probe_source(std::path::Path::new(&input));
            let _ = qt_thread.queue(move |mut c| {
                // A newer `open` superseded this probe.
                if c.as_ref().rust().input != input {
                    return;
                }
                c.as_mut().set_probing(false);
                match probed {
                    Ok(src) => {
                        c.as_mut().rust_mut().source = Some(src);
                        let plan = share::plan_with_source(&request(&input, &req_json), &src);
                        c.publish(plan);
                    }
                    Err(e) => c.as_mut().set_error(QString::from(&e)),
                }
            });
        });
    }

    pub fn replan(mut self: Pin<&mut Self>, request_json: QString) {
        let (input, src) = {
            let r = self.as_ref();
            let r = r.rust();
            (r.input.clone(), r.source)
        };
        let Some(src) = src else { return };
        let plan = share::plan_with_source(&request(&input, &request_json.to_string()), &src);
        // A new plan invalidates a file rendered for the old one, and any
        // render still running for it.
        {
            let mut r = self.as_mut().rust_mut();
            r.generation += 1;
            r.cancel.store(true, Ordering::Relaxed);
        }
        self.as_mut().set_running(false);
        self.as_mut().set_result_path(QString::default());
        self.publish(plan);
    }

    pub fn render(mut self: Pin<&mut Self>) {
        if *self.running() {
            return;
        }
        let Some(plan) = self.as_ref().rust().plan.clone() else { return };
        let cancel = Arc::new(AtomicBool::new(false));
        let generation = {
            let mut r = self.as_mut().rust_mut();
            r.cancel = cancel.clone();
            r.generation += 1;
            r.generation
        };
        self.as_mut().set_running(true);
        self.as_mut().set_progress(0.0);
        self.as_mut().set_error(QString::default());
        self.as_mut().set_result_path(QString::default());
        let qt_thread = self.qt_thread();
        let progress_thread = self.qt_thread();
        std::thread::spawn(move || {
            let out = share::default_output_path(&plan);
            let result = share::render(&plan, &out, &cancel, move |f| {
                let _ = progress_thread.queue(move |mut c| {
                    if c.as_ref().rust().generation == generation {
                        c.as_mut().set_progress(f);
                    }
                });
            });
            let _ = qt_thread.queue(move |mut c| {
                if c.as_ref().rust().generation != generation {
                    return; // superseded by a newer clip, plan or render
                }
                c.as_mut().set_running(false);
                match result {
                    Ok(p) => c.as_mut().set_result_path(QString::from(&p.to_string_lossy().to_string())),
                    // A user cancel is not an error.
                    Err(e) if e == share::CANCELLED => c.as_mut().set_progress(0.0),
                    Err(e) => c.as_mut().set_error(QString::from(&e)),
                }
            });
        });
    }

    pub fn cancel(self: Pin<&mut Self>) {
        self.rust().cancel.store(true, Ordering::Relaxed);
    }

    pub fn copy_file(&self, path: QString) -> QString {
        match share::copy_files_to_clipboard(&[path.to_string()]) {
            Ok(()) => QString::default(),
            Err(e) => QString::from(&e),
        }
    }

    pub fn open_upload_page(&self, platform: QString, caption: QString) -> QString {
        let p = Platform::from_id(&platform.to_string()).unwrap_or(Platform::Original);
        let url = match p {
            Platform::X | Platform::XPremium => Some(share::x_intent_url(&caption.to_string())),
            other => share::upload_page(other).map(str::to_string),
        };
        let Some(url) = url else { return QString::default() };
        match opengg_core::system::open_path(std::path::Path::new(&url)) {
            Ok(()) => QString::default(),
            Err(e) => QString::from(&e),
        }
    }

    pub fn caption(&self, game: QString, saved_tags: QString) -> QString {
        QString::from(&share::caption(&game.to_string(), &saved_tags.to_string()))
    }

    pub fn remember_tags(&self, caption: QString, game: QString) -> QString {
        QString::from(&share::remember_tags(&caption.to_string(), &game.to_string()))
    }

    pub fn platforms_json(&self) -> QString {
        let list: Vec<serde_json::Value> = Platform::ALL
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id(),
                    "label": p.label(),
                    "vertical": p.spec().vertical_default,
                })
            })
            .collect();
        QString::from(&serde_json::to_string(&list).unwrap_or_else(|_| "[]".into()))
    }
}

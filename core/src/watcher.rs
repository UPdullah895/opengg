//! Filesystem watcher for the clip library. Ported from the Tauri host's
//! `notify`-crate watcher (`frontend/src-tauri/src/main.rs`, the "Power
//! User: Live file-system watcher" block) — same directories, same
//! Create/Remove + video-extension filter. The Tauri original also watches
//! the extensions directory for `manifest.json` changes; not ported here
//! (no live consumer in qt-shell yet — `ExtensionsPanel.qml` reads the
//! manifest list on demand instead of needing push updates).

use notify::event::{CreateKind, EventKind, RemoveKind};
use notify::{Config, RecursiveMode, Watcher};
use std::time::Duration;

const VIDEO_WATCH_EXTS: &[&str] = &["mp4", "mkv", "webm", "avi", "mov", "ts", "flv"];

/// Spawns a background thread that watches `paths::get_all_clip_dirs("")`
/// for video file create/remove events and calls `on_change` — debounced
/// (leading-edge, 250ms cooldown) so a burst of events from one file write
/// collapses into a single call. Fire-and-forget: runs for the process
/// lifetime, nothing currently needs to stop it.
pub fn spawn_clip_watcher<F>(on_change: F)
where
    F: Fn() + Send + 'static,
{
    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
        let mut watcher = match notify::RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default(),
        ) {
            Ok(w) => w,
            Err(e) => {
                log::error!("clip watcher init failed: {e}");
                return;
            }
        };

        for dir in crate::paths::get_all_clip_dirs("") {
            let _ = std::fs::create_dir_all(&dir);
            if let Err(e) = watcher.watch(&dir, RecursiveMode::Recursive) {
                log::warn!("clip watcher: cannot watch {dir:?}: {e}");
            } else {
                log::info!("clip watcher: watching {dir:?}");
            }
        }

        let mut last_emit = std::time::Instant::now()
            .checked_sub(Duration::from_secs(10))
            .unwrap_or_else(std::time::Instant::now);

        for res in rx {
            let Ok(event) = res else { continue };
            let is_clip_event = event.paths.iter().any(|p| {
                let ext = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                VIDEO_WATCH_EXTS.contains(&ext.as_str())
            }) && matches!(
                event.kind,
                EventKind::Create(CreateKind::File)
                    | EventKind::Create(_)
                    | EventKind::Remove(RemoveKind::File)
                    | EventKind::Remove(_)
            );
            if is_clip_event && last_emit.elapsed() > Duration::from_millis(250) {
                on_change();
                last_emit = std::time::Instant::now();
            }
        }
        // Unreachable in practice (`rx` only closes if `watcher` — which
        // owns the sender's closure — is dropped, and it's kept alive by
        // this thread's own stack), kept for clarity that the watcher must
        // outlive the loop.
        drop(watcher);
    });
}

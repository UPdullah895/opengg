//! ClipsController — a `QAbstractListModel`-backed QML singleton exposing the
//! local clip library from `opengg_core::clips` (SQLite metadata + filesystem
//! scan; the "fast" lister, no ffprobe).
//!
//! Boundary (plan §2.2/§2.3): all clip DB + path resolution lives in
//! `opengg_core`; this is pure presentation glue. Search/sort/game-filter are
//! now server-side state (setSearchText/setGameFilter/setSortMode) instead of
//! QML-side array juggling: `all_clips` holds the full unfiltered scan,
//! `view` holds indices into it after filtering+sorting, and row_count()/
//! data() read through `view`. This is the plan's "C-model" upgrade,
//! replacing the earlier JSON-array-parsed-in-QML approach (clipsJson).
//!
//! Thumbnails: rather than a full `QQuickAsyncImageProvider` C++ shim (no
//! cxx-qt-lib binding for it in 0.9), `requestThumbnail` uses the documented
//! `cxx_qt::Threading` pattern — a background worker thread (single, so ffmpeg
//! invocations are serialized instead of stampeding the disk) generates the
//! JPEG via `opengg_core::media::generate_thumbnail`, then queues a `reload()`
//! back onto the Qt thread. QML calls it once per clip lacking a cached thumb;
//! the in-flight set on the Rust struct dedupes repeat calls across reloads.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        /// Base for the clip list model.
        type QAbstractListModel;

        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;

        include!("cxx-qt-lib/qbytearray.h");
        type QByteArray = cxx_qt_lib::QByteArray;

        include!("cxx-qt-lib/qhash.h");
        /// QHash<i32, QByteArray> from cxx_qt_lib
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
    }

    /// Roles exposed to QML for each row — see `role_names()`.
    #[qenum(ClipsController)]
    enum ClipRoles {
        FilePath,
        Thumbnail,
        Duration,
        Title,
        Game,
        Filesize,
        Favorite,
        // ClipCard.vue also shows the capture time and a resolution pill; the
        // data was already on ClipInfo, just never exposed as roles.
        Created,
        Width,
        Height,
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[base = QAbstractListModel]
        #[qproperty(bool, loading)]
        #[qproperty(QString, error)]
        #[qproperty(i32, count)]
        #[qproperty(i32, total_count, cxx_name = "totalCount")]
        #[qproperty(QStringList, game_list, cxx_name = "gameList")]
        /// Number of favourited clips in the whole library — drives the
        /// toolbar's favourites-filter button badge.
        #[qproperty(i32, fav_count, cxx_name = "favCount")]
        /// Aggregates over the CURRENTLY VISIBLE (filtered) rows, as JSON:
        /// `{count, totalDuration, totalSize, avgDuration}`. Backs the stats
        /// bar, which reports what you're looking at rather than the library.
        #[qproperty(QString, stats_json, cxx_name = "statsJson")]
        /// Bumped on every recompute of the view. The date-grouped view builds
        /// its own JS snapshot via `visibleJson()` and needs a reliable "the
        /// rows changed" signal to rebuild from; `count`/`statsJson` both miss
        /// changes that leave their values identical (toggling a favourite,
        /// renaming a clip), so this is an unconditional generation counter.
        #[qproperty(i32, revision)]
        type ClipsController = super::ClipsControllerRust;

        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &Self, _parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &Self, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &Self) -> QHash_i32_QByteArray;

        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut Self>);

        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut Self>);

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

        /// Generate a cached thumbnail for a clip on a background worker
        /// thread (no-op if already cached or already in flight), then
        /// rescan so the thumbnail role picks up the new path.
        #[qinvokable]
        #[cxx_name = "requestThumbnail"]
        fn request_thumbnail(self: Pin<&mut Self>, filepath: &QString);

        /// Free-text search over title + game (case-insensitive substring).
        #[qinvokable]
        #[cxx_name = "setSearchText"]
        fn set_search_text(self: Pin<&mut Self>, text: &QString);

        /// Restrict to one game; empty string = all games.
        #[qinvokable]
        #[cxx_name = "setGameFilter"]
        fn set_game_filter(self: Pin<&mut Self>, game: &QString);

        /// One of "newest" | "oldest" | "longest" | "shortest".
        #[qinvokable]
        #[cxx_name = "setSortMode"]
        fn set_sort_mode(self: Pin<&mut Self>, mode: &QString);

        /// Restrict the view to favourited clips only.
        #[qinvokable]
        #[cxx_name = "setFavoritesOnly"]
        fn set_favorites_only(self: Pin<&mut Self>, only: bool);

        /// Filepaths of every currently-visible row, in view order — backs
        /// "select all" without QML having to walk the model by role.
        #[qinvokable]
        #[cxx_name = "visibleFilepaths"]
        fn visible_filepaths(self: &Self) -> QStringList;

        /// Every currently-visible row as a JSON array, in view order. The
        /// date-grouped view needs to partition the rows by capture date and
        /// render its own headers, which a QAbstractListModel can't express —
        /// QML has no way to walk a model by role. Only that view calls this.
        #[qinvokable]
        #[cxx_name = "visibleJson"]
        fn visible_json(self: &Self) -> QString;

        /// Filepaths of every favourited clip in the library. Lets QML decide
        /// whether a bulk favourite action should set or clear the flag.
        #[qinvokable]
        #[cxx_name = "favoritePaths"]
        fn favorite_paths(self: &Self) -> QStringList;

        /// Bulk favourite/unfavourite. One reload at the end instead of the
        /// N reloads a QML-side loop over `setFavorite` would cause.
        #[qinvokable]
        #[cxx_name = "setFavorites"]
        fn set_favorites(self: Pin<&mut Self>, filepaths: &QStringList, favorite: bool);

        /// Bulk delete, same single-reload rationale as `setFavorites`.
        #[qinvokable]
        #[cxx_name = "deleteClips"]
        fn delete_clips(self: Pin<&mut Self>, filepaths: &QStringList);

        /// Start the background clip-directory filesystem watcher (idempotent
        /// — safe to call more than once, only the first call spawns it).
        /// Auto-refreshes the gallery when a clip file is added or removed on
        /// disk, instead of requiring a manual refresh.
        #[qinvokable]
        #[cxx_name = "startWatcher"]
        fn start_watcher(self: Pin<&mut Self>);

        /// The `limit` most-recently-created clips as a JSON array of
        /// `{filepath, title, thumbnail, favorite}`, newest first. Reads
        /// `all_clips` directly rather than the filtered `view` the model
        /// exposes, so the Dashboard's recent-clips popover always shows the
        /// true most-recent clips regardless of whatever search/game-filter/
        /// sort the Clips page currently has active — mirrors HomePage.vue's
        /// `recentClips`, which reads the raw `replay.clips` store array
        /// rather than ClipsPage's locally-filtered view.
        #[qinvokable]
        #[cxx_name = "recentJson"]
        fn recent_json(self: &Self, limit: i32) -> QString;
    }

    impl cxx_qt::Threading for ClipsController {}
}

use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QStringList, QVariant};
use opengg_core::clips::{ClipInfo, ClipMetaUpdate};
use qobject::{ClipRoles, QHash_i32_QByteArray};
use std::collections::HashSet;
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;

#[derive(Default)]
pub struct ClipsControllerRust {
    all_clips: Vec<ClipInfo>,
    /// Indices into `all_clips` after filter+sort — what the model exposes.
    view: Vec<usize>,
    search_text: String,
    game_filter: String,
    sort_mode: String,
    favorites_only: bool,
    loading: bool,
    error: QString,
    count: i32,
    total_count: i32,
    fav_count: i32,
    stats_json: QString,
    revision: i32,
    game_list: QStringList,
    /// Filepaths currently queued/generating on the thumbnail worker thread.
    thumbs_in_flight: HashSet<String>,
}

type ThumbJob = (String, cxx_qt::CxxQtThread<qobject::ClipsController>);

/// Single persistent worker thread serializing ffmpeg thumbnail generation
/// (avoids spawning dozens of concurrent ffmpeg processes when a grid full
/// of un-thumbnailed clips mounts at once).
fn thumb_worker() -> &'static Sender<ThumbJob> {
    static QUEUE: OnceLock<Sender<ThumbJob>> = OnceLock::new();
    QUEUE.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<ThumbJob>();
        std::thread::spawn(move || {
            for (filepath, qt_thread) in rx {
                let result = opengg_core::media::generate_thumbnail(filepath.clone(), None);
                let _ = qt_thread.queue(move |mut controller| {
                    controller.as_mut().rust_mut().thumbs_in_flight.remove(&filepath);
                    if result.is_ok() {
                        controller.reload();
                    }
                });
            }
        });
        tx
    })
}

/// Display title fallback chain, mirrors the old QML displayName(): custom
/// name, else game, else filename, else "Untitled".
fn title_for(clip: &ClipInfo) -> String {
    if !clip.custom_name.is_empty() {
        clip.custom_name.clone()
    } else if !clip.game.is_empty() {
        clip.game.clone()
    } else if !clip.filename.is_empty() {
        clip.filename.clone()
    } else {
        "Untitled".to_string()
    }
}

/// Compute the filtered+sorted row order (indices into `clips`). Ported
/// directly from the old QML `filteredClips` computed property.
fn compute_view(
    clips: &[ClipInfo],
    search: &str,
    game_filter: &str,
    sort_mode: &str,
    favorites_only: bool,
) -> Vec<usize> {
    let search = search.to_lowercase();
    let mut indices: Vec<usize> = clips
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            if favorites_only && !c.favorite {
                return false;
            }
            if !game_filter.is_empty() {
                let g = if c.game.is_empty() { "Unknown" } else { c.game.as_str() };
                if g != game_filter {
                    return false;
                }
            }
            if !search.is_empty() {
                let title = title_for(c).to_lowercase();
                let game = c.game.to_lowercase();
                if !title.contains(&search) && !game.contains(&search) {
                    return false;
                }
            }
            true
        })
        .map(|(i, _)| i)
        .collect();

    match sort_mode {
        "oldest" => indices.sort_by_key(|&i| clips[i].created_ts),
        "longest" => indices.sort_by(|&a, &b| {
            clips[b]
                .duration
                .partial_cmp(&clips[a].duration)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        "shortest" => indices.sort_by(|&a, &b| {
            clips[a]
                .duration
                .partial_cmp(&clips[b].duration)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        _ => indices.sort_by_key(|&i| std::cmp::Reverse(clips[i].created_ts)), // "newest" default
    }

    indices
}

impl qobject::ClipsController {
    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.view.len() as i32
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let row = index.row();
        if row < 0 {
            return QVariant::default();
        }
        let Some(&real_idx) = self.view.get(row as usize) else {
            return QVariant::default();
        };
        let Some(clip) = self.all_clips.get(real_idx) else {
            return QVariant::default();
        };
        let role = ClipRoles { repr: role };
        match role {
            ClipRoles::FilePath => QVariant::from(&QString::from(clip.filepath.as_str())),
            ClipRoles::Thumbnail => QVariant::from(&QString::from(clip.thumbnail.as_str())),
            ClipRoles::Duration => QVariant::from(&clip.duration),
            ClipRoles::Title => QVariant::from(&QString::from(title_for(clip).as_str())),
            ClipRoles::Game => QVariant::from(&QString::from(clip.game.as_str())),
            ClipRoles::Filesize => QVariant::from(&clip.filesize),
            ClipRoles::Favorite => QVariant::from(&clip.favorite),
            ClipRoles::Created => QVariant::from(&QString::from(clip.created.as_str())),
            ClipRoles::Width => QVariant::from(&(clip.width as i32)),
            ClipRoles::Height => QVariant::from(&(clip.height as i32)),
            _ => QVariant::default(),
        }
    }

    pub fn recent_json(&self, limit: i32) -> QString {
        let mut indices: Vec<usize> = (0..self.all_clips.len()).collect();
        indices.sort_by_key(|&i| std::cmp::Reverse(self.all_clips[i].created_ts));
        indices.truncate(limit.max(0) as usize);

        let items: Vec<serde_json::Value> = indices
            .iter()
            .map(|&i| {
                let clip = &self.all_clips[i];
                serde_json::json!({
                    "filepath": clip.filepath,
                    "title": title_for(clip),
                    "thumbnail": clip.thumbnail,
                    "favorite": clip.favorite,
                })
            })
            .collect();

        QString::from(&serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string()))
    }

    fn role_names(&self) -> QHash_i32_QByteArray {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ClipRoles::FilePath.repr, QByteArray::from("filepath"));
        roles.insert(ClipRoles::Thumbnail.repr, QByteArray::from("thumbnail"));
        roles.insert(ClipRoles::Duration.repr, QByteArray::from("duration"));
        roles.insert(ClipRoles::Title.repr, QByteArray::from("title"));
        roles.insert(ClipRoles::Game.repr, QByteArray::from("game"));
        roles.insert(ClipRoles::Filesize.repr, QByteArray::from("filesize"));
        roles.insert(ClipRoles::Favorite.repr, QByteArray::from("favorite"));
        roles.insert(ClipRoles::Created.repr, QByteArray::from("created"));
        roles.insert(ClipRoles::Width.repr, QByteArray::from("width"));
        roles.insert(ClipRoles::Height.repr, QByteArray::from("height"));
        roles
    }

    /// Rescan the library, rebuild the game list, and reapply the current
    /// filter/sort. Shared by refresh() and every mutation invokable.
    fn reload(mut self: Pin<&mut Self>) {
        self.as_mut().set_loading(true);
        // Empty folder → core resolves configured clip_directories or the
        // default (~/Videos/OpenGG) via resolve_clips_dir/get_all_clip_dirs.
        match opengg_core::clips::get_clips_fast(String::new()) {
            Ok(clips) => {
                self.as_mut().rust_mut().all_clips = clips;
                self.as_mut().set_error(QString::default());
            }
            Err(e) => {
                self.as_mut().rust_mut().all_clips = Vec::new();
                self.as_mut().set_error(QString::from(&e));
            }
        }
        self.as_mut().update_game_list();
        self.as_mut().apply_filter();
        self.as_mut().set_loading(false);
    }

    fn update_game_list(mut self: Pin<&mut Self>) {
        let mut games: Vec<String> = self
            .all_clips
            .iter()
            .map(|c| if c.game.is_empty() { "Unknown".to_string() } else { c.game.clone() })
            .collect();
        games.sort();
        games.dedup();
        let list: QStringList = std::iter::once(QString::from("All games"))
            .chain(games.iter().map(|g| QString::from(g.as_str())))
            .collect();
        self.as_mut().set_game_list(list);
    }

    /// Recompute `view` from the current search/game/sort state and reset
    /// the model. Called after every reload() and every filter-state change.
    fn apply_filter(mut self: Pin<&mut Self>) {
        let indices = compute_view(
            &self.all_clips,
            &self.search_text,
            &self.game_filter,
            &self.sort_mode,
            self.favorites_only,
        );
        let total = self.all_clips.len() as i32;
        let favs = self.all_clips.iter().filter(|c| c.favorite).count() as i32;

        // Stats describe the visible rows, so they have to be summed from the
        // freshly-computed indices rather than from `all_clips`.
        let count = indices.len();
        let total_duration: f64 = indices.iter().map(|&i| self.all_clips[i].duration).sum();
        let total_size: u64 = indices.iter().map(|&i| self.all_clips[i].filesize).sum();
        let stats = serde_json::json!({
            "count": count,
            "totalDuration": total_duration,
            "totalSize": total_size,
            "avgDuration": if count > 0 { total_duration / count as f64 } else { 0.0 },
        });

        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().view = indices;
        self.as_mut().end_reset_model();
        self.as_mut().set_count(count as i32);
        self.as_mut().set_total_count(total);
        self.as_mut().set_fav_count(favs);
        self.as_mut().set_stats_json(QString::from(&stats.to_string()));
        let next = self.revision.wrapping_add(1);
        self.as_mut().set_revision(next);
    }

    pub fn refresh(self: Pin<&mut Self>) {
        self.reload();
    }

    pub fn set_search_text(mut self: Pin<&mut Self>, text: &QString) {
        self.as_mut().rust_mut().search_text = text.to_string();
        self.apply_filter();
    }

    pub fn set_game_filter(mut self: Pin<&mut Self>, game: &QString) {
        self.as_mut().rust_mut().game_filter = game.to_string();
        self.apply_filter();
    }

    pub fn set_sort_mode(mut self: Pin<&mut Self>, mode: &QString) {
        self.as_mut().rust_mut().sort_mode = mode.to_string();
        self.apply_filter();
    }

    pub fn set_favorites_only(mut self: Pin<&mut Self>, only: bool) {
        self.as_mut().rust_mut().favorites_only = only;
        self.apply_filter();
    }

    pub fn visible_filepaths(&self) -> QStringList {
        self.view
            .iter()
            .map(|&i| QString::from(self.all_clips[i].filepath.as_str()))
            .collect()
    }

    pub fn visible_json(&self) -> QString {
        let rows: Vec<serde_json::Value> = self
            .view
            .iter()
            .map(|&i| {
                let c = &self.all_clips[i];
                serde_json::json!({
                    "filepath": c.filepath,
                    "thumbnail": c.thumbnail,
                    "duration": c.duration,
                    "title": title_for(c),
                    "game": c.game,
                    "filesize": c.filesize,
                    "favorite": c.favorite,
                    "created": c.created,
                    "width": c.width,
                    "height": c.height,
                })
            })
            .collect();
        QString::from(&serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into()))
    }

    pub fn favorite_paths(&self) -> QStringList {
        self.all_clips
            .iter()
            .filter(|c| c.favorite)
            .map(|c| QString::from(c.filepath.as_str()))
            .collect()
    }

    pub fn set_favorites(mut self: Pin<&mut Self>, filepaths: &QStringList, favorite: bool) {
        for fp in filepaths.iter() {
            let update = ClipMetaUpdate {
                favorite: Some(favorite),
                ..Self::meta_update(fp.to_string())
            };
            if let Err(e) = opengg_core::clips::set_clip_meta(update) {
                eprintln!("set_favorites: {e}");
            }
        }
        self.as_mut().reload();
    }

    pub fn delete_clips(mut self: Pin<&mut Self>, filepaths: &QStringList) {
        for fp in filepaths.iter() {
            if let Err(e) = opengg_core::clips::delete_clip(&fp.to_string()) {
                eprintln!("delete_clips: {e}");
            }
        }
        self.as_mut().reload();
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

    pub fn request_thumbnail(mut self: Pin<&mut Self>, filepath: &QString) {
        let fp = filepath.to_string();
        let already_queued = !self.as_mut().rust_mut().thumbs_in_flight.insert(fp.clone());
        if already_queued {
            return;
        }
        let qt_thread = self.qt_thread();
        let _ = thumb_worker().send((fp, qt_thread));
    }

    pub fn start_watcher(self: Pin<&mut Self>) {
        static STARTED: OnceLock<()> = OnceLock::new();
        let qt_thread = self.qt_thread();
        STARTED.get_or_init(|| {
            opengg_core::watcher::spawn_clip_watcher(move || {
                let _ = qt_thread.queue(|mut controller| {
                    controller.as_mut().refresh();
                });
            });
        });
    }
}

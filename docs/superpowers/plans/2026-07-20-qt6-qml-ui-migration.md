# OpenGG Qt6/QML UI Migration Plan

> **For agentic workers:** This is the authoritative *migration reference*, not a
> task-by-task executable plan. Each phase in §4 gets its own detailed
> implementation plan (via `superpowers:writing-plans`) at execution time, on the
> dedicated migration branch. Do not begin implementation from this document alone.

**Goal:** Rebuild OpenGG's entire UI in Qt6/QML + cxx-qt, matching the existing
Tauri/Vue UI's layout, features, and behavior (visual parity, not redesign),
with first-class multi-language support including Arabic (RTL) — while reusing
the existing Rust business logic unchanged.

**Architecture:** The Rust daemon (`openggd`, D-Bus/zbus) stays untouched. Rust
logic currently living in the Tauri shell (`frontend/src-tauri`) is *relocated*
(moved verbatim, not rewritten) into a shared `opengg-core` crate. A new
`qt-shell` crate exposes that logic to QML through thin cxx-qt QObject wrappers,
following the architecture validated by the `opengg-qt6-video-poc` prototype.

**Tech Stack:** Qt 6.11+ (Quick, QuickControls2, Multimedia), cxx-qt 0.9,
gstreamer-rs 0.25.x (manual pipeline for multi-track audio), zbus 4 (daemon
client), Cargo-only build via `cxx-qt-build` (no CMake).

## Global Constraints

- **No business-logic rewrite.** Existing Rust modules move via `git mv` /
  workspace restructure only; behavior changes are out of scope for every phase.
- **Backend stays Rust.** No logic is reimplemented in C++ or QML. The only
  permitted hand-written C++ is presentation *infrastructure* explicitly listed
  in this plan (currently one item: the `JsonTranslator` shim, §3.1).
- **Isolated branch.** All work happens on a `qt6-migration` branch (worktree
  recommended, per repo convention). `main` and the Tauri/Vue app keep building
  throughout; nothing merges until a phase passes its acceptance gate.
- **Visual parity is the goal.** Deviations from the current Vue UI require an
  explicit entry in the phase's "approved deviations" list (see §7.1) — never
  silent.
- **Version floors:** Qt ≥ 6.11, GStreamer ≥ 1.28, cxx-qt = 0.9.x (pinned),
  `qmake6` on `PATH` at build time. Integer-only `font.pixelSize` everywhere
  (see risk R1).
- **Licensing:** Qt6 is linked dynamically in every build and package target
  (LGPLv3 — hard rule, §5.1); codec and hardware-decode capability comes from
  system packages, never vendored into anything this project distributes
  (§5.1/§5.3). Distribution is deb/rpm, AUR, and Flatpak only (§5.2).
- **Reference environment:** Hyprland (native Wayland), NVIDIA RTX 3070 — the
  only environment the PoC validated. Anything else is untested until the §7
  matrix says otherwise.

---

## 0. Inputs & Ground Truth

What this plan is built from (verified in-repo on 2026-07-20):

**Current app** (`opengg/`):
- `daemon/` — `openggd` Rust daemon: `audio/` (PipeWire hub, routing, sinks,
  effects), `device/` (headset, ratbag, OpenRGB, D-Bus, process watch,
  profiles), `replay/` (recorder, clips, hotkey, D-Bus), `extensions/`,
  `ipc/` (zbus interfaces), `platform/` (linux + stub). **Untouched by this plan.**
- `frontend/src-tauri/` — Tauri 2 shell: `commands.rs` (~80 commands: clips DB
  (SQLite), ffmpeg probe/trim/export/thumbnails, storage stats, settings/theme/
  locale IO, extension scanning, Steam detection, screen recording control),
  `media_server.rs` (token-guarded HTTP server feeding the webview),
  `vu_native.rs` (native PipeWire VU taps → Tauri events), `subprocess.rs`.
- `frontend/src/` — Vue 3 + Pinia + vue-i18n: 5 pages, ~50 components,
  7 stores, 10 composables. i18n: `locales/en.json` + `locales/ar.json`
  (609 leaf keys each, `_meta: {name, dir}`), runtime user-droppable locales
  from `~/.config/opengg/locales/`, `scripts/check-locales.mjs` parity checker.
- Packaging: deb + rpm (Tauri bundler, depends on `webkit2gtk-4.1`), AUR
  `packaging/aur/PKGBUILD`, `packaging/flatpak/`, systemd + D-Bus service
  files, polkit policy, udev rules.

**Prototype** (`opengg-qt6-video-poc/`) — verdict **PASS**, with these findings
this plan treats as established facts:
1. Qt Multimedia `MediaPlayer`/`VideoOutput` on the **GStreamer backend**
   (`QT_MEDIA_BACKEND=gstreamer`) renders 1080p60 H.264 smoothly on native
   Hyprland/Wayland via the texture/scenegraph path — no subsurface hacks, no
   tearing, hardware decode (`libgstnvcodec.so`) confirmed loaded.
2. XWayland fallback (`QT_QPA_PLATFORM=xcb`) exists but was **not needed** and
   is therefore **not exercised/validated**.
3. Qt Multimedia decodes only the **first audio stream** of a multi-track clip.
   Resolved: video stays on Qt Multimedia (its audio muted), audio runs through
   a manual GStreamer pipeline (`filesrc ! decodebin` → per-track
   `queue!audioconvert!audioresample!volume` → `audiomixer` → `tee` → monitor
   branch + export branch), driven from QML via a cxx-qt `AudioMixerController`.
   Monitor volume sits after the tee → fully isolated from export output
   (verified by `scripts/verify-export-isolation.sh`).
4. cxx-qt QObjects (`PlayerController`, `AudioMixerController`) bridge Rust ↔
   QML for properties, signals, and invokables; `CxxQtBuilder` +
   `QmlModule` builds everything from Cargo alone.
5. **Landmine:** fractional `font.pixelSize` (13.5, 11.5) hangs the app forever
   on this system (0% CPU, no window). Reproducible. Integer sizes only.
6. Nothing resets `playing` to `false` at natural end-of-media in the PoC —
   must be handled in the real player.
7. `ydotool` held-drag gestures cannot be delivered reliably to a
   native-Wayland Qt window; `xdotool` can't target it at all. Constrains the
   automated-testing strategy (§7).

Reusable PoC artifacts: `qml/Titlebar.qml`, `qml/Sidebar.qml`,
`qml/ClipCard.qml`, `qml/PlayerPage.qml`, `qml/LibraryPage.qml`,
`qml/EditPage.qml`, `src/player.rs`, `src/audio_mixer.rs`,
`src/mixer_pipeline.rs`, `build.rs` pattern. These are *starting points* to be
adapted to parity, not shippable as-is (they were a design showcase).

---

## 1. UI Inventory & Parity Mapping

### 1.1 Screen inventory

Navigation model: **no router** — `App.vue` holds `currentPage: 'home' |
'mixer' | 'clips' | 'devices' | 'settings'`; `Sidebar` emits `navigate`.
QML equivalent: `StackLayout` indexed by a `currentPage` string on the root
window — same model, deliberately (the guided tour and `useNavSignal` drive
navigation through it).

| # | Screen | Vue source (lines) | QML target | Parity notes |
|---|--------|--------------------|------------|--------------|
| S1 | App shell | `App.vue` (~600) | `Main.qml` + `AppShell.qml` | Frameless 1280×800 window (`decorations:false` → `Qt.FramelessWindowHint`), custom resize handles → `Window.startSystemResize()`, titlebar drag → `startSystemMove()`. First-launch language picker + onboarding modal + global overlays live here. |
| S2 | Home dashboard | `pages/HomePage.vue` (641) | `pages/HomePage.qml` | Quick volume sliders (`VolumeSlider`), device pickers (`SelectField`), status cards. Exact match achievable. |
| S3 | Audio mixer | `pages/MixerPage.vue` (417) | `pages/MixerPage.qml` | `ChannelStrip` × channels, live VU meters, `GraphicEQ`, `DspControls`, `ChatMix`, drag-to-route `DropZone`, tabs. Exact match achievable; VU feed changes transport (§2.3). |
| S4 | Clip library | `pages/ClipsPage.vue` (2042) | `pages/ClipsPage.qml` | Largest page. Grid/list toggle, toolbar, stats bar, game filter/tag dropdowns, context menu, Steam banner, drop-import, async thumbnail queue. Needs `QAbstractListModel` (not JS arrays) for the clip list — see C-model note §2.2. |
| S5 | Video player | `components/CustomVideoPlayer.vue` (326) | `components/VideoPlayer.qml` | PoC-validated path. Must add end-of-media reset (PoC gap #6). |
| S6 | Clip editor (basic trim) | `components/ClipEditor.vue` (536) | `pages/EditorPage.qml` (trim mode) | Trim handles on a timeline strip; PoC `EditPage.qml` has the skeleton. |
| S7 | Advanced editor | `components/AdvancedEditor.vue` (1161) + `TimelineTrackRow.vue` | `pages/EditorPage.qml` (multi-track mode) + `TimelineTrack.qml` | Highest-risk screen. Current Vue impl syncs HTML `<audio>` elements to the video via rAF — fragile web idiom. Replaced by the PoC GStreamer mixer pipeline (behavioral improvement, listed as approved deviation A1). Per-track volume, mute/solo, trim, undo/redo, splitter, export with progress. |
| S8 | Devices | `pages/DevicesPage.vue` (231) | `pages/DevicesPage.qml` | `DeviceCard`, `HeadsetSettings`, `MouseSettings`, `MouseMacros`, `IconPicker`. Exact match achievable. |
| S9 | Settings | `pages/SettingsPage.vue` (192) + 12 panels under `components/settings/` | `pages/SettingsPage.qml` + `settings/*.qml` (one file per panel) | Panels: General, Language, Notifications, Shortcuts, Storage, Store, Extensions, MixerRouting, TrackManagement, CaptureSound, RecorderInstallHelper, About. Extensions panel is special — see W6. |
| S10 | Recording controls | `components/RecordingDropdown.vue` + `stores/replay.ts` | `components/RecordingMenu.qml` | Start/stop replay & recording, duration select, status pill. Daemon-backed; simple. |
| S11 | Clip-saved overlay | `components/ClipNotification.vue` (second webview window, `?overlay=1`, transparent bg) | separate `NotificationWindow.qml` (`Qt.FramelessWindowHint`, `color: "transparent"`) | Wayland cannot position windows globally → needs LayerShellQt or a Hyprland `windowrule` fallback. Risk R6. |
| S12 | Guided tour + onboarding | `components/tour/GuidedTour.vue` (405) + `tour/steps.ts` | `components/GuidedTour.qml` | Web idiom W5 — DOM-selector spotlighting must be re-anchored to QML item references. |
| S13 | Global chrome | `Titlebar.vue`, `Sidebar.vue`, `ToastContainer.vue`, `GlobalModal.vue` | `Titlebar.qml`, `Sidebar.qml`, `Toast.qml`, `AppModal.qml` | Titlebar/Sidebar skeletons exist in PoC. Sidebar = 5 items (home/mixer/clips/devices/settings) with stroke-based SVG path icons — reuse the exact SVG path data via `Shape`/`ShapePath` or pre-rendered SVG assets so icons match 1:1. |

### 1.2 Component parity map

Legend: **QQC2** = restyled Qt Quick Controls 2 control; **custom** = bespoke
QML; **PoC** = prototype artifact to adapt.

| Vue component | QML equivalent | Kind | Notes / adaptation |
|---|---|---|---|
| `Titlebar.vue` | `Titlebar.qml` | custom (PoC) | Window buttons call `close()/showMinimized()`; drag area → `startSystemMove()`. |
| `Sidebar.vue` | `Sidebar.qml` | custom (PoC) | Keep 13.5→14-style integer font sizes (R1). |
| `ToggleSwitch.vue` | `Switch` | QQC2 | Restyle to theme tokens. |
| `SelectField.vue` | `ComboBox` | QQC2 | Custom popup + delegate to match visuals. |
| `VolumeSlider.vue` | `Slider` | QQC2 | QQC2 auto-mirrors in RTL — desired for mixer volumes. |
| `SmartTooltip.vue` (@floating-ui) | `ToolTip` / custom `Popup` | QQC2 | floating-ui's flip/shift positioning → `Popup` position math; simpler in QML since no viewport clipping by DOM. |
| `OverlayScrollbar.vue` | `ScrollBar` | QQC2 | Custom-styled attached scrollbar; the Vue one exists only because webview scrollbars looked native — free win. |
| `GlobalModal.vue` + `stores/modal.ts` | `AppModal.qml` singleton + `Dialog` | QQC2 | Imperative `modal.show(...)` API becomes invokable on a QML singleton. |
| `ToastContainer.vue` + `useToast` | `Toast.qml` in `Overlay.overlay` | custom | Timed stack, same animation curve. |
| `ClipCard.vue` | `ClipCard.qml` | custom (PoC) | GridView delegate; async thumbnail via image provider (§2.2). |
| `ClipListRow.vue` | `ClipListRow.qml` | custom | ListView delegate, same model. |
| `clips/ClipsToolbar.vue` | `ClipsToolbar.qml` | custom | Search field, view toggle, sort. |
| `clips/ClipsStatsBar.vue` | `ClipsStatsBar.qml` | custom | Counts/size from model roles. |
| `clips/ClipsContextMenu.vue` | `Menu` | QQC2 | Right-click via `TapHandler`; QQC2 `Menu` renders as popup — verify styling parity on Wayland. |
| `clips/SteamAccessBanner.vue` | `SteamAccessBanner.qml` | custom | Static banner + action button. |
| `GameFilterDropdown.vue`, `GameTagDropdown.vue` | custom `Popup` + `ListView` | custom | Multi-select + tag editing; not a plain ComboBox. |
| `IconPicker.vue` | `IconPicker.qml` (`Popup` + `GridView`) | custom | |
| `DropZone.vue` | `DropArea` | QML built-in | Native drag-drop; also used for mixer app-routing drags → `Drag`/`DropArea` pair. |
| `CustomVideoPlayer.vue` | `VideoPlayer.qml` | custom (PoC) | `MediaPlayer` + `VideoOutput` + `PlayerController`. |
| `ClipEditor.vue`, `AdvancedEditor.vue`, `TimelineTrackRow.vue` | `EditorPage.qml`, `TimelineTrack.qml` | custom (PoC) | See S6/S7. |
| `ChannelStrip.vue` | `ChannelStrip.qml` | custom | Slider + VU bar + mute + device select. |
| `ChatMix.vue` | `ChatMix.qml` | custom | Two-sided balance slider. |
| `DspControls.vue` | `DspControls.qml` | custom | Toggle + parameter sliders per effect. |
| `GraphicEQ.vue` | `GraphicEQ.qml` (`Canvas` or `Shape`) | custom | Band curve drawing; `Canvas` is **not** auto-mirrored in RTL (see §3.3 rule L4). |
| `DeviceCard.vue` | `DeviceCard.qml` | custom | Device image from `assets/deviceAssets.ts` mapping → QML asset map. |
| `HeadsetSettings.vue`, `MouseSettings.vue`, `MouseMacros.vue` | same-named `.qml` | custom | Forms of QQC2 controls. |
| `RecordingDropdown.vue` | `RecordingMenu.qml` | custom | |
| `ClipNotification.vue` | `NotificationWindow.qml` | custom | Separate window, S11. |
| `PageHeader.vue`, `InfoIcon.vue` | `PageHeader.qml`, `InfoIcon.qml` | custom | Trivial. |
| `settings/*.vue` (12 panels) | `settings/*.qml` (12 files) | custom | 1:1 file mapping; Extensions panel reduced per W6 decision. |
| `tour/GuidedTour.vue` | `GuidedTour.qml` | custom | W5. |
| Composables: `useToast`, `useViewMode`, `useActiveMenu`, `useNavSignal`, `useTour`, `useAppVersion`, `useDependencyStatus` | QML singletons / controller properties | — | Pure UI state → QML singleton `UiState.qml`; anything touching data (e.g. `useClipThumbnail`, `useThumbnailQueue`) → Rust side (§2.2). |

### 1.3 Web-idiom patterns needing QML-native equivalents

| # | Pattern (where) | Web mechanism | QML-native replacement |
|---|---|---|---|
| W1 | Media serving (`media_server.rs`, `utils/assets.ts`, `provide('mediaPort'/'mediaToken')`) | Token-guarded local HTTP server because the webview can't read arbitrary files | **Deleted entirely.** Qt reads `file://` paths directly. `media_server.rs` does not move to `opengg-core`. |
| W2 | Editor audio sync (`AdvancedEditor.vue`) | Hidden HTML `<audio>` elements per track, rAF loop syncing to `<video>` | PoC GStreamer mixer pipeline (single clock, sample-accurate). Approved deviation A1: behavior improves; UI stays identical. |
| W3 | Themes (`utils/theme.ts`) | `theme.json` → CSS variables on `:root`, `.light/.dark` class | `Theme.qml` singleton whose properties are fed by `ThemeController` parsing the **same** `~/.config/opengg/theme.json` keys (`--accent` → `Theme.accent`, `--bg-surface` → `Theme.bgSurface`, `--radius` → `Theme.radius`, …). Unknown keys ignored. Existing user themes keep working. |
| W4 | CSS transitions/animations (hover states, page fades, toast slide) | CSS `transition`/`keyframes` | QML `Behavior on`, `Transition`, `NumberAnimation` — replicate durations/easings from the CSS (audit each during the phase's parity pass). |
| W5 | Guided tour (`GuidedTour.vue` + `tour/steps.ts`) | Spotlights DOM nodes by element refs/selectors, scrolls them into view | Steps reference QML items via `objectName` + a `TourAnchor` attached property; spotlight = full-screen `Overlay` item with a punched hole (`OpacityMask` or four rects). Navigation reuses `currentPage` exactly as `useTour` does. |
| W6 | **Extensions UI** (`extension-template/`, `ExtensionsSettings.vue`) | Extensions ship `dist/index.iife.js` running *inside the webview* with `window.Vue` + `window.opengg`, contributing live Vue settings components | **Fundamentally incompatible — cannot port.** Decision required before Phase 1 (owner: maintainer). Options, in recommended order: (a) declarative manifest-driven settings (extension declares a JSON schema of fields; qt-shell renders an auto-generated QML form; `window.opengg.invoke` calls become the same daemon/extension IPC), (b) QML-file extensions loaded via `Loader` (powerful but a sandboxing/security decision), (c) ship Qt UI without extension *settings panels* first — extensions still run (daemon-side `extensions/` is untouched), only their custom UI is unavailable. Plan assumes (c) for Phase 1 and (a) as the target, revisited at Phase 4 gate. Breaking change must be documented for extension authors. |
| W7 | Context-menu suppression, text-selection lockdown (`App.vue`) | `document.addEventListener('contextmenu', …)` | Not needed — native app has no browser context menu. Delete. |
| W8 | `?overlay=1` second webview window | URL query flag switching `App.vue` into overlay mode | Separate QML `Window` component — no mode switching hack needed. |
| W9 | First-paint transparent-background hack for overlay | Inline style before Vue mounts | `Window { color: "transparent" }` — native. |
| W10 | `useSharedIntersectionObserver` / `useThumbnailQueue` (lazy thumbnail loading) | IntersectionObserver on scroll | `GridView` delegates instantiate lazily by design; thumbnail requests go through an async `QQuickAsyncImageProvider` backed by the existing Rust `generate_thumbnail` path with the existing queue/dedup logic (moved, not rewritten). |

---

## 2. Shared Logic Boundary

### 2.1 Workspace restructure (move, don't rewrite)

Target layout on the migration branch:

```
opengg/
├─ daemon/            openggd — ZERO changes
├─ core/              opengg-core (NEW crate; code MOVED from frontend/src-tauri/src/)
│  ├─ clips/          clips DB (SQLite), meta, probe cache, trim state   ← commands.rs
│  ├─ media/          ffmpeg probe/trim/export/thumbnails, waveforms     ← commands.rs
│  ├─ recording/      recorder status, replay control (daemon client)    ← commands.rs
│  ├─ vu/             PipeWire VU taps                                   ← vu_native.rs
│  ├─ settings/       ui settings, theme.json, locales IO                ← commands.rs
│  ├─ storage/        fs stats, storage info, watch dirs                 ← commands.rs
│  ├─ extensions/     scan, enable-map, registry fetch                   ← commands.rs
│  ├─ steam/          steam games detection                              ← commands.rs
│  └─ subprocess.rs                                                      ← subprocess.rs
├─ qt-shell/          opengg-qt (NEW crate: cxx-qt bridge + QML, presentation only)
│  ├─ build.rs        CxxQtBuilder (pattern from PoC build.rs)
│  ├─ src/            bridge modules only (see 2.2)
│  ├─ cpp/            JsonTranslator shim (§3.1) — the ONLY hand-written C++
│  └─ qml/            all .qml files (§ Appendix A tree)
└─ frontend/          Tauri+Vue — untouched; src-tauri re-exports from opengg-core
```

Extraction rules:
1. Functions move **verbatim**; only `use` paths and the `#[tauri::command]`
   attributes change (core exposes plain `pub fn`/`pub async fn`; the Tauri
   crate keeps thin `#[tauri::command]` wrappers delegating to core).
2. `frontend/src-tauri` must still compile and behave identically after
   extraction — this is the proof that nothing was rewritten. Extraction is
   **Phase 0** and merges to `main` independently (it benefits both stacks).
3. `media_server.rs` stays in `src-tauri` (webview-only concern, W1).
4. Frontend-store logic that is actually business logic must move to core with
   its tests: whatever `stores/persistence.test.ts` and `stores/replay.test.ts`
   cover (settings persistence rules, clip list handling) gets ported to Rust
   unit tests in `core/` during Phase 0. Rule of thumb: if a Pinia store does
   more than cache + forward `invoke` results, that "more" belongs in core.

### 2.2 cxx-qt QObject wrappers (the entire bridge surface)

Each wrapper is *thin*: properties, signals, invokables that delegate to
`opengg-core` or to `openggd` over zbus. No decisions, no state machines, no
parsing in the bridge.

| QObject | Backs | Key properties | Key invokables | Key signals |
|---|---|---|---|---|
| `AppController` | window/app lifecycle | `version: QString`, `dependencyStatus: QString` | `quit()`, `openFileLocation(path)`, `openUrl(url)` | — |
| `AudioController` | daemon audio iface (zbus) + `core::` pactl scans | `channels: QVariantList`, `apps: QVariantList`, `devices: QVariantList`, `virtualAudioReady: bool` | `setVolume(ch, v)`, `setMute(ch, m)`, `setAppVolume(idx, v)`, `routeApp(id, ch, bin)`, `setChannelDevice(ch, dev)` | `channelsChanged`, `appsChanged` |
| `VuMeterController` | `core::vu` | `levels: QList<qreal>` (batched ≤30 Hz, single NOTIFY per batch — risk R7), `earBlast: bool` | `start()`, `stop()` | `earBlastChanged` |
| `DspController` | daemon effects + EQ | `eqBands: QList<qreal>`, per-effect enabled/params | `setEqBand(i, gain)`, `setEffect(name, on, params)` | — |
| `RecorderController` | `core::recording` → daemon replay iface | `status: QString` (`idle/replay/recording`), `replayDuration: int` | `startReplay(secs)`, `stopRecorder()`, `saveReplay()`, `startScreenRecording()`, `stopScreenRecording()` | `clipSaved(path)` (drives S11 overlay) |
| `ClipLibraryModel` | `core::clips` | **`QAbstractListModel`** — roles: path, title, game, tags, duration, size, resolution, createdAt, thumbnailUrl, favorite | `refresh(folder)`, `setMeta(path, …)`, `deleteClip(path)`, `rename(path, name)`, `screenshot(path, t)` | standard model signals |
| `ClipThumbnailProvider` | `core::media` thumbnails | — (`QQuickAsyncImageProvider`, registered as `image://thumbs/`) | — | — |
| `PlayerController` | Qt Multimedia glue (PoC `player.rs`) | `playing`, `position`, `duration`, `muted` | `play()`, `pause()`, `seek(ms)`, `setSource(path)` | `endOfMedia` (new — PoC gap #6) |
| `EditorController` | `core::clips` trim state + `core::media` export | `trimStart/trimEnd: qreal`, `canUndo/canRedo: bool`, `exportProgress: qreal`, `exporting: bool` | `saveTrimState()`, `undo()`, `redo()`, `exportTimeline(opts)`, `cancelExport()` | `exportFinished(ok, path)` |
| `AudioMixerController` | PoC `mixer_pipeline.rs` (GStreamer) | per-track `volume/muted`, `monitorVolume`, `position` | `load(path)`, `play()`, `pause()`, `seek(ms)`, `setTrackVolume(i, v)` | `positionChanged` |
| `DeviceController` | daemon device iface (zbus) | `devices: QVariantList` (battery, model, capabilities) | `setHeadsetSetting(…)`, `setMouseSetting(…)`, `setMacro(…)` | `devicesChanged` |
| `SettingsController` | `core::settings` | mirrored `ui_settings` fields incl. `language`, `rtlMode`, `tutorialSeen` | `save()`, `load()` | per-property NOTIFY |
| `ThemeController` | `core::settings` theme.json | one property per theme token (W3) | `reload()`, `save(json)` | `themeChanged` |
| `I18n` | `core::settings` locales + `JsonTranslator` | `language`, `rtl: bool`, `languages: QVariantList` | `setLanguage(code)`, `openLocalesFolder()` | `languageChanged` |
| `ExtensionsController` | `core::extensions` | `extensions: QVariantList` | `setEnabled(id, on)`, `openFolder()`, `fetchRegistry()` | — |
| `StorageController` | `core::storage` | `storageInfo: QVariantMap` | `refresh()`, `clearThumbnailCache()` | — |

Model-vs-list rule: any collection that can exceed ~50 items or updates
frequently (clips, apps list) is a `QAbstractListModel` (cxx-qt supports
subclassing it); small static lists may be `QVariantList`.

### 2.3 Explicit non-duplication guarantees

- **No logic in C++:** the only `.cpp` file in the repo is
  `qt-shell/cpp/jsontranslator.{h,cpp}` (§3.1). CI check: a script asserting no
  other hand-written C++ sources exist under `qt-shell/`.
- **No logic in QML:** QML may hold view state only (current page, hover, drag
  position before commit, animation state). Anything that survives a restart or
  affects files/audio/daemon goes through a controller. Review checklist item
  on every phase PR.
- **Boundary test:** deleting `qt-shell/` must not remove any capability from
  `opengg-core` or `openggd`. Conversely `qt-shell` must contain no `ffmpeg`,
  `sqlite`, `pipewire`, `gstreamer`, or `zbus` dependency except via
  `opengg-core` (enforceable by inspecting `qt-shell/Cargo.toml` in review —
  this implies the PoC's `mixer_pipeline.rs` and the zbus daemon clients are
  homed in `core/`, with only their QObject faces in `qt-shell`).
- The VU fast path changes *transport only*: Tauri event `vu-levels` → batched
  property NOTIFY. The tap code in `vu_native.rs` moves unmodified.

---

## 3. Internationalization & RTL Architecture

### 3.1 Translation pipeline — keep the JSON catalogs, skip .ts/.qm authoring

The app already has a working key-based catalog system: `en.json` + `ar.json`
(609 keys each, verified in sync), `_meta {name, dir}`, **user-droppable
runtime locales** in `~/.config/opengg/locales/`, and a CI parity checker
(`check-locales.mjs`). Qt Linguist's compiled `.qm` workflow would (a) orphan
609 already-translated key pairs, (b) kill the user-droppable-JSON feature
(users can't run `lrelease`), and (c) split catalog maintenance across two
formats while the Tauri app still ships. So:

**Decision: JSON catalogs remain the single source of truth.** Mechanism:

1. `JsonTranslator` — a small C++ subclass of `QTranslator` (~100 lines,
   `qt-shell/cpp/`) overriding `translate()` to serve strings from the loaded
   JSON map. This is the one sanctioned hand-written C++ component (cxx-qt 0.9
   cannot subclass `QTranslator` directly). Installed via
   `QCoreApplication::installTranslator`.
2. QML uses **ID-based translation**: `qsTrId("nav.home")`, `qsTrId("clips.deleteConfirm")`
   — key names are exactly the existing JSON key paths. No source-English
   strings embedded in QML.
3. Language switching: `I18n.setLanguage("ar")` swaps the map and calls
   `QQmlEngine::retranslate()` — every `qsTrId` binding re-evaluates live
   (same behavior as vue-i18n's reactive `locale`).
4. Interpolation: catalogs use `{error}`, `{count}`, `{n}`, `{version}`, …
   (26 occurrences audited). Keep the placeholder syntax; provide
   `I18n.fmt(qsTrId("ext.installError"), {"error": msg})` (a JS helper in
   `qml/i18n/Fmt.js` — pure string substitution, presentation-side).
5. Plurals: exactly **one** pipe-syntax plural string exists in `en.json`
   today. `JsonTranslator` supports an optional object form per key
   (`{"zero":…,"one":…,"two":…,"few":…,"many":…,"other":…}`, CLDR categories —
   Arabic uses all six) selected via the `n` argument of `translate()`. Convert
   the single piped key to this form; document the form for future strings.
6. User locales: `I18n` scans `~/.config/opengg/locales/*.json` at startup
   (same path, same format — existing community locale files keep working) and
   registers them exactly like `registerLocale()` does today.
7. CI: keep `check-locales.mjs` (it validates JSON, frontend-agnostic) and
   extend it with a QML scan: every `qsTrId("…")` literal in `qml/` must exist
   in `en.json`, and warn on unused keys.

Adding language N+1 later = drop `<code>.json` with `_meta` into
`src/locales/` (bundled) or the user dir (runtime). No compile step.

### 3.2 Layout mirroring architecture

Source of truth stays `settings.rtlMode` (as in `App.vue` today: selecting an
RTL language sets it; the user can force LTR). Exposed as `I18n.rtl`.

- Root window: `LayoutMirroring.enabled: I18n.rtl;
  LayoutMirroring.childrenInherit: true`.
- **Every additional `Window` re-declares both lines** (mirroring does not
  cross window boundaries): `NotificationWindow.qml`, any future detached
  windows. Checklist item in the RTL test list (§7.3).
- **Popups:** QQC2 `Popup`/`Menu`/`ToolTip` content lives in the `Overlay` —
  set `LayoutMirroring` on each custom popup's `contentItem` root, or inherit
  by making popup content children of mirrored items. Convention: all custom
  popups start from a shared `base/AppPopup.qml` that declares mirroring once.
- **Custom components:** mirroring auto-handles anchors, `Row`/`Grid`/`Flow`,
  positioners, and QQC2 controls. It does **not** touch: raw `x:` arithmetic,
  `Canvas` drawing, `Shape` path data, or manually computed drag positions.
  Component rules (enforced in review):
  - L1: horizontal placement via anchors/Layouts only; raw `x:` requires a
    written justification comment and a `mirrored ? … : …` branch.
  - L2: drag math (timeline trim handles, splitter, ChatMix balance) reads
    positions relative to the mirrored container so it works untouched where
    possible; where not (see L4), the component is explicitly exempted.
  - L3: shared primitives (`base/` directory: `AppPopup`, `Icon`, `PathText`,
    …) encapsulate the mirroring-sensitive patterns so feature components
    rarely deal with RTL directly.
  - L4: **exempt-from-mirroring list** (rendered LTR even in RTL, by media
    convention): video timeline/seek bars, editor `TimelineTrack` rows, trim
    handles, waveform displays, `GraphicEQ` frequency axis (low→high stays
    left→right), VU meters. Implemented as `LayoutMirroring.enabled: false` on
    those subtrees. This matches industry convention (media transport is not
    mirrored) — verify against current Vue-app-in-Arabic behavior during the
    Phase 5 parity pass and adjust the list to match it.

### 3.3 Arabic-specific rules

**Numerals.** Current parity baseline is *Western digits everywhere* — the Vue
app hardcodes `en-US` formatting (`utils/format.ts`: `toLocaleDateString('en-US')`,
`fmtDur`, `fmtSize`). Therefore:
- Default: Western (Latin) digits in all locales — matches today's app.
  Implement `qml/i18n/Format.js` porting `fmtDur/fmtSize/fmtRes/fmtDate/fmtTime`
  verbatim (display-only logic, allowed in QML).
- Timecodes, resolutions, file sizes, bitrates: **always** Western digits,
  never localized (technical convention).
- Eastern Arabic-Indic numerals (٠١٢٣…) for counts/dates via `QLocale("ar")`:
  offered later as an opt-in setting — recorded as post-parity enhancement E1,
  not part of any phase's parity gate.
- Dates: today Arabic users see English-formatted dates. Keeping that is the
  parity default; locale-aware dates (`QLocale(I18n.language).toString(date)`)
  is enhancement E2 requiring maintainer sign-off as an approved deviation.

**Mixed-direction text.** Qt's text engine handles bidi runs natively — Arabic
UI labels containing Latin game names render correctly without intervention.
The known failure mode is *neutral-character scrambling* in LTR fragments
embedded in RTL context (file paths, CLI strings, URLs — `/`, `.`, `-` get
reordered). Rule: paths/URLs/commands are always displayed through
`base/PathText.qml`, which wraps the string in Unicode isolates
(`⁦` LRI … `⁩` PDI) and elides from the *start* (`Text.ElideLeft`
keeps the filename visible — matching how the Vue app shows long paths).
Text inputs holding paths set `horizontalAlignment: TextInput.AlignLeft` and
LTR input direction explicitly.

**Icon mirroring.** Central `base/Icon.qml` with `property bool mirrorInRtl:
false` (opt-in flip via `mirror: mirrorInRtl && I18n.rtl` on the underlying
`Image`/`Shape`). Ruleset:

| Flip in RTL | Icons |
|---|---|
| Yes | chevrons (SelectField, dropdown carets, collapse arrows), back/forward navigation arrows, undo/redo, "open in new" arrow, sidebar tip arrow, list-indent affordances |
| No | media transport (play ▶, pause, skip ±10 s, record dot), seek/timeline elements (L4), volume/VU glyphs, all 5 sidebar nav icons (non-directional: home, mixer faders, clapper, headset, settings), close/minimize/maximize, checkmarks, warning/info glyphs, brand/logos |

The Vue app's icons are inline SVG stroke paths — port the exact `d` data, so
flipping is a transform on the shared component, never per-icon redrawing.

---

## 4. Phased Migration Roadmap

Branching: all phases land on `qt6-migration` (worktree). Phase 0's core
extraction is cherry-picked/merged to `main` early (it's stack-neutral and
de-risks drift). From Phase 2 onward, CI produces a parallel `opengg-qt` binary
so testers can run both UIs against the same daemon and data. The Tauri UI is
removed from `main` only after Phase 5's gate passes **and** one release has
shipped both UIs side by side.

Each phase = scope → its own executable implementation plan → build → parity
pass (§7.1) → RTL pass (§7.3) → gate review. A failed gate pauses that phase
only; the rollback criteria (§6.2) govern abandoning the effort as a whole.

### Phase 0 — Foundation (no visible UI product)
**Scope:** workspace restructure per §2.1 (extract `opengg-core`, port the two
store test files to Rust tests); scaffold `qt-shell` from the PoC `build.rs`
pattern; `Theme.qml` + `ThemeController` reading `theme.json`; `I18n` +
`JsonTranslator` + catalog loading (bundled en/ar + user dir); `base/`
primitives (`Icon`, `AppPopup`, `PathText`, `Toast`, `AppModal`); frameless
window shell with `Titlebar.qml` + `Sidebar.qml` (adapted from PoC) and an
empty `StackLayout`; CI job building the qt-shell on a clean Arch container +
the R1 lint (`grep -rE 'pixelSize:\s*[0-9]+\.[0-9]' qml/` must return empty) +
the §5.1 Qt dynamic-linkage `ldd` check; initial `THIRD_PARTY_LICENSES.md`
(§5.3).
**Depends on:** nothing.
**Acceptance test:** app launches on Hyprland native Wayland; window is
frameless, draggable by titlebar, resizable from edges; sidebar shows the 5
nav items with correct icons; switching language en↔ar at runtime live-updates
all visible strings *and* mirrors the layout (sidebar flips to the right,
chevrons flip, titlebar buttons per platform convention) with no restart;
`theme.json` accent-color change reflected on restart; `frontend` (Tauri) still
builds and passes its existing tests against the extracted core; core Rust
tests (ported from the two `.test.ts` files) pass; the §5.1 linkage check
passes on the built binary; `THIRD_PARTY_LICENSES.md` exists and has passed
its one-time §5.3 review.

### Phase 1 — Settings + Home dashboard
**Scope:** `SettingsPage.qml` + all 12 panels (Extensions panel in reduced W6-(c)
form: list, enable/disable, open folder — no custom extension UI);
`SettingsController`, `StorageController`, `ExtensionsController`, `I18n`
language panel; first-launch language picker + onboarding modal; `HomePage.qml`
with `AudioController`-backed quick volume + device pickers; toasts + global
modal wired.
**Depends on:** Phase 0.
**Acceptance test:** parity checklist for S2 + S9 passes against the Vue app
side by side (§7.1 protocol); settings written by the Qt UI are read correctly
by the Tauri UI and vice versa (shared `ui_settings` file — the two UIs must
stay data-compatible until switchover); language + RTL settings persist across
restart; storage stats match Tauri's numbers on the same folder.

### Phase 2 — Clip library + player
**Scope:** `ClipsPage.qml` complete (grid/list, toolbar, stats, filters, tags,
context menu, Steam banner, drop-import); `ClipLibraryModel` +
`ClipThumbnailProvider`; `VideoPlayer.qml` with `PlayerController` incl. new
`endOfMedia` handling; delete/rename/screenshot/favorite actions; watch-dir
rescan.
**Depends on:** Phase 0 (Phase 1 not required — can run in parallel with it).
**Acceptance test:** library with ≥500 real clips scrolls at 60 fps with lazy
thumbnails (measure with `QSG_RENDER_TIMING=1`); all S4 parity checklist items
pass; playback acceptance = PoC criteria re-run on the real app (smooth 1080p60,
hardware decode confirmed via `/proc/<pid>/maps`, no tearing) **plus** clip
end resets to play state; delete/rename reflected in the Tauri UI reading the
same DB.

### Phase 3 — Audio mixer page
**Scope:** `MixerPage.qml`: `ChannelStrip` per channel with live VU
(`VuMeterController` batched path), app routing via drag (`Drag`/`DropArea`),
`GraphicEQ`, `DspControls`, `ChatMix`, ear-blast indicator; `DspController`.
**Depends on:** Phase 0 (independent of Phases 1–2 beyond shared primitives).
**Acceptance test:** S3 parity checklist; VU meters update smoothly with
qt-shell process CPU overhead ≤ the Tauri app's on the same stream count
(measured, R7); volume/mute/routing changes verified audible and visible in
`pactl`/Helvum; EQ curve matches the Vue rendering visually; all controls
verified in ar/RTL including the L4 exemptions (EQ axis stays LTR).

### Phase 4 — Recording controls + notification overlay + devices
**Scope:** `RecordingMenu.qml` + `RecorderController` (replay buffer, screen
recording, save); `NotificationWindow.qml` for clip-saved overlay (LayerShellQt
if available, Hyprland windowrule fallback documented); `DevicesPage.qml` +
`DeviceController`; guided tour (`GuidedTour.qml`) now that all its target
pages exist; W6 decision checkpoint: confirm or revise the extensions approach.
**Depends on:** Phases 1–3 (tour spans all pages; overlay triggers on save).
**Acceptance test:** S8 + S10 + S11 + S12 parity checklists; save-replay
produces a clip that appears in the library and fires the overlay at the
configured screen position on Hyprland; overlay is click-through-correct and
auto-dismisses; tour walks all five pages in both en and ar (spotlight
positions correct under mirroring); headset battery/mouse settings round-trip
to hardware.

### Phase 5 — Editor + timeline (highest risk — deliberately last)
**Scope:** `EditorPage.qml` trim mode (ClipEditor parity) and multi-track mode
(AdvancedEditor parity): `TimelineTrack` rows, trim handles, per-track
volume/mute, monitor volume, undo/redo, splitter, waveforms
(`generate_waveform` → rendered strip), export with progress + cancel;
`EditorController` + `AudioMixerController` (PoC pipeline productionized:
export branch writes real files, A/V drift handling on seek/pause).
**Depends on:** Phase 2 (player, library). Failure here blocks nothing
earlier — Phases 0–4 constitute a shippable app with "basic trim only"
(fallback: expose trim mode only, keep advanced editing in the Tauri app until
resolved).
**Acceptance test:** S6 + S7 parity checklists; 3-track custom clip
(game/chat/mic) plays with all tracks audible and per-track volume live;
monitor volume provably absent from exported file (re-run
`verify-export-isolation.sh` methodology on the real export path); A/V drift
after 10 min playback + 20 seek cycles ≤ 40 ms (measured against burned-in
timecode clip); export output byte-compatible in ffprobe stream layout with a
Tauri-app export of the same edit; undo/redo depth ≥ Vue app's; full RTL pass
with timeline L4 exemption verified.

**Switchover gate (after Phase 5):** all six phase gates green + §7.2 device
matrix complete + one dual-UI release shipped → Qt becomes default, Tauri UI
enters removal countdown. Until then every release note documents how to fall
back (`opengg --ui tauri` or separate binary, decided at Phase 2 packaging).

---

## 5. Build & Packaging Impact

### 5.1 Build system

- **Stays Cargo-only.** The PoC proves `cxx-qt-build`'s `CxxQtBuilder` +
  `QmlModule` handles moc/QML-module generation from `build.rs` — **no CMake
  is introduced**. New workspace members: `core/`, `qt-shell/`.
- Build-time requirements (new): Qt 6.11+ dev packages (`qt6-base`,
  `qt6-declarative`, `qt6-multimedia`), `qmake6` on `PATH`, a C++ compiler
  (for cxx-qt glue + `JsonTranslator`). Dropped: Node/Vite/npm and
  `webkit2gtk` for the qt-shell path (Tauri path keeps them until removal).
- Runtime requirements (new): `qt6-multimedia-gstreamer` backend +
  GStreamer 1.28+ core/base/good plugins (+ `nvcodec`/`vaapi` for hw decode),
  `qt6-wayland`. Dropped at switchover: `webkit2gtk-4.1`,
  `libayatana-appindicator` (tray solution to be re-evaluated —
  `QSystemTrayIcon`/StatusNotifier).
- Dev conveniences to carry from the PoC: `.qmlls.ini` for QML LSP,
  `QT_MEDIA_BACKEND=gstreamer` set programmatically in `main.rs` (not left to
  the environment) with `QT_QPA_PLATFORM=xcb` as a documented fallback flag.
- **Licensing constraint — Qt linking mode (hard rule).** Qt6 MUST be linked
  **dynamically** (shared libraries) in every build and package target, never
  statically. OpenGG holds no Qt commercial license; static linking against
  LGPLv3 Qt is not permitted for this project. Per LGPLv3, users must be able
  to relink the application against a modified or replaced Qt — dynamic
  linking to separately-updatable Qt packages (distro `qt6-*` packages for
  deb/rpm/AUR; the `org.kde.Platform` runtime for Flatpak) satisfies this
  requirement, and this plan states that as the compliance mechanism
  explicitly rather than leaving it implied. **Enforcement:** the Phase 0 CI
  pipeline runs a linkage check on the built binary — `ldd
  target/release/opengg-qt` must list `libQt6Core`, `libQt6Gui`, `libQt6Qml`,
  `libQt6Quick`, and `libQt6Multimedia` as dynamic dependencies; a binary with
  no `libQt6*` dynamic entries (the static-linking signature) **fails the
  build**. cxx-qt links dynamically by default — the check exists to catch
  regressions (e.g. a vendored static Qt introduced by a build-environment
  change), and doubles as the documented manual verification step for
  release builds.
- **Licensing constraint — codecs (hard rule).** All codec and hardware-decode
  capability comes from **system-provided** packages
  (`qt6-multimedia-gstreamer`, GStreamer core/base/good; hardware decode via
  the distro's `nvcodec`/VAAPI plugin packages as *optional* dependencies).
  The project never vendors or bundles proprietary or patent-encumbered
  decode libraries (H.264/HEVC) inside any package it builds or distributes.
  Rationale and per-target application: §5.3 and the §5.2 packaging notes.

### 5.2 Per-target packaging

Distribution model: **native deb/rpm, AUR, and Flatpak only** — every target
resolves Qt, GStreamer, and codecs as *dependencies* on separately-updatable
system packages (which is also what makes the §5.1 licensing constraints hold
by construction). See the recorded non-goal below the table.

| Target | Today | After migration | Work items |
|---|---|---|---|
| **AUR** | `packaging/aur/PKGBUILD` (webkit2gtk et al.) | Update `depends`: remove `webkit2gtk-4.1`; add `qt6-declarative qt6-multimedia qt6-multimedia-gstreamer qt6-wayland gst-plugins-good`; hardware-decode plugin packages as `optdepends`; drop npm build steps | Small; do at Phase 2 when the parallel binary first ships. |
| **Flatpak** | `packaging/flatpak/` on a GNOME/webkit base | Switch to `org.kde.Platform` 6.x runtime (ships Qt + GStreamer — the whole problem class disappears); codec provisioning delegated to the runtime and its standard codec extension (`org.freedesktop.Platform.ffmpeg-full`), not to our manifest | Manifest rewrite; PipeWire + D-Bus portal permissions unchanged. |
| **deb/rpm** | Tauri bundler targets | Replace with `cargo-deb`/`cargo-generate-rpm` (or keep distro-native packaging); depends swap as per AUR | The Tauri bundler goes away with the Tauri shell. |
| **Windows / macOS (later)** | n/a | qt-shell + core compile there (cxx-qt is cross-platform); `windeployqt`/`macdeployqt` for bundling | Blocked on business logic, not UI: `openggd` is Linux-only (PipeWire, `procfs`); `daemon/src/platform/stub.rs` marks the seam. Out of scope beyond keeping `qt-shell` free of Linux-only assumptions (no `/proc` paths, path handling via `std::path`). |

**Packaging notes (all targets):**
- Every target must ensure **both** the `wayland` and `xcb` Qt platform
  plugins are present at runtime (deb/rpm/AUR: depend on `qt6-wayland`
  alongside the base Qt packages, whose xcb plugin ships in the base/QPA
  packages; Flatpak: the `org.kde.Platform` runtime ships both). The
  `QT_QPA_PLATFORM=xcb` XWayland fallback (R2) is a supported path and must
  not be broken by packaging.
- GStreamer plugins — including `nvcodec`/VAAPI hardware decode — are declared
  as package dependencies (hardware decode: *optional* dependencies), never
  bundled (§5.1 codec rule). Where hardware-decode plugins are absent on the
  user's system, playback falls back to software decode via the system's
  GStreamer decoders or the `QT_MEDIA_BACKEND=ffmpeg` path (R2/M4); the
  project does not ship its own codec binaries to guarantee hardware
  acceleration.
- The daemon plus its systemd/D-Bus/polkit/udev files remain distro-packaged
  in all cases; UI packages depend on the installed `openggd`.

**Deliberately not a target: AppImage** *(product decision, recorded here so a
future contributor does not silently reverse it as an apparent oversight).*
A self-contained AppImage was evaluated (an earlier revision of this plan
sketched a `linuxdeploy` + Qt-plugin recipe, estimated at ~90–130 MB per
image) and **rejected**, for three reasons: (a) it duplicates
dependency-bundling responsibility that deb/rpm/AUR and the Flatpak runtime
already handle correctly; (b) it adds a permanent maintenance surface of its
own — the linuxdeploy plugin chain, manual GStreamer plugin-path management
inside the image, and per-release size/QA cost; and (c) a self-contained image
must *bundle* Qt and codec libraries, which conflicts with the §5.1 licensing
constraints — bundling H.264/HEVC decode libraries shifts patent-licensing
exposure onto this project, whereas depending on the user's system/distro
codec packages keeps the established lower-risk model used by most Linux
media applications (the same model the current Tauri app follows by depending
on system `webkit2gtk` instead of shipping a browser engine). Reintroducing
AppImage — or any other self-contained bundle format — requires revisiting
this decision *and* the §5.1/§5.3 licensing constraints, not just build work.

### 5.3 Licensing safeguards & third-party license inventory

**Codec / patent exposure — rationale for the §5.1 codec rule.** Bundling
H.264/HEVC decode libraries (software decoders or hardware-decode plugins)
inside a package this project builds and distributes would make OpenGG itself
the distributor of patent-encumbered codec implementations, shifting
patent-licensing exposure from the parties that actually manage it at scale
(distros, the Flathub codec extension) onto this project. Depending on the
user's system packages is the established lower-risk model for Linux media
applications — and is already OpenGG's own model today (the Tauri app depends
on system `webkit2gtk` rather than shipping a browser engine). The migration
keeps that model: codec capability is a package *dependency*, never vendored
content, and missing hardware decode degrades to software decode (§5.2
packaging notes) rather than being "solved" by shipping our own binaries.

**Third-party license inventory.** A `THIRD_PARTY_LICENSES.md` at the repo
root lists every **new runtime dependency introduced by the migration**, with
its license and how it is consumed. Initial required entries:

| Dependency | License | Consumption |
|---|---|---|
| Qt6 modules (Core, Gui, Qml, Quick, QuickControls2, Multimedia, Wayland) | LGPLv3 | dynamic system libraries only (§5.1 hard rule) |
| cxx-qt / cxx-qt-lib / cxx-qt-build 0.9 | MIT OR Apache-2.0 | Rust crates (statically compiled; permissive) |
| gstreamer-rs bindings (gstreamer, gstreamer-audio 0.25) | MIT OR Apache-2.0 | Rust crates binding the LGPL C libraries, which are consumed dynamically |
| GStreamer core + gst-plugins-base + gst-plugins-good | LGPL | dynamic system packages, declared dependencies |
| Hardware-decode plugins (`nvcodec`, VAAPI — distro-packaged from the per-plugin-licensed `gst-plugins-bad` set) | LGPL (verify per plugin) | *optional* system dependencies — never bundled |
| LayerShellQt (only if adopted for R6) | LGPLv3 | dynamic system library; inventory entry is a precondition of adoption |

Rules:
- The inventory is written and reviewed **once before the Phase 0 gate
  closes** (it is a Phase 0 acceptance item), then kept current: any phase PR
  that adds a runtime dependency updates the file in the same PR (review
  checklist item alongside the §2.3 boundary checks).
- **GPL flagging:** any dependency or proposed package dependency carrying a
  GPL license must be a conscious, recorded inclusion decision — a
  `THIRD_PARTY_LICENSES.md` entry with one line of justification — never an
  incidental or transitive addition. This matters concretely for GStreamer:
  the `-bad` and `-ugly` plugin sets are licensed *per plugin* and differ
  from `-good` (some `-ugly` plugins are GPL). Default posture: the project's
  own declared package dependencies stay within core/base/good plus the
  LGPL hardware-decode plugins listed above; anything beyond that is
  user-installed, not a project dependency.

---

## 6. Risk Register

| # | Risk | Evidence | L×I | Mitigation | Fallback |
|---|---|---|---|---|---|
| R1 | Fractional `font.pixelSize` hangs the app (no window, 0% CPU) | Reproduced twice in PoC (13.5, 11.5) on Qt 6.11.1 | High×High (trivially reintroduced) | CI lint from Phase 0 (grep in §4-P0); integer sizes rule in component guidelines; report upstream to Qt with the PoC reproducer | Lint makes recurrence build-breaking; if broader Qt 6.11.x text bugs emerge, pin to a known-good Qt patch release |
| R2 | Wayland-native playback only validated on one machine (NVIDIA/Hyprland) | PoC test env section | Med×High | §7.2 matrix (AMD, Intel, KDE/GNOME Wayland, X11) during Phase 2, *before* deeper phases build on playback | Auto-fallback: detect first-frame failure → relaunch/advise `QT_QPA_PLATFORM=xcb` (XWayland); secondary: `QT_MEDIA_BACKEND=ffmpeg` path kept working for single-track clips |
| R3 | A/V drift between Qt Multimedia video clock and the separate GStreamer audio pipeline on long clips/seeks | Architectural (two clocks by design, PoC only tested short clips) | Med×High | Drift measured in Phase 5 acceptance (≤40 ms after 10 min + 20 seeks); resync audio pipeline to video position on every seek/pause/play boundary | If unfixable: move *editor* video to the GStreamer pipeline too (single clock) using qml6glsink-style `VideoOutput` sink integration — editor-only, library player unaffected |
| R4 | cxx-qt 0.9 immaturity: version coupling to Qt, missing base-class support, breaking releases | `QTranslator` subclass already impossible in pure Rust; 0.x semver | Med×Med | Pin cxx-qt exactly; isolate all bridge code in `qt-shell/src/` so churn is localized; CI on clean container catches env drift | Bridge layer is thin by design (§2.3) — worst case, regenerating it against a changed cxx-qt API is mechanical |
| R5 | **Extension UI incompatibility** (Vue components in webview cannot run in QML) | `extension-template/src/index.ts` contract | Certain×Med | W6 decision before Phase 1; ship reduced panel (c) first; design manifest-schema UI (a) by Phase 4 checkpoint; announce breaking change to extension authors with migration guide | Extensions keep *functioning* (daemon-side) regardless; only custom settings UIs degrade to "configure via file" |
| R6 | Clip-saved overlay cannot be globally positioned on Wayland | Wayland protocol restriction (xdg-shell has no global positioning) | High×Low | LayerShellQt (optional dep) for layer-surface anchoring on wlr-based compositors incl. Hyprland; document Hyprland `windowrule` alternative | Plain top-level window centered by compositor; feature-flag position setting off on Wayland-without-layer-shell |
| R7 | VU/property-update storms through cxx-qt make the mixer page more expensive than the webview version | 30 Hz × N channels × QML binding re-eval | Med×Med | Batch to one `levels` list property + single NOTIFY ≤30 Hz (§2.2); Phase 3 acceptance includes CPU comparison vs Tauri app | Reduce to 20 Hz; render VU bars in a single `Canvas`/`QQuickPaintedItem` fed imperatively instead of per-channel bindings |
| R8 | RTL regressions from manual x-math in custom drag components (timeline, splitter, ChatMix) | Known QML LayoutMirroring blind spots | High×Med | Rules L1–L4 (§3.2); shared `base/` primitives; ar smoke test in *every* phase gate, not just at the end (§7.3) | Per-component `mirrored ? …` branches; L4 exemption list keeps the hardest components LTR by convention anyway |
| R9 | User theme.json files render differently (CSS variables → QML property mapping mismatches) | W3 mapping is name-based | Med×Low | Mapping table written in Phase 0 with the shipped default theme as fixture; unknown keys logged+ignored; screenshot comparison with a maximally-customized theme in §7.1 | Ship a theme-migration note; worst case users adjust a few keys |
| R10 | Automated UI testing limited: held-drag gestures undeliverable via ydotool on native Wayland; xdotool can't target the window | PoC Task 4 differential test | Certain×Med | Test strategy designed around it (§7): in-process `qmltest` for gesture logic (no compositor input needed), manual checklist for real-input drags | Run drag E2E under `QT_QPA_PLATFORM=xcb` in CI (XWayland accepts xdotool), accepting it tests app logic, not the Wayland input path |
| R11 | Dual-UI period: settings/DB written by one UI corrupt or confuse the other | Shared `ui_settings`, clips.db, theme.json during Phases 1–5 | Med×High | Compatibility is an explicit acceptance item in Phases 1 and 2 (cross-read tests); schema changes forbidden on the migration branch (they'd be logic changes anyway) | If a conflict ships: qt-shell reads/writes a namespaced copy until switchover (worst case, settings set in one UI don't appear in the other — degraded, not corrupt) |
| R12 | Undocumented licensing exposure from bundled codec/runtime libraries (LGPLv3 Qt ending up statically linked, or patent-encumbered H.264/HEVC decoders shipped inside a project-built package) | Qt is LGPLv3; hardware-decode plugins come from the per-plugin-licensed `gst-plugins-bad` set; the migration introduces many new runtime dependencies at once | Low×High | §5.1 hard rules (dynamic Qt linking enforced by the CI `ldd` linkage check; codecs as system dependencies, never vendored) + §5.3 `THIRD_PARTY_LICENSES.md` inventory reviewed at the Phase 0 gate and updated in any PR adding a runtime dependency, with GPL additions requiring an explicit recorded decision | Remove or replace the offending dependency before release (drop it from package depends and rely on user-installed packages; for a Qt-linkage violation, the release is blocked until the build is dynamic again) |

### 6.2 Rollback / pause criteria

Pause or abandon the Qt migration and stay on Tauri/Vue if **any** of:

1. **Playback fails the matrix:** Phase 2's §7.2 matrix shows tearing, stutter,
   or crashes on ≥2 of the 4 non-reference environments that neither the
   XWayland fallback nor the FFmpeg-backend fallback resolves within one
   milestone of effort.
2. **The two-clock editor architecture fails:** Phase 5 drift acceptance
   (≤40 ms) *and* the R3 fallback (all-GStreamer editor path) both fail — the
   flagship editor feature would regress versus the current app.
3. **cxx-qt becomes a dead end:** an unavoidable Qt or cxx-qt upgrade breaks
   the bridge with no fixed release within a milestone, twice.
4. **Parity cost explosion:** any single phase overruns its estimate by >2× on
   parity/RTL rework specifically (signal that QML-vs-CSS fidelity is more
   expensive than assessed across the board — re-evaluate before the next phase).

Because everything lives on `qt6-migration` and Phase 0's core extraction is
independently valuable on `main`, abandoning costs: the qt-shell crate and QML
tree only. Nothing in daemon, core, or the Tauri app will have changed
semantically. A pause decision is recorded in
`docs/superpowers/plans/` with the failing evidence, so the effort can resume
when the blocking factor changes (e.g., a Qt release fixing R1/R2 class bugs).

---

## 7. Testing & Acceptance Strategy

### 7.1 Visual/behavioral parity verification (per phase)

Screenshot pixel-diffing across the two stacks is **not** used as a gate —
different text rasterization (webkit vs Qt) guarantees noise. Instead:

1. **Per-screen state checklist.** For each screen in the phase, a checklist
   file (`docs/superpowers/parity/<screen>.md`) enumerating every state:
   empty / loading / populated / error / hover / pressed / disabled / with
   modal / with menu open / during drag. Written by walking the Vue component's
   template branches (`v-if`/`v-show`) so states are enumerated from code, not
   memory.
2. **Side-by-side capture.** Both apps at exactly 1280×800 on the same data
   set (a fixture clips folder + fixture settings), same theme. Capture with
   `grim -g "$(hyprctl activewindow …)"` per state; store pairs under
   `docs/superpowers/parity/shots/<screen>/{vue,qt}-<state>.png`. Reviewed by a
   human against the checklist; layout deltas either fixed or added to the
   phase's **approved deviations** list with a one-line rationale (seed
   entries: A1 = editor audio engine swap W2; A2 = i18n mechanism — a cxx-qt
   `I18n` singleton with `I18n.t(key)` bindings instead of the §3.1
   C++ `QTranslator` subclass + `qsTrId` (cxx-qt 0.9 cannot subclass
   QTranslator; UX and JSON-catalog source of truth are unchanged); E1/E2
   numeral & date enhancements deliberately deferred).
3. **Behavioral parity:** each checklist row includes the interaction outcome
   ("clicking Save Replay → toast + overlay + clip appears in library"),
   verified manually on both stacks in the same session.
4. **Data compatibility:** Phases 1–2 include cross-stack read/write checks
   (R11) as explicit checklist rows.

### 7.2 Environment matrix (Phase 2 gate, re-run at switchover)

| Env | GPU | Session | Priority |
|---|---|---|---|
| Reference | NVIDIA RTX 3070 | Hyprland/Wayland | validated in PoC |
| M1 | AMD (RADV) | KDE Plasma/Wayland | must pass |
| M2 | Intel iGPU | GNOME/Wayland | must pass |
| M3 | NVIDIA | X11 (or forced `QT_QPA_PLATFORM=xcb`) | must pass (this *is* the fallback path — R2) |
| M4 | any | Hyprland + `QT_MEDIA_BACKEND=ffmpeg` | should pass (single-track clips only) |

Checks per env: 1080p60 playback smoothness, hw-decode plugin actually loaded
(`/proc/<pid>/maps`, per PoC methodology), no tearing, overlay window behavior,
frameless move/resize.

### 7.3 RTL test suite (separate from LTR — explicitly verified, never assumed)

Automated (in-process `qmltest`, immune to the R10 input limitation):
- T1: `I18n.setLanguage("ar")` → `rtl === true`, `qsTrId("nav.home")` returns
  the Arabic string from `ar.json`, `retranslate()` fires (binding probe).
- T2: catalog integrity — every `qsTrId` id in `qml/` exists in both `en.json`
  and `ar.json` (extended `check-locales.mjs`, runs in CI).
- T3: `PathText` renders `مقطع` + `/home/user/Videos/clip.mp4` without
  neutral-character reordering (compare glyph run directions) and elides left.
- T4: plural key returns the correct Arabic CLDR category for
  n ∈ {0, 1, 2, 3, 11, 100}.
- T5: components under `base/` report mirrored anchors when
  `LayoutMirroring.enabled` (position assertions on a probe layout).
- T6: L4-exempt components (`TimelineTrack`, `GraphicEQ`, seek bar) report
  **unmirrored** geometry under RTL.

Manual, per phase gate, app running in Arabic:
- M-RTL1: sidebar on the right; nav order visually reversed; active-item
  indicator on the correct edge.
- M-RTL2: every popup/menu/tooltip in the phase opens on the mirrored side and
  is itself mirrored (AppPopup inheritance check).
- M-RTL3: `NotificationWindow` mirrored (windows don't inherit — §3.2).
- M-RTL4: icon flip audit against the §3.3 table for every icon visible in the
  phase (chevrons flipped; play button, timelines, VU meters not).
- M-RTL5: mixed-direction spot checks: Latin game name inside Arabic clip
  title; long path in StorageSettings; Arabic search query over Latin-named
  clips.
- M-RTL6: numerals per §3.3 policy (Western everywhere; timecodes LTR).
- M-RTL7: drag interactions in the phase (trim handles, splitter, ChatMix,
  volume sliders) move in the visually-correct direction.

### 7.4 Non-UI regression safety

- `opengg-core` carries the ported Rust tests from Phase 0 plus the existing
  daemon tests; `cargo test --workspace` in CI on every migration-branch PR.
- The Tauri app's build + `vitest` suite stays in CI until switchover — it is
  the living proof that core extraction changed nothing (§2.1 rule 2).
- Export correctness (Phase 5): ffprobe stream-layout comparison and the
  monitor-isolation check are scripted (adapt PoC `verify-export-isolation.sh`)
  and run in CI on a fixture clip.

---

## Appendix A — Proposed `qt-shell` source tree

```
qt-shell/
├─ Cargo.toml            # deps: cxx-qt 0.9, cxx-qt-lib, opengg-core ONLY; no ffmpeg/sqlite/zbus/gstreamer directly (daemon clients and the mixer pipeline live in core)
├─ build.rs              # CxxQtBuilder::new_qml_module("com.opengg.app") — PoC pattern
├─ cpp/
│  └─ jsontranslator.{h,cpp}   # §3.1 — the only hand-written C++
├─ src/
│  ├─ main.rs            # engine setup, backend env, translator install, controller registration
│  └─ bridge/            # one file per QObject in §2.2, thin delegation only
└─ qml/
   ├─ Main.qml
   ├─ base/              # Icon, AppPopup, PathText, Toast, AppModal, PageHeader, InfoIcon
   ├─ i18n/              # Fmt.js, Format.js
   ├─ Theme.qml          # singleton fed by ThemeController
   ├─ Titlebar.qml  Sidebar.qml  GuidedTour.qml  NotificationWindow.qml
   ├─ pages/             # HomePage, MixerPage, ClipsPage, DevicesPage, SettingsPage, EditorPage
   ├─ settings/          # 12 panel files (§1.1 S9)
   ├─ clips/             # ClipCard, ClipListRow, ClipsToolbar, ClipsStatsBar, SteamAccessBanner, dropdowns
   ├─ mixer/             # ChannelStrip, GraphicEQ, DspControls, ChatMix
   ├─ player/            # VideoPlayer + controls
   ├─ editor/            # TimelineTrack, trim handles, track headers
   └─ devices/           # DeviceCard, HeadsetSettings, MouseSettings, MouseMacros, IconPicker
```

## Appendix B — Open decisions requiring maintainer input

| ID | Decision | Needed by | Options (recommended first) |
|---|---|---|---|
| D1 | Extension UI model (W6/R5) | Phase 1 start (interim), Phase 4 gate (final) | (c) reduced panel now → (a) manifest-schema forms; (b) QML extensions only with a sandboxing story |
| D2 | Dual-UI distribution form | Phase 2 packaging | separate `opengg-qt` binary (recommended: zero risk to main package) vs `--ui` flag in one package |
| D3 | Tray icon replacement for `libayatana-appindicator` | switchover | Qt StatusNotifier via `QSystemTrayIcon` (recommended) vs drop tray |
| D4 | E1/E2 (Arabic-Indic numerals, localized dates) | post-parity | opt-in setting (recommended) vs locale-automatic |
```

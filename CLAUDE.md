# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is OpenGG

OpenGG is a Linux gaming hub providing unified audio mixing, instant replay, and device/RGB management. It is a modular alternative to SteelSeries GG (Sonar + Engine + Moments), built with:
- **Daemon** (`daemon/`) — Rust background process, exposes D-Bus service `org.opengg.Daemon`
- **Frontend** (`frontend/`) — Tauri 2 app (Vue 3 + TypeScript), contains both the Vue UI and the Rust Tauri backend (`src-tauri/`)

---

## Build & Dev Commands

```bash
# First-time setup (npm deps, data dirs)
./dev.sh setup

# Full-stack dev (daemon + Tauri frontend with live reload)
./dev.sh

# Daemon only
./dev.sh daemon

# Frontend (Tauri + Vite) only
./dev.sh ui

# Release build
./dev.sh build
```

`make dev / daemon / ui / build / clean / install / check / lint` are aliases for the above.

```bash
# Individual commands
cd daemon && cargo build [--release]
cd daemon && cargo clippy

cd frontend && npm install
cd frontend && npx tauri dev          # full Tauri dev mode
cd frontend && npx tauri build        # release bundle (deb, appimage, rpm)
cd frontend && npm run build          # Vue/Vite build only
cd frontend && npx vue-tsc --noEmit   # TypeScript check
```

There is no automated test suite. Testing is manual via `./dev.sh` + browser devtools.

---

## Architecture

```
Vue 3 UI (src/pages, src/components, src/stores)
    ↓ invoke('command_name', args)
Tauri Rust backend (src-tauri/src/commands.rs — ~2500 lines, 60+ #[command] fns)
    ↓ D-Bus call to org.opengg.Daemon (fallback: pactl/shell)
openggd daemon (daemon/src/) — audio routing, device mgmt, replay
    ↓
Linux subsystems: PipeWire, pactl, FFmpeg, gpu-screen-recorder, ratbagd, OpenRGB
```

### Tauri Command Pattern

All frontend→Rust calls go through `invoke()`. Commands are registered in `frontend/src-tauri/src/main.rs` via `tauri::generate_handler![...]`. Every command is defined in `commands.rs`:

```rust
#[command]
pub async fn set_volume(channel: String, volume: u32) -> Result<(), String> {
    // 1. Try D-Bus daemon
    if call_dbus_void("SetVolume", AU_PATH, AU_IFACE, (channel.as_str(), volume)).await.is_ok() {
        return Ok(());
    }
    // 2. Fall back to pactl
    run_cmd("pactl", &["set-sink-volume", "@DEFAULT_SINK@", &format!("{volume}%")])?;
    Ok(())
}
```

D-Bus interfaces: `AU_PATH/AU_IFACE` = `/org/opengg/Daemon/Audio` / `org.opengg.Daemon.Audio`, `RP_PATH/RP_IFACE` = `.../Replay`.

### Pinia Stores

| Store | Owns |
|-------|------|
| `audio.ts` | Channel volumes/mutes, app routing, VU meters, device selection |
| `replay.ts` | Clip array (`shallowRef`), filtering, sorting, selection, skeleton injection |
| `persistence.ts` | `~/.config/opengg/ui-settings.json` — load/save with 500ms debounce |

`replay.ts` uses `shallowRef<Clip[]>` + `triggerRef()` + `markRaw` on individual clips — do not change to deep `ref` as it causes severe performance issues with large clip lists.

### Media Server

A Warp HTTP server starts at an auto-assigned port on app launch (`src-tauri/src/media_server.rs`):
- `GET /media/...` — video files with HTTP Range support
- `GET /audio?file=X&stream=N` — FFmpeg-extracted audio stream as WAV

Frontend gets the port via `invoke('get_media_server_port')`.

### Clip Grid & Scroll Architecture

`ClipCard.vue` renders in a **native CSS Grid** inside `ClipsPage.vue` — there is no virtual scroll. Cards use CSS containment (`contain: style; will-change: transform`) and `content-visibility: auto` with `contain-intrinsic-size` so the browser can skip off-screen paint.

Scroll structure (per view: grid / list / grouped):
```
.scroll-area           flex column, overflow:hidden
  .scroll-host         flex column, position:relative (anchor for scrollbar)
    .native-grid-host  flex:1, overflow-y:scroll, scrollbar-width:none, -webkit-overflow-scrolling:touch
      .native-grid     CSS grid of <ClipCard> elements
    <OverlayScrollbar> position:absolute over .native-grid-host (sibling, NOT child of scrollable content)
```

`<OverlayScrollbar>` receives the `.native-grid-host` element via ref. It auto-hides after 1.5s idle, expands on hover, and supports drag. Do NOT place it inside `.native-grid-host` or it scrolls away with the content.

Thumbnail lazy-load uses a shared `IntersectionObserver` (`rootMargin: '300px'`). After a thumbnail loads, the card unobserves itself (`observeOnce` pattern) to stop triggering callbacks. Max 3 concurrent `generate_thumbnail` IPC calls are queued in `useThumbnailQueue.ts`.

### Audio Routing Complexity

App IDs may be PipeWire node IDs **or** PulseAudio sink-input indices — they are not the same. `route_app()` in `commands.rs` tries the received ID first, then cross-references PipeWire properties to find the correct pactl index. This dual-index logic is intentional and must be preserved.

### Anti-Snap Debounce

`audio.ts` tracks `recentlyChanged` per channel (3-second grace window). Polling updates skip channels that were recently changed by the user to prevent the slider snapping back mid-drag.

### Skeleton Pattern

When the filesystem watcher detects a new clip file, a skeleton placeholder is injected into `replay.ts` immediately. Once FFmpeg probing finishes, `replaceSkeleton()` swaps it for the real clip — avoids grid flicker.

### Mic Volume Lock

`main.rs` spawns a background thread that polls every 1.5s and snaps the mic volume back if it drifted from the locked value. Controlled via `LOCKED_MIC_VOLUME` / `MIC_LOCK_ENABLED` atomics in `commands.rs`.

---

## File Locations at Runtime

```
~/.config/opengg/
  ui-settings.json        # all frontend settings
  locales/                # user-dropped translation JSON files

~/.local/share/opengg/
  thumbnails/             # JPEG clip thumbnails (640px wide)
  waveforms/              # PNG waveform previews
  opengg.db               # SQLite: clip_meta, trim_state tables
  opengg_crash.log        # Tauri + Rust panic logs

~/Videos/OpenGG/          # default clips folder
```

### SQLite Schema

```sql
clip_meta  (filepath PK, game, custom_name, favorite,
            duration REAL, width INT, height INT, mtime REAL)
trim_state (filepath PK, trim_start REAL, trim_end REAL)
```

`duration/width/height/mtime` are the ffprobe cache — if present and mtime matches, ffprobe is skipped on load.

---

## Adding a New Tauri Command

1. Define in `frontend/src-tauri/src/commands.rs`:
```rust
#[command]
pub async fn my_command(arg: String) -> Result<String, String> {
    // try D-Bus first, fall back to shell if applicable
    Ok(result)
}
```

2. Register in `frontend/src-tauri/src/main.rs` inside `tauri::generate_handler![...]`:
```rust
commands::my_command,
```

3. Call from Vue:
```typescript
const result = await invoke<string>('my_command', { arg: 'value' })
```

All command errors must be `String`. Complex return types should be serialized as JSON strings and parsed on the Vue side.

---

## Clip Filtering & Sorting

In `replay.ts`:
- **`filteredClips`** — full pipeline: skeletons float to top, then search (name/filename/game), then favorites filter, then multi-game filter, then sort by `sortMode` (newest/oldest/longest/shortest)
- **`sortedClips`** — sort only, no filtering; skeletons first
- **`filteredRealClips`** (ClipsPage.vue) — `filteredClips` minus skeletons; fed directly to the native CSS grid `v-for`

---

## Thumbnail URL Pattern

Thumbnails are served through the local media server. `ClipCard.vue` lazy-loads via the shared `IntersectionObserver` composable — when a card enters the viewport, it calls `invoke('generate_thumbnail', { filepath })`, then constructs the URL:

```typescript
// utils/assets.ts
`http://localhost:${port}/media${absolutePath}`
// e.g. http://localhost:33955/media/home/user/.local/share/opengg/thumbnails/abc123.jpg
```

The media server (`media_server.rs`) serves all paths under `/media` from the filesystem root with HTTP Range support.

---

## Context Menu

A single context menu lives in `ClipsPage.vue` (not in each `ClipCard`). State is in `replay.ts`:
- `activeMenuClipId` — which clip's menu is open
- `activeMenuPos` — `{ x, y }` screen position

`ClipCard` right-click sets these store values. `ClipsPage` renders one `<Teleport to="body">` context menu, looks up the clip by ID, and dispatches actions via `ctxAction(action)`. A single `mousedown` listener on the page closes the menu when clicking outside `.ctx-menu` or `.kebab`.

---

## Global Shortcuts

Shortcuts are stored in `persistence.ts` settings and registered on app start (and re-registered on change) via:

```typescript
await invoke('register_global_shortcuts', {
  saveReplay: 'Alt+F10',
  toggleRecording: 'Alt+F9',
  screenshot: 'Alt+F12',
})
```

The Rust side uses `tauri-plugin-global-shortcut` and emits Tauri events (`global-shortcut-save_replay`, `global-shortcut-toggle_recording`, `global-shortcut-screenshot`) which `App.vue` listens to via `listen()`.

---

## Theme System

Theme is a JSON object with `colors` and `layout` keys containing CSS variable names as keys:

```json
{ "colors": { "--accent": "#E94560", "--bg-surface": "#0f1117" },
  "layout": { "--radius": "6px" } }
```

`utils/theme.ts` applies them via `root.style.setProperty(key, value)`. Persisted to `~/.config/opengg/theme.json` via `invoke('save_theme')` / `invoke('load_theme')`.

---

## i18n

Build-time locales live in `frontend/src/locales/*.json` and are loaded eagerly via `import.meta.glob`. User-contributed locales are dropped into `~/.config/opengg/locales/`, fetched at startup via `invoke('list_user_locales')`, and registered dynamically with `registerLocale(code, data, name, dir)`.

Each locale JSON may include `_meta: { name, dir: 'ltr'|'rtl' }`. Usage in components:
```typescript
const { t } = useI18n()
// {{ t('settings.audio.title') }}
```

---

## Extension System

Extensions live in `~/.local/share/opengg/plugins/<name>/manifest.json`:

```json
{
  "id": "my-ext", "name": "My Extension",
  "version": "1.0.0", "author": "...",
  "hooks": { "sidebar_tab": false, "export_filter": false, "settings_section": false }
}
```

`invoke('scan_extensions')` returns `ExtensionInfo[]`. `invoke('open_extensions_folder')` opens the folder and writes a developer guide on first visit. Hook execution is not yet implemented — hooks are currently metadata only.

---

## Known Gotchas & Past Bugs

### `tauri-plugin-dialog` / xdg-portal
Must use `default-features = false` in `Cargo.toml`:
```toml
tauri-plugin-dialog = { version = "2", default-features = false, features = ["xdg-portal"] }
```
Without it, rfd enables both `gtk3` and `xdg-portal` simultaneously and the build fails.

### Opening files / folders in the file manager
Use the `open` crate (`open::that(path)`) — **not** `Command::new("xdg-open")`. The `open` crate respects `$XDG_CURRENT_DESKTOP` and works correctly on KDE/Dolphin. All four `xdg-open` calls in `commands.rs` have been replaced.

### Tokio semaphore in `get_clips` / `generate_thumbnails_batch`
Use `tokio::spawn(async { sem.acquire().await; tokio::task::spawn_blocking(...) })` — **not** `spawn_blocking` + `try_acquire()`. `try_acquire()` is non-blocking and immediately fails when no permits are available, so it provides no concurrency control.

### Thumbnail scale
Generate at `640:-1` (not `1280:-1`), quality `5` (not `2`). Larger thumbnails cause memory pressure with hundreds of clips.

### `replay.ts` — do not switch `clips` to deep `ref`
`clips` is `shallowRef<Clip[]>` with `triggerRef()` + `markRaw` on each clip object. Switching to deep `ref` causes severe reactivity overhead with large clip lists.

---

## Daemon Activation

The daemon auto-starts via D-Bus activation — no manual `systemctl start` needed. Packaging files:
- `packaging/org.opengg.Daemon.service` — D-Bus service descriptor, maps `org.opengg.Daemon` to the systemd unit
- `packaging/openggd.service` — systemd user service, `Type=dbus`, `After=pipewire.service wireplumber.service`, `Restart=on-failure`

Install daemon binary to `~/.local/bin/openggd` (`make install`). The daemon starts on the first IPC call from the frontend and restarts automatically on failure.

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

```bash
# Full-stack dev (daemon + Tauri frontend, unified logs)
./dev.sh          # or: make dev

# Individual targets
./dev.sh daemon   # Rust daemon only
./dev.sh ui       # Tauri frontend only (Vue + Vite dev server)
./dev.sh build    # Release build of everything
./dev.sh setup    # First-time setup (npm install, create XDG dirs)

# Type-checking and linting (verification steps — no test suite exists)
make check        # cargo check for both crates
make lint         # cargo clippy + npx vue-tsc --noEmit

# Working directory matters for cargo:
cd frontend/src-tauri && cargo check   # Tauri crate
cd daemon && cargo check               # Background daemon crate

# Frontend type-check only:
cd frontend && npx vue-tsc --noEmit
```

## Architecture Overview

This is a **two-process desktop app** for Linux gaming:

```
opengg/
├── daemon/          ← Rust background daemon (openggd) — audio, devices, replay
└── frontend/        ← Tauri 2 + Vue 3 desktop UI
    ├── src/         ← Vue frontend
    └── src-tauri/   ← Tauri Rust crate (NOT the daemon — separate binary)
```

### The Two Rust Crates Are Different Things

- **`daemon/`** (`openggd`): Long-running background process. Manages audio via D-Bus (`org.opengg.Daemon`), intended to start via systemd/D-Bus activation. Uses zbus + tokio. Currently partially implemented.
- **`frontend/src-tauri/`** (`opengg`): The Tauri binary that wraps the Vue UI. Contains `commands.rs` with all IPC handlers the frontend calls via `invoke()`. Also spawns a local warp HTTP server for media serving. This is where most active development happens.

In practice, almost all functionality lives in `frontend/src-tauri/src/commands.rs` (audio routing, clip management, recording, etc.) because the daemon D-Bus integration is still being built out. Commands try D-Bus first (`call_dbus` / `call_dbus_void`) and fall back to direct `pactl`/`pw-link` CLI calls.

### Frontend → Backend IPC

All frontend→Tauri communication is `invoke('command_name', { args })` from Vue components. Every `#[command]` function in `commands.rs` must be registered in `main.rs` inside `tauri::generate_handler![...]`. **This is the most common source of "Command not found" errors** — always register new commands there.

### Media Serving

The browser inside Tauri cannot load local files directly (WebKitGTK `asset://` bugs). Instead, `media_server.rs` spawns a `warp` HTTP server on a random port at startup. The port is fetched once via `invoke('get_media_server_port')` in `App.vue` and `provide()`d to all components as `'mediaPort'`. Components `inject<Ref<number>>('mediaPort', ref(0))` and convert paths with `mediaUrl(absolutePath, port)` from `utils/assets.ts`.

**Never use local file paths directly as `<img>` or `<video>` src in the Tauri WebView — always go through `mediaUrl()`.**

### State Management

Three Pinia stores:
- **`useReplayStore`** (`stores/replay.ts`): Clip list, search/sort/filter state, watcher skeletons. `clips` array is the single source of truth for the clips gallery.
- **`useAudioStore`** (`stores/audio.ts`): Mixer channels (Game/Chat/Media/Aux/Mic/Master), app routing, VU levels from Tauri events.
- **`usePersistenceStore`** (`stores/persistence.ts`): All user settings AND extension feature flags, auto-saved to `~/.config/opengg/ui-settings.json`. Use `persist.state.settings.*` for settings and `persist.state.extensions.*` for feature flags.

### Key Data Flows

**Clip gallery**: `commands::get_clips` scans video files from `clipsFolder` + `clipSources[]` (stored in settings JSON), reads SQLite metadata from `~/.local/share/opengg/clips.db`, returns `Vec<ClipInfo>`. The live file-watcher (`notify` crate in `main.rs`) emits `clip_added`/`clip_removed` Tauri events — the frontend in `ClipsPage.vue` handles these by injecting skeleton cards then calling `get_clip_by_path` to fill them.

**Audio routing**: Virtual PipeWire sinks named `OpenGG_Game`, `OpenGG_Chat`, etc. are created via `pactl load-module module-null-sink`. App routing uses `pactl move-sink-input <index> <sink>`. Physical device routing uses `pw-link`. All args must be PulseAudio integer indices — **using node names silently fails**.

**Virtual audio onboarding**: `check_virtual_audio_status` → if false, `MixerPage.vue` hides mixer columns and shows a "Create Virtual Audio Engine" button that calls `create_virtual_audio`. Settings has a "Remove Virtual Audio & Restore OS Defaults" button that calls `remove_virtual_audio`. After creation, `hydrate_audio_routing` re-links all previously routed apps.

**Thumbnail generation**: `generate_thumbnail` command runs `ffmpeg` to extract a frame, stores it in `~/.local/share/opengg/thumbnails/`. `ClipCard.vue` uses an IntersectionObserver to trigger lazy generation as cards scroll into view.

**Multi-track audio in editor**: `AdvancedEditor.vue` detects clips with multiple audio streams (`analyze_media` returns `audio_streams` count). For single-track clips the `<video>` handles audio. For multi-track, the video is muted and separate `<audio>` elements are created per track using the media server's `/audio?file=X&stream=N` endpoint (served via ffmpeg pipe in `media_server.rs`), synced to video `currentTime` via RAF.

**Screen recording**: Two paths exist. Legacy: `start_screen_recording` / `stop_screen_recording` using `gpu-screen-recorder` directly. GSR replay (preferred): `start_gsr_replay` keeps a rolling buffer; `save_gsr_replay` flushes it to a file; `stop_gsr_replay` kills the process.

**Mic Volume Lock**: `LOCKED_MIC_VOLUME` (AtomicU32) and `MIC_LOCK_ENABLED` (AtomicBool) are module-level statics in `commands.rs`. When `set_volume("Mic", v)` is called it also updates `LOCKED_MIC_VOLUME`. `set_mic_volume_lock(enabled)` toggles the enforcement. A background thread in `main.rs` polls the OS mic volume and re-applies the locked value if it diverges by more than 1%.

**VU Meters**: `start_vu_stream` / `stop_vu_stream` commands toggle a background thread that emits Tauri events with audio levels. `ChannelStrip.vue` uses a `requestAnimationFrame` loop with lerp (`current += (target - current) * 0.4`) to animate the meter bars — **do not replace with CSS transitions** (they cannot keep up with audio payloads).

### UI Component Conventions

- **`CustomVideoPlayer.vue`**: The project's shared video player. Props: `src`, `muted?`, `captureKeyboard?` (opt-in keyboard shortcuts), `showControls?` (set false to hide built-in control bar and use slot content instead). Exposes: `videoRef`, `seekTo`, `togglePlay`, `skip`, `setVolume`, `toggleFullscreen`, etc. Use `<CustomVideoPlayer :show-controls="false">…slot content…</CustomVideoPlayer>` in `AdvancedEditor` to inject custom overlays and controls as slot content.
- **`SelectField.vue`**: The project's custom dark-themed dropdown. Use instead of native `<select>` everywhere. Props: `v-model` + `:options` (array of `{ value, label }` or strings). Supports mouse-wheel scroll when open.
- All modals use `<Teleport to="body">` + a `.overlay` wrapper div.
- CSS variables (all in `App.vue` `:root`): `--bg-surface`, `--bg-card`, `--bg-deep`, `--bg-input`, `--bg-hover`, `--border`, `--text`, `--text-sec`, `--text-muted`, `--accent` (`#E94560`), `--danger`, `--success`.
- The `mediaPort` inject is `<Ref<number>>` — in `<script>` use `.value`, in templates it auto-unwraps. Declare a `computed(() => ref.value)` if template type errors occur.

### Settings Persistence Pattern

`usePersistenceStore` auto-saves via a `watch` on `state`. When adding new settings fields, add them to both the `PersistedState` interface AND the `DEFAULTS` constant in `stores/persistence.ts`. The `deepMerge(DEFAULTS, saved)` pattern ensures new fields get their default value for users with old saved configs.

Extension feature flags live in `persist.state.extensions` (e.g., `extensions.overlays`, `extensions.tiktokExport`). These gate the `extOverlays` / `extTiktok` computed refs in `AdvancedEditor.vue`.

### i18n

`src/i18n.ts` sets up `vue-i18n` in composition API mode (`legacy: false`). Bundled locales in `src/locales/*.json` are loaded at build time via `import.meta.glob`. Users can drop additional JSON files into `~/.config/opengg/locales/` — the `list_user_locales` command reads them and the frontend calls `registerLocale()` to register them at runtime. Each JSON may have a `_meta: { name, dir }` top-level key for display name and text direction.

### Extensions / Plugins

Extensions are directories under `~/.config/opengg/plugins/` each containing a `manifest.json` with `{ id, name, description, version }`. `scan_extensions` returns the list; the Settings → Extensions section shows them and lets users toggle built-in feature flags. The plugins directory is opened via `open_extensions_folder`.

### File Locations at Runtime

| Data | Path |
|------|------|
| Settings JSON | `~/.config/opengg/ui-settings.json` |
| Theme JSON | `~/.config/opengg/theme.json` |
| SQLite clips DB | `~/.local/share/opengg/clips.db` |
| Thumbnails | `~/.local/share/opengg/thumbnails/` |
| Crash log | `~/.local/share/opengg/opengg_crash.log` |
| User locales | `~/.config/opengg/locales/` |
| Plugins | `~/.config/opengg/plugins/` |
| Default clips folder | `~/Videos/OpenGG` |
| XDG autostart entry | `~/.config/autostart/opengg.desktop` |

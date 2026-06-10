# OpenGG Extension — Authoring Contract (for AI agents & developers)

This is the complete, authoritative specification for building an OpenGG
extension. It is written so an AI coding assistant can read it once and produce a
working extension with no other context. If you are a human, `README.md` is a
gentler walkthrough — but everything you need is here.

OpenGG is a Linux gaming hub (Tauri 2 + Vue 3 frontend; an unprivileged Rust
background daemon). An extension can add **UI** (a settings panel inside the
app), a **background process** (a supervised executable run by the daemon), or
**both**. Extensions live in `~/.local/share/opengg/extensions/<id>/`.

---

## 1. Anatomy of an extension

```
<id>/                     ← folder name should equal the manifest "id"
  manifest.json           ← REQUIRED — metadata + capability declaration
  dist/
    index.iife.js         ← UI part: built IIFE bundle (referenced by "main")
  bin/
    <executable>          ← daemon part: any executable (referenced by "daemon")
  assets/
    icon.svg              ← shown in Settings → Extensions
  locales/
    en.json               ← optional i18n, merged under ext.<id>.*
    ar.json
  src/                    ← UI source (not shipped; only dist/ is loaded)
    index.ts
    Settings.vue
```

An extension may declare **only `main`** (UI-only), **only `daemon`**
(background-only, e.g. an audio router), or **both**.

---

## 2. manifest.json

```jsonc
{
  "id":          "my-extension",   // REQUIRED. kebab-case, [a-z0-9-]. Folder name should match.
  "name":        "My Extension",   // REQUIRED. Display name.
  "description": "What it does.",  // optional, ≤120 chars
  "version":     "0.1.0",          // optional, SemVer
  "author":      "Your Name",      // optional
  "icon":        "assets/icon.svg",// optional, path relative to the extension root
  "main":        "dist/index.iife.js", // optional — UI part (IIFE bundle)
  "hasSettings": true,             // optional — show the gear button (requires a settingsComponent)
  "daemon":      "bin/my-daemon"   // optional — background executable (path relative to root)
}
```

Rules:
- `id` and `name` are mandatory; everything else is optional.
- A manifest with no `main` and no `daemon` is listed but does nothing.
- `daemon` must point to an **executable file** (chmod +x). It can be any
  language (shell, Python, a compiled binary) — the daemon just runs it.

---

## 3. The UI part (frontend IIFE)

### 3.1 Registration contract
The built bundle runs as an IIFE inside the OpenGG WebView and **must** assign a
registration object to a global whose name is `__ext_` + the `id` with dashes
replaced by underscores:

| manifest id     | global key                |
|-----------------|---------------------------|
| `my-extension`  | `window.__ext_my_extension` |
| `cool-ext`      | `window.__ext_cool_ext`     |

```ts
window.__ext_my_extension = {
  settingsComponent: SettingsPanel,  // a Vue 3 component, or null
}
```

- If `hasSettings` is `true`, `settingsComponent` must be a Vue 3 component. It
  is rendered in a modal when the user clicks the gear icon on the extension
  card in Settings → Extensions.
- If you have no settings UI, set `settingsComponent: null` and omit
  `hasSettings` (or set it to `false`).

### 3.2 Runtime globals available to the bundle
- **`window.Vue`** — the full Vue 3 API (`ref`, `computed`, `defineComponent`,
  `h`, `watch`, `defineAsyncComponent`, …). Vue is provided by OpenGG; do **not**
  bundle it (it is marked `external` in `vite.config.js`).
- **`window.opengg`** — a restricted bridge:
  ```ts
  const clips = await window.opengg.invoke('get_clip_list')
  const port  = window.opengg.mediaPort   // local media-server port (number)
  ```
  `invoke()` accepts **only** this read-only whitelist; anything else rejects:

  | Command               | Returns                |
  |-----------------------|------------------------|
  | `get_clip_list`       | clip metadata array    |
  | `get_audio_devices`   | audio device list      |
  | `get_recorder_status` | recorder state         |
  | `scan_extensions`     | extension metadata     |
  | `list_user_locales`   | user locale files      |

  Extensions cannot call destructive or system-modifying commands.

### 3.3 Assets at runtime
Extension files are served over HTTP at
`http://localhost:<mediaPort>/ext/<id>/<path>`. Use `window.opengg.mediaPort` to
build URLs for images or extra JSON your bundle needs to fetch.

### 3.4 Build (Vite, IIFE, self-contained)
`vite.config.js` builds `src/index.ts` into a single self-contained
`dist/index.iife.js`. CSS from scoped `<style>` blocks is injected by JS at
runtime (OpenGG fetches only the one JS file — no separate `.css` is loaded).
Vue is external and mapped to the `Vue` global. See the template's
`vite.config.js` and `package.json`. Build with:

```bash
npm install
npm run build      # → dist/index.iife.js
```

---

## 4. The daemon part (background executable)

If the manifest declares `daemon`, the OpenGG daemon runs that executable as a
**supervised subprocess** when the extension is enabled:

- **Crash → restart** with exponential backoff (2s → 4s → … → 60s max).
- **Clean exit (status 0) → not restarted** (use this for one-shot tasks).
- **Disable / SIGTERM → graceful stop.** The daemon sends `SIGTERM`; trap it to
  clean up (e.g. `trap cleanup EXIT INT TERM` in bash) within ~3s before it is
  force-killed.
- Runs **unprivileged** (no sudo). It inherits group access to `audio`, `input`,
  `video`. Talk to the system via normal CLIs/sockets (e.g. `pactl`, D-Bus).
- It should be **long-running and event-driven** if it monitors something — busy
  loops waste CPU. (The bundled `sunshine-audio` example blocks on
  `pactl subscribe`, so it costs ~zero CPU while idle.)

Enable/disable is controlled from Settings → Extensions and persisted in
`~/.config/opengg/extensions.json`. The daemon starts/stops the process live —
no app or daemon restart needed.

---

## 5. Locales (optional)

Place `locales/<lang>.json` next to `manifest.json` (`en` and `ar` are merged).
They are namespaced under `ext.<id>.*` so they never collide with core strings:

```json
// locales/en.json
{ "settingsTitle": "My Extension Settings", "countLabel": "Count" }
```

Access from a settings component via vue-i18n: `t('ext.my-extension.settingsTitle')`.

---

## 6. Lifecycle summary

1. User drops the folder into `~/.local/share/opengg/extensions/` (or uses
   `make new-extension NAME=…`).
2. Settings → Extensions → Refresh lists it (any folder with a valid `manifest.json`).
3. Enabling loads the UI part (IIFE) live and starts the daemon part (if any).
4. The gear opens `settingsComponent`. Disabling unloads/stops both parts.

---

## 7. Checklist before shipping

- [ ] `manifest.json` has `id` (kebab-case) and `name`.
- [ ] If UI: `main` points to a built `dist/index.iife.js` that sets
      `window.__ext_<id_underscored>` with a `settingsComponent`.
- [ ] If UI with settings: `hasSettings: true`.
- [ ] If daemon: `daemon` points to a `chmod +x` executable that traps `SIGTERM`.
- [ ] `npm run build` succeeds and emits a single `dist/index.iife.js`.
- [ ] Only the read-only `window.opengg.invoke` whitelist is used.

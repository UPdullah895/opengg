# OpenGG Extension Development Guide

Build your first OpenGG extension in 10 minutes.

---

## What Are Extensions?

OpenGG extensions are modular add-ons that can:
- **Add UI panels** with Vue 3 components and settings
- **Run background daemons** (supervisory child processes with crash restart)
- **Integrate with OpenGG's audio system** via the 5-channel mixer and daemon APIs

Extensions are discovered from `~/.local/share/opengg/extensions/` and managed in Settings → Extensions.

---

## Quick Start: Create Your First Extension

### 1. Scaffold a New Extension

From the OpenGG repo root:

```bash
make new-extension NAME=my-first-ext
```

This creates:
- `~/.local/share/opengg/extensions/my-first-ext/` with the full template
- `manifest.json` (extension metadata)
- `src/Settings.vue` (UI component)
- `dist/index.iife.js` (built IIFE bundle)
- `locales/en.json` and `locales/ar.json` (i18n)
- `package.json` and `vite.config.js` (build setup)

The script also:
- Replaces template placeholders (id, name, global key)
- Runs `npm install && npm run build` automatically
- Prints next steps

### 2. Edit Your Extension

All source files are in the extension folder. Key files:

**`manifest.json`** — Metadata (auto-read by OpenGG):
```json
{
  "id": "my-first-ext",
  "name": "My First Extension",
  "description": "A demo extension.",
  "version": "0.1.0",
  "icon": "assets/icon.svg",
  "main": "dist/index.iife.js",
  "hasSettings": true,
  "locales": {
    "en": { "name": "My First Extension", "description": "A demo." },
    "ar": { "name": "إضافتي الأولى", "description": "عرض توضيحي." }
  }
}
```

**`src/Settings.vue`** — The UI component:
```vue
<script setup lang="ts">
import { useI18n } from 'vue-i18n'
const { t } = useI18n()
</script>

<template>
  <div>
    <h3>{{ t('ext.my-first-ext.title') }}</h3>
    <p>Edit this in src/Settings.vue</p>
  </div>
</template>

<style scoped>
/* Scoped styles only (CSS isolation) */
</style>
```

**`locales/{en,ar}.json`** — User-facing strings:
```json
{
  "title": "My Extension Title",
  "description": "Shown in settings"
}
```

### 3. Build & Install

```bash
cd ~/.local/share/opengg/extensions/my-first-ext
npm run build
```

This generates `dist/index.iife.js` (IIFE bundle that runs in the OpenGG WebView).

### 4. Test in OpenGG

1. Open OpenGG → Settings → Extensions
2. Click **Refresh** to reload the extensions list
3. Toggle your extension **on** (blue switch)
4. Click the **gear icon** (⚙) on your extension card to open settings

Changes to `src/Settings.vue` require rebuild (`npm run build`). In dev mode (`import.meta.env.DEV`), a **reload button** appears next to the gear icon — click it to hot-reload without full app restart.

---

## Manifest Fields Reference

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | auto | Kebab-case unique identifier (auto-derived from folder name) |
| `name` | string | yes | Display name in Settings → Extensions |
| `description` | string | yes | Short description (max 120 chars) |
| `version` | string | no | Semver (e.g. "1.2.3") shown in UI |
| `icon` | string | no | Relative path to SVG/PNG (42×42px); defaults to puzzle piece |
| `main` | string | no | Relative path to IIFE bundle (`dist/index.iife.js`) |
| `daemon` | string | no | Relative path to executable (`bin/daemon`) |
| `hasSettings` | bool | no | Show gear icon if true (default: false) |
| `locales` | object | no | Map of `{ en: { name, description }, ar: { ... } }` |

### Daemon Path Security

The `daemon` field must:
- **Not start with `/`** (no absolute paths — security risk)
- **Not contain `..` segments** (no path traversal — security risk)
- Point to an **executable file** within the extension directory
- Canoncially resolve inside the extension folder (after symlink resolution)

Invalid examples:
```json
{
  "daemon": "/usr/bin/something"  // Error: absolute path
  "daemon": "../../../bin/rm"     // Error: path traversal
  "daemon": "../../etc/passwd"    // Error: escapes extension dir
}
```

Valid example:
```json
{
  "daemon": "bin/my-daemon"  // OK: relative, no '..'
}
```

### JSON Schema & Editor Autocomplete

The template's `manifest.json` includes a `$schema` reference:

```json
{
  "$schema": "./manifest.schema.json",
  "id": "my-ext",
  ...
}
```

IDEs (VS Code, JetBrains, etc.) automatically pick this up and provide:
- Field validation
- Autocomplete
- Inline documentation

The schema is always available at `extension-template/manifest.schema.json` in the repo.

---

## Frontend: Settings Component

Your `Settings.vue` is a standard Vue 3 component. It's loaded as an IIFE module and rendered inside OpenGG's settings modal.

### Constraints

1. **Scoped CSS only** — use `<style scoped>` to isolate styles (no global side effects)
2. **No breaking imports** — keep dependencies minimal; avoid heavy npm packages
3. **i18n first** — all user-facing text via `useI18n()` and `t('key')`
4. **Reactive stores** — use `reactive()` or `ref()` for local state; Pinia if complex

### Example: Counter Setting

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
const count = ref(0)

const increment = () => count.value++
</script>

<template>
  <div class="ext-panel">
    <h3>{{ t('ext.my-first-ext.counterTitle') }}</h3>
    <p>{{ t('ext.my-first-ext.counterDesc') }}</p>
    <button @click="increment">{{ t('ext.my-first-ext.increment') }}</button>
    <p>Count: {{ count }}</p>
  </div>
</template>

<style scoped>
.ext-panel {
  padding: 16px;
  background: var(--bg-card);
  border-radius: 8px;
}

button {
  padding: 8px 16px;
  background: var(--accent);
  color: white;
  border: none;
  border-radius: 4px;
  cursor: pointer;
}

button:hover {
  opacity: 0.8;
}
</style>
```

With locale strings in `locales/en.json`:
```json
{
  "counterTitle": "Counter",
  "counterDesc": "Click to increment.",
  "increment": "Increment"
}
```

### API: Reading OpenGG State

The `window.opengg` object exposes a read-only 5-command API:

```ts
// 1. Get current audio channel volumes (0–100)
const volumes = await window.opengg.invoke('get_channel_volumes')
// Returns: { Game: 75, Chat: 100, Media: 50, Aux: 0, Mic: 80 }

// 2. Get list of detected apps + their current routing
const apps = await window.opengg.invoke('list_audio_apps')
// Returns: [ { id: 42, name: 'Discord', binary: 'discord', channel: 'Chat', ... }, ... ]

// 3. List available monitors (GSR targets)
const monitors = await window.opengg.invoke('list_monitors')
// Returns: [ 'screen', 'DP-1', 'HDMI-A-1' ]

// 4. Get current session type (X11 or Wayland)
const sessionType = await window.opengg.invoke('get_session_type')
// Returns: 'x11' or 'wayland'

// 5. Check if a module is currently enabled
const audioEnabled = await window.opengg.invoke('is_module_enabled', 'audio')
// Returns: true or false
```

These are read-only snapshots — your extension cannot modify state. To persist settings, use `localStorage` (scoped by extension id).

---

## Daemon: Background Part

If `manifest.json` declares a `daemon` field, OpenGG's supervisor runs and manages it:

1. **Startup** — daemon is spawned when the extension is enabled
2. **Monitoring** — if it crashes, it's restarted with exponential backoff (2s → 4s → 8s → ... → 60s)
3. **Graceful stop** — on disable or app shutdown, `SIGTERM` is sent; 3-second grace period before `SIGKILL`
4. **Process group** — the daemon runs in its own process group, so helper subprocesses are also signaled

### Example: Simple Daemon

`bin/my-daemon`:
```bash
#!/usr/bin/env bash
set -euo pipefail

log() { printf '[my-daemon] %s\n' "$*" >&2; }

trap 'log "Shutting down..."; exit 0' TERM INT

log "Starting"

# Do work in a loop
while true; do
  log "Tick $(date +%s)"
  sleep 5
done
```

Make it executable:
```bash
chmod +x bin/my-daemon
```

When enabled:
- `log "Starting"` appears in OpenGG's daemon logs
- Every 5 seconds, a "Tick" line is logged
- If the process exits with code 0, it stays stopped (not restarted)
- If it crashes, it restarts after a backoff delay

### Daemon API

Daemons communicate with the frontend via the daemon itself (write files, emit events, etc.). The supervisor provides no direct inter-process API.

### Example: Sunshine Audio Router

See `packaging/extensions/sunshine/` for a real daemon extension that:
- Monitors PipeWire sink events via `pactl subscribe`
- Auto-routes OpenGG audio channels to Sunshine streaming sinks
- Cleans up on shutdown

---

## Validation

Before shipping, validate your extension:

```bash
make validate-extension DIR=~/.local/share/opengg/extensions/my-first-ext
```

Checks:
- ✓ `manifest.json` parses and declares required fields (name, description)
- ✓ `id` is kebab-case
- ✓ `version` is valid semver (if present)
- ✓ `daemon` path has no leading `/` or `..` segments
- ✓ `daemon` file exists and is executable (if declared)
- ✓ `main` file exists (if declared)
- ✓ `locales/*.json` files parse as valid JSON
- ✓ `manifest.json` validates against the schema (if `jsonschema` available)

---

## Development Workflow

### Local Dev (Hot Reload)

1. Build once:
   ```bash
   cd ~/.local/share/opengg/extensions/my-first-ext
   npm run build
   ```

2. In OpenGG Settings → Extensions, enable your extension.

3. Click the **reload button** (↻) next to your extension name (dev mode only).

4. Modify `src/Settings.vue` and rebuild:
   ```bash
   npm run build
   ```

5. Click reload again. No full app restart needed.

### Building for Release

```bash
npm run build
git add -A
git commit -m "Release v1.0.0"
git tag v1.0.0
```

Package the entire folder and share with users:
```bash
tar czf my-first-ext-1.0.0.tar.gz ~/.local/share/opengg/extensions/my-first-ext/
```

Users extract to `~/.local/share/opengg/extensions/my-first-ext/` and refresh in Settings.

---

## Localization

Both `en.json` and `ar.json` **must be provided** for your extension to fully support the app's bilingual design.

### English (`locales/en.json`)

```json
{
  "title": "My Extension",
  "description": "Does cool things",
  "settingLabel": "Enable feature X",
  "settingHint": "This controls behavior Y"
}
```

### Arabic (`locales/ar.json`)

```json
{
  "title": "إضافتي",
  "description": "تفعل أشياء رائعة",
  "settingLabel": "تفعيل الميزة X",
  "settingHint": "هذا يتحكم في السلوك Y"
}
```

In your component:
```vue
<script setup>
import { useI18n } from 'vue-i18n'
const { t } = useI18n()
</script>

<template>
  <p>{{ t('ext.my-first-ext.settingLabel') }}</p>
</template>
```

Keys are namespaced as `ext.<extension-id>.*`.

---

## Distribution & Install

Users can install your extension by:

1. Copying the folder to `~/.local/share/opengg/extensions/<id>/`
2. Opening OpenGG → Settings → Extensions → **Refresh**
3. Toggling your extension on

Or programmatically via:
```bash
mkdir -p ~/.local/share/opengg/extensions/
cp -r my-first-ext ~/.local/share/opengg/extensions/
```

---

## Troubleshooting

### Extension doesn't appear in Settings

1. Check folder location: `~/.local/share/opengg/extensions/<id>/`
2. Verify `manifest.json` exists and is valid JSON
3. Ensure the folder name matches the `id` field (or is inferred as the folder name)
4. Click **Refresh** in Settings → Extensions

### Settings button (gear) doesn't appear

1. Check `"hasSettings": true` in `manifest.json`
2. Ensure the extension is **enabled** (blue toggle)
3. Verify `main` file exists and built successfully

### Hot-reload button (↻) missing

1. You're in production mode — hot-reload is dev-only
2. Run OpenGG with `NODE_ENV=development` or build with `--dev`
3. Restart OpenGG in dev mode

### Daemon crashes immediately

1. Check logs in OpenGG's daemon output
2. Ensure the file is executable: `chmod +x bin/my-daemon`
3. Test manually: `bin/my-daemon` (should run without error)
4. Check for missing dependencies (bash, python, etc.)

---

## Next Steps

- Read the full [Extension Manifest Schema](../EXTENSION_MANIFEST_SCHEMA.md)
- Explore the [Extension Template](../extension-template/)
- Study the [Sunshine Audio Router](../packaging/extensions/sunshine/) daemon example
- Check [CLAUDE.md](../CLAUDE.md) for architecture details

---

## Summary

1. **Scaffold** → `make new-extension NAME=my-ext`
2. **Edit** → `src/Settings.vue` + `locales/*.json`
3. **Build** → `npm run build`
4. **Test** → OpenGG Settings → Extensions → Reload (dev) or Refresh
5. **Validate** → `make validate-extension DIR=~/.local/share/opengg/extensions/my-ext`
6. **Share** → Users copy folder to `~/.local/share/opengg/extensions/`

Happy extending!

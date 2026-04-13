# OpenGG Extension Template

A minimal starter for building OpenGG extensions.

## What is an OpenGG Extension?

Extensions are self-contained JavaScript IIFE bundles that OpenGG loads at runtime.
They can:

- **Render a custom settings panel** shown in Settings → Extensions (gear icon)
- **Read clip and audio data** via the `window.opengg.invoke()` API
- **Add i18n strings** by placing locale JSON files next to the bundle

Extensions cannot call destructive Tauri commands — only a read-only whitelist
is accessible through the API bridge.

---

## Directory Structure

```
my-extension/           ← drop this folder into ~/.local/share/opengg/extensions/
  manifest.json         ← metadata + capability declarations (required)
  dist/
    index.iife.js       ← built IIFE bundle (referenced by manifest.json "main")
  assets/
    icon.svg            ← shown in Settings → Extensions
  locales/
    en.json             ← i18n strings merged under ext.<id>.*
    ar.json
  src/                  ← source (not shipped, only dist/ is needed)
    index.ts
    Settings.vue
```

---

## Quick Start

```bash
# 1. Clone this template
cp -r extension-template ~/.local/share/opengg/extensions/my-extension
cd ~/.local/share/opengg/extensions/my-extension

# 2. Install dependencies (Vite + Vue 3 dev dependencies)
npm install

# 3. Edit manifest.json — set id, name, description
# 4. Edit src/Settings.vue — build your UI
# 5. Build the IIFE bundle
npm run build

# 6. Open OpenGG → Settings → Extensions → Refresh
#    Your extension should appear in the list.
```

---

## manifest.json Fields

| Field         | Type    | Required | Description |
|---------------|---------|----------|-------------|
| `id`          | string  | ✓        | Unique kebab-case identifier (a-z, 0-9, hyphen) |
| `name`        | string  | ✓        | Display name shown in the Extensions UI |
| `description` | string  | ✗        | Short description (≤ 120 chars) |
| `version`     | string  | ✗        | SemVer string e.g. `"1.0.0"` |
| `author`      | string  | ✗        | Your name or handle |
| `icon`        | string  | ✗        | Path to icon file relative to extension root |
| `main`        | string  | ✗        | Path to the IIFE bundle relative to extension root |
| `hasSettings` | boolean | ✗        | Show gear button → opens `settingsComponent` |

---

## IIFE Registration Pattern

Your bundle must set `window.__ext_<id>` after running.
Dashes in the extension `id` are replaced with underscores in the global key:

```
id: "my-extension"  →  window.__ext_my_extension
id: "cool-ext"      →  window.__ext_cool_ext
```

Minimum bundle:

```js
(function () {
  window.__ext_my_extension = {
    settingsComponent: null,   // or a Vue 3 component
  };
})();
```

---

## window.Vue

OpenGG exposes the full Vue 3 API on `window.Vue` before any extension loads.
You can use it directly without bundling Vue:

```js
const { defineComponent, ref, computed, h, watch } = window.Vue;
```

Set `vue` as an external in your Vite config (see `vite.config.js`) so the
build doesn't include Vue in your bundle.

---

## window.opengg API

```ts
// Read-only command bridge
const clips = await window.opengg.invoke('get_clip_list');
const port  = window.opengg.mediaPort;  // media server port number
```

**Allowed commands:**

| Command                | Returns |
|------------------------|---------|
| `get_clip_list`        | Clip metadata array |
| `get_audio_devices`    | Audio device list |
| `get_recorder_status`  | Recorder state |
| `scan_extensions`      | Extension metadata array |
| `list_user_locales`    | User locale files |

---

## Locales

Place `locales/<lang>.json` next to `manifest.json`.
OpenGG merges them into vue-i18n under `ext.<id>.*`:

```json
// locales/en.json
{ "settingsTitle": "My Extension Settings" }
```

Access in your component via `inject('$i18n')` or `useI18n()` if the extension
is rendered inside the OpenGG Vue app tree (which it is — settings components
are mounted via `defineAsyncComponent`).

---

## Vite Config

```js
// vite.config.js
export default {
  build: {
    lib: {
      entry: 'src/index.ts',
      name: 'MyExtension',
      formats: ['iife'],
      fileName: () => 'index.iife.js',
    },
    outDir: 'dist',
    rollupOptions: {
      external: ['vue'],
      output: { globals: { vue: 'Vue' } },
    },
  },
};
```

---

## package.json

```json
{
  "name": "my-extension",
  "version": "0.1.0",
  "scripts": {
    "build": "vite build",
    "dev":   "vite build --watch"
  },
  "devDependencies": {
    "vue":        "^3.4.0",
    "vite":       "^5.0.0",
    "@vitejs/plugin-vue": "^5.0.0"
  }
}
```

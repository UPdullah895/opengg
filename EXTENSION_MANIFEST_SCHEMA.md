# Extension Manifest Schema

Every OpenGG extension is a **directory** placed in:

```
~/.local/share/opengg/extensions/<id>/
```

The directory must contain a `manifest.json`. The folder name is used as the extension `id`.

---

## Directory Structure

```
~/.local/share/opengg/extensions/
└── my-extension/
    ├── manifest.json          ← required
    ├── daemon                 ← optional: backend executable (any language)
    ├── dist/index.iife.js     ← optional: frontend IIFE bundle
    ├── locales/
    │   ├── en.json
    │   └── ar.json
    └── icon.svg
```

An extension may be **daemon-only**, **frontend-only**, or **both**.

---

## Schema

```json
{
  "id": "my-extension",
  "name": "My Extension",
  "description": "A short description of what this extension does.",
  "version": "1.0.0",
  "author": "Your Name",
  "icon": "icon.svg",
  "main": "dist/index.iife.js",
  "hasSettings": true,
  "daemon": "daemon"
}
```

---

## Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | auto (folder name) | Unique identifier. Derived from the folder name — the field in `manifest.json` is informational only. |
| `name` | string | yes | Display name shown in the Extensions UI. |
| `description` | string | yes | Short description shown below the name (≤ 120 chars). |
| `version` | string | no | SemVer string shown in the UI (e.g. `"1.2.0"`). Defaults to `"0.0.0"` if absent. |
| `author` | string | no | Author name or handle. |
| `icon` | string | no | Icon filename relative to the extension directory (SVG or PNG). |
| `main` | string | no | Relative path to the IIFE JavaScript bundle. Omit for daemon-only extensions. |
| `ui` | string | no | Alias for `main` — same meaning. |
| `hasSettings` | bool | no | When `true`, a gear button appears on the extension card that opens the settings panel. Defaults to `false`. |
| `daemon` | string | no | Relative path to the daemon-side executable. OpenGG starts it at daemon launch and restarts it automatically (with exponential backoff up to 60 s) if it exits. The executable can be a shell script, Python script, or compiled binary — anything with an executable bit set. |

---

## Example — Frontend only

```json
{
  "id": "tiktok-export",
  "name": "TikTok Vertical Export",
  "description": "Export clips as 9:16 vertical video optimized for TikTok.",
  "version": "0.3.1",
  "icon": "icon.svg",
  "main": "index.iife.js",
  "hasSettings": false
}
```

## Example — Daemon only

```json
{
  "id": "sunshine",
  "name": "Sunshine Routing",
  "description": "Routes OpenGG audio sinks into Sunshine/Moonlight when streaming starts.",
  "version": "1.0.0",
  "author": "OpenGG Community",
  "daemon": "daemon"
}
```

The `daemon` file is an executable that the OpenGG daemon supervises. It appears in Settings → Extensions but produces no settings panel (no `main`).

## Example — Both frontend and daemon

```json
{
  "id": "discord-rpc",
  "name": "Discord Rich Presence",
  "description": "Shows what game you are playing and your current clip count on Discord.",
  "version": "2.0.0",
  "daemon": "daemon",
  "main": "dist/index.iife.js",
  "hasSettings": true
}
```

---

## `locales` (file-based)

Place `locales/<lang>.json` files alongside `manifest.json`. OpenGG merges them
into vue-i18n under the namespace `ext.<id>.*`.

```json
// locales/en.json
{
  "settingsTitle": "My Extension Settings"
}
```

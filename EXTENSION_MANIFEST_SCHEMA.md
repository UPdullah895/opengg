# Extension Manifest Schema

Every OpenGG extension must have a `manifest.json` at the root of its directory.

## Directory Locations

| Type | Path |
|------|------|
| Core extensions | `~/.local/share/opengg/Extensions/<id>/manifest.json` |
| Third-party plugins | `~/.local/share/opengg/plugins/<id>/manifest.json` |

The folder name is used as the extension `id`. Core extensions take precedence — if both directories contain a folder with the same `id`, the one in `Extensions/` wins.

---

## Schema

```json
{
  "id": "my-extension",
  "name": "My Extension",
  "description": "A short description of what this extension does.",
  "version": "1.0.0",
  "icon": "<base64-encoded PNG>",
  "daemon": "bin/my-daemon",
  "main": "dist/index.iife.js",
  "hasSettings": true,
  "locales": {
    "en": {
      "name": "My Extension",
      "description": "A short description of what this extension does."
    },
    "ar": {
      "name": "إضافتي",
      "description": "وصف مختصر لما تفعله هذه الإضافة."
    }
  }
}
```

---

## Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | auto (folder name) | Unique identifier. Derived from the folder name — the field in `manifest.json` is informational only. |
| `name` | string | yes | Display name shown in the Extensions UI when no matching locale entry exists. |
| `description` | string | yes | Short description shown below the name. Used as fallback when no locale entry matches. |
| `version` | string | no | Semver string shown in the UI (e.g. `"1.2.0"`). Defaults to empty string if absent. |
| `icon` | string | no | Relative path to an icon file inside the extension directory (SVG or PNG, 42×42px). If absent, a generic puzzle-piece SVG is shown. |
| `daemon` | string | no | Relative path to a `chmod +x` executable inside the extension directory. The OpenGG daemon runs it as a supervised background process — crashes are restarted with exponential backoff; a clean exit (code 0) is left alone. Omit for UI-only extensions. |
| `main` | string | no | Relative path to a frontend IIFE bundle (built with Vite). Loaded into the OpenGG WebView at startup. Omit for daemon-only extensions. |
| `hasSettings` | bool | no | When `true`, a gear (⚙) button appears on the extension card that opens the extension's settings panel (exported as `settingsComponent` from the IIFE). Defaults to `false`. |
| `locales` | object | no | Map of locale code → `{ name, description }`. The app displays the entry matching the user's active language, with `name`/`description` root fields as fallback. |

---

## `locales` Object

Keys are BCP 47 language codes (e.g. `"en"`, `"ar"`, `"fr"`). Each value has:

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Localized display name |
| `description` | string | Localized description |

The app checks `locales[currentLocale]` first. If the key is missing or `locales` is omitted entirely, it falls back to the root `name` and `description` fields.

---

## Example — Minimal

```json
{
  "name": "Clip Overlay",
  "description": "Adds a real-time clip counter overlay to your screen."
}
```

## Example — Daemon only (background script, no UI)

```json
{
  "name": "Sunshine Audio Router",
  "description": "Routes OpenGG audio into Sunshine/Moonlight when streaming starts.",
  "version": "1.0.0",
  "daemon": "bin/sunshine"
}
```

Extension directory layout:
```
sunshine-audio/
├── manifest.json
└── bin/
    └── sunshine    ← chmod +x shell script or binary
```

## Example — Full (UI + daemon)

```json
{
  "name": "TikTok Vertical Export",
  "description": "Export clips as 9:16 vertical video optimized for TikTok.",
  "version": "0.3.1",
  "icon": "assets/icon.svg",
  "daemon": "bin/exporter",
  "main": "dist/index.iife.js",
  "hasSettings": true,
  "locales": {
    "en": {
      "name": "TikTok Vertical Export",
      "description": "Export clips as 9:16 vertical video optimized for TikTok."
    },
    "ar": {
      "name": "تصدير عمودي لتيك توك",
      "description": "تصدير المقاطع كفيديو عمودي 9:16 محسّن لتيك توك."
    }
  }
}
```

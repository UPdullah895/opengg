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
| `icon` | string | no | Base64-encoded PNG image displayed as the extension's icon (42×42px). If absent, a generic puzzle-piece SVG is shown. |
| `hasSettings` | bool | no | When `true`, a gear (⚙) button appears on the extension card that opens the extension's settings panel. Defaults to `false`. |
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

## Example — Full

```json
{
  "name": "TikTok Vertical Export",
  "description": "Export clips as 9:16 vertical video optimized for TikTok.",
  "version": "0.3.1",
  "icon": "iVBORw0KGgo...",
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

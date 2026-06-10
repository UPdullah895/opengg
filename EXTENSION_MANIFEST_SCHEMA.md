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
| `permissions` | array of strings | no | Optional list of data-access permission tiers the extension declares (see Permissions section below). If omitted, the extension is treated as legacy: all read-only commands are allowed and the consent modal lists "all read-only data access (legacy)". Note: first-run consent for extensions with a `daemon` part is required regardless of this field. Future write-capable tiers will require explicit declaration. |
| `locales` | object | no | Map of locale code → `{ name, description }`. The app displays the entry matching the user's active language, with `name`/`description` root fields as fallback. |

---

## Permissions

The optional `permissions` field declares which data-access tiers the extension needs. If omitted, the extension defaults to **legacy behavior**: all read-only commands are allowed without explicit consent.

Currently defined permission tiers (mapped to Tauri commands):

| Permission | Commands | Access |
|------------|----------|--------|
| `clips:read` | `get_clip_list` | Read-only access to the clip database and gallery |
| `audio:read` | `get_audio_devices` | Read-only access to audio device info and routing state |
| `recorder:read` | `get_recorder_status` | Read-only access to replay buffer and recording state |
| `extensions:read` | `scan_extensions` | Read-only access to installed extensions metadata |
| `locales:read` | `list_user_locales` | Read-only access to installed locale files |

**Backward Compatibility:** Extensions without a `permissions` field are treated as legacy and granted all five read-only tiers; their UI parts keep working unchanged. However, **first-run consent applies to any extension with a `daemon` part regardless of the `permissions` field** — a previously-installed daemon extension will ask once after upgrading to this version (safe default: a background process should never run without an explicit yes).

**Future Write Tiers:** Planned write-capable tiers (e.g., `clips:write`, `audio:write`) will require explicit declaration in `permissions` and will be itemized in the same consent modal.

**Known limitation (v1):** consent is enforced by the frontend at enable time; denial is persisted as `enabled=false` in the shared `~/.config/opengg/extensions.json`, which the daemon honors at startup. A daemon extension that was already enabled *before* this feature existed will still be auto-started by the daemon until the user visits Settings → Extensions once (where the needs-consent state is surfaced). Daemon-side consent checking is a planned follow-up.

### First-Run Consent for Daemon Extensions

When enabling an extension with a `daemon` field for the first time, the frontend:
1. Checks if the extension's manifest declares a `daemon` part
2. Checks if user consent has been previously recorded
3. If consent is absent, shows a **consent modal** listing:
   - Extension name and version
   - "Runs a background process with your user permissions"
   - List of declared permissions (or "all read-only data access (legacy)" if `permissions` is omitted)
   - Allow and Deny buttons
4. If the user clicks **Deny**, the extension stays disabled (no consent recorded)
5. If the user clicks **Allow**, consent is recorded in `settings.extensionConsents` and the daemon part is started

Consent is one-time per extension (stored in persistence). Users can re-enable a consented extension without seeing the modal again.

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

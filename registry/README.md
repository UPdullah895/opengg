# OpenGG Extension Registry

The extension registry is a static, git-based registry for OpenGG extensions. It enables users to discover and learn about community-contributed extensions directly from the in-app Browse view.

## Registry Architecture

The registry is served as a static JSON file from the OpenGG GitHub repository:
```
https://raw.githubusercontent.com/UPdullah895/opengg/main/registry/index.json
```

**Zero infrastructure** — no central server, no authentication, no database. Publishing an extension is as simple as submitting a PR that adds one entry to `index.json`.

## Publishing Your Extension

### Prerequisites
1. Your extension is tested and ready for release.
2. You have created a release on GitHub with a downloadable `.tar.gz` archive (see [Extension Distribution](#extension-distribution) below).
3. Your extension manifest is valid against the [Extension Manifest Schema](../EXTENSION_MANIFEST_SCHEMA.md).

### Step 1: Validate Your Manifest

Run the validation script:
```bash
./scripts/validate-extension.sh path/to/your/extension
```

This checks:
- Manifest is valid JSON
- Required fields are present (`name`, `description`, `version`)
- All referenced files exist (icon, main, daemon, locales)
- Permission tier names are recognized
- Description length is <= 120 characters
- If `daemon` field is present, the path does not traverse (`../`) or start with `/`

### Step 2: Create Your Release

Package your extension as a `.tar.gz` archive:
```bash
tar -czf my-extension-1.0.0.tar.gz my-extension/
```

Upload this as a release asset on GitHub or another public host.

### Step 3: Submit a Registry Entry

Edit `registry/index.json` and add an entry to the `extensions` array:

```json
{
  "id": "my-extension",
  "name": "My Extension",
  "description": "A short description of what this extension does.",
  "version": "1.0.0",
  "author": "Your Name",
  "repo": "https://github.com/yourusername/my-extension",
  "download": "https://github.com/yourusername/my-extension/releases/download/v1.0.0/my-extension-1.0.0.tar.gz",
  "permissions": ["clips:read"],
  "hasDaemon": false,
  "icon": null
}
```

### Registry Entry Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Unique identifier in kebab-case (e.g. `my-extension`). Must match the folder name when unpacked. |
| `name` | string | yes | Display name shown in the Browse view. |
| `description` | string | yes | Short description (max 120 characters). Should describe what the extension does, not how to install it. |
| `version` | string | yes | Semantic version of this release (e.g. `1.0.0`). |
| `author` | string | no | Author name or organization (for attribution). |
| `repo` | string | no | URL to the source code repository (GitHub, GitLab, etc.). |
| `download` | string | yes | Public URL to download the `.tar.gz` archive. The app uses this URL to fetch extension binaries. |
| `permissions` | array of strings | no | List of permission tiers declared in the manifest (e.g. `["clips:read", "audio:read"]`). Omit if the extension declares no explicit permissions (legacy mode). See [Extension Manifest Schema](../EXTENSION_MANIFEST_SCHEMA.md#permissions). |
| `hasDaemon` | boolean | no | `true` if the manifest contains a `daemon` field; otherwise `false` or omitted. Used for UI indicators. |
| `icon` | string or null | no | Absolute URL to a 42×42px icon image (PNG or SVG), or `null` if no icon. This is populated from the released archive by the maintainer; see below. |

### Icon Hosting

If your extension includes an icon file in the manifest, you can optionally host it for the registry display:

1. Extract your `.tar.gz` release
2. Locate the icon file referenced in `manifest.json` (e.g. `assets/icon.svg`)
3. Upload it to a public host (GitHub raw, Githubusercontent, a CDN, etc.)
4. Include the full URL in the registry entry's `icon` field

**Example:** if your extension's icon is at `https://raw.githubusercontent.com/yourusername/my-extension/main/assets/icon.svg`, set:
```json
"icon": "https://raw.githubusercontent.com/yourusername/my-extension/main/assets/icon.svg"
```

Leave as `null` if you don't want to provide an icon.

## Registry Review Expectations

When you submit a PR to add your extension to the registry:

1. **Manifest validation** — the CI pipeline runs `./scripts/validate-extension.sh` on your entry to confirm the manifest structure is correct.
2. **Security review** — maintainers verify:
   - The `daemon` field (if present) does not contain path traversal (`../` or `/`)
   - The download URL points to a legitimate, signed release or trusted host
   - The extension description is accurate and does not contain misleading claims
3. **Metadata quality** — ensure:
   - Description is clear, concise, and <= 120 characters
   - Author and repo links are accurate
   - Icon URL (if provided) is stable and responsive
4. **Community feedback** — maintainers may request changes if the extension scope seems out-of-place or conflicts with existing plugins

Once approved, your entry is merged and live in the registry within minutes.

## Registry Index Schema

The `index.json` file follows this structure:

```json
{
  "version": 1,
  "updated": "2026-06-10",
  "extensions": [
    { /* entry 1 */ },
    { /* entry 2 */ }
  ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `version` | number | Registry format version (currently `1`). Clients reject registries with unsupported versions. |
| `updated` | string | ISO 8601 date (YYYY-MM-DD) of the last registry update. Used for caching / change detection. |
| `extensions` | array | List of extension entries (see fields above). |

## Extension Distribution

### Why `.tar.gz`?

- **Simplicity** — no build-specific installers or signatures needed (MVP phase).
- **Platform-agnostic** — works on all Linux distributions.
- **Integrity** — SHA256 checksums can be listed in the release notes.

### Recommended Release Layout

When creating your `.tar.gz`, the structure should be:

```
my-extension-1.0.0.tar.gz
└── my-extension/
    ├── manifest.json
    ├── dist/                  (if main JS/CSS bundle)
    │   └── index.iife.js
    ├── bin/                   (if daemon executable)
    │   └── my-daemon
    ├── assets/                (icons, images, etc.)
    │   └── icon.svg
    └── README.md              (optional: install/usage instructions)
```

When unpacked, the top-level directory becomes the extension ID.

## Offline & Caching

The OpenGG app fetches `index.json` on demand when the user opens the Browse view (Settings → Store). If the network is unavailable, the Browse view shows an error with a retry button. There is no local caching of the registry.

## In-App Browsing

When users visit Settings → Store → Browse:
1. The app fetches `index.json` from the registry URL
2. Validates `version == 1`
3. Displays each extension as a card showing:
   - Icon (if provided)
   - Name, version, author
   - Description
   - Permission chips (if declared)
   - "Has daemon" indicator (if applicable)
   - "View repository" button (links to `repo` URL)
4. A note states: "Install from registry coming soon — see docs/EXTENSION_DEV.md for manual install"

**No remote code execution:** the app does NOT download or execute archives from the registry in this MVP. Users install manually (see [docs/EXTENSION_DEV.md](../docs/EXTENSION_DEV.md#publishing-your-extension)).

## Future Roadmap

- **In-app install** — fetch and unpack `.tar.gz` directly (requires trust model and signing)
- **Update checks** — compare local vs. registry versions
- **Ratings & reviews** — community feedback integration
- **Search & filtering** — tag-based discovery
- **Central registry mirror** — organization-hosted CDN for reliability

## Registry Maintenance

To update the registry:

1. Edit `registry/index.json` and/or `registry/index.schema.json`
2. Run `python3 -c "import json; json.load(open('registry/index.json')); print('OK')"` to validate JSON
3. Commit and push to the `main` branch
4. Changes are live within minutes (no deployment step)

## Questions?

See [docs/EXTENSION_DEV.md](../docs/EXTENSION_DEV.md) for the complete extension development guide.

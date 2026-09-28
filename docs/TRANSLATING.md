# Translating OpenGG

OpenGG's Qt6/QML shell has its own lightweight i18n implementation (`qt-shell/src/i18n.rs`) — a `QObject` singleton exposed to QML as `I18n`, deliberately kept simple rather than pulling in a full i18n framework. Currently, English (`en`) and Arabic (`ar`, with full RTL support) are available. We'd love help adding more languages!

## How i18n Works

1. **Locale directory** — `I18n` reads every `*.json` file in a single locales directory: `qt-shell/locales/` in a dev build, or the path in the `OPENGG_LOCALES_DIR` environment variable in production/installed builds. There is deliberately no separate "user locales" merge layer — a translator's file dropped into that one directory is already in the right place.
2. **Runtime reload** — `I18n.reloadLocales()` (wired to a "reload locales" action in Settings) re-scans the directory without restarting the app, so a language pack dropped in while OpenGG is running becomes available immediately.
3. **Fallback** — `I18n.t(key)` looks up a dotted key (e.g. `"nav.home"`) in the current language's catalog, falls back to English if missing, and falls back to the raw key string as a last resort.

All user-facing QML text must come from `I18n.t("key.path")`, backed by both `en.json` and `ar.json`.

## Adding a New Language

### Step 1: Copy the English Template

```bash
cd qt-shell/locales
cp en.json es.json           # for Spanish, for example
```

### Step 2: Set Metadata

Open your new file (e.g., `es.json`) and update the `_meta` object at the top:

```json
{
  "_meta": {
    "name": "Español",
    "dir": "ltr"
  },
  "welcome": {
    "title": "Bienvenido a OpenGG"
  },
  ...
}
```

- **`name`** — Display name shown in Settings → Language
- **`dir`** — Text direction: `"ltr"` for left-to-right (most languages) or `"rtl"` for right-to-left (Arabic, Hebrew, etc.)

**Note on RTL:** unlike some i18n setups, selecting an RTL-tagged language does **not** automatically flip the UI to RTL layout. `I18n` exposes `rtl` (the effective mirroring flag `LayoutMirroring.enabled` binds to) and a separate `rtlOverride` property — the user opts into RTL layout via a toggle in Settings, independent of which language is active. This preserves a language choice like Arabic-with-LTR-layout across restarts if that's what the user picked.

### Step 3: Translate All String Values

Keep all keys exactly as they are in `en.json`. Translate only the values:

```json
{
  "_meta": { "name": "Español", "dir": "ltr" },
  "dashboard": {
    "title": "Panel de Control",
    "recordingStatus": "Estado de grabación",
    "gsr": {
      "startRecording": "Iniciar grabación",
      "stopRecording": "Detener grabación"
    }
  }
}
```

### Step 4: Test Without a Rebuild

During development, drop your file directly into `qt-shell/locales/` (the dev-mode locales directory) and use Settings → "reload locales" instead of restarting:

```bash
cp qt-shell/locales/es.json qt-shell/locales/es.json  # already there if you followed Step 1
./dev.sh ui
```

Launch OpenGG, change the language in **Settings → Language**, and your new locale will appear.

### Step 5: Check for Missing Keys

There is currently **no automated locale-parity checker** for the Qt UI (the old Vue app had `npm run check:locales`; it has not been ported). Until one exists, verify key parity by hand — e.g. `diff <(jq -r 'keys' en.json) <(jq -r 'keys' es.json)` per nested object, or a quick script comparing flattened dotted keys. If you're up for porting a proper checker, that's a great first contribution (see `qt-shell/src/i18n.rs`'s `flatten()` helper for the dotted-key logic to mirror).

### Step 6: Commit and Open a PR

```bash
git add qt-shell/locales/es.json
git commit -m "i18n: Add Spanish (es) translation"
git push origin i18n/spanish
# Open a pull request
```

## What NOT to Translate

- **Variable placeholders** — e.g., `{filename}`, `{duration}` — leave these unchanged
- **File paths** — e.g., `~/.config/opengg/`
- **Technical terms** — when there's no standard translation in your language, keep the English term. Examples: "PipeWire", "NVENC", "VAAPI"
- **Button labels with keyboard shortcuts** — translate the label, keep the shortcut syntax (e.g., `"Save (Ctrl+S)"` → `"Guardar (Ctrl+S)"` in Spanish)

## RTL (Right-to-Left) Notes

If you're translating to Arabic, Hebrew, or another RTL language, set `"dir": "rtl"` in `_meta` so it's available as an option — but remember from Step 2 that the UI only actually mirrors when the user separately enables the RTL toggle in Settings; `dir` alone doesn't switch layout.

When translating to an RTL language:
- Test with the RTL toggle both on and off in Settings
- Check that text displays correctly and doesn't overflow
- Verify that icons flip appropriately where the QML uses `LayoutMirroring`

## Locale File Structure Example

```json
{
  "_meta": {
    "name": "Español",
    "dir": "ltr"
  },
  "common": {
    "yes": "Sí",
    "no": "No",
    "ok": "Aceptar",
    "cancel": "Cancelar"
  },
  "dashboard": {
    "title": "Panel de Control",
    "recordingStatus": "Estado de grabación: {status}"
  },
  "settings": {
    "general": {
      "title": "General",
      "startup": "Iniciar con el sistema",
      "clipDirectory": "Directorio de clips"
    }
  }
}
```

`{status}`-style placeholders are substituted by the calling QML, not by `I18n.t()` itself — check how the existing key is consumed in QML before assuming automatic interpolation.

## Testing Your Translation

1. **In dev mode:**
   ```bash
   ./dev.sh ui
   ```
   Your locale file in `qt-shell/locales/` is picked up automatically (or use Settings → reload locales after adding it).

2. **Check for missing keys:** see Step 5 above — no automated tool yet, verify by hand.

3. **Visual inspection:**
   - Check that text doesn't overflow UI elements
   - Verify that buttons and form fields display correctly
   - For RTL languages: toggle RTL on in Settings and ensure direction is correct and elements align properly

## CI Requirements

There is currently no CI job enforcing locale key parity for the Qt UI (unlike the old Vue app's `npm run check:locales` gate). Until that's ported:
- No hardcoded strings in QML — all text must use `I18n.t()`
- Manually verify no new keys in `en.json` without a matching addition to `ar.json` (and vice versa)

## Questions?

If you're unsure about a translation or terminology, feel free to open a draft PR or discussion. We're happy to help refine translations with native speakers.

---

Thank you for helping OpenGG reach more users! 🌍

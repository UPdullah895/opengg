# Translating OpenGG

OpenGG uses vue-i18n for internationalization. Currently, English (`en`) and Arabic (`ar` with full RTL support) are available. We'd love help adding more languages!

## How i18n Works

1. **Build time** — OpenGG scans `frontend/src/locales/*.json` and bundles all locales
2. **Runtime** — Users can drop additional locale files in `~/.config/opengg/locales/` (discovered automatically on startup)
3. **Fallback** — If a string is missing from the user's chosen locale, OpenGG falls back to English

All user-facing strings in templates use `t('key.path')` or `tm('key.path')` backed by both `en.json` and `ar.json`.

## Adding a New Language

### Step 1: Copy the English Template

```bash
cd frontend/src/locales
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

- **`name`** — Display name shown in Settings → Language dropdown
- **`dir`** — Text direction: `"ltr"` for left-to-right (most languages) or `"rtl"` for right-to-left (Arabic, Hebrew, etc.)

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

During development, test your translation immediately without rebuilding:

```bash
mkdir -p ~/.config/opengg/locales
cp frontend/src/locales/es.json ~/.config/opengg/locales/es.json
```

Launch OpenGG and change the language in **Settings → Language**. Your new locale will appear in the dropdown.

### Step 5: Run the Locale Checker

Verify that your new locale has all the same keys as English (no missing or extra keys):

```bash
cd frontend
npm run check:locales
```

This script (`scripts/check-locales.mjs`) ensures key parity. If keys are missing, it will tell you which ones.

### Step 6: Commit and Open a PR

Once your translation is complete and `check:locales` passes:

```bash
git add frontend/src/locales/es.json
git commit -m "i18n: Add Spanish (es) translation"
git push origin i18n/spanish
# Open a pull request
```

CI will run `npm run check:locales` automatically. Your PR must have zero locale drift to merge.

## What NOT to Translate

- **Variable placeholders** — e.g., `{filename}`, `{duration}` — leave these unchanged
- **File paths** — e.g., `~/.config/opengg/`
- **Technical terms** — when there's no standard translation in your language, keep the English term. Examples: "Tauri", "PipeWire", "NVENC", "VAAPI"
- **Button labels with keyboard shortcuts** — translate the label, keep the shortcut syntax (e.g., `"Save (Ctrl+S)"` → `"Guardar (Ctrl+S)"` in Spanish)
- **Numeric formats** — localization of numbers, dates, etc. is handled by vue-i18n's `n()` and `d()` functions in the template

## RTL (Right-to-Left) Notes

If you're translating to Arabic, Hebrew, or another RTL language, set `"dir": "rtl"` in `_meta`. OpenGG automatically applies `dir="rtl"` and reverses CSS layouts for RTL locales.

Current RTL implementation:
- Template uses `dir` binding: `<html :dir="$i18n.locale === 'ar' ? 'rtl' : 'ltr'">`
- Flexbox components flip automatically with `direction: rtl` in parent
- Text alignment reverses (e.g., `text-right` in LTR becomes `text-left` in RTL)
- Padding/margins swap sides where needed

When translating to an RTL language:
- Test both LTR and RTL modes in Settings
- Check that text displays correctly and doesn't overflow
- Verify that icons and images flip appropriately (some should flip, some shouldn't)

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
    "recordingStatus": "Estado de grabación: {status}",
    "clipsSaved": "clips guardados | 1 clip guardado | {count} clips guardados"
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

## Pluralization

OpenGG uses vue-i18n's standard pluralization syntax. In English:

```json
{
  "clips": "clips guardados | 1 clip guardado | {count} clips guardados"
}
```

In your template:
```ts
const { tm } = useI18n()
tm('clips', count)  // automatically selects the right plural form
```

Different languages have different plural rules. Consult vue-i18n docs for your language's rules.

## Testing Your Translation

1. **In dev mode:**
   ```bash
   ./dev.sh ui
   ```
   Drop your locale file in `~/.config/opengg/locales/` and change the language in Settings.

2. **Check for missing keys:**
   ```bash
   cd frontend && npm run check:locales
   ```

3. **Visual inspection:**
   - Check that text doesn't overflow UI elements
   - Verify that buttons and form fields display correctly
   - For RTL languages: ensure direction is correct and elements align properly

## CI Requirements

Pull requests must pass:

- `npm run check:locales` — ensures all keys match `en.json`
- No hardcoded strings in templates — all text must use `t()` or `tm()`
- No new keys in `en.json` without updates to `ar.json` (and vice versa)

## Questions?

If you're unsure about a translation or terminology, feel free to open a draft PR or discussion. We're happy to help refine translations with native speakers.

---

Thank you for helping OpenGG reach more users! 🌍

# Design Tokens Reference

This document describes the design token system used in the OpenGG frontend.

## How Theming Works

OpenGG's theme system operates through CSS custom properties (CSS variables) that are loaded and applied at runtime:

1. **Theme Loading**: On app startup, `App.vue` calls `loadTheme()` from `src/utils/theme.ts`.
2. **Source**: The theme is loaded from `~/.config/opengg/theme.json` via the Tauri `load_theme` command. If the file doesn't exist, a minimal default theme is returned from the backend.
3. **Application**: The theme object contains two sections:
   - `colors`: Color tokens (e.g., `"--accent": "#E94560"`)
   - `layout`: Layout and spacing tokens (e.g., `"--clips-grid-cols": "4"`)
4. **DOM Injection**: The `applyTheme()` function sets each token as a CSS custom property on the `:root` element via `root.style.setProperty(key, value)`.
5. **Default Fallbacks**: Hard-coded `:root` defaults are defined in `App.vue` `<style>` block (lines 510–533) and are overridden by values from `theme.json`.

### Theme File Location

```
~/.config/opengg/theme.json
```

### Default Theme Structure

The default theme (when no file exists) is defined in `frontend/src-tauri/src/commands.rs` line 2562:

```json
{
  "colors": {
    "--accent": "#E94560"
  },
  "layout": {
    "--clips-grid-cols": "4"
  }
}
```

Hard-coded defaults in `App.vue` `:root` block provide a complete fallback palette.

## Design Tokens

### Color Tokens

| Token | Purpose | Typical Use | Default (Dark) | Default (Light) | Example |
|-------|---------|-------------|---|---|---------|
| `--accent` | Primary interactive color | Buttons, active states, highlights | `#E94560` | `#E94560` | `SelectField.vue:86` |
| `--accent-rgb` | RGB components of accent (no `#`) | Calculated opacity overlays | `233, 69, 96` | `233, 69, 96` | `App.vue:521` |
| `--bg-surface` | Main app background | App root, page backgrounds | `#0f1117` | `#f0f2f5` | `App.vue:511` |
| `--bg-card` | Card/panel background | Clip cards, panels, modals | `#171923` | `#ffffff` | `ClipCard.vue` |
| `--bg-deep` | Darkest background | Disabled states, recessed areas | `#0d0f14` | `#e4e7ec` | `DspControls.vue:117` |
| `--bg-hover` | Hover state background | Interactive elements on hover | `#1e2030` | `#dde1e8` | `Sidebar.vue:114` |
| `--bg-input` | Input field background | Text inputs, select fields | `#131520` | `#f8f9fb` | `SelectField.vue:84` |
| `--border` | Border/divider color | All borders, separator lines | `#2a2d3a` | `#d1d5db` | `App.vue:516` |
| `--text` | Primary text color | All body text, headings | `#e2e8f0` | `#111827` | `App.vue:555` |
| `--text-sec` | Secondary text color | Less prominent labels, hints | `#94a3b8` | `#4b5563` | `SelectField.vue:96` |
| `--text-muted` | Muted/tertiary text | Disabled text, metadata, small labels | `#4a5568` | `#9ca3af` | `GraphicEQ.vue:424` |
| `--danger` | Error/destructive action color | Delete buttons, error states | `#dc2626` | `#dc2626` | `App.vue:524` |
| `--success` | Success/confirmation color | Success states, positive indicators | `#10b981` | `#10b981` | `App.vue:525` |
| `--purple` | Secondary accent (accent-color alternative) | Complementary highlights | `#a855f7` | `#a855f7` | `App.vue:526` |

### Composite Color Tokens

| Token | Definition | Purpose | Example |
|-------|-----------|---------|---------|
| `--color-accent-alpha-10` | `color-mix(in srgb, var(--accent) 10%, transparent)` | Subtle accent backgrounds | `App.vue:522` |
| `--color-accent-alpha-50` | `color-mix(in srgb, var(--accent) 50%, transparent)` | Semi-transparent accent overlays | `App.vue:523` |

### Layout & Spacing Tokens

| Token | Purpose | Default | Example |
|-------|---------|---------|---------|
| `--radius` | Standard border-radius | `6px` | `SelectField.vue:85` |
| `--radius-lg` | Large border-radius | `10px` | `App.vue:528` |
| `--titlebar-h` | Titlebar height | `40px` | `Titlebar.vue:80` |
| `--sidebar-w` | Sidebar width | `200px` | `Sidebar.vue:102` |
| `--clips-grid-cols` | Clips page grid columns | `4` | Theme-configurable only |

### Dynamic/Component-Local Tokens

These tokens are set dynamically by components at runtime via inline `:style` bindings. They are **not** defined globally and must have fallback defaults.

| Token | Purpose | Default Fallback | Set By | Example |
|-------|---------|------------------|--------|---------|
| `--ch` | Channel color (for DSP sliders) | `var(--accent)` | DspControls.vue on slider | `DspControls.vue:54, 209` |
| `--dz` | Drop zone color (per channel) | None (always set by parent) | DropZone.vue from channel color | `DropZone.vue:283, 389` |
| `--tc` | Track color (timeline) | None (always set by parent) | AdvancedEditor.vue from track definition | `AdvancedEditor.vue:631, 908` |
| `--vol-pct` | Volume percentage (visual fill) | `0%` | VolumeSlider.vue dynamically | Computed via JS |
| `--vs-color` | Volume slider color | None (always set) | VolumeSlider.vue | Inline style binding |

### Font Size Tokens (List/Clip View)

| Token | Purpose | Default | Example |
|-------|---------|---------|---------|
| `--list-font` | Standard list item text | `13px` | ClipCard.vue |
| `--list-meta-font` | Metadata text (dates, etc.) | `11px` | ClipCard.vue |
| `--list-act-font` | Action button text | `12px` | ClipCard.vue |
| `--name-size` | Clip/app name text size | `13px` | ClipCard.vue |
| `--meta-size` | Metadata size | `11px` | ClipCard.vue |

### Padding/Dimension Tokens (List/Clip View)

| Token | Purpose | Default | Example |
|-------|---------|---------|---------|
| `--list-pad` | Standard list padding | `8px` | ClipCard.vue |
| `--list-act-pad-y` | Action element Y padding | `4px` | ClipCard.vue |
| `--list-chip-pad-x` | Chip/pill horizontal padding | `8px` | ClipCard.vue |
| `--list-pill-pad-x` | Pill horizontal padding | `6px` | ClipCard.vue |
| `--list-thumb-w` | Thumbnail width | `160px` | ClipCard.vue |
| `--list-thumb-h` | Thumbnail height | `90px` | ClipCard.vue |
| `--device-img-bg` | Device image background | `var(--bg-surface)` | DevicesPage.vue |

### Semantic Color Fallbacks

Some tokens are used with inline fallbacks for backward compatibility or optional features:

```css
/* Example from DspControls.vue line 209 */
accent-color: var(--ch, var(--accent));

/* Example from DropZone.vue line 389 */
border-color: color-mix(in srgb, var(--dz) 40%, var(--border));
```

## Rules for Contributors

### 1. Always Use Tokens, Never Hardcoded Colors

**Bad:**
```vue
<style scoped>
.my-button { background: #E94560; }  /* ✗ Hardcoded — breaks theming */
</style>
```

**Good:**
```vue
<style scoped>
.my-button { background: var(--accent); }  /* ✓ Uses token */
</style>
```

### 2. Scoped CSS Only

All component styles must use `<style scoped>` to prevent global pollution:

```vue
<style scoped>  <!-- ✓ Required -->
.component { color: var(--text); }
</style>
```

Do NOT use non-scoped `<style>` blocks in components (except App.vue which defines root defaults).

### 3. Use Semantic Tokens for Intent

Choose tokens based on their semantic meaning, not color:

- `--text-sec` for secondary labels (not just any grey)
- `--accent` for primary interactive elements (not any color)
- `--danger` for destructive actions (not a generic red)

### 4. Adding a New Token Properly

If a new token is needed:

1. **Define it in App.vue `:root`** (dark theme defaults, lines 510–533):
   ```css
   :root {
     --my-new-token: #value;
   }
   ```

2. **Define light-mode override** (if applicable, lines 534–545):
   ```css
   html.light {
     --my-new-token: #light-value;
   }
   ```

3. **Document it in this file** with its purpose, default, and at least one usage example.

4. **Use it in component styles** with `var(--my-new-token)`.

5. **Test both dark and light modes** to ensure the new token is visible and readable.

### 5. Dynamic Component-Local Tokens

For tokens set dynamically by a component (like channel colors), follow this pattern:

```vue
<!-- Template -->
<div class="slider" :style="{ '--ch': channelColor }">
  <input type="range" />
</div>

<!-- Style -->
<style scoped>
.slider input {
  accent-color: var(--ch, var(--accent));  /* Fallback to --accent if --ch is not set */
}
</style>
```

## Example Theme JSON

A complete custom theme file at `~/.config/opengg/theme.json`:

```json
{
  "colors": {
    "--accent": "#3B82F6",
    "--bg-surface": "#1f2937",
    "--bg-card": "#374151",
    "--bg-deep": "#1a202c",
    "--bg-hover": "#4b5563",
    "--bg-input": "#2d3748",
    "--border": "#4b5563",
    "--text": "#f3f4f6",
    "--text-sec": "#d1d5db",
    "--text-muted": "#9ca3af",
    "--danger": "#ef4444",
    "--success": "#22c55e",
    "--purple": "#8b5cf6"
  },
  "layout": {
    "--radius": "8px",
    "--radius-lg": "12px",
    "--clips-grid-cols": "5",
    "--titlebar-h": "44px"
  },
  "mode": "dark"
}
```

## Gaps and Inconsistencies

### 1. Font/Dimension Tokens Not in theme.json

Tokens like `--list-font`, `--list-pad`, `--name-size`, etc. are defined only as hard-coded defaults in component styles and are **not** currently exposed in `theme.json` for user customization. They could be added to the `layout` section if clip card styling becomes customizable in the future.

**Location**: ClipCard.vue, AdvancedEditor.vue — search for `var(--list-*)` and `var(--*-size)`.

### 2. Dynamic Tokens Always Set

Tokens `--ch`, `--dz`, `--tc`, `--vol-pct`, `--vs-color` are always set by their parent component at render time via inline `:style` bindings. They are **never** stored in `theme.json` and **should not be** — they are instance-specific, not theme-wide.

### 3. Light Mode Defaults

Light mode color overrides are defined in `App.vue` line 534–545 (via `html.light` selector), not in `theme.json`. Users can override them by providing values in a custom `theme.json` with the same key names.

### 4. Unused Hard-Coded Color

One hardcoded color outside the token system exists:

- `#fff` (white) in `GraphicEQ.vue` line 395 for the toggle thumb — should arguably be `var(--bg-surface)` or a new `--toggle-thumb` token.

**Location**: `GraphicEQ.vue:395` — `.tog-thumb { background: #fff; }`

## Token Inventory

**Total tokens in use across frontend**: 32

- **Core color tokens**: 13 (`--accent`, `--bg-*`, `--border`, `--text*`, `--danger`, `--success`, `--purple`)
- **Composite tokens**: 2 (`--color-accent-alpha-*`)
- **Layout tokens**: 4 (`--radius*`, `--titlebar-h`, `--sidebar-w`)
- **Content/grid tokens**: 1 (`--clips-grid-cols`)
- **Dynamic/local tokens**: 5 (`--ch`, `--dz`, `--tc`, `--vol-pct`, `--vs-color`)
- **Font/dimension tokens (clip view)**: 11 (`--list-*`, `--*-size`)

## Further Reading

- `frontend/src/utils/theme.ts` — Theme loading and application logic
- `frontend/src-tauri/src/commands.rs` line 2556–2574 — Tauri theme I/O commands
- `frontend/src/App.vue` line 509–545 — Default theme definitions
- `frontend/CLAUDE.md` — Frontend-specific development notes

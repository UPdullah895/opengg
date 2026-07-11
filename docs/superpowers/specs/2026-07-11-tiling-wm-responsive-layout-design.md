# Tiling WM Responsive Layout

## Context

OpenGG is used inside tiling window managers (Hyprland, Niri), where the window is typically given roughly half the screen width (~900-960px), not the full 1280px+ the UI was designed at. The app currently has **zero responsive design** — no `@media` queries anywhere in the frontend. The sidebar is a fixed 200px with icon+label, the custom titlebar is a fixed 40px bar with a wordmark/Beta badge plus window controls, and `tauri.conf.json` hard-locks `minWidth` to 960px, leaving no margin once a Hyprland/Niri gap or border eats into the tiled column.

Screenshots from the actual app at ~933px width confirmed this isn't just "a bit cramped" — it's real broken layout in multiple places: the Home dashboard's 4th stat card is clipped in half past the window edge, and the Advanced Editor's top toolbar cuts off the Export button and format badges. Reading the CSS confirmed the root cause in both cases (a CSS Grid blowout and a non-wrapping flex toolbar, respectively), and also confirmed two other suspects — the Clips toolbar and the Mixer header/tab-bar — already handle this fine (`flex-wrap: wrap` already present) and need no change.

The goal: reclaim horizontal space via a global "compact mode" for the app shell (sidebar, titlebar), fix the two confirmed broken layouts (Home, Advanced Editor), and give the window more headroom against tiling WM sizing.

## Design

### 1. `useCompactMode()` composable — new file

`frontend/src/composables/useCompactMode.ts`. Mirrors the existing `useAppVersion.ts` pattern: module-level shared `ref`, not per-component state. Listens to the window `resize` event (debounced ~100ms), exposes:

```ts
export const isCompact = ref(window.innerWidth < 900)
```

No ResizeObserver needed — this cares about the whole window, not a specific element.

### 2. `Sidebar.vue` — auto-collapse to icons

When `isCompact`, adds a `.compact` class: width 200px → ~60px, `.nav-item span` (text label) hidden, each `.nav-item` gets a `:title` attribute for a native hover tooltip, `.sidebar-tip` footer hidden (no room, low value). Click targets and navigation behavior are unchanged — presentation only.

### 3. `Titlebar.vue` — shrink, keep window controls

When `isCompact`, adds a `.compact` class: the "OpenGG" wordmark and Beta badge are hidden (logo icon alone remains, still draggable). Minimize/maximize/close buttons are unchanged — they stay available regardless of window width, since not everyone has WM keybinds for window management.

### 4. `MixerPage.vue` — let channel strips wrap

`.strips-row` gets `flex-wrap: wrap` (pure CSS, no JS/breakpoint — flexbox already stops shrinking strips at `min-width: 110px` and wraps naturally once a row can't fit them all). `min-height: 55vh` on `.strips-row` changes to `min-height: unset` so a wrapped multi-row layout isn't forced artificially tall. `.mixer-hdr`/`.tab-bar` already wrap correctly today — no change needed there.

### 5. `HomePage.vue` — fix the stat-card grid blowout

`.grid { grid-template-columns: repeat(4, 1fr) }` → `repeat(auto-fit, minmax(200px, 1fr))`. This is the confirmed root cause of the clipped 4th card: `1fr` tracks don't shrink below their content's intrinsic size by default, so the row overflows instead of shrinking. `auto-fit`/`minmax` lets cards wrap to a 2×2 layout automatically once 4-across no longer fits at a readable width.

### 6. `AdvancedEditor.vue` — declutter the toolbar, not wrap it

The `.bar` toolbar (back button, name field, game selector, codec/resolution/fps tags, undo-count tag, Export button) has no wrap and no shrink fallback once the name field and game dropdown hit their minimums. Rather than wrapping (would look messy for a toolbar), hide the codec/resolution/fps tags when `isCompact` — they're already duplicated in the Info side panel below (`.info-row` shows Resolution, Codec, Audio track count), so nothing is actually lost. Keep the undo-count tag (unique, not shown elsewhere) and the Export button always visible.

### 7. `tauri.conf.json` — lower the window floor

`minWidth: 960` → `720`. At a ~900-960px tiling target, 960 leaves zero margin — any Hyprland/Niri gap or border pushes the window under its own declared minimum, fighting the WM's tiling math. 720 gives real headroom and edges toward supporting a true 1/3-screen tile in the future.

### Explicitly out of scope

- Clips page toolbar and Mixer header/tab-bar: verified already correct (`flex-wrap: wrap` present), no changes.
- Any other page not named above (Devices, Settings): not reported as broken, not audited — revisit only if a real problem shows up.
- WM detection (Hyprland vs Niri vs floating): deliberately not done. The fix responds to window width generically, which is simpler, more robust (no reliable cross-compositor WM-sniffing API exists for a Wayland client), and helps in any narrow-width scenario, not just these two compositors.

## Testing / verification

CSS/layout change — verified by driving the actual Tauri window (not a browser preview: `Titlebar.vue` calls `getCurrentWindow()` synchronously and throws outside a real Tauri context, so this app cannot render in a plain browser). Verification plan:
1. Launch via `tauri dev`, resize the window across the 900px threshold, confirm sidebar collapses/expands and titlebar wordmark hides/shows cleanly with no layout jump.
2. At ~930px width: confirm Home's stat cards wrap to 2×2 with no clipping, Mixer strips wrap instead of over-squeezing, Advanced Editor's toolbar keeps Back/Name/Export visible with format tags hidden.
3. Check both LTR and Arabic (RTL) — Sidebar uses logical properties (`border-inline-end`), compact mode must not break that.

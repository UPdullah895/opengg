# Tiling WM Responsive Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make OpenGG usable when tiled to ~900-960px in Hyprland/Niri by collapsing the sidebar/titlebar in a new compact mode, fixing two confirmed broken layouts (Home's stat-card grid, the Advanced Editor's toolbar), letting the Mixer's channel strips wrap, and giving the window more size headroom.

**Architecture:** A single module-level `isCompact` ref (in a new `useCompactMode.ts` composable) is the one source of truth for "window is narrow right now." `Sidebar.vue` and `Titlebar.vue` read it directly and toggle a CSS class. `MixerPage.vue` and `HomePage.vue` need no JS at all — pure CSS (`flex-wrap`, `grid-template-columns` with `minmax`). `AdvancedEditor.vue` reads `isCompact` to conditionally hide three toolbar tags.

**Tech Stack:** Vue 3 `<script setup>`, Vitest (existing project test runner, no new dependencies), plain CSS (no `@media`/`@container` queries — the codebase has none today and this plan doesn't introduce the pattern either).

## Global Constraints

- Compact-mode breakpoint: window width `< 900px` (from spec, based on user's ~900-960px tiling target).
- `tauri.conf.json` `minWidth`: `960` → `720` (from spec).
- No new npm dependencies (no jsdom, no `@vue/test-utils` — neither is installed; this plan works within that).
- Follow the existing module-level-singleton composable pattern (see `frontend/src/composables/useViewMode.ts` and `useAppVersion.ts`) — a plain exported `ref`, not a function returning fresh state per call.
- Follow the existing `typeof window !== 'undefined'` SSR/test-safety guard already used in `App.vue`'s `isOverlay` (line 44-45) for any top-level `window` access in a composable.

---

### Task 1: `useCompactMode` composable + unit test + wire into App.vue

**Files:**
- Create: `frontend/src/composables/useCompactMode.ts`
- Create: `frontend/src/composables/useCompactMode.test.ts`
- Modify: `frontend/src/App.vue:19` (add import), `frontend/src/App.vue:238` (add init call in `onMounted`)

**Interfaces:**
- Produces: `isCompact` (Vue `Ref<boolean>`), `computeIsCompact(width: number): boolean` (pure function), `initCompactModeListener(): void` — all exported from `frontend/src/composables/useCompactMode.ts`. Tasks 2, 3, and 6 import `isCompact` from this file.

- [ ] **Step 1: Write the failing test**

Create `frontend/src/composables/useCompactMode.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { computeIsCompact, COMPACT_BREAKPOINT } from './useCompactMode'

describe('computeIsCompact', () => {
  it('is compact below the breakpoint', () => {
    expect(computeIsCompact(899)).toBe(true)
  })

  it('is not compact at or above the breakpoint', () => {
    expect(computeIsCompact(900)).toBe(false)
    expect(computeIsCompact(1280)).toBe(false)
  })

  it('breakpoint constant matches the documented tiling target', () => {
    expect(COMPACT_BREAKPOINT).toBe(900)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && npx vitest run src/composables/useCompactMode.test.ts`
Expected: FAIL — `Cannot find module './useCompactMode'` (file doesn't exist yet).

- [ ] **Step 3: Write the composable**

Create `frontend/src/composables/useCompactMode.ts`:

```ts
import { ref } from 'vue'

/** Window width (px) below which the app switches to compact mode. */
export const COMPACT_BREAKPOINT = 900

/** Pure threshold check — kept separate from the window/ref wiring so it's testable without a DOM. */
export function computeIsCompact(width: number): boolean {
  return width < COMPACT_BREAKPOINT
}

/** Module-level singleton — shared across all consumers (Sidebar, Titlebar, AdvancedEditor). */
export const isCompact = ref(
  typeof window !== 'undefined' ? computeIsCompact(window.innerWidth) : false
)

let _initialized = false
let _resizeTimer: ReturnType<typeof setTimeout> | undefined

/** Starts the resize listener. Safe to call multiple times — only wires up once. Call once from App.vue. */
export function initCompactModeListener() {
  if (_initialized || typeof window === 'undefined') return
  _initialized = true
  window.addEventListener('resize', () => {
    clearTimeout(_resizeTimer)
    _resizeTimer = setTimeout(() => {
      isCompact.value = computeIsCompact(window.innerWidth)
    }, 100)
  })
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && npx vitest run src/composables/useCompactMode.test.ts`
Expected: PASS — 3 tests passed.

- [ ] **Step 5: Wire the listener into App.vue**

In `frontend/src/App.vue`, add the import alongside the other composable/util imports (near line 19, after `import { installAudioUnlocker } from './utils/audio'`):

```ts
import { installAudioUnlocker } from './utils/audio'
import { initCompactModeListener } from './composables/useCompactMode'
```

Then in the `onMounted` block (around line 238), add the call right after `installAudioUnlocker()`:

```ts
  installAudioUnlocker()
  initCompactModeListener()
  loadUserLocales()
```

- [ ] **Step 6: Run the full frontend test suite to confirm nothing else broke**

Run: `cd frontend && npx vitest run`
Expected: PASS — all existing tests (`persistence.test.ts`, `replay.test.ts`) plus the 3 new ones pass.

- [ ] **Step 7: Typecheck**

Run: `cd frontend && npx vue-tsc --noEmit`
Expected: no errors.

- [ ] **Step 8: Commit**

```bash
cd /home/updullah/Documents/Projects/opengg
git add frontend/src/composables/useCompactMode.ts frontend/src/composables/useCompactMode.test.ts frontend/src/App.vue
git commit -m "feat(ui): add useCompactMode composable for narrow-window layout"
```

---

### Task 2: `Sidebar.vue` — auto-collapse to icons in compact mode

**Files:**
- Modify: `frontend/src/components/Sidebar.vue`

**Interfaces:**
- Consumes: `isCompact` (`Ref<boolean>`) from `frontend/src/composables/useCompactMode.ts` (Task 1).

- [ ] **Step 1: Import `isCompact` and bind the `:title` tooltip + compact class**

In `frontend/src/components/Sidebar.vue`, the `<script setup>` block currently reads:

```ts
<script setup lang="ts">
import { useI18n } from 'vue-i18n'
defineProps<{ active: string }>()
const emit = defineEmits<{ navigate: [page: string] }>()
const { t } = useI18n()
```

Change to:

```ts
<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { isCompact } from '../composables/useCompactMode'
defineProps<{ active: string }>()
const emit = defineEmits<{ navigate: [page: string] }>()
const { t } = useI18n()
```

- [ ] **Step 2: Update the template**

The current template (`Sidebar.vue` lines 16-44):

```html
<template>
  <nav class="sidebar">
    <div class="nav-items">
      <button
        v-for="item in items"
        :key="item.id"
        class="nav-item"
        :class="{ active: active === item.id }"
        :data-tour="`nav-${item.id}`"
        @click="emit('navigate', item.id)"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path :d="item.icon" />
        </svg>
        <span>{{ t(`nav.${item.id}`) }}</span>
      </button>
    </div>
    <div class="sidebar-footer">
      <div class="sidebar-tip">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="tip-icon">
          <circle cx="12" cy="12" r="10"/>
          <line x1="12" y1="16" x2="12" y2="12"/>
          <line x1="12" y1="8" x2="12.01" y2="8"/>
        </svg>
        <span class="tip-text"><span class="tip-text-inner">{{ t('sidebar.tip') }}</span></span>
      </div>
    </div>
  </nav>
</template>
```

Replace with:

```html
<template>
  <nav class="sidebar" :class="{ compact: isCompact }">
    <div class="nav-items">
      <button
        v-for="item in items"
        :key="item.id"
        class="nav-item"
        :class="{ active: active === item.id }"
        :data-tour="`nav-${item.id}`"
        :title="isCompact ? t(`nav.${item.id}`) : undefined"
        @click="emit('navigate', item.id)"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path :d="item.icon" />
        </svg>
        <span v-if="!isCompact">{{ t(`nav.${item.id}`) }}</span>
      </button>
    </div>
    <div v-if="!isCompact" class="sidebar-footer">
      <div class="sidebar-tip">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="tip-icon">
          <circle cx="12" cy="12" r="10"/>
          <line x1="12" y1="16" x2="12" y2="12"/>
          <line x1="12" y1="8" x2="12.01" y2="8"/>
        </svg>
        <span class="tip-text"><span class="tip-text-inner">{{ t('sidebar.tip') }}</span></span>
      </div>
    </div>
  </nav>
</template>
```

- [ ] **Step 3: Add compact-mode CSS**

In the `<style scoped>` block, right after the existing `.sidebar { ... }` rule (line 47-55), add:

```css
.sidebar.compact {
  width: 60px;
  min-width: 60px;
}
.sidebar.compact .nav-item {
  justify-content: center;
  padding: 10px;
}
```

- [ ] **Step 4: Typecheck**

Run: `cd frontend && npx vue-tsc --noEmit`
Expected: no errors.

- [ ] **Step 5: Manual verification**

Run: `cd frontend && npx vite` (dev server), then in a real Tauri window (`cd frontend/src-tauri && cargo tauri dev` if a full check is wanted — the plain Vite dev server alone will throw in `Titlebar.vue`'s `getCurrentWindow()` call outside Tauri, so this component's own visual check must happen through `tauri dev`, not the browser).
Expected: at window width ≥900px, sidebar shows icon+label as before. Resizing below 900px, sidebar shrinks to a 60px icon rail with no labels; hovering an icon shows its name as a native tooltip; clicking still navigates correctly.

- [ ] **Step 6: Commit**

```bash
cd /home/updullah/Documents/Projects/opengg
git add frontend/src/components/Sidebar.vue
git commit -m "feat(ui): collapse sidebar to icon rail in compact mode"
```

---

### Task 3: `Titlebar.vue` — hide wordmark/badge in compact mode

**Files:**
- Modify: `frontend/src/components/Titlebar.vue`

**Interfaces:**
- Consumes: `isCompact` (`Ref<boolean>`) from `frontend/src/composables/useCompactMode.ts` (Task 1).

- [ ] **Step 1: Import `isCompact`**

In `frontend/src/components/Titlebar.vue`, the `<script setup>` currently starts:

```ts
<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { onMounted, onBeforeUnmount } from 'vue'
```

Change to:

```ts
<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { onMounted, onBeforeUnmount } from 'vue'
import { isCompact } from '../composables/useCompactMode'
```

- [ ] **Step 2: Update the template to hide the wordmark + badge**

Current (lines 39-49):

```html
  <header class="titlebar">
    <div class="titlebar-drag" data-tauri-drag-region>
      <div class="titlebar-left" data-tauri-drag-region>
        <svg class="logo" viewBox="0 0 512 512" width="22" height="22" data-tauri-drag-region style="fill-rule:evenodd;clip-rule:evenodd;">
          <g transform="matrix(2.778643,0,0,2.778643,-447.380743,-285.942888)">
            <path d="M291.671,187.47L332.636,187.47L340.205,195.039L280.739,254.504L259.3,232.814L271.068,221.046L280.97,230.948L307.949,203.97L291.671,203.97L291.671,187.47ZM285.002,195.039L225.537,254.504L166.072,195.039L225.537,135.573L247.24,157.276L235.885,168.632L226.383,159.129L190.473,195.039L226.383,230.948L241.783,215.548L221.274,195.039L280.739,135.573L311.599,166.433L300.04,177.991L280.97,159.129L265.032,175.068L285.002,195.039ZM253.289,211.821L270.072,195.039L253.289,178.256L236.507,195.039L253.289,211.821Z" fill="var(--accent)"/>
          </g>
        </svg>
        <span class="title" data-tauri-drag-region>
          OpenGG
          <span class="beta-badge">Beta</span>
        </span>
      </div>
```

Change to (wrap the `<span class="title">` in `v-if="!isCompact"`, add `compact` class to `.titlebar`):

```html
  <header class="titlebar" :class="{ compact: isCompact }">
    <div class="titlebar-drag" data-tauri-drag-region>
      <div class="titlebar-left" data-tauri-drag-region>
        <svg class="logo" viewBox="0 0 512 512" width="22" height="22" data-tauri-drag-region style="fill-rule:evenodd;clip-rule:evenodd;">
          <g transform="matrix(2.778643,0,0,2.778643,-447.380743,-285.942888)">
            <path d="M291.671,187.47L332.636,187.47L340.205,195.039L280.739,254.504L259.3,232.814L271.068,221.046L280.97,230.948L307.949,203.97L291.671,203.97L291.671,187.47ZM285.002,195.039L225.537,254.504L166.072,195.039L225.537,135.573L247.24,157.276L235.885,168.632L226.383,159.129L190.473,195.039L226.383,230.948L241.783,215.548L221.274,195.039L280.739,135.573L311.599,166.433L300.04,177.991L280.97,159.129L265.032,175.068L285.002,195.039ZM253.289,211.821L270.072,195.039L253.289,178.256L236.507,195.039L253.289,211.821Z" fill="var(--accent)"/>
          </g>
        </svg>
        <span v-if="!isCompact" class="title" data-tauri-drag-region>
          OpenGG
          <span class="beta-badge">Beta</span>
        </span>
      </div>
```

(The `titlebar-btns` block below — minimize/maximize/close — is unchanged; per spec, window controls stay visible regardless of width.)

- [ ] **Step 3: Typecheck**

Run: `cd frontend && npx vue-tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Manual verification (via `tauri dev`, not browser — see Task 2 Step 5 note)**

Expected: at ≥900px, "OpenGG" wordmark + Beta badge visible next to the logo. Below 900px, only the logo icon remains in the top-left; minimize/maximize/close buttons unaffected; window is still draggable via the logo area.

- [ ] **Step 5: Commit**

```bash
cd /home/updullah/Documents/Projects/opengg
git add frontend/src/components/Titlebar.vue
git commit -m "feat(ui): hide titlebar wordmark/badge in compact mode"
```

---

### Task 4: `MixerPage.vue` — let channel strips wrap instead of over-squeezing

**Files:**
- Modify: `frontend/src/pages/MixerPage.vue`

**Interfaces:** None (pure CSS, no JS/composable dependency).

- [ ] **Step 1: Update `.strips-row` CSS**

Current (`MixerPage.vue` lines 376-382):

```css
/* ★ FIX 3: strips-row fills vertical space, min 55vh for tall faders */
.strips-row {
  display: flex;
  gap: 10px;
  align-items: stretch;   /* ← all cols same height */
  flex: 1;
  min-height: 55vh;       /* ← guarantees tall faders */
}
```

Change to:

```css
/* ★ FIX 3: strips-row fills vertical space, min 55vh for tall faders (single row only —
   once strips wrap to multiple rows there's no single "row" to stretch tall, so drop the min-height) */
.strips-row {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: stretch;   /* ← all cols same height within a row */
  flex: 1;
}
.strips-row:not(.wrapped) {
  min-height: 55vh;
}
```

- [ ] **Step 2: Add wrap detection so `min-height: 55vh` only applies to a genuine single row**

In `frontend/src/pages/MixerPage.vue`, the `<script setup>` block currently starts (line 15):

```ts
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
```

Change to:

```ts
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { isCompact } from '../composables/useCompactMode'
```

In the template, line 262 currently reads:

```html
      <div v-else class="strips-row" data-tour="mixer-channels">
```

Change to:

```html
      <div v-else class="strips-row" :class="{ wrapped: isCompact }" data-tour="mixer-channels">
```

(Using `isCompact` as the "likely wrapped" signal is a reasonable approximation — it's the same width signal already driving every other compact-mode change, and avoids adding a second, redundant width-measurement mechanism just for this one class.)

- [ ] **Step 3: Typecheck**

Run: `cd frontend && npx vue-tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Manual verification (via `tauri dev`)**

Expected: with 6 channel strips open at ≥900px, all fit in one row at a comfortable width (unchanged from today). Below 900px with several strips open, strips wrap onto a second row rather than each strip squeezing toward the 110px floor; the page becomes taller and scrolls vertically instead.

- [ ] **Step 5: Commit**

```bash
cd /home/updullah/Documents/Projects/opengg
git add frontend/src/pages/MixerPage.vue
git commit -m "fix(mixer): wrap channel strips onto multiple rows instead of over-squeezing"
```

---

### Task 5: `HomePage.vue` — fix the stat-card CSS Grid blowout

**Files:**
- Modify: `frontend/src/pages/HomePage.vue`

**Interfaces:** None (pure CSS).

- [ ] **Step 1: Update `.grid` CSS**

Current (`HomePage.vue` line 454):

```css
.grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 14px; }
```

Change to:

```css
.grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 14px; }
```

- [ ] **Step 2: Manual verification (via `tauri dev`)**

Expected: at ≥900px (enough room for 4 × 200px+gaps), the 4 stat cards (Audio Mixer, Recorder, Clips, Devices) stay in one row, unchanged from today. Below that, they wrap to a 2×2 grid with no card clipped or cut off at the window edge.

- [ ] **Step 3: Commit**

```bash
cd /home/updullah/Documents/Projects/opengg
git add frontend/src/pages/HomePage.vue
git commit -m "fix(home): stop stat-card grid from overflowing the window at narrow widths"
```

---

### Task 6: `AdvancedEditor.vue` — hide duplicated format tags in compact mode

**Files:**
- Modify: `frontend/src/components/AdvancedEditor.vue`

**Interfaces:**
- Consumes: `isCompact` (`Ref<boolean>`) from `frontend/src/composables/useCompactMode.ts` (Task 1).

- [ ] **Step 1: Import `isCompact`**

In `frontend/src/components/AdvancedEditor.vue`, add to the existing import block (near line 2-16, alongside `import { usePersistenceStore } from '../stores/persistence'`):

```ts
import { isCompact } from '../composables/useCompactMode'
```

- [ ] **Step 2: Gate the redundant format tags**

Current (`AdvancedEditor.vue`, inside the `.bar` toolbar):

```html
    <div style="flex:1"></div>
    <span v-if="info" class="tag">{{ info.video_codec }}</span>
    <span v-if="info" class="tag">{{ info.width }}×{{ info.height }}</span>
    <span v-if="info" class="tag">{{ info.fps.toFixed(0) }}fps</span>
    <span v-if="undoStack.length" class="tag">↩{{ undoStack.length }}</span>
    <button class="btn accent" :disabled="exporting" @click="openExport">{{ t('editor.exportClip') }}</button>
```

Change to (codec/resolution/fps tags hidden when compact — they're already shown in the Info side panel's `.info-row`s for Resolution/Codec; undo-count and Export stay visible always):

```html
    <div style="flex:1"></div>
    <span v-if="info && !isCompact" class="tag">{{ info.video_codec }}</span>
    <span v-if="info && !isCompact" class="tag">{{ info.width }}×{{ info.height }}</span>
    <span v-if="info && !isCompact" class="tag">{{ info.fps.toFixed(0) }}fps</span>
    <span v-if="undoStack.length" class="tag">↩{{ undoStack.length }}</span>
    <button class="btn accent" :disabled="exporting" @click="openExport">{{ t('editor.exportClip') }}</button>
```

- [ ] **Step 3: Typecheck**

Run: `cd frontend && npx vue-tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Manual verification (via `tauri dev`)**

Expected: at ≥900px, toolbar shows codec/resolution/fps tags as before. Below 900px, those three tags disappear from the toolbar (still visible in the Info side panel if open); Back button, name field, game selector, undo-count, and Export button remain visible and usable with no clipping.

- [ ] **Step 5: Commit**

```bash
cd /home/updullah/Documents/Projects/opengg
git add frontend/src/components/AdvancedEditor.vue
git commit -m "fix(editor): hide redundant format tags from toolbar in compact mode"
```

---

### Task 7: `tauri.conf.json` — lower the window's minWidth floor

**Files:**
- Modify: `frontend/src-tauri/tauri.conf.json`

**Interfaces:** None.

- [ ] **Step 1: Change `minWidth`**

Current (`tauri.conf.json`, in the window config block):

```json
        "minWidth": 960,
```

Change to:

```json
        "minWidth": 720,
```

- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -c "import json; json.load(open('frontend/src-tauri/tauri.conf.json')); print('OK')"`
Expected: `OK`

- [ ] **Step 3: Commit**

```bash
cd /home/updullah/Documents/Projects/opengg
git add frontend/src-tauri/tauri.conf.json
git commit -m "fix(window): lower minWidth 960->720 for tiling WM headroom"
```

---

### Task 8: Full manual verification pass

**Files:** None (verification only, no code changes).

- [ ] **Step 1: Build check**

Run: `cd frontend && npx vue-tsc --noEmit && npx vite build`
Expected: build succeeds with no errors.

- [ ] **Step 2: Rust check (tauri.conf.json is read by the Tauri backend at build time)**

Run: `cd frontend/src-tauri && cargo check`
Expected: succeeds (no Rust code changed this plan, this just confirms the workspace still builds after the config edit).

- [ ] **Step 3: Full interactive pass via `tauri dev`**

Run: `cd frontend/src-tauri && cargo tauri dev`

With the real Tauri window open, resize it across 900px width and confirm, per the design spec's testing section:
- Sidebar collapses to icon rail below 900px, expands back above it, no layout jump, tooltips work, navigation still works.
- Titlebar wordmark/badge hide below 900px, window buttons (minimize/maximize/close) always present and functional.
- Home dashboard: stat cards wrap to 2×2 below the width where 4-across no longer fits comfortably; no card clipped.
- Mixer: with several channel strips open, strips wrap to a second row instead of squeezing past their 110px floor.
- Advanced Editor: format tags (codec/resolution/fps) disappear from the toolbar below 900px; Back/Name/Export/undo-count stay visible and usable; open a clip and confirm Export still works.
- Switch the app language to Arabic (RTL) in Settings, repeat the sidebar check — confirm `border-inline-end` styling and icon-rail layout still look correct mirrored.
- Confirm the window can actually be resized down to 720px now (previously blocked at 960px).

- [ ] **Step 4: No commit for this task** — it's verification only. If any check fails, return to the relevant task above, fix, and re-commit there.

---

## Self-Review

**Spec coverage:** All 7 numbered items in the spec (composable, Sidebar, Titlebar, Mixer wrap, Home grid, Editor declutter, minWidth) map 1:1 to Tasks 1-7. The spec's "Testing / verification" section maps to Task 8. The spec's "out of scope" section (Clips, Mixer header, other pages, WM detection) has no corresponding task, correctly — nothing to build there.

**Placeholder scan:** No TBD/TODO. Every step has literal, complete code or an exact runnable command with expected output.

**Type consistency:** `isCompact` (the `Ref<boolean>`), `computeIsCompact(width: number): boolean`, `COMPACT_BREAKPOINT` (number), and `initCompactModeListener(): void` are defined once in Task 1 and referenced with matching names/signatures in Tasks 2, 3, 4, and 6 — no renaming drift.

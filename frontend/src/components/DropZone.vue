<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAudioStore } from '../stores/audio'
import { usePersistenceStore } from '../stores/persistence'
import { activeMenuId, nextMenuId, openMenu, closeMenu } from '../composables/useActiveMenu'
import VolumeSlider from './VolumeSlider.vue'

const { t } = useI18n()
const props = defineProps<{
  channel: string
  color: string
  apps: { id: number; name: string; binary: string; volume?: number; locked?: boolean }[]
}>()

const audio = useAudioStore()
const persist = usePersistenceStore()

// ── App-box layout (global settings, shared across all channel boxes) ──
const ROW_H = 26   // px per chip row
const ROW_GAP = 2  // px gap between rows
const BOX_PAD = 8  // px total vertical padding of the box
const appBoxCount = computed(() => Math.max(1, Math.min(12, persist.state.settings.appBoxCount ?? 3)))
const appBoxPerRow = computed<1 | 2>(() => (persist.state.settings.appBoxPerRow === 2 ? 2 : 1))
const boxHeight = computed(() => {
  const rows = Math.ceil(appBoxCount.value / appBoxPerRow.value)
  return rows * ROW_H + (rows - 1) * ROW_GAP + BOX_PAD
})
const boxStyle = computed(() => ({
  '--dz': props.color,
  '--box-h': boxHeight.value + 'px',
  '--box-cols': String(appBoxPerRow.value),
}))

// (App-box layout count/columns are controlled by a single global gear in the Mixer
//  top toolbar — see MixerPage.vue. This component only consumes the global settings.)
const hovering = ref(false)
const isDragging = computed(() => audio.draggedApp !== null)
const contextMenuOpen = ref(false)
// Unique owner id for the single-open-menu coordinator (only one DropZone menu at a time).
const menuOwnerId = nextMenuId()
const contextMenuPos = ref({ x: 0, y: 0 })
const contextApp = ref<{ id: number; name: string; binary: string; locked?: boolean } | null>(null)
const menuRef = ref<HTMLElement | null>(null)
const menuSize = ref({ w: 0, h: 0 })

// ★ Pointer-based custom drag state
interface DragState {
  appId: number
  appName: string
  startX: number
  startY: number
  isDragging: boolean
  ghostElement: HTMLElement | null
}
const dragState = ref<DragState | null>(null)
const dragOverChannel = ref<string | null>(null)

const ROUTE_TARGETS = ['Master', 'Game', 'Chat', 'Media', 'Aux']
const DRAG_THRESHOLD_PX = 6

// ── Boundary-aware menu positioning ──
const menuStyle = computed(() => {
  let x = contextMenuPos.value.x
  let y = contextMenuPos.value.y
  const w = menuSize.value.w || 120
  const h = menuSize.value.h || 160
  if (x + w > window.innerWidth)  x = window.innerWidth - w - 8
  if (y + h > window.innerHeight) y = y - h - 8
  x = Math.max(4, x)
  y = Math.max(4, y)
  return { left: x + 'px', top: y + 'px' }
})

// ★ Create ghost element as a Vue-friendly HTML div (styled pill)
function createGhost(appName: string): HTMLElement {
  const ghost = document.createElement('div')
  ghost.className = 'dz-ghost'
  ghost.style.cssText = `
    position: fixed;
    z-index: 10000;
    pointer-events: none;
    background: rgba(30, 30, 40, 0.92);
    border: 1px solid ${props.color}60;
    border-radius: 6px;
    padding: 6px 10px;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: #e2e8f0;
    white-space: nowrap;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
    opacity: 0.95;
    transform: scale(1.05);
  `

  const dot = document.createElement('span')
  dot.style.cssText = `
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: ${props.color};
    flex-shrink: 0;
  `

  const name = document.createElement('span')
  name.textContent = appName
  name.style.cssText = 'overflow: hidden; text-overflow: ellipsis; max-width: 140px;'

  ghost.appendChild(dot)
  ghost.appendChild(name)
  document.body.appendChild(ghost)

  return ghost
}

// ★ Cleanup ghost and drag state
function cleanupDrag() {
  if (dragState.value?.ghostElement) {
    dragState.value.ghostElement.remove()
  }
  dragState.value = null
  dragOverChannel.value = null
  // Ensure store is also cleared (safety belt)
  if (audio.draggedApp) {
    audio.endDrag()
  }
}

// ★ Hit-test to find channel container under pointer
function findChannelAtPoint(x: number, y: number): string | null {
  const elements = document.elementsFromPoint(x, y)
  for (const el of elements) {
    const channel = (el as HTMLElement).getAttribute('data-channel')
    if (channel) return channel
  }
  return null
}

// ★ Pointer down: record candidate drag (threshold-gated to allow clicks)
function onChipPointerDown(e: PointerEvent, app: { id: number; name: string; binary: string; locked?: boolean }) {
  if (app.locked) return

  // Don't start drag if interacting with volume button
  if ((e.target as HTMLElement).closest('.dz-vol-btn')) return

  closeVolumeMenu()
  dragState.value = {
    appId: app.id,
    appName: app.name,
    startX: e.clientX,
    startY: e.clientY,
    isDragging: false,
    ghostElement: null as HTMLElement | null,
  } as DragState

  // Capture pointer to track moves even outside the element
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
}

// ★ Pointer move: check threshold, initiate drag if crossed
function onChipPointerMove(e: PointerEvent) {
  if (!dragState.value || dragState.value.isDragging) {
    // Already dragging — move ghost
    if (dragState.value?.ghostElement) {
      dragState.value.ghostElement.style.left = (e.clientX + 8) + 'px'
      dragState.value.ghostElement.style.top = (e.clientY + 8) + 'px'
    }

    // Hit-test for hover target
    const targetChannel = findChannelAtPoint(e.clientX, e.clientY)
    dragOverChannel.value = targetChannel
    return
  }

  // Not yet dragging — check if threshold crossed
  const dx = e.clientX - dragState.value.startX
  const dy = e.clientY - dragState.value.startY
  const dist = Math.sqrt(dx * dx + dy * dy)

  if (dist < DRAG_THRESHOLD_PX) return

  // ★ Threshold crossed — initiate drag
  dragState.value.isDragging = true
  audio.startDrag({
    id: dragState.value.appId,
    name: dragState.value.appName,
    binary: '',
    channel: props.channel,
    icon: '',
  })

  // Create and position ghost
  const ghost = createGhost(dragState.value.appName)
  dragState.value.ghostElement = ghost
  ghost.style.left = (e.clientX + 8) + 'px'
  ghost.style.top = (e.clientY + 8) + 'px'
}

// ★ Pointer up: finalize drop or cancel
function onChipPointerUp(e: PointerEvent) {
  if (!dragState.value?.isDragging) {
    // Never started dragging — allow click fallback
    cleanupDrag()
    return
  }

  // Was dragging — check if over a drop target
  const targetChannel = findChannelAtPoint(e.clientX, e.clientY)
  if (targetChannel && targetChannel !== props.channel) {
    audio.dropOnChannelById(dragState.value.appId, targetChannel)
  }

  cleanupDrag()

  // Release capture
  try {
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId)
  } catch {}
}

// ★ Pointer cancel: cleanup without routing
function onChipPointerCancel() {
  cleanupDrag()
}

// ── Click-to-route fallback (preserved; works if pointer move threshold not crossed) ──
function onChipClick(app: { id: number; name: string; binary: string; locked?: boolean }) {
  if (app.locked || dragState.value?.isDragging) return // locked apps cannot be routed; don't click while dragging
  // If this app is already selected, deselect it
  if (audio.selectedAppForClickRoute?.id === app.id) {
    audio.deselectApp()
    return
  }
  // Select this app for click-routing
  audio.selectAppForRoute({ id: app.id, name: app.name, binary: app.binary, channel: props.channel, icon: '' })
}

function onZoneClick() {
  // If an app is selected for click-routing, drop it here
  if (audio.selectedAppForClickRoute) {
    audio.dropOnChannelById(audio.selectedAppForClickRoute.id, props.channel)
  }
}

// ── Context menu for quick routing ──
function onChipContextMenu(e: MouseEvent, app: { id: number; name: string; binary: string; locked?: boolean }) {
  e.preventDefault()
  contextApp.value = app
  contextMenuPos.value = { x: e.clientX, y: e.clientY }
  contextMenuOpen.value = true
  // Become the sole open menu — any other DropZone's menu closes via the activeMenuId watch.
  openMenu(menuOwnerId)
  menuSize.value = { w: 0, h: 0 }
  nextTick(() => requestAnimationFrame(() => {
    const el = menuRef.value
    if (el) menuSize.value = { w: el.offsetWidth, h: el.offsetHeight }
  }))
}

function routeViaMenu(targetChannel: string) {
  if (!contextApp.value) return
  const appId = contextApp.value.id
  contextMenuOpen.value = false
  contextApp.value = null
  closeMenu(menuOwnerId)
  if (targetChannel === props.channel) return // already there
  audio.dropOnChannelById(appId, targetChannel)
}

function closeContextMenu() {
  contextMenuOpen.value = false
  contextApp.value = null
  closeMenu(menuOwnerId)
}

// Single-open enforcement: when another DropZone becomes the active menu, close ours.
watch(activeMenuId, (id) => {
  if (id !== menuOwnerId && contextMenuOpen.value) {
    contextMenuOpen.value = false
    contextApp.value = null
  }
})

// Close context menu / volume menu when clicking anywhere outside
function onWindowClick(e: MouseEvent) {
  const menu = document.querySelector('.dz-ctx-menu')
  if (menu && !menu.contains(e.target as Node)) {
    closeContextMenu()
  }
  const volMenu = document.querySelector('.dz-vol-menu')
  if (volMenu && !volMenu.contains(e.target as Node)) {
    closeVolumeMenu()
  }
}

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    closeVolumeMenu()
    closeContextMenu()
  }
}

onMounted(() => {
  window.addEventListener('click', onWindowClick)
  window.addEventListener('keydown', onKeyDown)
})
onBeforeUnmount(() => {
  if (autoHideTimer) clearTimeout(autoHideTimer)
  window.removeEventListener('click', onWindowClick)
  window.removeEventListener('keydown', onKeyDown)
  // Clean up any active drag ghost
  cleanupDrag()
})

// ── Per-app volume ──
const hoverAppId = ref<number | null>(null)

function onVolumeChange(appId: number, val: number) {
  audio.setAppVolume(appId, val)
}

// ── Volume popover menu ──
const volumeMenuOpen = ref(false)
const volumeMenuAppId = ref<number | null>(null)
const volMenuRef = ref<HTMLElement | null>(null)
const volMenuPos = ref({ x: 0, y: 0 })
const volMenuSize = ref({ w: 0, h: 0 })

// Live computed so slider / mute button react to store updates immediately
const volumeMenuApp = computed(() => {
  if (volumeMenuAppId.value === null) return null
  return props.apps.find(a => a.id === volumeMenuAppId.value) || null
})

// Live view of the right-clicked app so the context-menu volume slider reflects
// the app's real PipeWire stream volume (independent of the channel volume).
const contextAppLive = computed(() => {
  if (!contextApp.value) return null
  return props.apps.find(a => a.id === contextApp.value!.id) || null
})

const volMenuStyle = computed(() => {
  let x = volMenuPos.value.x
  let y = volMenuPos.value.y
  const w = volMenuSize.value.w || 160
  const h = volMenuSize.value.h || 110
  if (x + w > window.innerWidth)  x = window.innerWidth - w - 8
  if (y + h > window.innerHeight) y = y - h - 8
  x = Math.max(4, x)
  y = Math.max(4, y)
  return { left: x + 'px', top: y + 'px' }
})

function openVolumeMenu(app: { id: number; name: string; volume?: number }, btnEl: EventTarget | null) {
  const rect = (btnEl as HTMLElement)?.getBoundingClientRect?.()
  if (!rect) return
  volumeMenuAppId.value = app.id
  volMenuPos.value = { x: rect.left, y: rect.bottom + 4 }
  volumeMenuOpen.value = true
  volMenuSize.value = { w: 0, h: 0 }
  nextTick(() => requestAnimationFrame(() => {
    const el = volMenuRef.value
    if (el) volMenuSize.value = { w: el.offsetWidth, h: el.offsetHeight }
  }))
}

function closeVolumeMenu() {
  volumeMenuOpen.value = false
  volumeMenuAppId.value = null
}

// ── Auto-hide timer ──
let autoHideTimer: ReturnType<typeof setTimeout> | null = null
const AUTO_HIDE_DELAY_MS = 1500

function onChipMouseEnter(appId: number) {
  hoverAppId.value = appId
  if (autoHideTimer) {
    clearTimeout(autoHideTimer)
    autoHideTimer = null
  }
}

function onChipMouseLeave() {
  hoverAppId.value = null
  autoHideTimer = setTimeout(() => {
    closeVolumeMenu()
  }, AUTO_HIDE_DELAY_MS)
}

function cancelAutoHide() {
  if (autoHideTimer) {
    clearTimeout(autoHideTimer)
    autoHideTimer = null
  }
}

// ── Mute / unmute ──
const preMuteVolumes = new Map<number, number>()

function toggleMute(appId: number, currentVol: number) {
  if (currentVol === 0) {
    const restored = preMuteVolumes.get(appId) || 50
    onVolumeChange(appId, restored)
    preMuteVolumes.delete(appId)
  } else {
    preMuteVolumes.set(appId, currentVol)
    onVolumeChange(appId, 0)
  }
}
</script>

<template>
  <div class="dropzone" :class="{ hovering: hovering || dragOverChannel === props.channel, 'show-target': isDragging || audio.selectedAppForClickRoute, 'click-target': audio.selectedAppForClickRoute }" :style="boxStyle"
    :data-channel="props.channel"
    @click="onZoneClick">
    <div v-if="apps.length > 0" class="dz-apps">
      <div v-for="app in apps" :key="app.id" class="dz-chip"
        :class="{
          'dz-chip--selected': audio.selectedAppForClickRoute?.id === app.id,
          'dz-chip--locked': app.locked,
          'dz-chip--muted': !app.locked && app.volume === 0,
          'dz-chip--dragging': dragState?.appId === app.id && dragState?.isDragging
        }"
        @pointerdown="onChipPointerDown($event, app)"
        @pointermove="onChipPointerMove($event)"
        @pointerup="onChipPointerUp($event)"
        @pointercancel="onChipPointerCancel"
        @click.stop="onChipClick(app)"
        @contextmenu="onChipContextMenu($event, app)"
        @mouseenter="onChipMouseEnter(app.id)" @mouseleave="onChipMouseLeave()"
        style="touch-action: none;">
        <template v-if="!app.locked && typeof app.volume === 'number'">
          <button
            v-if="hoverAppId === app.id"
            class="dz-vol-btn"
            :title="`Volume: ${app.volume}%`"
            @click.stop="openVolumeMenu(app, $event.currentTarget)"
          >
            <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5">
              <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
              <path v-if="app.volume > 0" d="M15.54 8.46a5 5 0 0 1 0 7.07"/>
              <path v-if="app.volume > 50" d="M19.07 4.93a10 10 0 0 1 0 14.14"/>
            </svg>
          </button>
          <span v-else class="dz-dot" :style="{ background: color }"></span>
        </template>
        <span v-else class="dz-dot" :style="{ background: color }"></span>
        <span class="dz-name">{{ app.name }}</span>
        <span v-if="app.locked" class="dz-lock" title="System process — cannot be routed">🔒</span>
      </div>
    </div>
    <div v-else class="dz-empty" :class="{ active: isDragging || audio.selectedAppForClickRoute }">
      <span v-if="isDragging || audio.selectedAppForClickRoute">{{ t('devices.dropHere') }}</span>
      <span v-else>{{ t('devices.noApps') }}</span>
    </div>
  </div>

  <!-- Context menu for quick routing -->
  <Teleport to="body">
    <div v-if="contextMenuOpen" ref="menuRef" class="dz-ctx-menu" :style="menuStyle" @mouseenter="cancelAutoHide" @mouseleave="onChipMouseLeave()">
      <div class="dz-ctx-title">{{ contextApp?.name }}</div>
      <template v-if="contextApp?.locked">
        <div class="dz-ctx-locked">
          <span class="dz-ctx-lock-icon">🔒</span>
          <span>System Process — Locked</span>
        </div>
      </template>
      <template v-else>
        <!-- Per-app volume: controls THIS app's own PipeWire stream, independent of the
             channel volume. -->
        <div v-if="contextAppLive && typeof contextAppLive.volume === 'number'" class="dz-ctx-vol" @click.stop>
          <div class="dz-ctx-vol-row">
            <span class="dz-ctx-vol-label">{{ t('devices.appVolume') }}</span>
            <span class="dz-ctx-vol-pct" :class="{ 'dz-ctx-vol-pct--muted': contextAppLive.volume === 0 }">{{ contextAppLive.volume }}%</span>
          </div>
          <VolumeSlider
            :model-value="contextAppLive.volume"
            :color="color"
            :show-value="false"
            @update:model-value="onVolumeChange(contextApp!.id, $event)"
          />
          <button
            class="dz-ctx-vol-mute"
            :class="{ 'dz-ctx-vol-mute--active': contextAppLive.volume === 0 }"
            @click="toggleMute(contextApp!.id, contextAppLive.volume || 0)"
          >
            {{ contextAppLive.volume === 0 ? t('devices.unmute') : t('devices.mute') }}
          </button>
        </div>
        <div class="dz-ctx-sep"></div>
        <div class="dz-ctx-subtitle">{{ t('devices.routeTo') }}</div>
        <button v-for="target in ROUTE_TARGETS" :key="target"
          class="dz-ctx-item"
          :class="{ 'dz-ctx-item--active': target === props.channel }"
          :disabled="target === props.channel"
          @click="routeViaMenu(target)">
          {{ target }}
        </button>
      </template>
    </div>
  </Teleport>

  <!-- Volume popover menu -->
  <Teleport to="body">
    <div v-if="volumeMenuOpen" ref="volMenuRef" class="dz-vol-menu" :style="{ ...volMenuStyle, '--dz': color }" @mouseenter="cancelAutoHide" @mouseleave="onChipMouseLeave()">
      <div class="dz-vol-header">
        <span class="dz-vol-title">
          <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5">
            <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
            <path v-if="(volumeMenuApp?.volume || 0) > 0" d="M15.54 8.46a5 5 0 0 1 0 7.07"/>
            <path v-if="(volumeMenuApp?.volume || 0) > 50" d="M19.07 4.93a10 10 0 0 1 0 14.14"/>
          </svg>
          <span class="dz-vol-label">{{ volumeMenuApp?.name }}</span>
        </span>
        <span class="dz-vol-pct" :class="{ 'dz-vol-pct--muted': volumeMenuApp?.volume === 0 }">
          {{ volumeMenuApp?.volume }}%
        </span>
      </div>
      <VolumeSlider
        :model-value="volumeMenuApp?.volume || 0"
        :color="color"
        :show-value="false"
        @update:model-value="onVolumeChange(volumeMenuApp!.id, $event)"
      />
      <button
        class="dz-vol-mute"
        :class="{ 'dz-vol-mute--active': volumeMenuApp?.volume === 0 }"
        @click="toggleMute(volumeMenuApp!.id, volumeMenuApp!.volume || 0)"
      >
        <svg v-if="volumeMenuApp?.volume === 0" viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2.5">
          <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
          <line x1="23" y1="9" x2="17" y2="15"/>
          <line x1="17" y1="9" x2="23" y2="15"/>
        </svg>
        <svg v-else viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2.5">
          <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
        </svg>
        {{ volumeMenuApp?.volume === 0 ? 'Unmute' : 'Mute' }}
      </button>
    </div>
  </Teleport>
</template>

<style scoped>
.dropzone { width: 100%; height: var(--box-h, 70px); overflow-y: auto; border: 1px dashed var(--border); border-radius: 6px; padding: 4px; display: flex; flex-direction: column; gap: 2px; transition: border-color .15s, background .15s; scrollbar-width: thin; scrollbar-color: var(--border) transparent; overscroll-behavior: contain; position: relative; }

.show-target { border-color: color-mix(in srgb, var(--dz) 40%, var(--border)); background: color-mix(in srgb, var(--dz) 3%, transparent); }
.click-target { cursor: pointer; border-color: color-mix(in srgb, var(--dz) 60%, var(--border)); background: color-mix(in srgb, var(--dz) 6%, transparent); }
.hovering { border-color: var(--dz) !important; background: color-mix(in srgb, var(--dz) 10%, transparent) !important; border-style: solid !important; box-shadow: 0 0 12px color-mix(in srgb, var(--dz) 20%, transparent); }
/* ★ FIX: expand drop target during active drag */
.dropzone--drag-active {
  overflow-y: visible !important;
  height: auto !important;
  min-height: 70px;
}
.dropzone--drag-active::after {
  content: '';
  position: absolute;
  inset: -16px;
  z-index: -1;
}
.dz-apps { display: grid; grid-template-columns: repeat(var(--box-cols, 1), minmax(0, 1fr)); gap: 2px; align-content: start; }
.dz-chip {
  display: flex; align-items: center; gap: 5px; padding: 6px 8px; background: var(--bg-deep); border-radius: 4px; font-size: 11px; color: var(--text-sec); cursor: grab; overflow: hidden; transition: background .1s, box-shadow .15s, opacity .1s;
  /* ★ FIX: prevent text selection from hijacking drag gesture */
  user-select: none; -webkit-user-select: none;
  pointer-events: auto;
}
.dz-chip:hover { background: var(--bg-hover); }
.dz-chip:active { cursor: grabbing; opacity: .6; }
/* ★ Dragging state: dim the source chip */
.dz-chip--dragging {
  opacity: 0.4;
  background: var(--bg-deep);
}
.dz-chip--selected {
  box-shadow: 0 0 0 1.5px var(--dz), inset 0 0 0 1px var(--dz);
  background: color-mix(in srgb, var(--dz) 12%, var(--bg-deep));
}
.dz-dot { width: 5px; height: 5px; border-radius: 50%; flex-shrink: 0; }
.dz-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; min-width: 0; }
.dz-empty { display: flex; align-items: center; justify-content: center; font-size: 11px; color: var(--text-muted); opacity: .6; flex: 1; }
.dz-empty.active { color: var(--dz); opacity: 1; font-weight: 600; }

/* Volume icon button (replaces dot on hover) */
.dz-vol-btn {
  display: flex; align-items: center; justify-content: center;
  width: 16px; height: 16px; padding: 0; margin: 0;
  background: transparent; border: none; border-radius: 3px;
  color: var(--text-sec); cursor: pointer; flex-shrink: 0;
  transition: color .1s, background .1s;
}
.dz-vol-btn:hover {
  color: var(--text);
  background: var(--bg-hover);
}
.dz-vol-btn svg {
  display: block;
}

/* Muted chips */
.dz-chip--muted {
  opacity: 0.4;
}
.dz-chip--muted .dz-name {
  color: var(--text-muted);
}

/* Volume popover menu */
.dz-vol-menu {
  position: fixed; z-index: 9998;
  background: var(--bg-card); border: 1px solid var(--border);
  border-top: 2px solid var(--dz); border-radius: 8px;
  padding: 10px; min-width: 160px;
  box-shadow: 0 8px 24px rgba(0,0,0,.35);
  backdrop-filter: blur(8px);
  display: flex; flex-direction: column; gap: 8px;
}
.dz-vol-header {
  display: flex; align-items: center; justify-content: space-between;
  font-size: 11px;
}
.dz-vol-title {
  display: flex; align-items: center; gap: 5px;
  color: var(--text-sec);
}
.dz-vol-title svg {
  display: block; flex-shrink: 0;
}
.dz-vol-label {
  font-weight: 600;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100px;
}
.dz-vol-pct {
  color: var(--dz); font-weight: 700; font-size: 10px;
  font-variant-numeric: tabular-nums;
}
.dz-vol-pct--muted {
  color: #E94560;
}
.dz-vol-mute {
  display: flex; align-items: center; justify-content: center; gap: 5px;
  padding: 5px 8px; border-radius: 5px; border: 1px solid var(--border);
  background: transparent; color: var(--text-sec); font-size: 10px;
  cursor: pointer; text-align: center; transition: all .15s;
}
.dz-vol-mute:hover {
  background: var(--bg-hover); color: var(--text);
}
.dz-vol-mute--active {
  background: rgba(233, 69, 96, 0.12);
  color: #E94560; border-color: #E94560;
}
.dz-vol-mute svg {
  display: block; flex-shrink: 0;
}

/* Context menu */
.dz-ctx-menu {
  position: fixed; z-index: 9999;
  background: var(--bg-card); border: 1px solid var(--border); border-radius: 8px;
  padding: 4px; min-width: 120px;
  box-shadow: 0 8px 24px rgba(0,0,0,.35);
  display: flex; flex-direction: column; gap: 1px;
}
.dz-ctx-title {
  padding: 5px 8px; font-size: 10px; font-weight: 700; color: var(--text-muted);
  text-transform: uppercase; letter-spacing: .5px; border-bottom: 1px solid var(--border);
  margin-bottom: 2px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.dz-ctx-item {
  padding: 5px 8px; border-radius: 4px; border: none; background: transparent;
  color: var(--text-sec); font-size: 11px; text-align: left; cursor: pointer;
  transition: background .1s;
}
.dz-ctx-item:hover:not(:disabled) { background: var(--bg-hover); color: var(--text); }
.dz-ctx-item:disabled { opacity: .4; cursor: default; }
.dz-ctx-item--active { color: var(--dz); font-weight: 600; }

/* Per-app volume block inside the context menu */
.dz-ctx-menu { min-width: 170px; }
.dz-ctx-vol { padding: 6px 8px 8px; display: flex; flex-direction: column; gap: 6px; }
.dz-ctx-vol-row { display: flex; align-items: center; justify-content: space-between; }
.dz-ctx-vol-label { font-size: 10px; font-weight: 700; color: var(--text-muted); text-transform: uppercase; letter-spacing: .5px; }
.dz-ctx-vol-pct { font-size: 11px; font-weight: 700; color: var(--text-sec); font-variant-numeric: tabular-nums; }
.dz-ctx-vol-pct--muted { color: var(--danger); }
.dz-ctx-vol-mute {
  padding: 4px 8px; border-radius: 4px; border: 1px solid var(--border); background: transparent;
  color: var(--text-sec); font-size: 10px; font-weight: 600; cursor: pointer; transition: all .1s;
}
.dz-ctx-vol-mute:hover { background: var(--bg-hover); color: var(--text); }
.dz-ctx-vol-mute--active { color: var(--danger); border-color: color-mix(in srgb, var(--danger) 40%, var(--border)); }
.dz-ctx-sep { height: 1px; background: var(--border); margin: 2px 0; }
.dz-ctx-subtitle { padding: 3px 8px; font-size: 9px; font-weight: 700; color: var(--text-muted); text-transform: uppercase; letter-spacing: .5px; }

/* Locked chips */
.dz-chip--locked {
  opacity: 0.45;
  cursor: not-allowed;
  filter: grayscale(0.6);
}
.dz-chip--locked .dz-dot { opacity: 0.4; }
.dz-lock {
  font-size: 10px;
  margin-left: 2px;
  opacity: 0.7;
  flex-shrink: 0;
}

/* Locked context menu state */
.dz-ctx-locked {
  padding: 8px;
  font-size: 11px;
  color: var(--text-muted);
  text-align: center;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
}
.dz-ctx-lock-icon { font-size: 12px; opacity: 0.7; }
</style>

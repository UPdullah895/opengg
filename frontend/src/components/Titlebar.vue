<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { onMounted, onBeforeUnmount } from 'vue'

const win = getCurrentWindow()

async function minimize() {
  try { await win.minimize() } catch (e) { console.warn('minimize:', e) }
}
async function toggleMaximize() {
  try { await win.toggleMaximize() } catch (e) { console.warn('maximize:', e) }
}
// ★ Epic 4: Close = hide to background
async function close() {
  try { await win.hide() } catch (e) { console.warn('hide:', e) }
}
// Real quit via Ctrl+Q
async function quit() {
  try { await invoke('quit_app') } catch { window.close() }
}
function onGlobalKey(e: KeyboardEvent) {
  if (e.code === 'KeyQ' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); quit() }
}
onMounted(() => document.addEventListener('keydown', onGlobalKey))
onBeforeUnmount(() => document.removeEventListener('keydown', onGlobalKey))
</script>

<template>
  <!--
    RESIZE MARGIN: The outer <header> does NOT have data-tauri-drag-region.
    The inner .titlebar-drag has it, positioned 5px inset from top/left/right.
    This leaves a 5px non-draggable border at each edge so the OS resize
    handles can be triggered at the window boundary.

    BUTTONS: .titlebar-btns and .tb-btn have pointer-events: auto so they
    remain clickable while the surrounding drag region is active.
  -->
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
          <span class="version">V0.1.0</span>
          <span class="beta-badge">Beta</span>
        </span>
      </div>

      <div class="titlebar-btns">
        <button @click="minimize" class="tb-btn" title="Minimize">
          <svg viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" fill="none">
            <line x1="5" y1="12" x2="19" y2="12"/>
          </svg>
        </button>
        <button @click="toggleMaximize" class="tb-btn" title="Maximize">
          <svg viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" fill="none">
            <rect x="4" y="4" width="16" height="16" rx="2"/>
          </svg>
        </button>
        <button @click="close" class="tb-btn close" title="Close">
          <svg viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" fill="none">
            <line x1="6" y1="6" x2="18" y2="18"/><line x1="18" y1="6" x2="6" y2="18"/>
          </svg>
        </button>
      </div>
    </div>
  </header>
</template>

<style scoped>
.titlebar {
  position: relative;
  height: var(--titlebar-h);
  background: var(--bg-surface);
  border-bottom: 1px solid var(--border);
  /* No drag-region here — leaves OS resize handles at window edges */
}

/* Inner drag area inset 5px from top/left/right, flush at bottom.
   This creates non-draggable margins where the OS resize cursor activates. */
.titlebar-drag {
  position: absolute;
  inset: 5px 5px 0 5px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 3px 0 9px;
}

.titlebar-left {
  display: flex;
  align-items: center;
  gap: 10px;
  pointer-events: none; /* Let drag pass through text */
}

.logo { color: var(--accent); pointer-events: none; }
.title {
  font-size: 13px; font-weight: 600;
  letter-spacing: .5px;
  pointer-events: none;
  user-select: none;
}
.version { font-size: 11px; color: var(--text-muted); font-weight: 400; }
.beta-badge {
  display: inline-block;
  background: var(--accent);
  color: #fff;
  font-size: 9px; font-weight: 700; letter-spacing: .4px;
  padding: 1px 5px; border-radius: 3px;
  vertical-align: middle; margin-left: 4px;
  text-transform: uppercase;
}

.titlebar-btns {
  display: flex; gap: 2px;
  /* Buttons must receive clicks — not be draggable */
}

.tb-btn {
  width: 36px; height: 30px;
  display: flex; align-items: center; justify-content: center;
  border: none; background: transparent;
  color: var(--text-sec); border-radius: 4px;
  cursor: pointer;
  transition: all .15s;
}
.tb-btn svg { width: 16px; height: 16px; pointer-events: none; }
.tb-btn:hover { background: var(--bg-hover); color: var(--text); }
.tb-btn.close:hover { background: var(--danger); color: #fff; }
</style>

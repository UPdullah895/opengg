<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'

const win = getCurrentWindow()

async function minimize() {
  try { await win.minimize() } catch (e) { console.warn('minimize:', e) }
}

async function toggleMaximize() {
  try { await win.toggleMaximize() } catch (e) { console.warn('maximize:', e) }
}

async function close() {
  try { await win.close() } catch (e) { console.warn('close:', e) }
}
</script>

<template>
  <header class="titlebar" data-tauri-drag-region>
    <!--
      CRITICAL: data-tauri-drag-region on the header makes the entire bar draggable.
      Child buttons/interactive elements automatically exclude themselves.
      The "pointer-events: none" on .titlebar-left prevents text selection from
      interfering with the drag, while buttons get "pointer-events: auto".
    -->
    <div class="titlebar-left" data-tauri-drag-region>
      <svg class="logo" viewBox="0 0 24 24" fill="currentColor" width="18" height="18" data-tauri-drag-region>
        <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"/>
      </svg>
      <span class="title" data-tauri-drag-region>OpenGG <span class="version">v1.0.0</span></span>
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
  </header>
</template>

<style scoped>
.titlebar {
  height: var(--titlebar-h);
  display: flex;
  align-items: center;
  justify-content: space-between;
  background: var(--bg-surface);
  border-bottom: 1px solid var(--border);
  padding: 0 8px 0 14px;
  /* Entire bar is draggable by default */
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

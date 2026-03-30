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
  <header class="titlebar" data-tauri-drag-region>
    <!--
      CRITICAL: data-tauri-drag-region on the header makes the entire bar draggable.
      Child buttons/interactive elements automatically exclude themselves.
      The "pointer-events: none" on .titlebar-left prevents text selection from
      interfering with the drag, while buttons get "pointer-events: auto".
    -->
    <div class="titlebar-left" data-tauri-drag-region>
      <svg class="logo" viewBox="0 0 123 123" width="20" height="20" data-tauri-drag-region>
        <path d="M86.222,18.976c-4.827,4.95-5.209,6.301-7.628,5.162c-39.561-18.626-75.536,23.707-56.984,58.294c5.741,10.703,11.504,13.706,15.258,16.509c3.776,1.94,8.442,5.482,20.418,6.467c8.211.675,28.587-1.969,40.081-22.952c10.888-19.877.581-36.95,1.529-38.803c.053-.104,10.529-10.782,10.806-10.951c2.651-1.619,18.317,30.839,1.439,59.152c-28.925,48.521-94.48,32.954-107.876-10.188c-17.546-56.509,42.565-97.008,87.593-69.61c2.101,1.279-.424,2.876-4.635,6.92Z" fill="var(--accent)"/>
        <path d="M85.462,50.144c-2.511,2.633-4.12,5.512-6.249,3.409c-4.582-4.524-6.525-7.516-8.114-5.975c-1.528,1.481-5.956,5.252-4.795,6.5c6.397,6.875,9.68,7.044,6.553,10.197c-24.099,24.298-24.527,25.408-26.021,24.127c-.326-.28-24.469-24.482-24.866-24.9c-1.134-1.195-.243-1.681,19.711-21.686c3.489-3.498,5.856-6.378,6.841-5.492c9.902,8.906,9.896,9.537,9.162,10.364c-1.197,1.348-4.29,5.457-5.931,3.812c-2.64-2.647-2.919-4.975-5.567-2.306c-12.895,12.997-13.598,12.646-13.718,14.222c-.125,1.646.449,1.465,14.26,14.906c1.89,1.84,4.603-2.235,6.445-4.1c1.149-1.163,1.383-1.446.26-2.606c-7.078-7.314-8.523-7.338-7.241-8.749c1.016-1.119,25.342-25.537,25.437-25.581c.601-.28.817-.297,2.333.899c2.039.177,1.807-.694,20.729-19.575c3.249-3.242,6.011-4.932,4.175-6.751c-2.571-2.547-6.498-5.551-4.299-5.944c6.043-1.08,23.52-3.509,25.568-3.794c4.074-.384,2.252,1.917,1.846,5.233c-2.317,18.923-2.484,18.888-2.593,20.54c-.017.255-.268,4.059-1.474,2.835c-4.648-4.715-5.185-7.11-8.269-4.013c-1.044,1.048-7.731,7.764-12.977,13.167c-.885.912-4.632,4.771-11.206,11.258Z" fill="var(--accent)"/>
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
.beta-badge {
  display: inline-block;
  background: #2563eb;
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

<script setup lang="ts">
import { ref, provide, onMounted } from 'vue'
import Sidebar from './components/Sidebar.vue'
import Titlebar from './components/Titlebar.vue'
import HomePage from './pages/HomePage.vue'
import MixerPage from './pages/MixerPage.vue'
import ClipsPage from './pages/ClipsPage.vue'
import DevicesPage from './pages/DevicesPage.vue'
import SettingsPage from './pages/SettingsPage.vue'
import { usePersistenceStore } from './stores/persistence'
import { loadTheme } from './utils/theme'
import { getMediaPort } from './utils/assets'

const currentPage = ref('home')
const persist = usePersistenceStore()

// ★ Global media server port — provided to all components
const mediaPort = ref(0)
provide('mediaPort', mediaPort)

function navigate(page: string) { currentPage.value = page }

onMounted(async () => {
  await persist.load()
  await loadTheme()
  mediaPort.value = await getMediaPort()
})
</script>

<template>
  <div class="app-layout">
    <Titlebar />
    <div class="app-body">
      <Sidebar :current="currentPage" @navigate="navigate" />
      <main class="content">
        <KeepAlive include="ClipsPage">
          <component :is="{ home: HomePage, mixer: MixerPage, clips: ClipsPage, devices: DevicesPage, settings: SettingsPage }[currentPage]" />
        </KeepAlive>
      </main>
    </div>
  </div>
</template>

<style>
:root {
  --bg-surface: #0f1117;
  --bg-card: #171923;
  --bg-deep: #0d0f14;
  --bg-hover: #1e2030;
  --bg-input: #131520;
  --border: #2a2d3a;
  --text: #e2e8f0;
  --text-sec: #94a3b8;
  --text-muted: #4a5568;
  --accent: #E94560;
  --danger: #dc2626;
  --success: #10b981;
  --radius: 6px;
  --radius-lg: 10px;
  --titlebar-h: 40px;
  --sidebar-w: 200px;
  --clips-grid-cols: 4;
  color-scheme: dark;
}
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  background: var(--bg-surface);
  color: var(--text);
  overflow: hidden;
}
.app-layout { display: flex; flex-direction: column; height: 100vh; }
.app-body { display: flex; flex: 1; overflow: hidden; }
.content { flex: 1; padding: 20px 28px; overflow-y: auto; }
::-webkit-scrollbar { width: 6px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }

/*
 * ★ FULLSCREEN VIDEO FIX for WebKitGTK / Tauri
 *
 * Root causes of black screen:
 *   1. overflow:hidden on body, .app-body, .modal clips the fullscreen element
 *   2. backdrop-filter:blur() on .overlay creates a stacking context trap
 *   3. z-index of titlebar overlays the fullscreen pseudo-class
 *
 * Fix: force fullscreen to highest z-index and override container clipping.
 */

/* The video itself in fullscreen */
video:fullscreen,
video:-webkit-full-screen {
  z-index: 2147483647 !important;
  position: fixed !important;
  inset: 0 !important;
  width: 100vw !important;
  height: 100vh !important;
  object-fit: contain !important;
  background: #000 !important;
}

/* The ::backdrop pseudo-element */
video::backdrop,
video::-webkit-backdrop {
  background: #000 !important;
}

/* When fullscreen: remove ALL container restrictions that clip the video */
body:has(video:fullscreen),
body:has(video:-webkit-full-screen) {
  overflow: visible !important;
}

body:has(video:fullscreen) .app-layout,
body:has(video:fullscreen) .app-body,
body:has(video:fullscreen) .content,
body:has(video:-webkit-full-screen) .app-layout,
body:has(video:-webkit-full-screen) .app-body,
body:has(video:-webkit-full-screen) .content {
  overflow: visible !important;
}

/* Remove backdrop-filter that creates stacking context trap */
body:has(video:fullscreen) .overlay,
body:has(video:-webkit-full-screen) .overlay {
  backdrop-filter: none !important;
  -webkit-backdrop-filter: none !important;
}

/* Hide titlebar in fullscreen */
body:has(video:fullscreen) [data-tauri-drag-region],
body:has(video:-webkit-full-screen) [data-tauri-drag-region] {
  display: none !important;
}

/* General fullscreen z-index */
:fullscreen, :-webkit-full-screen {
  z-index: 2147483647 !important;
}
</style>

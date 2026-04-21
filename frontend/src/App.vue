<script setup lang="ts">
import { ref, computed, provide, onMounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import Sidebar from './components/Sidebar.vue'
import Titlebar from './components/Titlebar.vue'
import HomePage from './pages/HomePage.vue'
import MixerPage from './pages/MixerPage.vue'
import ClipsPage from './pages/ClipsPage.vue'
import DevicesPage from './pages/DevicesPage.vue'
import SettingsPage from './pages/SettingsPage.vue'
import SelectField from './components/SelectField.vue'
import ClipNotification from './components/ClipNotification.vue'
import { usePersistenceStore } from './stores/persistence'
import { useDeviceStore } from './stores/devices'
import type { DeviceInfo } from './stores/devices'
import { loadTheme } from './utils/theme'
import { getMediaPort } from './utils/assets'
import { installAudioUnlocker } from './utils/audio'
import { registerLocale } from './i18n'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { AudioDevice } from './stores/audio'
import ToastContainer from './components/ToastContainer.vue'
import { useToast } from './composables/useToast'

const { t, locale } = useI18n()

// Overlay mode: this window was opened by show_clip_notification (Rust) with ?overlay=1
const isOverlay = typeof window !== 'undefined' &&
  new URLSearchParams(window.location.search).get('overlay') === '1'

// Apply transparent background immediately (before any Vue paint) so the WebView
// chrome doesn't flash the default white/grey before the component renders.
// This replaces the previous non-scoped <style> block that polluted the main app.
if (isOverlay) {
  document.documentElement.style.background = 'transparent'
  document.body.style.background = 'transparent'
  const app = document.getElementById('app')
  if (app) app.style.background = 'transparent'
}

const currentPage = ref('home')
const persist = usePersistenceStore()
const toast = useToast()

// ★ Global media server port — provided to all components
const mediaPort = ref(0)
provide('mediaPort', mediaPort)

function navigate(page: string) { currentPage.value = page }

// ═══ Epic 2: Virtual Audio Onboarding ═══
const showOnboarding = ref(false)
const onboardStep = ref<1 | 2>(1)
const onboardLoading = ref(false)
const onboardMsg = ref('')
const audioDevices = ref<AudioDevice[]>([])
const selectedHeadphones = ref('')
const selectedMic = ref('')

const outputDevices = () => audioDevices.value.filter(d => d.device_type === 'sink' && !d.name.includes('OpenGG'))
const inputDevices  = () => audioDevices.value.filter(d => d.device_type === 'source')

// SelectField-compatible option arrays derived from filtered device lists
const headphoneOptions = computed(() => outputDevices().map(d => ({ value: d.name, label: d.description })))
const micOptions       = computed(() => inputDevices().map(d => ({ value: d.name, label: d.description })))

async function fetchOnboardDevices() {
  try { audioDevices.value = await invoke<AudioDevice[]>('get_audio_devices') } catch {}
  const defOut = outputDevices().find(d => d.is_default)
  const defIn  = inputDevices().find(d => d.is_default)
  if (defOut) selectedHeadphones.value = defOut.name
  if (defIn)  selectedMic.value = defIn.name
}

async function doCreateVirtualAudio() {
  onboardLoading.value = true; onboardMsg.value = ''
  try {
    await invoke('create_virtual_audio')
    await invoke('hydrate_audio_routing')
    await fetchOnboardDevices()
    onboardStep.value = 2
  } catch (e) { onboardMsg.value = t('onboarding.errorCreate', { error: String(e) }) }
  finally { onboardLoading.value = false }
}

async function completeOnboarding() {
  onboardLoading.value = true; onboardMsg.value = ''
  try {
    if (selectedHeadphones.value) {
      const { useAudioStore } = await import('./stores/audio')
      const audio = useAudioStore()
      await audio.setChannelDevice('Master', selectedHeadphones.value)
    }
    await invoke('hydrate_audio_routing')
    showOnboarding.value = false
    onboardStep.value = 1
  } catch (e) { onboardMsg.value = t('onboarding.errorComplete', { error: String(e) }) }
  finally { onboardLoading.value = false }
}

async function registerGlobalShortcuts() {
  try {
    const sc = persist.state.settings.shortcuts
    await invoke('register_global_shortcuts', {
      saveReplay: sc.saveReplay,
      toggleRecording: sc.toggleRecording,
      screenshot: sc.screenshot,
    })
  } catch (e) { console.warn('global shortcuts:', e) }
}

onMounted(async () => {
  await persist.load()
  locale.value = persist.state.settings.language || 'en'
  document.documentElement.dir = persist.state.settings.rtlMode ? 'rtl' : 'ltr'
  await loadTheme()
  mediaPort.value = await getMediaPort()
  installAudioUnlocker()
  loadUserLocales()
  await registerGlobalShortcuts()

  // ── Extension API — expose a restricted invoke bridge and Vue helpers ──
  // Extensions evaluate as IIFEs and can use window.opengg.invoke() to call
  // a whitelist of read-only Tauri commands.  window.Vue gives them access to
  // Vue 3 composition helpers without bundling Vue themselves.
  const _extAllowed = new Set(['get_clip_list', 'get_audio_devices', 'get_recorder_status', 'scan_extensions', 'list_user_locales'])
  ;(window as unknown as Record<string, unknown>).opengg = {
    invoke: (cmd: string, args?: Record<string, unknown>) => {
      if (!_extAllowed.has(cmd)) return Promise.reject(new Error(`opengg: '${cmd}' is not allowed for extensions`))
      return invoke(cmd, args)
    },
    get mediaPort() { return mediaPort.value },
  }
  // Expose Vue composition API on window.Vue so extension IIFEs can use it
  import('vue').then(Vue => { (window as unknown as Record<string, unknown>).Vue = Vue })

  // ── Load all enabled extensions (non-blocking, errors logged per-ext) ──
  if (mediaPort.value) {
    const { useExtensionStore } = await import('./stores/extensions')
    useExtensionStore().loadAllEnabled(mediaPort.value)
  }
  // Listen for global shortcut events fired from Rust
  listen('global-shortcut-save_replay', async () => {
    // If GSR replay buffer is active, save via GSR (handles restart-on-save internally)
    if (persist.state.settings.gsrEnabled) {
      try {
        await invoke('save_gsr_replay', { restartOnSave: persist.state.settings.gsrRestartOnSave })
      } catch (e) { console.warn('save_gsr_replay:', e) }
      return
    }
    try { await invoke('save_replay') } catch (e) { console.warn('save_replay:', e) }
  })
  listen('global-shortcut-toggle_recording', async () => {
    try {
      const { useReplayStore } = await import('./stores/replay')
      const replay = useReplayStore()
      if (replay.status === 'idle') await replay.startReplay()
      else await replay.stopRecorder()
    } catch (e) { console.warn('toggle_recording:', e) }
  })
  listen('global-shortcut-screenshot', async () => {
    try { await invoke('take_screenshot', { outputDir: persist.state.settings.screenshotDirs?.[0] || '' }) } catch (e) { console.warn('screenshot:', e) }
  })

  // ── GSR state sync: keep replay store in sync with backend process state ──
  listen<{ running: boolean }>('gsr-status-changed', async () => {
    const { useReplayStore } = await import('./stores/replay')
    const replay = useReplayStore()
    await replay.fetchStatus()
  })

  // ── Clip saved overlay notification ──
  listen<{ game: string; filename: string; filesize_mb: number; success: boolean }>('clip-saved', async (event) => {
    const style = persist.state.settings.notificationStyle ?? 'auto'
    if (!persist.state.settings.enableClipNotifications || style === 'disabled') return
    try {
      await invoke('show_clip_notification', {
        game:         event.payload.game,
        filename:     event.payload.filename,
        filesizeMb:   event.payload.filesize_mb,
        success:      event.payload.success,
        enabled:      true,
        mode:         style,
        position:     persist.state.settings.notificationPosition ?? 'top-right',
        durationSecs: persist.state.settings.notificationDuration ?? 4,
      })
    } catch (e) { console.warn('show_clip_notification:', e) }
  })

  // ★ Epic 3: Toast on new clip saved (event payload is filepath string)
  listen<string>('clip_added', async (event) => {
    const filepath = event.payload
    try {
      const clip = await invoke<{ game?: string; game_tag?: string } | null>('get_clip_by_path', { filepath })
      const game = clip?.game || clip?.game_tag || 'Unknown Game'
      toast.info(`New Clip Saved! 🎬 ${game}`)
    } catch {
      toast.info('New Clip Saved! 🎬 Unknown Game')
    }
  })

  // ── Global device-changed: keep device store in sync + chatmix → Audio Mixer ──
  const devicesStore = useDeviceStore()
  const prevChatmix = new Map<string, number>()
  listen<string>('device-changed', async (ev) => {
    let updated: DeviceInfo[]
    try { updated = JSON.parse(ev.payload) } catch { return }
    devicesStore.devices = updated
    for (const d of updated) {
      if (d.chatmix === undefined) continue
      const prev = prevChatmix.get(d.id)
      if (prev === undefined) { prevChatmix.set(d.id, d.chatmix); continue }
      if (prev === d.chatmix) continue
      prevChatmix.set(d.id, d.chatmix)
      const n = (d.chatmix - 64) / 64
      const gameVol = n > 0 ? Math.round((1 - n) * 100) : 100
      const chatVol = n < 0 ? Math.round((1 + n) * 100) : 100
      const { useAudioStore } = await import('./stores/audio')
      const audio = useAudioStore()
      audio.setVolume('Game', gameVol)
      audio.setVolume('Chat', chatVol)
    }
  })

  // Restore audio routing immediately after system wake (logind PrepareForSleep → false)
  listen('system-resume', async () => {
    const { useAudioStore } = await import('./stores/audio')
    const audioStore = useAudioStore()
    audioStore.fetchApps()
    audioStore.fetchChannels()
  })

  // ★ Epic 4: Listen for audio reset flow from SettingsPage danger zone
  window.addEventListener('openOnboarding', (e: Event) => {
    const detail = (e as CustomEvent<{ step?: number }>).detail
    onboardStep.value = (detail?.step === 2 ? 2 : 1) as 1 | 2
    if (detail?.step === 2) fetchOnboardDevices()
    onboardMsg.value = ''
    showOnboarding.value = true
  })
})

// Re-register global OS shortcuts whenever the user changes them in Settings
watch(
  () => [
    persist.state.settings.shortcuts.saveReplay,
    persist.state.settings.shortcuts.toggleRecording,
    persist.state.settings.shortcuts.screenshot,
  ],
  () => { if (persist.loaded) registerGlobalShortcuts() },
)

async function loadUserLocales() {
  try {
    const list = await invoke<Array<{ code: string; json_content: string }>>('list_user_locales')
    for (const ul of list) {
      try {
        const data = JSON.parse(ul.json_content)
        const meta = (data._meta ?? {}) as { name?: string; dir?: 'ltr' | 'rtl' }
        registerLocale(ul.code, data, meta.name ?? (ul.code.charAt(0).toUpperCase() + ul.code.slice(1)), meta.dir ?? 'ltr')
      } catch { /* skip malformed JSON */ }
    }
  } catch { /* locales dir not created yet — fine */ }
}
</script>

<template>
  <!-- Overlay notification window — minimal render, no layout chrome -->
  <ClipNotification v-if="isOverlay" />

  <div v-else class="app-layout" @contextmenu.prevent>
    <Titlebar />
    <div class="app-body">
      <Sidebar :active="currentPage" @navigate="navigate" />
      <main class="content">
        <KeepAlive include="ClipsPage">
          <component :is="{ home: HomePage, mixer: MixerPage, clips: ClipsPage, devices: DevicesPage, settings: SettingsPage }[currentPage]" @navigate="navigate" />
        </KeepAlive>
      </main>
    </div>

    <!-- ═══ Epic 2: Virtual Audio Onboarding Modal ═══ -->
    <Teleport to="body">
      <div v-if="showOnboarding" class="onboard-overlay">
        <div class="onboard-box">

          <!-- Step indicator -->
          <div class="onboard-steps">
            <div class="onboard-step" :class="{ active: onboardStep === 1, done: onboardStep > 1 }">
              <div class="step-dot">{{ onboardStep > 1 ? '✓' : '1' }}</div>
              <span>{{ t('onboarding.steps.createVirtualAudio') }}</span>
            </div>
            <div class="step-connector"></div>
            <div class="onboard-step" :class="{ active: onboardStep === 2 }">
              <div class="step-dot">2</div>
              <span>{{ t('onboarding.steps.linkDevices') }}</span>
            </div>
          </div>

          <!-- Step 1: Create virtual sinks -->
          <template v-if="onboardStep === 1">
            <div class="onboard-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <path d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2z"/>
              </svg>
            </div>
            <h2 class="onboard-title">{{ t('onboarding.step1.title') }}</h2>
            <p class="onboard-desc">{{ t('onboarding.step1.description') }}</p>
            <div class="onboard-channels">
              <div class="ch-pill" v-for="(color, ch) in { Game: '#E94560', Chat: '#3B82F6', Media: '#10B981', Aux: '#A855F7' }" :key="ch">
                <div class="ch-dot" :style="{ background: color }"></div>
                {{ t(`settings.captureSound.sources.${ch}`) }}
              </div>
            </div>
            <p v-if="onboardMsg" class="onboard-err">{{ onboardMsg }}</p>
            <div class="onboard-actions">
              <button class="onboard-skip" @click="showOnboarding = false">{{ t('onboarding.skip') }}</button>
              <button class="onboard-primary" :disabled="onboardLoading" @click="doCreateVirtualAudio">
                <svg v-if="!onboardLoading" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="9 11 12 14 22 4"/><path d="M21 12v7a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2h11"/></svg>
                {{ onboardLoading ? t('onboarding.creating') : t('onboarding.step1.create') }}
              </button>
            </div>
          </template>

          <!-- Step 2: Link physical devices -->
          <template v-if="onboardStep === 2">
            <div class="onboard-icon onboard-icon--ok">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"/></svg>
            </div>
            <h2 class="onboard-title">{{ t('onboarding.step2.title') }}</h2>
            <p class="onboard-desc">{{ t('onboarding.step2.description') }}</p>
            <div class="onboard-device-rows">
              <div class="device-field">
                <label>
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 18v-6a9 9 0 0118 0v6"/><path d="M21 19a2 2 0 01-2 2h-1a2 2 0 01-2-2v-3a2 2 0 012-2h3zM3 19a2 2 0 002 2h1a2 2 0 002-2v-3a2 2 0 00-2-2H3z"/></svg>
                  {{ t('onboarding.step2.headphonesLabel') }}
                </label>
                <SelectField v-model="selectedHeadphones" :options="headphoneOptions" :placeholder="t('onboarding.step2.selectOutput')" />
              </div>
              <div class="device-field">
                <label>
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 1a3 3 0 00-3 3v8a3 3 0 006 0V4a3 3 0 00-3-3z"/><path d="M19 10v2a7 7 0 01-14 0v-2"/><line x1="12" y1="19" x2="12" y2="23"/><line x1="8" y1="23" x2="16" y2="23"/></svg>
                  {{ t('onboarding.step2.micLabel') }}
                </label>
                <SelectField v-model="selectedMic" :options="micOptions" :placeholder="t('onboarding.step2.selectInput')" />
              </div>
            </div>
            <p v-if="onboardMsg" class="onboard-err">{{ onboardMsg }}</p>
            <div class="onboard-actions">
              <button class="onboard-skip" @click="showOnboarding = false">{{ t('onboarding.skipShort') }}</button>
              <button class="onboard-primary" :disabled="onboardLoading" @click="completeOnboarding">
                {{ onboardLoading ? t('onboarding.saving') : t('onboarding.step2.finish') }}
              </button>
            </div>
          </template>

        </div>
      </div>
    </Teleport>

    <ToastContainer />
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
  --accent-rgb: 233, 69, 96;
  --color-accent-alpha-10: color-mix(in srgb, var(--accent) 10%, transparent);
  --color-accent-alpha-50: color-mix(in srgb, var(--accent) 50%, transparent);
  --danger: #dc2626;
  --success: #10b981;
  --purple: #a855f7;
  --radius: 6px;
  --radius-lg: 10px;
  --titlebar-h: 40px;
  --sidebar-w: 200px;
  --clips-grid-cols: 4;
  color-scheme: dark;
}
html.light {
  --bg-surface: #f0f2f5;
  --bg-card:    #ffffff;
  --bg-deep:    #e4e7ec;
  --bg-hover:   #dde1e8;
  --bg-input:   #f8f9fb;
  --border:     #d1d5db;
  --text:       #111827;
  --text-sec:   #4b5563;
  --text-muted: #9ca3af;
  color-scheme: light;
}
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
/* Text is selectable by default — informational content should be copyable.
   Only interactive chrome is locked down to preserve native-app feel. */
body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  background: var(--bg-surface);
  color: var(--text);
  overflow: hidden;
  -webkit-user-select: text;
  user-select: text;
}
/* Prevent drag-highlighting on non-text UI chrome */
img, svg { user-select: none; -webkit-user-drag: none; }
/* Interactive elements: no text selection (native app feel) */
button,
input,
textarea,
select,
.sidebar-btn,
.nav-item,
.tab-btn,
.thumb,
.thumb-img,
[data-tauri-drag-region],
.card-head,
.tb-btn { user-select: none; -webkit-user-select: none; }
.app-layout { display: flex; flex-direction: column; height: 100vh; }
.app-body { display: flex; flex: 1; overflow: hidden; }
.content { flex: 1; padding: 20px 28px; overflow-y: auto; }
::-webkit-scrollbar { width: 6px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }
::-webkit-scrollbar-thumb:hover { background: var(--text-muted); }

/* Fullscreen video fix */
video:fullscreen, video:-webkit-full-screen { z-index: 2147483647 !important; position: fixed !important; inset: 0 !important; width: 100vw !important; height: 100vh !important; object-fit: contain !important; background: #000 !important; }
video::backdrop, video::-webkit-backdrop { background: #000 !important; }
body:has(video:fullscreen), body:has(video:-webkit-full-screen) { overflow: visible !important; }
body:has(video:fullscreen) .app-layout,
body:has(video:fullscreen) .app-body,
body:has(video:fullscreen) .content,
body:has(video:-webkit-full-screen) .app-layout,
body:has(video:-webkit-full-screen) .app-body,
body:has(video:-webkit-full-screen) .content { overflow: visible !important; }
body:has(video:fullscreen) .overlay,
body:has(video:-webkit-full-screen) .overlay { backdrop-filter: none !important; -webkit-backdrop-filter: none !important; }
body:has(video:fullscreen) [data-tauri-drag-region],
body:has(video:-webkit-full-screen) [data-tauri-drag-region] { display: none !important; }
:fullscreen, :-webkit-full-screen { z-index: 2147483647 !important; }

/* ═══ Virtual Audio Onboarding Modal ═══ */
.onboard-overlay {
  position: fixed; inset: 0; z-index: 9999;
  background: rgba(0,0,0,.78);
  backdrop-filter: blur(6px);
  display: flex; align-items: center; justify-content: center;
  padding: 20px;
}
.onboard-box {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 16px;
  padding: 32px;
  width: 100%; max-width: 480px;
  box-shadow: 0 32px 80px rgba(0,0,0,.65);
  display: flex; flex-direction: column; gap: 20px;
}

/* Step indicator */
.onboard-steps { display: flex; align-items: center; }
.onboard-step { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-muted); font-weight: 500; }
.onboard-step.active { color: var(--text); }
.onboard-step.done { color: var(--success); }
.step-dot {
  width: 26px; height: 26px; border-radius: 50%; flex-shrink: 0;
  display: flex; align-items: center; justify-content: center;
  font-size: 11px; font-weight: 700;
  background: var(--bg-deep); border: 2px solid var(--border); color: var(--text-muted);
}
.onboard-step.active .step-dot { border-color: var(--accent); color: var(--accent); }
.onboard-step.done .step-dot { border-color: var(--success); background: color-mix(in srgb, var(--success) 15%, transparent); color: var(--success); }
.step-connector { flex: 1; height: 2px; background: var(--border); margin: 0 10px; }

/* Icon */
.onboard-icon {
  width: 58px; height: 58px; border-radius: 14px;
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
  display: flex; align-items: center; justify-content: center; color: var(--accent);
}
.onboard-icon svg { width: 30px; height: 30px; }
.onboard-icon--ok { background: color-mix(in srgb, var(--success) 12%, transparent); border-color: color-mix(in srgb, var(--success) 30%, transparent); color: var(--success); }

.onboard-title { font-size: 20px; font-weight: 800; letter-spacing: -.3px; }
.onboard-desc { font-size: 13px; color: var(--text-sec); line-height: 1.6; }

.onboard-channels { display: flex; gap: 8px; flex-wrap: wrap; }
.ch-pill { display: flex; align-items: center; gap: 6px; padding: 5px 11px; background: var(--bg-deep); border: 1px solid var(--border); border-radius: 20px; font-size: 12px; font-weight: 600; color: var(--text-sec); }
.ch-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }

.onboard-err { font-size: 12px; color: var(--danger); padding: 8px 12px; background: color-mix(in srgb, var(--danger) 8%, transparent); border-radius: 6px; border: 1px solid color-mix(in srgb, var(--danger) 25%, transparent); }

.onboard-actions { display: flex; align-items: center; justify-content: flex-end; gap: 10px; padding-top: 4px; }
.onboard-skip { background: transparent; border: none; color: var(--text-muted); font-size: 13px; cursor: pointer; padding: 8px 4px; }
.onboard-skip:hover { color: var(--text-sec); }
.onboard-primary {
  display: flex; align-items: center; gap: 8px;
  padding: 10px 20px; border-radius: 8px;
  border: none; background: var(--accent);
  color: #fff; font-size: 13px; font-weight: 700;
  cursor: pointer; transition: opacity .15s;
}
.onboard-primary svg { width: 15px; height: 15px; }
.onboard-primary:hover { opacity: .88; }
.onboard-primary:disabled { opacity: .45; cursor: not-allowed; }

/* Device selectors */
.onboard-device-rows { display: flex; flex-direction: column; gap: 12px; }
.device-field { display: flex; flex-direction: column; gap: 6px; }
.device-field label { display: flex; align-items: center; gap: 6px; font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: .5px; color: var(--text-sec); }
.device-field label svg { width: 13px; height: 13px; }
/* device-select replaced by SelectField component */
</style>

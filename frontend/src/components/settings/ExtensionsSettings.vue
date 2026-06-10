<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { usePersistenceStore } from '../../stores/persistence'
import { useExtensionStore, type ExtManifest, type ExtRuntime } from '../../stores/extensions'
import { useToast } from '../../composables/useToast'
import InfoIcon from '../InfoIcon.vue'

const { t } = useI18n()
const persist = usePersistenceStore()
const extStore = useExtensionStore()
const toast = useToast()

const scannedExtensions = ref<any[]>([])
const extensionScanLoading = ref(false)
const activeExtSettings = ref<ExtRuntime | null>(null)
const gsrInstallOpen = ref(false)
const copiedCommand = ref<string | null>(null)
const reloadingExtId = ref<string | null>(null)
let copiedTimer: ReturnType<typeof setTimeout> | null = null

function getExtensionIconUrl(p: any): string | null {
  if (p._builtin || !p.icon) return null
  return `http://localhost:8080/ext/${encodeURIComponent(p.id)}/${encodeURIComponent(p.icon)}`
}

function canConfigure(p: any): boolean {
  return !!p.has_settings && isExtEnabled(p) && !!extStore.getRuntime(p.id)?.settingsComponent
}

function openExtSettings(p: any) {
  const rt = extStore.getRuntime(p.id)
  if (rt?.settingsComponent) activeExtSettings.value = rt
}

function isExtEnabled(p: any): boolean {
  return p.enabled ?? true
}

async function setExtEnabled(p: any, val: boolean) {
  p.enabled = val
  persist.state.extensions[p.id] = val
  try {
    await invoke('set_extension_enabled', { id: p.id, enabled: val })
  } catch (e) {
    console.error('[extensions] set_extension_enabled failed:', e)
  }
  if (val) {
    if (p.main) await extStore.loadExtension(p as unknown as ExtManifest, 0, '')
  } else {
    extStore.unload(p.id)
  }
}

async function scanExtensions() {
  extensionScanLoading.value = true
  try { scannedExtensions.value = await invoke<any[]>('scan_extensions') }
  catch { scannedExtensions.value = [] }
  finally { extensionScanLoading.value = false }
}

async function openExtensionsFolder() {
  try { await invoke('open_extensions_folder') } catch (e) { console.error(e) }
  await scanExtensions()
}

async function refreshExtensions() {
  await scanExtensions()
}

async function reloadExtensionDev(p: any) {
  if (!import.meta.env.DEV) return

  reloadingExtId.value = p.id
  try {
    // Reload requires the media server port and token from the app state
    // For now, pass port 0 and empty token since we're in dev mode
    // The store will handle loading from the appropriate URL
    const rt = extStore.runtimes[p.id]
    if (rt?.manifest.main) {
      // Get port from window location or use default dev port
      const port = window.location.port ? parseInt(window.location.port) : 1420
      const token = '' // Empty in dev
      await extStore.reloadExtension(p.id, port, token)
      toast.success(`Reloaded ${p.name}`)
    }
  } catch (e) {
    toast.error(`Reload failed: ${e}`)
    console.error('[extensions] reload failed:', e)
  } finally {
    reloadingExtId.value = null
  }
}

async function toggleGsr() {
  const settings = persist.state.settings
  try {
    if (settings.gsrEnabled) {
      await invoke('stop_gsr_replay')
    } else {
      await invoke('start_gsr_replay', {
        outputDir: settings.clip_directories?.[0] ?? '~/Videos/OpenGG',
        replaySecs: settings.gsrReplaySecs,
        fps: settings.gsrFps,
        quality: settings.gsrQuality,
        bitrateKbps: settings.gsrQuality === 'cbr' ? settings.gsrCbrBitrate : null,
        monitorTarget: settings.gsrMonitorTarget || 'screen',
        audioSources: settings.captureTracks.map((t: any) => t.source),
      })
    }
    settings.gsrEnabled = !settings.gsrEnabled
  } catch (e) {
    console.error('GSR toggle:', e)
  }
}

async function doCopy(text: string) {
  if (!text) return
  try {
    await invoke('write_clipboard', { text })
  } catch (e2) {
    toast.error(`Copy failed: ${e2}`)
    return
  }
  copiedCommand.value = text
  if (copiedTimer) clearTimeout(copiedTimer)
  copiedTimer = setTimeout(() => { copiedCommand.value = null }, 1500)
  toast.success('Copied!')
}

onMounted(async () => {
  await scanExtensions()
  await listen('plugins-changed', () => {
    scanExtensions()
  })
})

onBeforeUnmount(() => {
  if (copiedTimer) clearTimeout(copiedTimer)
})

defineEmits<{ navigate: [page: string] }>()
</script>

<template>
  <section>
    <h2 class="sec-title">{{ t('settings.extensions.title') }}</h2>

    <!-- Core Modules -->
    <div class="card">
      <div class="card-head">
        {{ t('settings.general.modules') }}
        <InfoIcon :title="t('settings.extensions.hint')" />
      </div>
      <label class="toggle-row"><input type="checkbox" v-model="persist.state.modules.audio"><span class="tname">{{ t('settings.general.audioHub') }}</span><span class="tdesc">{{ t('settings.general.audioHubDesc') }}</span></label>
      <label class="toggle-row"><input type="checkbox" v-model="persist.state.modules.device"><span class="tname">{{ t('settings.general.deviceManager') }}</span><span class="tdesc">{{ t('settings.general.deviceManagerDesc') }}</span></label>
      <label class="toggle-row"><input type="checkbox" v-model="persist.state.modules.replay"><span class="tname">{{ t('settings.general.replayClips') }}</span><span class="tdesc">{{ t('settings.general.replayClipsDesc') }}</span></label>
    </div>

    <!-- GPU Screen Recorder -->
    <div class="card">
      <div class="card-head gsr-head">
        <span>{{ t('settings.captureGsr.title') }}</span>
        <span class="badge-beta">Beta</span>
        <InfoIcon :title="t('settings.captureGsr.hint')" />
      </div>
      <button class="gsr-install-toggle" @click="gsrInstallOpen = !gsrInstallOpen">{{ gsrInstallOpen ? t('settings.captureGsr.installToggleHide') : t('settings.captureGsr.installToggleShow') }}</button>
      <div v-if="gsrInstallOpen" class="gsr-install-guide">
        <div class="install-section">
          <span class="install-distro">Ubuntu / Debian</span>
          <div class="install-cmd-wrap">
            <code class="install-cmd">sudo add-apt-repository ppa:dec05eba/gpu-screen-recorder && sudo apt install gpu-screen-recorder</code>
            <button class="copy-btn" :class="{ copied: copiedCommand === 'sudo add-apt-repository ppa:dec05eba/gpu-screen-recorder && sudo apt install gpu-screen-recorder' }" @click="doCopy('sudo add-apt-repository ppa:dec05eba/gpu-screen-recorder && sudo apt install gpu-screen-recorder')">
              <svg v-if="copiedCommand !== 'sudo add-apt-repository ppa:dec05eba/gpu-screen-recorder && sudo apt install gpu-screen-recorder'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:13px;height:13px"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
              <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:13px;height:13px"><polyline points="20 6 9 17 4 12"/></svg>
            </button>
          </div>
        </div>
        <div class="install-section">
          <span class="install-distro">Arch / Manjaro</span>
          <div class="install-cmd-wrap">
            <code class="install-cmd">yay -S gpu-screen-recorder</code>
            <button class="copy-btn" :class="{ copied: copiedCommand === 'yay -S gpu-screen-recorder' }" @click="doCopy('yay -S gpu-screen-recorder')">
              <svg v-if="copiedCommand !== 'yay -S gpu-screen-recorder'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:13px;height:13px"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
              <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:13px;height:13px"><polyline points="20 6 9 17 4 12"/></svg>
            </button>
          </div>
        </div>
        <div class="install-section">
          <span class="install-distro">Fedora</span>
          <div class="install-cmd-wrap">
            <code class="install-cmd">sudo dnf install gpu-screen-recorder</code>
            <button class="copy-btn" :class="{ copied: copiedCommand === 'sudo dnf install gpu-screen-recorder' }" @click="doCopy('sudo dnf install gpu-screen-recorder')">
              <svg v-if="copiedCommand !== 'sudo dnf install gpu-screen-recorder'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:13px;height:13px"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
              <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:13px;height:13px"><polyline points="20 6 9 17 4 12"/></svg>
            </button>
          </div>
        </div>
      </div>
      <div class="gsr-toggle-row">
        <span class="gsr-label">Enable GSR Replay Buffer</span>
        <button class="toggle-btn" :class="{ on: persist.state.settings.gsrEnabled }" @click="toggleGsr">
          <span class="toggle-knob"></span>
        </button>
      </div>
    </div>

    <!-- Unified Extensions List -->
    <div class="card">
      <div class="card-head ext-section-head">
        <span>{{ t('settings.extensions.sectionTitle') }}</span>
        <div class="ext-head-actions">
          <button class="ext-icon-btn" :title="t('settings.extensions.refresh')" @click="refreshExtensions" :disabled="extensionScanLoading">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/></svg>
          </button>
          <button class="ext-icon-btn" :title="t('settings.extensions.openFolder')" @click="openExtensionsFolder">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>
          </button>
        </div>
      </div>

      <div v-if="extensionScanLoading" class="hint" style="padding:8px 0">{{ t('settings.extensions.scanning') }}</div>
      <div v-else-if="!scannedExtensions.length" class="hint" style="padding:8px 0">{{ t('settings.extensions.noExtensions') }}</div>
      <div v-else>
        <div v-for="p in scannedExtensions" :key="p.id" class="ext-card-row">
          <div class="ext-card-icon-wrap">
            <img v-if="getExtensionIconUrl(p)" :src="getExtensionIconUrl(p)!" class="plugin-icon" alt="" />
            <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M20.24 12.24a6 6 0 0 0-8.49-8.49L5 10.5V19h8.5z"/><line x1="16" y1="8" x2="2" y2="22"/><line x1="17.5" y1="15" x2="9" y2="15"/></svg>
          </div>
          <div class="ext-card-info">
            <div class="ext-card-title-row">
              <span class="ext-name">{{ p.name }}</span>
              <span class="plugin-ver">v{{ p.version }}</span>
              <button v-if="import.meta.env.DEV && p.main && isExtEnabled(p)" class="ext-reload-btn" :title="t('ext.reloadDevMode')" :disabled="reloadingExtId === p.id" @click.stop="reloadExtensionDev(p)">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" :class="{ spinning: reloadingExtId === p.id }"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/></svg>
              </button>
              <button v-if="canConfigure(p)" class="ext-gear-btn" @click.stop="openExtSettings(p)">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
              </button>
            </div>
            <p class="ext-desc">{{ p.description }}</p>
          </div>
          <label class="switch ext-card-switch">
            <input type="checkbox" :checked="isExtEnabled(p)" @change="setExtEnabled(p, ($event.target as HTMLInputElement).checked)" />
            <span class="switch-track"></span>
          </label>
        </div>
      </div>
    </div>

    <p class="hint ext-restart-hint">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:12px;height:12px;vertical-align:middle;margin-right:4px"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
      {{ t('settings.extensions.restartHint') }}
    </p>

    <!-- Extension settings panel modal -->
    <Teleport to="body">
      <div v-if="activeExtSettings" class="ext-modal-overlay" @click.self="activeExtSettings = null">
        <div class="ext-modal-box">
          <div class="ext-modal-head">
            <span class="ext-modal-title">{{ activeExtSettings.manifest.name }}</span>
            <button class="ext-modal-close" @click="activeExtSettings = null">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
            </button>
          </div>
          <div class="ext-modal-body">
            <component :is="activeExtSettings.settingsComponent" />
          </div>
        </div>
      </div>
    </Teleport>
  </section>
</template>

<style scoped>
@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

.ext-reload-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  background: none;
  color: var(--text-secondary);
  cursor: pointer;
  transition: color 0.2s, opacity 0.2s;
}

.ext-reload-btn:hover:not(:disabled) {
  color: var(--accent);
}

.ext-reload-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.ext-reload-btn svg {
  width: 16px;
  height: 16px;
}

.ext-reload-btn svg.spinning {
  animation: spin 1s linear infinite;
}
</style>

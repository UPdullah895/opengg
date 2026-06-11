<script setup lang="ts">
import { ref, computed, watch, onMounted, inject, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { usePersistenceStore } from '../../stores/persistence'
import { useReplayStore } from '../../stores/replay'
import { mediaUrl } from '../../utils/assets'
import InfoIcon from '../InfoIcon.vue'
import './settings-shared.css'

const { t } = useI18n()
const persist = usePersistenceStore()
const replay = useReplayStore()

const settings = computed(() => persist.state.settings)
const mediaPort = inject<Ref<number>>('mediaPort', ref(0))
const mediaToken = inject<Ref<string>>('mediaToken', ref(''))

async function addClipSource() {
  try {
    const s = await openDialog({ directory: true, multiple: false, title: 'Add Clip Directory' })
    if (s && typeof s === 'string') {
      if (!settings.value.clip_directories) settings.value.clip_directories = []
      if (!settings.value.clip_directories.includes(s)) settings.value.clip_directories.push(s)
      await persist.save()
      await replay.fetchClips('', true)
      try { await invoke('update_watch_dirs') } catch {}
    }
  } catch {}
}

async function removeClipSource(idx: number) {
  settings.value.clip_directories?.splice(idx, 1)
  await persist.save()
  await replay.fetchClips('', true)
  try { await invoke('update_watch_dirs') } catch {}
}

async function addScreenshotDir() {
  try {
    const s = await openDialog({ directory: true, multiple: false, title: 'Add Screenshot Directory' })
    if (s && typeof s === 'string') {
      if (!settings.value.screenshotDirs) settings.value.screenshotDirs = []
      if (!settings.value.screenshotDirs.includes(s)) settings.value.screenshotDirs.push(s)
    }
  } catch {}
}

function removeScreenshotDir(idx: number) {
  settings.value.screenshotDirs?.splice(idx, 1)
}

// ─── Cache ───
const cacheClearing = ref(false)
const cacheMsg = ref('')

async function clearCache() {
  cacheClearing.value = true; cacheMsg.value = ''
  try {
    const count = await invoke<number>('clear_thumbnail_cache')
    cacheMsg.value = t('settings.clipSettings.cleared', { count })
  } catch (e) { cacheMsg.value = `Error: ${e}` }
  finally { cacheClearing.value = false }
}

// ─── Storage ───
interface StorageInfo { clip_count: number; used_bytes: number; total_bytes: number; free_bytes: number }
const storageInfo = ref<StorageInfo | null>(null)
const storageLoading = ref(false)
const steamImportBusy = ref(false)
const steamAccess = computed(() => persist.state.settings.steamLibraryAccess)
const steamGamesLoaded = computed(() => replay.steamGames.length > 0)

function steamIconUrl(path: string | null | undefined) {
  if (!path) return ''
  return path.startsWith('/') ? mediaUrl(path, mediaPort.value, mediaToken.value) : path
}

watch(steamAccess, (access) => {
  if (access === 'granted' && replay.steamGames.length === 0) {
    void replay.fetchSteamGames()
  }
}, { immediate: true })

async function loadStorage() {
  storageLoading.value = true
  try { storageInfo.value = await invoke<StorageInfo>('get_storage_info', { clipDirectories: settings.value.clip_directories ?? ['~/Videos/OpenGG'] }) }
  catch { storageInfo.value = null }
  finally { storageLoading.value = false }
}

onMounted(() => { if (true) loadStorage() })

async function importSteamLibrary(forcePrompt = false) {
  const { ask } = await import('@tauri-apps/plugin-dialog')
  if (steamImportBusy.value) return

  const access = persist.state.settings.steamLibraryAccess
  if (access !== 'granted' || forcePrompt) {
    const confirmed = await ask(t('clips.steamImport.consentMessage'), {
      title: t('clips.steamImport.consentTitle'),
      kind: 'info',
    })
    persist.state.settings.steamLibraryAccess = confirmed ? 'granted' : 'denied'
    if (!confirmed) return
  }

  steamImportBusy.value = true
  const ok = await replay.fetchSteamGames()
  steamImportBusy.value = false
  if (!ok) return
}

const steamStorageTitle = computed(() => {
  if (steamGamesLoaded.value) return t('settings.storage.steamReadyTitle', { count: replay.steamGames.length })
  if (steamAccess.value === 'denied') return t('settings.storage.steamDeniedTitle')
  return t('settings.storage.steamTitle')
})

const steamStorageBody = computed(() => {
  if (steamGamesLoaded.value) return t('settings.storage.steamReadyBody')
  if (steamAccess.value === 'denied') return t('settings.storage.steamDeniedBody')
  return t('settings.storage.steamBody')
})

const steamStorageButtonLabel = computed(() => {
  if (steamImportBusy.value) return t('clips.steamImport.loading')
  if (steamGamesLoaded.value) return t('clips.steamImport.refresh')
  if (steamAccess.value === 'granted') return t('clips.steamImport.import')
  return t('clips.steamImport.allowAndImport')
})

function fmtBytes(b: number) {
  if (b >= 1e9) return (b / 1e9).toFixed(1) + ' GB'
  if (b >= 1e6) return (b / 1e6).toFixed(1) + ' MB'
  return (b / 1e3).toFixed(0) + ' KB'
}

defineEmits<{ navigate: [page: string] }>()
</script>

<template>
  <section class="settings-section">
    <h2 class="sec-title">{{ t('settings.storage.title') }}</h2>

    <!-- Media directories -->
    <div class="card">
      <div class="card-head">
        {{ t('settings.storage.title') }}
        <InfoIcon :title="t('settings.storage.mediaDirsHint')" />
      </div>

      <!-- Clip Directories list -->
      <div class="media-dir-row">
        <div class="media-dir-label">{{ t('settings.storage.clipDirectories') }}</div>
        <div class="media-dirs-list">
          <div v-for="(src, i) in (settings.clip_directories || [])" :key="i" class="source-row">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="dir-icon"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
            <span class="source-path">{{ src || t('settings.storage.defaultClipPath') }}</span>
            <button class="btn-icon-sm" @click="removeClipSource(i)" :title="t('settings.storage.removeDir')">✕</button>
          </div>
          <button class="btn btn-sm folder-add-btn" @click="addClipSource">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="12" height="12"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
            {{ t('settings.storage.addClipPath') }}
          </button>
        </div>
      </div>

      <div class="media-dir-divider"></div>

      <!-- Screenshot Directories list -->
      <div class="media-dir-row">
        <div class="media-dir-label">{{ t('settings.storage.screenshotDirectories') }}</div>
        <div class="media-dirs-list">
          <div v-for="(dir, i) in (settings.screenshotDirs || [])" :key="i" class="source-row">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="dir-icon"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
            <span class="source-path">{{ dir }}</span>
            <button class="btn-icon-sm" @click="removeScreenshotDir(i)" :title="t('settings.storage.removeDir')">✕</button>
          </div>
          <button class="btn btn-sm folder-add-btn" @click="addScreenshotDir">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="12" height="12"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
            {{ t('settings.storage.addScreenshotPath') }}
          </button>
        </div>
      </div>
    </div>

    <!-- Thumbnail cache + Disk usage side-by-side -->
    <div class="storage-grid">
      <div class="card">
        <div class="card-head">{{ t('settings.clipSettings.thumbnailCache') }} <InfoIcon :title="t('settings.clipSettings.thumbnailHint')" /></div>
        <div class="action-row">
          <button class="btn btn-warn" @click="clearCache" :disabled="cacheClearing">
            {{ cacheClearing ? t('settings.clipSettings.clearing') : t('settings.clipSettings.clearCache') }}
          </button>
          <span v-if="cacheMsg" class="cache-msg">{{ cacheMsg }}</span>
        </div>
      </div>

      <div class="card">
        <div class="card-head">{{ t('settings.storage.diskUsage') }}</div>
        <div v-if="storageLoading" class="hint">{{ t('settings.storage.loading') }}</div>
        <template v-else-if="storageInfo">
          <div class="storage-stats">
            <div class="stat-pill">
              <span class="stat-label">{{ t('settings.storage.clips') }}</span>
              <span class="stat-val accent">{{ storageInfo.clip_count }}</span>
            </div>
            <div class="stat-pill">
              <span class="stat-label">{{ t('settings.storage.used') }}</span>
              <span class="stat-val">{{ fmtBytes(storageInfo.used_bytes) }}</span>
            </div>
          </div>
        </template>
        <div v-else class="hint">{{ t('settings.storage.readError') }}</div>
      </div>
    </div>

    <div class="card">
      <div class="card-head">{{ t('settings.storage.steamSection') }}</div>
      <div class="steam-storage-head">
        <div class="steam-storage-copy">
          <div class="steam-storage-title">{{ steamStorageTitle }}</div>
          <div class="steam-storage-body">{{ steamStorageBody }}</div>
        </div>
        <button class="btn btn-sm steam-storage-btn" :disabled="steamImportBusy" @click="importSteamLibrary(steamAccess === 'denied')">
          {{ steamStorageButtonLabel }}
        </button>
      </div>
      <div v-if="steamGamesLoaded" class="steam-storage-list">
        <div v-for="game in replay.steamGames" :key="game.appid" class="steam-storage-row">
          <img v-if="steamIconUrl(game.icon_url)" :src="steamIconUrl(game.icon_url)" alt="" class="steam-storage-icon" loading="lazy" />
          <div v-else class="steam-storage-icon steam-storage-icon--fallback">S</div>
          <span class="steam-storage-name">{{ game.name }}</span>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* Inherited from parent */
</style>

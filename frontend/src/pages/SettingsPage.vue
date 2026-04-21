<script setup lang="ts">
import { ref, computed, watch, onMounted, inject, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getVersion } from '@tauri-apps/api/app'
import { ask, open as openDialog } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { usePersistenceStore, DEFAULTS } from '../stores/persistence'
import { useReplayStore } from '../stores/replay'
import { useAudioStore } from '../stores/audio'
import { loadTheme, saveTheme, getCurrentTheme, applyThemeMode, getThemeMode } from '../utils/theme'
import { LANGUAGES, registerLocale } from '../i18n'
import SelectField from '../components/SelectField.vue'
import IconPicker from '../components/IconPicker.vue'
import InfoIcon from '../components/InfoIcon.vue'
import { settingsTargetTab } from '../composables/useNavSignal'
import { mediaUrl } from '../utils/assets'

const { t, locale } = useI18n()
const persist = usePersistenceStore()
const appVersion = ref('')
const replay = useReplayStore()
const audio = useAudioStore()
const mediaPort = inject<Ref<number>>('mediaPort', ref(0))
onMounted(async () => {
  try { appVersion.value = await getVersion() } catch { appVersion.value = '0.1.1' }
  if (!persist.loaded) await persist.load()
  syncLocale()
  // ★ Epic 4: Sync autostart UI with actual OS state on every open
  try { settings.value.runAtStartup = await invoke<boolean>('get_autostart') } catch { /* ignore */ }
  // ★ Epic 4: Push saved run-in-background flag to Rust state
  try { await invoke('set_run_in_background', { val: settings.value.runInBackground }) } catch { /* ignore */ }
  // ── Cross-page deep link: auto-select tab when navigated from another page ──
  if (settingsTargetTab.value) {
    active.value = settingsTargetTab.value as typeof active.value
    settingsTargetTab.value = null
  }
})

const settings = computed(() => persist.state.settings)

function syncLocale() {
  if (settings.value.language) locale.value = settings.value.language
}
function setLanguage(code: string) {
  settings.value.language = code
  locale.value = code
}

const isRtlLanguage = computed(() => {
  const entry = LANGUAGES.find(l => l.code === settings.value.language)
  return entry?.dir === 'rtl'
})
function enableRtl() {
  settings.value.rtlMode = true
  document.documentElement.dir = 'rtl'
}
function disableRtl() {
  settings.value.rtlMode = false
  document.documentElement.dir = 'ltr'
}
watch(() => settings.value.language, (newLang) => {
  const entry = LANGUAGES.find(l => l.code === newLang)
  if (entry?.dir !== 'rtl' && settings.value.rtlMode) disableRtl()
})

// ─── Nav ───
type Section = 'general' | 'language' | 'shortcuts' | 'mixerRouting' | 'captureSound' | 'trackManagement' | 'storage' | 'extensions' | 'store' | 'about' | 'notifications'
type NavItem = { key: Section; label: string; badge?: string }
const active = ref<Section>('general')

const navGroups = computed(() => [
  {
    key: 'general', label: t('settings.groups.general'),
    items: [
      { key: 'general'   as Section, label: t('settings.sections.general')   } as NavItem,
      { key: 'language'  as Section, label: t('settings.sections.language')  } as NavItem,
      { key: 'shortcuts' as Section, label: t('settings.sections.shortcuts') } as NavItem,
    ],
  },
  {
    key: 'audioEngine', label: t('settings.groups.audioEngine'),
    items: [
      { key: 'mixerRouting'  as Section, label: t('settings.sections.mixerRouting')  } as NavItem,
    ],
  },
  {
    key: 'moments', label: t('settings.groups.moments'),
    items: [
      { key: 'captureSound'    as Section, label: t('settings.sections.captureSound') } as NavItem,
      { key: 'trackManagement' as Section, label: t('settings.sections.trackManagement') } as NavItem,
      { key: 'storage'         as Section, label: t('settings.sections.storage')      } as NavItem,
      { key: 'notifications'   as Section, label: t('settings.sections.notifications') } as NavItem,
    ],
  },
  {
    key: 'extensions', label: t('settings.groups.extensions'),
    items: [
      { key: 'extensions' as Section, label: t('settings.sections.extensions'), badge: 'Beta' } as NavItem,
      { key: 'store'      as Section, label: t('settings.sections.store') } as NavItem,
    ],
  },
  {
    key: 'info', label: '',
    items: [
      { key: 'about' as Section, label: t('settings.sections.about') } as NavItem,
    ],
  },
])

// ─── Theme ───
const themeAccent = ref('#E94560')
const themeLoading = ref(false)
const themeDarkMode = ref(true)
onMounted(async () => {
  const th = getCurrentTheme()
  if (th?.colors?.['--accent']) themeAccent.value = th.colors['--accent']
  themeDarkMode.value = getThemeMode() !== 'light'
})
async function reloadTheme() {
  themeLoading.value = true
  try { await loadTheme() } finally { themeLoading.value = false }
}
let _accentTimer: ReturnType<typeof setTimeout> | null = null
async function applyAccentColor() {
  const th = getCurrentTheme() || { colors: {}, layout: {} }
  th.colors['--accent'] = themeAccent.value
  await saveTheme(th)
}
function onAccentInput() {
  if (_accentTimer) clearTimeout(_accentTimer)
  _accentTimer = setTimeout(() => applyAccentColor(), 300)
}
async function onToggleDarkMode() {
  const mode = themeDarkMode.value ? 'dark' : 'light'
  applyThemeMode(mode)
  const th = getCurrentTheme() || { colors: {}, layout: {} }
  th.mode = mode
  await saveTheme(th)
}

// ─── Language: open locales folder + dynamic locale reload ───
const localesFolderPath = ref('')
const localesReloading = ref(false)

async function loadUserLocales() {
  localesReloading.value = true
  try {
    const list = await invoke<Array<{ code: string; json_content: string }>>('list_user_locales')
    for (const ul of list) {
      try {
        const data = JSON.parse(ul.json_content)
        const meta = (data._meta ?? {}) as { name?: string; dir?: 'ltr' | 'rtl' }
        registerLocale(
          ul.code,
          data,
          meta.name ?? (ul.code.charAt(0).toUpperCase() + ul.code.slice(1)),
          meta.dir  ?? 'ltr',
        )
      } catch { /* skip malformed JSON */ }
    }
  } catch { /* dir not created yet */ }
  finally { localesReloading.value = false }
}

async function openLocalesFolder() {
  try { localesFolderPath.value = await invoke<string>('open_locales_folder') } catch (e) { console.error(e) }
  // Also reload so files added before clicking the button appear immediately
  await loadUserLocales()
}

// ─── Epic 2: Diagnostics ───
async function openCrashLogsFolder() {
  try { await invoke('open_crash_logs_folder') } catch (e) { console.error(e) }
}

// ─── Epic 4: Daemon / autostart ───
async function onRunAtStartupChange() {
  try { await invoke('set_autostart', { enable: settings.value.runAtStartup }) } catch (e) { console.error(e) }
}
async function onRunInBackgroundChange() {
  try { await invoke('set_run_in_background', { val: settings.value.runInBackground }) } catch (e) { console.error(e) }
}

// ─── Shortcuts ───
const recordingKey = ref<string | null>(null) // which action key is being recorded

function startRecord(key: string) { recordingKey.value = key }
function cancelRecord() { recordingKey.value = null }

function onShortcutKeydown(e: KeyboardEvent) {
  if (!recordingKey.value) return
  e.preventDefault()
  if (e.key === 'Escape') { cancelRecord(); return }
  const parts: string[] = []
  if (e.ctrlKey)  parts.push('Ctrl')
  if (e.shiftKey) parts.push('Shift')
  if (e.altKey)   parts.push('Alt')
  if (e.metaKey)  parts.push('Meta')
  const bare = e.key
  if (!['Control','Shift','Alt','Meta'].includes(bare)) {
    parts.push(bare.length === 1 ? bare.toUpperCase() : bare)
  }
  if (parts.length > 0 && !['Control','Shift','Alt','Meta'].includes(e.key)) {
    ;(settings.value.shortcuts as Record<string, string>)[recordingKey.value] = parts.join('+')
    recordingKey.value = null
  }
}

const shortcutActions = computed<Array<{ key: string; label: string; hint: string }>>(() => [
  { key: 'saveReplay',      label: t('settings.shortcuts.actions.saveReplay'),      hint: t('settings.shortcuts.hints.saveReplay') },
  { key: 'toggleRecording', label: t('settings.shortcuts.actions.toggleRecording'), hint: t('settings.shortcuts.hints.toggleRecording') },
  { key: 'screenshot',      label: t('settings.shortcuts.actions.screenshot'),      hint: t('settings.shortcuts.hints.screenshot') },
  { key: 'splitClip',       label: t('settings.shortcuts.actions.splitClip'),       hint: t('settings.shortcuts.hints.splitClip') },
  { key: 'exportClip',      label: t('settings.shortcuts.actions.exportClip'),      hint: t('settings.shortcuts.hints.exportClip') },
  { key: 'toggleMic',       label: t('settings.shortcuts.actions.toggleMic'),       hint: t('settings.shortcuts.hints.toggleMic') },
  { key: 'undo',            label: t('settings.shortcuts.actions.undo'),            hint: t('settings.shortcuts.hints.undo') },
  { key: 'redo',            label: t('settings.shortcuts.actions.redo'),            hint: t('settings.shortcuts.hints.redo') },
])

// ─── GPU Screen Recorder ───
const gsrQualityOptions = computed(() => [
  { value: 'cbr',       label: t('settings.captureGsr.qualityCbr')      },
  { value: 'medium',    label: t('settings.captureGsr.qualityMedium')   },
  { value: 'high',      label: t('settings.captureGsr.qualityHigh')     },
  { value: 'very_high', label: t('settings.captureGsr.qualityVeryHigh') },
  { value: 'ultra',     label: t('settings.captureGsr.qualityUltra')    },
])
const gsrFpsOptions = computed(() => [30, 60, 120].map(v => ({ value: v, label: t(`dashboard.gsrFps.${v}`) })))
const gsrReplayPresets = computed(() => [
  { value: '15',     label: t('dashboard.gsrReplay.15')       },
  { value: '30',     label: t('dashboard.gsrReplay.30')       },
  { value: '60',     label: t('dashboard.gsrReplay.60')       },
  { value: '90',     label: t('dashboard.gsrReplay.90')       },
  { value: '120',    label: t('dashboard.gsrReplay.120')      },
  { value: 'custom', label: t('settings.captureGsr.replayCustom') },
])

function onReplayPresetChange(preset: string | number) {
  settings.value.gsrReplayPreset = String(preset) as any
  if (preset !== 'custom') {
    settings.value.gsrReplaySecs = Number(preset)
    restartGsr()
  }
}

function onCustomReplaySecs(e: Event) {
  const raw = Number((e.target as HTMLInputElement).value)
  settings.value.gsrReplaySecs = Math.max(5, Math.min(600, raw || 30))
  restartGsr()
}

const gsrAudioSources = computed(() =>
  settings.value.captureTracks.map((t: { source: string }) => t.source)
)

function gsrInvokeParams() {
  const outputDir = settings.value.clip_directories?.[0] ?? '~/Videos/OpenGG'
  return {
    outputDir,
    replaySecs:    settings.value.gsrReplaySecs,
    fps:           settings.value.gsrFps,
    quality:       settings.value.gsrQuality,
    bitrateKbps:   settings.value.gsrQuality === 'cbr' ? settings.value.gsrCbrBitrate : null,
    monitorTarget: settings.value.gsrMonitorTarget || 'screen',
    audioSources:  gsrAudioSources.value,
  }
}

async function toggleGsr() {
  try {
    if (settings.value.gsrEnabled) {
      await invoke('stop_gsr_replay')
    } else {
      await invoke('start_gsr_replay', gsrInvokeParams())
    }
    settings.value.gsrEnabled = !settings.value.gsrEnabled
  } catch (e) { console.error('GSR toggle:', e) }
}

async function restartGsr() {
  if (!settings.value.gsrEnabled) return
  try {
    await invoke('restart_gsr_replay', gsrInvokeParams())
  } catch (e) { console.error('GSR restart:', e) }
}

// ─── Resource estimation ───
const gsrEstFileMb = computed(() => {
  const kbps = settings.value.gsrQuality === 'cbr'
    ? (settings.value.gsrCbrBitrate ?? 8000)
    : ({ medium: 4000, high: 6000, very_high: 12000, ultra: 20000 } as Record<string, number>)[settings.value.gsrQuality] ?? 8000
  return ((kbps * settings.value.gsrReplaySecs) / 8 / 1024).toFixed(0)
})
const gsrEstRamMb = computed(() => {
  // Rough heuristic: replay RAM ≈ file size × 1.2 (encoder buffers + ring buffer overhead)
  return Math.ceil(Number(gsrEstFileMb.value) * 1.2)
})

// Restart GSR when capture track sources change
watch(
  () => settings.value.captureTracks.map((t: { source: string }) => t.source).join(','),
  () => restartGsr(),
)

const isDefaultShortcuts = computed(() =>
  JSON.stringify(settings.value.shortcuts) === JSON.stringify(DEFAULTS.settings.shortcuts)
)
function resetShortcuts() { persist.resetShortcuts() }

// ─── Capture tracks ───
const CAPTURE_SOURCES = ['Game', 'Chat', 'Media', 'Aux', 'Mic']
const captureSourceOptions = computed(() =>
  CAPTURE_SOURCES.map(s => ({ value: s, label: t(`settings.captureSound.sources.${s}`) }))
)

// Dynamic audio sinks from PipeWire (populated on mount via list_audio_sinks)
const audioSinkOptions = ref<Array<{ value: string; label: string }>>([])

// Session type + dynamic monitor list for GSR target dropdown
const sessionType = ref<'x11' | 'wayland' | 'unknown'>('unknown')
const monitorOptions = ref<Array<{ value: string; label: string }>>([
  { value: 'screen', label: 'Primary Monitor' },
])
const isWayland = computed(() => sessionType.value === 'wayland')

onMounted(async () => {
  try {
    const sinks = await invoke<string[]>('list_audio_sinks')
    audioSinkOptions.value = sinks.map(name => ({ value: name, label: name }))
  } catch {
    // Fallback to static OpenGG sinks if pactl is unavailable
    audioSinkOptions.value = CAPTURE_SOURCES.map(s => ({ value: `OpenGG_${s}`, label: `OpenGG_${s}` }))
  }
  try {
    sessionType.value = (await invoke<string>('get_session_type')) as typeof sessionType.value
  } catch { /* ignore */ }
  try {
    const monitors = await invoke<Array<{ name: string; label: string }>>('list_monitors')
    const opts = monitors.map(m => ({ value: m.name, label: m.label }))
    if (!isWayland.value) {
      opts.push({ value: 'focused', label: 'Fullscreen Application' })
    }
    monitorOptions.value = opts
  } catch { /* keep default */ }
})
function addCaptureTrack() {
  const n = settings.value.captureTracks.length + 1
  settings.value.captureTracks.push({ name: `Track ${n}`, source: 'Game' })
}
function removeCaptureTrack(i: number) {
  settings.value.captureTracks.splice(i, 1)
}

// ─── Track definitions (Timeline Tracks) ───
const colorInputRefs = ref<HTMLInputElement[]>([])
function setColorRef(el: any, idx: number) { if (el) colorInputRefs.value[idx] = el }
function openColorPicker(idx: number) { colorInputRefs.value[idx]?.click() }


function addTrackDef() {
  const idx = settings.value.trackDefs.length
  settings.value.trackDefs.push({ id: `A${idx}`, name: `Audio ${idx}`, color: '#64748b', icon: 'game', visible: true })
}
function removeTrackDef(i: number) {
  if (settings.value.trackDefs.length <= 1) return
  settings.value.trackDefs.splice(i, 1)
}

// ─── Epic 3: Danger Zone ───
const dangerLoading = ref(false)
const gsrInstallOpen = ref(false)
const dangerMsg = ref('')
async function removeVirtualAudio() {
  const confirmed = await ask(t('settings.dangerZone.confirmMsg'), { title: t('settings.dangerZone.title'), kind: 'warning' })
  if (!confirmed) return
  dangerLoading.value = true; dangerMsg.value = ''
  try {
    await invoke('remove_virtual_audio')
    audio.setVirtualAudioReady(false)
    dangerMsg.value = '✓ Virtual audio removed.'
  } catch (e) { dangerMsg.value = `Error: ${e}` }
  finally { dangerLoading.value = false }
}

// ─── Clip directories ───
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

// ─── Screenshot directories ───
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
  return path.startsWith('/') ? mediaUrl(path, mediaPort.value) : path
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
watch(active, v => { if (v === 'storage') loadStorage() })
onMounted(() => { if (active.value === 'storage') loadStorage() })

async function importSteamLibrary(forcePrompt = false) {
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
// ─── Misc ───
async function openExternal(url: string) {
  try { await openUrl(url) } catch { window.open(url, '_blank') }
}

// SelectField option helpers
const clickOptions = computed(() => [
  { value: 'preview', label: t('settings.clipSettings.defaultClickPreview') },
  { value: 'editor',  label: t('settings.clipSettings.defaultClickEditor') },
])
const dateFormatOptions = [
  { value: 'YMD', label: 'YYYY/MM/DD' },
  { value: 'YDM', label: 'YYYY/DD/MM' },
]
const notificationPositionOptions = [
  { value: 'top-right', label: t('settings.notificationsPage.positionTopRight') },
  { value: 'top-left', label: t('settings.notificationsPage.positionTopLeft') },
  { value: 'bottom-right', label: t('settings.notificationsPage.positionBottomRight') },
  { value: 'bottom-left', label: t('settings.notificationsPage.positionBottomLeft') },
]

// ─── Extensions ───
interface ExtensionInfo {
  id: string
  name: string
  description: string
  version: string
  path: string
  has_settings?: boolean
  icon?: string | null
  main?: string | null
  ui?: string | null
  /** True for extensions bundled as built-in hardcoded cards (no path on disk) */
  _builtin?: boolean
  /** Inline SVG markup for built-in extensions */
  _svgIcon?: string
  /** Accent color for built-in extensions */
  _color?: string
}

const scannedExtensions = ref<ExtensionInfo[]>([])
const extensionScanLoading = ref(false)

/** Returns the icon URL for an extension loaded from the extensions directory. */
function getExtensionIconUrl(p: ExtensionInfo): string | null {
  if (p._builtin || !p.icon || !mediaPort.value) return null
  return `http://localhost:${mediaPort.value}/ext/${encodeURIComponent(p.id)}/${encodeURIComponent(p.icon)}`
}

/** Gear button is only shown if the extension has declared hasSettings: true. */
function canConfigure(p: ExtensionInfo): boolean {
  return !!p.has_settings
}

/** True if extension is currently enabled (keyed by id in persistence). */
function isExtEnabled(p: ExtensionInfo): boolean {
  return persist.state.extensions[p.id] ?? true
}
function setExtEnabled(p: ExtensionInfo, val: boolean) {
  persist.state.extensions[p.id] = val
}

async function scanExtensions() {
  extensionScanLoading.value = true
  try { scannedExtensions.value = await invoke<ExtensionInfo[]>('scan_extensions') }
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

// Scan whenever the extensions section becomes active
watch(active, v => { if (v === 'extensions') scanExtensions() })

// Auto-rescan when the extensions folder changes (file watcher event from Rust)
onMounted(async () => {
  await listen('plugins-changed', () => {
    if (active.value === 'extensions') scanExtensions()
  })
})
</script>

<template>
  <!-- keydown listener for shortcut recording (global within settings) -->
  <div class="settings-layout" @keydown="onShortcutKeydown" tabindex="-1">

    <!-- ── Left Sidebar Nav ── -->
    <aside class="settings-nav">
      <div v-for="group in navGroups" :key="group.key" class="nav-group">
        <div class="nav-group-label">{{ group.label }}</div>
        <button
          v-for="item in group.items" :key="item.key"
          class="nav-item" :class="{ active: active === item.key }"
          @click="active = item.key"
        >
          {{ item.label }}
          <span v-if="item.badge" class="nav-badge">{{ item.badge }}</span>
        </button>
      </div>
    </aside>

    <!-- ── Content ── -->
    <div class="settings-content">

      <!-- ════════════════════ GENERAL ════════════════════ -->
      <section v-if="active === 'general'">
        <h2 class="sec-title">{{ t('settings.general.title') }}</h2>

        <div class="card">
          <div class="card-head">
            {{ t('settings.general.themeFile') }}
            <InfoIcon :title="t('settings.general.themeHint')" />
            <div class="theme-actions">
              <button
                class="theme-icon-btn"
                :title="themeDarkMode ? 'Switch to Light Mode' : 'Switch to Dark Mode'"
                @click="themeDarkMode = !themeDarkMode; onToggleDarkMode()"
              >
                <svg v-if="!themeDarkMode" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/></svg>
                <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12.79A9 9 0 1111.21 3 7 7 0 0021 12.79z"/></svg>
              </button>
              <button
                class="theme-icon-btn"
                :title="themeLoading ? 'Reloading…' : t('settings.general.reloadTheme')"
                :disabled="themeLoading"
                @click="reloadTheme"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" :class="{ spinning: themeLoading }"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 11-2.12-9.36L23 10"/></svg>
              </button>
            </div>
          </div>
          <div class="color-row">
            <input type="color" v-model="themeAccent" class="color-picker" @input="onAccentInput" />
            <input type="text" v-model="themeAccent" class="color-hex" spellcheck="false" @input="onAccentInput" />
          </div>
        </div>

        <div class="card">
          <div class="card-head">{{ t('settings.clipPreferences.title') }}</div>
          <div class="form-grid">
            <div class="field">
              <label>{{ t('settings.clipSettings.defaultClick') }}</label>
              <SelectField v-model="settings.defaultClickAction" :options="clickOptions" />
            </div>
            <div class="field">
              <label>{{ t('settings.general.dateFormat') }}<InfoIcon :title="t('settings.general.dateFormatHint')" /></label>
              <SelectField v-model="settings.dateFormat" :options="dateFormatOptions" />
            </div>
          </div>
        </div>

        <!-- ★ Epic 4: Daemon & Startup toggles -->
        <div class="card">
          <div class="card-head">{{ t('settings.daemon.title') }}</div>
          <div class="daemon-toggle-row">
            <div class="daemon-toggle-info">
              <span class="tname">
                {{ t('settings.daemon.runAtStartup') }}
                <InfoIcon :title="t('settings.daemon.runAtStartupTooltip')" />
              </span>
            </div>
            <button class="toggle-btn" :class="{ on: settings.runAtStartup }"
                    @click="settings.runAtStartup = !settings.runAtStartup; onRunAtStartupChange()">
              <span class="toggle-knob"></span>
            </button>
          </div>
          <div class="daemon-toggle-row">
            <div class="daemon-toggle-info">
              <span class="tname">
                {{ t('settings.daemon.keepInBackground') }}
                <InfoIcon :title="t('settings.daemon.keepInBackgroundTooltip')" />
              </span>
            </div>
            <button class="toggle-btn" :class="{ on: settings.runInBackground }"
                    @click="settings.runInBackground = !settings.runInBackground; onRunInBackgroundChange()">
              <span class="toggle-knob"></span>
            </button>
          </div>
        </div>

        <!-- ★ Epic 2: Diagnostics / crash log -->
        <div class="card">
          <div class="card-head">{{ t('settings.diagnostics.title') }} <InfoIcon :title="t('settings.diagnostics.hint')" /></div>
          <div class="action-row">
            <button class="btn btn-accent" @click="openCrashLogsFolder">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>
              {{ t('settings.diagnostics.openCrashLogs') }}
            </button>
          </div>
        </div>
      </section>

      <!-- ════════════════════ LANGUAGE ════════════════════ -->
      <section v-if="active === 'language'">
        <h2 class="sec-title">{{ t('settings.language.title') }}</h2>
        <div class="card">
          <div class="card-head">
            {{ t('settings.language.selectLanguage') }}
            <InfoIcon :title="t('settings.language.hint')" />
            <div class="lang-actions">
              <button class="theme-icon-btn" @click="openLocalesFolder" aria-label="Open locales folder">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/><line x1="12" y1="11" x2="12" y2="17"/><line x1="9" y1="14" x2="15" y2="14"/></svg>
                <span class="info-tooltip-wrap"><span class="btn-tooltip">{{ t('settings.language.addLanguage') }}</span></span>
              </button>
              <button class="theme-icon-btn" :disabled="localesReloading" @click="loadUserLocales" aria-label="Reload languages">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" :class="{ spinning: localesReloading }"><path d="M23 4v6h-6"/><path d="M1 20v-6h6"/><path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"/></svg>
                <span class="info-tooltip-wrap"><span class="btn-tooltip">{{ t('settings.language.reloadLanguages') }}</span></span>
              </button>
              <button
                v-if="isRtlLanguage"
                class="theme-icon-btn"
                :class="{ active: settings.rtlMode }"
                @click="settings.rtlMode ? disableRtl() : enableRtl()"
                aria-label="Toggle RTL layout"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 10H3"/><path d="M21 6H3"/><path d="M21 14H3"/><path d="M17 18H3"/><polyline points="21 10 17 14 21 18"/></svg>
                <span class="info-tooltip-wrap"><span class="btn-tooltip">{{ t('settings.language.rtlModeHint') }}</span></span>
              </button>
            </div>
          </div>
          <div class="lang-list">
            <button
              v-for="lang in LANGUAGES" :key="lang.code"
              class="lang-btn" :class="{ active: settings.language === lang.code }"
              @click="setLanguage(lang.code)"
            >
              <span class="lang-left">
                <span class="lang-code">{{ lang.code.toUpperCase() }}</span>
                <span class="lang-label">{{ lang.label }}</span>
              </span>
              <span class="lang-dir-badge">{{ lang.dir === 'rtl' ? 'RTL' : 'LTR' }}</span>
            </button>
          </div>
        </div>

      </section>

      <!-- ════════════════════ SHORTCUTS ════════════════════ -->
      <section v-if="active === 'shortcuts'">
        <h2 class="sec-title">{{ t('settings.shortcuts.title') }}</h2>
        <div class="card">
          <div class="shortcut-hdr">
            <span class="shortcut-hdr-label">{{ t('settings.shortcuts.title') }} <InfoIcon :title="t('settings.shortcuts.hint')" /></span>
            <button class="btn-reset-sc" :disabled="isDefaultShortcuts" @click="resetShortcuts">{{ t('settings.shortcuts.resetToDefaults') }}</button>
          </div>
          <div class="shortcut-list">
            <div
              v-for="action in shortcutActions" :key="action.key"
              class="shortcut-row"
            >
              <span class="shortcut-action">
                {{ action.label }}
                <InfoIcon :title="action.hint" />
              </span>
              <button
                class="shortcut-key"
                :class="{ recording: recordingKey === action.key }"
                @click="recordingKey === action.key ? cancelRecord() : startRecord(action.key)"
              >
                <span v-if="recordingKey === action.key" class="rec-dot"></span>
                {{ recordingKey === action.key
                    ? t('settings.shortcuts.recording')
                    : (settings.shortcuts as Record<string,string>)[action.key] || '—' }}
              </button>
            </div>
          </div>
        </div>
      </section>

      <!-- ════════════════════ MIXER ROUTING ════════════════════ -->
      <section v-if="active === 'mixerRouting'">
        <h2 class="sec-title">{{ t('settings.sections.mixerRouting') }}</h2>
        <div class="card">
          <div class="placeholder-box">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2z"/></svg>
            <span>{{ t('settings.mixerRouting.comingSoon') }}</span>
          </div>
        </div>

        <!-- ★ Epic 3: Danger Zone -->
        <div class="card danger-zone-card">
          <div class="card-head danger-head">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:15px;height:15px;flex-shrink:0"><path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
            {{ t('settings.dangerZone.title') }}
          </div>
          <p class="hint" style="color:color-mix(in srgb,var(--danger) 80%,var(--text-sec))">{{ t('settings.dangerZone.subtitle') }}</p>
          <div class="danger-action-row">
            <div class="danger-info">
              <span class="danger-label">
                {{ t('settings.dangerZone.removeVirtualAudio') }}
                <InfoIcon :title="t('settings.dangerZone.removeVirtualAudioDesc')" />
              </span>
            </div>
            <button class="btn-danger" :disabled="dangerLoading" @click="removeVirtualAudio">
              <svg v-if="!dangerLoading" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6m3 0V4a1 1 0 011-1h4a1 1 0 011 1v2"/></svg>
              <span>{{ dangerLoading ? t('settings.captureGsr.removeVirtualAudioRemoving') : t('settings.dangerZone.removeVirtualAudio') }}</span>
            </button>
          </div>
          <div v-if="dangerMsg" class="danger-msg" :class="{ 'danger-ok': dangerMsg.startsWith('✓') }">{{ dangerMsg }}</div>
        </div>
      </section>

      <!-- ════════════════════ CAPTURE & SOUND ════════════════════ -->
      <section v-if="active === 'captureSound'">
        <h2 class="sec-title">{{ t('settings.captureSound.title') }}</h2>

        <!-- GPU Screen Recorder panel (top) -->
        <div class="card">
          <div class="card-head gsr-head">
            <span>{{ t('settings.captureGsr.title') }}</span>
            <span class="badge-beta">Beta</span>
            <InfoIcon :title="t('settings.captureGsr.hint')" />
            <span v-if="settings.gsrEnabled" class="gsr-est">
              Est. RAM: ~{{ gsrEstRamMb }} MB &nbsp;|&nbsp; File: ~{{ gsrEstFileMb }} MB
            </span>
          </div>
          <div v-if="settings.gsrEnabled" class="form-grid gsr-grid">
            <div class="field">
              <label>{{ t('settings.captureGsr.quality') }}</label>
              <SelectField v-model="settings.gsrQuality" :options="gsrQualityOptions" @update:modelValue="restartGsr" />
              <input
                v-if="settings.gsrQuality === 'cbr'"
                type="number"
                class="gsr-custom-secs"
                :value="settings.gsrCbrBitrate"
                min="500"
                max="100000"
                step="500"
                placeholder="Bitrate (kbps)"
                title="Target bitrate in kbps (e.g. 8000 = 8 Mbps)"
                @change="e => { settings.gsrCbrBitrate = Math.max(500, Math.min(100000, Number((e.target as HTMLInputElement).value) || 8000)); restartGsr() }"
              />
            </div>
            <div class="field">
              <label>{{ t('settings.captureGsr.fps') }}</label>
              <SelectField v-model="settings.gsrFps" :options="gsrFpsOptions" @update:modelValue="restartGsr" />
            </div>
            <div class="field">
              <label>{{ t('settings.captureGsr.replayBuffer') }}</label>
              <SelectField
                :modelValue="settings.gsrReplayPreset"
                :options="gsrReplayPresets"
                @update:modelValue="onReplayPresetChange"
              />
              <input
                v-if="settings.gsrReplayPreset === 'custom'"
                type="number"
                class="gsr-custom-secs"
                :value="settings.gsrReplaySecs"
                min="5"
                max="600"
                placeholder="Seconds"
                @change="onCustomReplaySecs"
              />
            </div>
            <div class="field">
              <label>{{ t('settings.captureGsr.monitorTarget') }}</label>
              <SelectField
                v-model="settings.gsrMonitorTarget"
                :options="monitorOptions"
                @update:modelValue="restartGsr"
              />
              <span v-if="isWayland" class="hint" style="color:var(--warn,#f59e0b);margin-top:4px;font-size:11px">
                {{ t('settings.captureGsr.waylandHint') }}
              </span>
            </div>
          </div>
          <div v-if="settings.gsrEnabled" class="gsr-toggle-row">
            <span class="gsr-label">{{ t('settings.captureGsr.autoStart') }}
              <InfoIcon :title="t('settings.captureGsr.autoStartTooltip')" />
            </span>
            <button class="toggle-btn" :class="{ on: settings.gsrAutoStart }"
                    @click="settings.gsrAutoStart = !settings.gsrAutoStart">
              {{ settings.gsrAutoStart ? 'On' : 'Off' }}
            </button>
          </div>
          <div v-else class="hint" style="margin-top:8px">{{ t('settings.captureGsr.extensionsHint') }}</div>
        </div>

        <!-- OBS-style Audio Capture Devices -->
        <div class="card">
          <div class="card-head">{{ t('settings.captureSound.captureDevices') }} <InfoIcon :title="t('settings.captureSound.captureHint')" /></div>
          <div class="capture-tracks">
            <div
              v-for="(track, i) in settings.captureTracks"
              :key="i"
              class="capture-row"
            >
              <span class="capture-track-name">{{ t('settings.captureSound.trackLabel') }} {{ i + 1 }}</span>
              <div class="capture-select-wrap">
                <SelectField
                  v-model="track.source"
                  :options="captureSourceOptions"
                />
              </div>
              <button
                v-if="settings.captureTracks.length > 1"
                class="btn-icon btn-remove"
                @click="removeCaptureTrack(i)"
                title="Remove track"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 6L6 18M6 6l12 12"/></svg>
              </button>
            </div>
          </div>
          <button class="btn btn-ghost add-row" @click="addCaptureTrack">
            {{ t('settings.captureSound.addTrack') }}
          </button>
        </div>
      </section>

      <!-- ════════════════════ TIMELINE TRACKS ════════════════════ -->
      <section v-if="active === 'trackManagement'">
        <h2 class="sec-title">{{ t('settings.timelineTracks.title') }}</h2>

        <div class="card">
          <div class="card-head">{{ t('settings.timelineTracks.trackList') }} <InfoIcon :title="t('settings.timelineTracks.trackListHint')" /></div>
          <div class="tdef-list">
            <div v-for="(def, idx) in settings.trackDefs" :key="def.id" class="tdef-row">
              <button
                class="track-vis-btn"
                :class="{ active: def.visible }"
                :title="t('settings.timelineTracks.visibilityTooltip')"
                @click="def.visible = !def.visible"
              >
                <svg v-if="def.visible" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
                  <circle cx="12" cy="12" r="3"/>
                </svg>
                <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M17.94 17.94A10.07 10.07 0 0112 20c-7 0-11-8-11-8a18.45 18.45 0 015.06-5.94M9.9 4.24A9.12 9.12 0 0112 4c7 0 11 8 11 8a18.5 18.5 0 01-2.16 3.19m-6.72-1.07a3 3 0 11-4.24-4.24"/>
                  <line x1="1" y1="1" x2="23" y2="23"/>
                </svg>
              </button>
              <div class="tdef-swatch" :style="{ background: def.color }" @click="openColorPicker(idx)" title="Pick color"></div>
              <input type="color" :ref="(el) => setColorRef(el, idx)" :value="def.color" class="tdef-color-input tdef-color-hidden"
                @input="def.color = ($event.target as HTMLInputElement).value" />
              <input type="text" v-model="def.name" class="tdef-name-input" :placeholder="def.id" maxlength="20" />
              <IconPicker v-model="def.icon" />
              <button
                class="btn-icon btn-remove"
                :disabled="def.id === 'V1' || def.id === 'O1'"
                :title="def.id === 'V1' ? 'The primary Video track cannot be deleted'
                      : def.id === 'O1' ? 'The Overlays track cannot be deleted'
                      : 'Remove'"
                @click="def.id !== 'V1' && def.id !== 'O1' && removeTrackDef(idx)"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 6L6 18M6 6l12 12"/></svg>
              </button>
            </div>
          </div>
          <button class="btn btn-ghost add-row" @click="addTrackDef">{{ t('settings.timelineTracks.addAudioTrack') }}</button>
        </div>

        <!-- Live Preview -->
        <div class="card">
          <div class="card-head">{{ t('settings.timelineTracks.livePreview') }} <InfoIcon :title="t('settings.timelineTracks.livePreviewHint')" /></div>
          <div class="tl-preview">
            <div v-for="def in settings.trackDefs" :key="def.id" class="tl-preview-row" :style="{ '--pv': def.color }">
              <div class="tl-pv-accent"></div>
              <span class="tl-pv-id">{{ def.id }}</span>
              <svg v-if="def.visible" class="tl-pv-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path v-if="def.icon==='video'"   d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M3 6h10a2 2 0 012 2v8a2 2 0 01-2 2H3a2 2 0 01-2-2V8a2 2 0 012-2z"/>
                <path v-else-if="def.icon==='game'"    d="M6 11h4m-2-2v4m7-1h.01M18 11h.01M2 6a2 2 0 012-2h16a2 2 0 012 2v10a4 4 0 01-4 4H6a4 4 0 01-4-4V6z"/>
                <path v-else-if="def.icon==='mic'"     d="M12 1a3 3 0 00-3 3v8a3 3 0 006 0V4a3 3 0 00-3-3zM19 10v2a7 7 0 01-14 0v-2M12 19v3M8 23h8"/>
                <path v-else-if="def.icon==='chat'"    d="M3 18v-6a9 9 0 0118 0v6M21 19a2 2 0 01-2 2h-1a2 2 0 01-2-2v-3a2 2 0 012-2h3zM3 19a2 2 0 002 2h1a2 2 0 002-2v-3a2 2 0 00-2-2H3z"/>
                <path v-else-if="def.icon==='media'"   d="M9 18V5l12-2v13M9 19c0 1.1-1.34 2-3 2s-3-.9-3-2 1.34-2 3-2 3 .9 3 2zm12-3c0 1.1-1.34 2-3 2s-3-.9-3-2 1.34-2 3-2 3 .9 3 2z"/>
                <path v-else-if="def.icon==='overlay'" d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/>
              </svg>
              <span class="tl-pv-name" :style="{ color: def.color }">{{ def.name || def.id }}</span>
              <div class="tl-pv-track-body">
                <div class="tl-pv-bar" :style="{ background: def.color }"></div>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- ════════════════════ STORAGE ════════════════════ -->
      <section v-if="active === 'storage'">
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

      <!-- ════════════════════ EXTENSIONS ════════════════════ -->
      <section v-if="active === 'extensions'">
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
              <code class="install-cmd">sudo add-apt-repository ppa:dec05eba/gpu-screen-recorder &amp;&amp; sudo apt install gpu-screen-recorder</code>
            </div>
            <div class="install-section">
              <span class="install-distro">Arch / Manjaro</span>
              <code class="install-cmd">yay -S gpu-screen-recorder</code>
            </div>
            <div class="install-section">
              <span class="install-distro">Fedora</span>
              <code class="install-cmd">sudo dnf install gpu-screen-recorder</code>
            </div>
          </div>
          <div class="gsr-toggle-row">
            <span class="gsr-label">Enable GSR Replay Buffer</span>
            <button class="toggle-btn" :class="{ on: settings.gsrEnabled }" @click="toggleGsr">
              <span class="toggle-knob"></span>
            </button>
          </div>
        </div>

        <!-- ─── Unified Extensions List ─── -->
        <div class="card">
          <div class="card-head ext-section-head">
            <span>{{ t('settings.extensions.sectionTitle') }}</span>
            <div class="ext-head-actions">
              <!-- Refresh button — manually re-triggers scan_extensions -->
              <button class="ext-icon-btn" :title="t('settings.extensions.refresh')" @click="refreshExtensions" :disabled="extensionScanLoading">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <polyline points="23 4 23 10 17 10"/>
                  <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
                </svg>
              </button>
              <!-- Open folder button -->
              <button class="ext-icon-btn" :title="t('settings.extensions.openFolder')" @click="openExtensionsFolder">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/>
                </svg>
              </button>
            </div>
          </div>

          <div v-if="extensionScanLoading" class="hint" style="padding:8px 0">{{ t('settings.extensions.scanning') }}</div>
          <div v-else-if="!scannedExtensions.length" class="hint" style="padding:8px 0">{{ t('settings.extensions.noExtensions') }}</div>
          <div v-else>
            <div v-for="p in scannedExtensions" :key="p.id" class="ext-card-row">
              <!-- Icon: from extension folder or fallback SVG -->
              <div class="ext-card-icon-wrap" :style="p._color ? `--ext-clr: ${p._color}` : ''">
                <img v-if="getExtensionIconUrl(p)"
                     :src="getExtensionIconUrl(p)!"
                     class="plugin-icon"
                     alt=""
                     @error="($event.target as HTMLImageElement).style.display='none'" />
                <!-- Generic plugin icon fallback -->
                <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
                  <path d="M20.24 12.24a6 6 0 0 0-8.49-8.49L5 10.5V19h8.5z"/>
                  <line x1="16" y1="8" x2="2" y2="22"/>
                  <line x1="17.5" y1="15" x2="9" y2="15"/>
                </svg>
              </div>
              <div class="ext-card-info">
                <div class="ext-card-title-row">
                  <span class="ext-name">{{ p.name }}</span>
                  <span class="plugin-ver">v{{ p.version }}</span>
                  <!-- Gear button only if extension declares hasSettings: true in manifest -->
                  <button v-if="canConfigure(p)" class="ext-gear-btn" :title="t('settings.extensions.title')" @click.stop>
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <circle cx="12" cy="12" r="3"/>
                      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
                    </svg>
                  </button>
                </div>
                <p class="ext-desc">{{ p.description }}</p>
              </div>
              <!-- Enable/disable toggle keyed by extension id -->
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
      </section>

      <!-- ════════════════════ STORE ════════════════════ -->
      <section v-if="active === 'store'">
        <h2 class="sec-title">{{ t('settings.store.title') }}</h2>
        <div class="card store-coming-soon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="store-icon">
            <path d="M6 2 3 6v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6l-3-4z"/>
            <line x1="3" y1="6" x2="21" y2="6"/>
            <path d="M16 10a4 4 0 0 1-8 0"/>
          </svg>
          <p class="store-coming-soon-title">{{ t('settings.store.comingSoon') }}</p>
          <p class="hint">{{ t('settings.store.comingSoonDesc') }}</p>
        </div>
      </section>

      <!-- ════════════════════ ABOUT ════════════════════ -->
      <section v-if="active === 'about'">
        <h2 class="sec-title">{{ t('settings.sections.about') }}</h2>

        <!-- Hero card -->
        <div class="hero-card">
          <svg class="logo-svg" viewBox="0 0 512 512" width="90" height="90" style="fill-rule:evenodd;clip-rule:evenodd;">
            <g transform="matrix(2.778643,0,0,2.778643,-447.380743,-285.942888)">
              <path d="M291.671,187.47L332.636,187.47L340.205,195.039L280.739,254.504L259.3,232.814L271.068,221.046L280.97,230.948L307.949,203.97L291.671,203.97L291.671,187.47ZM285.002,195.039L225.537,254.504L166.072,195.039L225.537,135.573L247.24,157.276L235.885,168.632L226.383,159.129L190.473,195.039L226.383,230.948L241.783,215.548L221.274,195.039L280.739,135.573L311.599,166.433L300.04,177.991L280.97,159.129L265.032,175.068L285.002,195.039ZM253.289,211.821L270.072,195.039L253.289,178.256L236.507,195.039L253.289,211.821Z" fill="var(--accent)"/>
            </g>
          </svg>
          <h1 class="about-app-name">OpenGG</h1>
          <p class="version-badge">v{{ appVersion }}</p>
          <p class="about-tagline">{{ t('settings.aboutPage.tagline') }}</p>
          <p class="about-desc">{{ t('settings.aboutPage.description') }}</p>
        </div>

        <!-- Project Goals -->
        <div class="card goals-card">
          <div class="card-head">{{ t('settings.aboutPage.projectGoals') }}</div>
          <ul class="goals-list">
            <li>{{ t('settings.aboutPage.goal_1') }}</li>
            <li>{{ t('settings.aboutPage.goal_2') }}</li>
            <li>{{ t('settings.aboutPage.goal_3') }}</li>
          </ul>
        </div>

        <!-- Connect With Us -->
        <div class="card connect-card">
          <div class="card-head">{{ t('settings.aboutPage.connectWithUs') }}</div>
          <div class="social-links">
            <button class="social-btn" @click="openExternal('https://github.com/UPdullah895/opengg')">
              <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor"><path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z"/></svg>
              {{ t('settings.aboutPage.github') }}
            </button>
            <button class="social-btn" @click="openExternal('#')">
              <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor"><path d="M20.317 4.37a19.791 19.791 0 00-4.885-1.515.074.074 0 00-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 00-5.487 0 12.64 12.64 0 00-.617-1.25.077.077 0 00-.079-.037A19.736 19.736 0 003.677 4.37a.07.07 0 00-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 00.031.057 19.9 19.9 0 005.993 3.03.078.078 0 00.084-.028 14.09 14.09 0 001.226-1.994.076.076 0 00-.041-.106 13.107 13.107 0 01-1.872-.892.077.077 0 01-.008-.128 10.2 10.2 0 00.372-.292.074.074 0 01.077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 01.078.01c.12.098.246.198.373.292a.077.077 0 01-.006.127 12.299 12.299 0 01-1.873.892.077.077 0 00-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 00.084.028 19.839 19.839 0 006.002-3.03.077.077 0 00.032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 00-.031-.03zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z"/></svg>
              {{ t('settings.aboutPage.discord') }}
            </button>
            <button class="social-btn" @click="openExternal('#')">
              <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 014 10 15.3 15.3 0 01-4 10 15.3 15.3 0 01-4-10 15.3 15.3 0 014-10z"/></svg>
              {{ t('settings.aboutPage.website') }}
            </button>
          </div>
        </div>

        <p class="about-contributors">{{ t('settings.aboutPage.contributors') }}</p>
      </section>

      <!-- ════════════════════ NOTIFICATIONS ════════════════════ -->
      <section v-if="active === 'notifications'">
        <h2 class="sec-title">{{ t('settings.notificationsPage.title') }}</h2>

        <!-- Notification Style card -->
        <div class="card">
          <div class="card-head">{{ t('settings.notificationsPage.style') }} <InfoIcon :title="t('settings.notificationsPage.description')" /></div>

          <!-- Style selection grid -->
          <div class="notif-style-grid">
            <div class="notif-option" :class="{ active: settings.notificationStyle === 'auto' }" @click="settings.notificationStyle = 'auto'">
              <div class="notif-option-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 014 10 15.3 15.3 0 01-4 10 15.3 15.3 0 01-4-10 15.3 15.3 0 014-10z"/></svg>
              </div>
              <span class="notif-option-label">{{ t('settings.notificationsPage.styleAuto') }}</span>
            </div>
            <div class="notif-option" :class="{ active: settings.notificationStyle === 'gsr-notify' }" @click="settings.notificationStyle = 'gsr-notify'">
              <div class="notif-option-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>
              </div>
              <span class="notif-option-label">{{ t('settings.notificationsPage.styleGsrNotify') }}</span>
            </div>
            <div class="notif-option" :class="{ active: settings.notificationStyle === 'x11-overlay' }" @click="settings.notificationStyle = 'x11-overlay'">
              <div class="notif-option-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="3" width="20" height="14" rx="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>
              </div>
              <span class="notif-option-label">{{ t('settings.notificationsPage.styleX11Overlay') }}</span>
            </div>
            <div class="notif-option" :class="{ active: settings.notificationStyle === 'system' }" @click="settings.notificationStyle = 'system'">
              <div class="notif-option-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9"/><path d="M13.73 21a2 2 0 01-3.46 0"/></svg>
              </div>
              <span class="notif-option-label">{{ t('settings.notificationsPage.styleSystem') }}</span>
            </div>
            <div class="notif-option" :class="{ active: settings.notificationStyle === 'disabled' }" @click="settings.notificationStyle = 'disabled'">
              <div class="notif-option-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="4.93" y1="4.93" x2="19.07" y2="19.07"/></svg>
              </div>
              <span class="notif-option-label">{{ t('settings.notificationsPage.styleDisabled') }}</span>
            </div>
          </div>
        </div>

        <!-- Position + Duration card — hidden when disabled -->
        <div class="card" v-if="settings.notificationStyle !== 'disabled'">
          <!-- Position -->
          <div class="field">
            <label>{{ t('settings.notificationsPage.position') }} <InfoIcon :title="t('settings.notificationsPage.positionDesc')" /></label>
            <SelectField v-model="settings.notificationPosition" :options="notificationPositionOptions" />
          </div>

          <!-- Duration — only for X11 Overlay -->
          <div class="field notif-duration-field" v-if="settings.notificationStyle === 'x11-overlay'">
            <label>{{ t('settings.notificationsPage.duration') }}: {{ settings.notificationDuration }}s <InfoIcon :title="t('settings.notificationsPage.durationDesc')" /></label>
            <input
              type="range"
              class="notif-duration-slider"
              :value="settings.notificationDuration"
              min="1"
              max="10"
              step="1"
              @input="settings.notificationDuration = Number(($event.target as HTMLInputElement).value)"
            />
          </div>
        </div>
      </section>

    </div><!-- /settings-content -->
  </div><!-- /settings-layout -->
</template>

<style scoped>
.settings-layout {
  display: flex; height: 100%; overflow: hidden;
  outline: none; /* prevent focus ring on the keydown trap div */
}

/* ── Left nav ── */
.settings-nav {
  width: 196px; flex-shrink: 0;
  border-right: 1px solid var(--border);
  padding: 4px 0; overflow-y: auto;
}
.nav-group { margin-bottom: 2px; }
.nav-group-label {
  font-size: 10px; font-weight: 800; letter-spacing: 1.2px;
  text-transform: uppercase; color: var(--text-muted);
  padding: 12px 16px 5px;
}
.nav-group-label:empty { display: none; padding: 4px 16px; }
.nav-item {
  display: flex; align-items: center; gap: 6px;
  width: 100%; padding: 8px 16px;
  background: transparent; border: none; color: var(--text-sec);
  font-size: 13px; text-align: left; cursor: pointer;
  transition: background .12s, color .12s, border-color .12s;
  border-right: 2px solid transparent;
}
.nav-item:hover { background: color-mix(in srgb, var(--accent) 10%, transparent); color: var(--accent); }
.nav-item.active {
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  color: var(--accent); border-right-color: var(--accent);
}
/* ★ Epic 5: Beta badge */
.nav-badge {
  margin-inline-start: auto;
  background: var(--accent); color: #fff;
  font-size: 9px; font-weight: 700; letter-spacing: .4px;
  padding: 1px 5px; border-radius: 4px; line-height: 1.5;
  text-transform: uppercase; flex-shrink: 0;
}

/* ── Content ── */
.settings-content {
  flex: 1; padding: 24px 28px; overflow-y: auto; min-width: 0;
}
.sec-title {
  font-size: 18px; font-weight: 700; margin-bottom: 20px;
  padding-bottom: 12px; border-bottom: 1px solid var(--border);
}

/* ── Card ── */
.card {
  background: var(--bg-card); border: 1px solid var(--border);
  border-radius: var(--radius-lg); padding: 20px; margin-bottom: 16px;
}
.card-head {
  display: flex; align-items: center;
  font-size: 13px; font-weight: 700; margin-bottom: 14px;
  padding-bottom: 10px; border-bottom: 1px solid var(--border);
}
.card-head-row {
  display: flex; align-items: center; justify-content: space-between;
  font-size: 13px; font-weight: 700; margin-bottom: 14px;
  padding-bottom: 10px; border-bottom: 1px solid var(--border);
}
.card-head-label { font-size: 13px; font-weight: 700; color: var(--text); }
.hint { font-size: 12px; color: var(--text-sec); margin-bottom: 14px; line-height: 1.5; }

/* ── Row setting (label + toggle) ── */
.row-setting { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 4px 0; }
.row-label { display: flex; flex-direction: column; gap: 3px; }
.row-title { font-size: 13px; font-weight: 600; }
.row-sub   { font-size: 11px; color: var(--text-muted); line-height: 1.4; }
/* iOS-style toggle */
.tog { position: relative; display: inline-flex; align-items: center; cursor: pointer; flex-shrink: 0; }
.tog input { position: absolute; opacity: 0; width: 0; height: 0; }
.tog-track { display: block; width: 40px; height: 22px; border-radius: 11px; background: var(--border); transition: background .2s; }
.tog input:checked ~ .tog-track { background: var(--accent); }
.tog-thumb { position: absolute; top: 3px; left: 3px; width: 16px; height: 16px; border-radius: 50%; background: #fff; box-shadow: 0 1px 3px rgba(0,0,0,.3); transition: left .2s; }
.tog input:checked ~ .tog-track .tog-thumb { left: 21px; }

/* ── Fields ── */
.field-row { display: flex; gap: 20px; flex-wrap: wrap; }
.field { flex: 1; min-width: 180px; }
.field label {
  display: block; font-size: 11px; font-weight: 700; text-transform: uppercase;
  letter-spacing: .5px; color: var(--text-sec); margin-bottom: 6px;
}
.form-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 16px; }
.gsr-grid { margin-top: 14px; }
.gsr-custom-secs {
  margin-top: 6px; width: 100%; padding: 5px 8px;
  border-radius: 5px; border: 1px solid var(--border);
  background: var(--bg-input); color: var(--text);
  font-size: 12px; outline: none;
}
.gsr-custom-secs:focus { border-color: var(--accent); }
.gsr-head { display: flex; align-items: center; gap: 8px; }
.gsr-est { margin-inline-start: auto; font-size: 11px; color: var(--text-muted); white-space: nowrap; }
.gsr-toggle-row { display: flex; align-items: center; justify-content: space-between; padding: 6px 0; }
.gsr-label { font-size: 13px; color: var(--text-sec); }
.gsr-install-toggle { display: block; margin: 6px 0; padding: 0; border: none; background: transparent; color: var(--accent); font-size: 12px; cursor: pointer; text-align: left; font-weight: 600; }
.gsr-install-toggle:hover { text-decoration: underline; }
.gsr-install-guide { margin: 8px 0 12px; padding: 12px; background: var(--bg-deep); border: 1px solid var(--border); border-radius: var(--radius, 6px); display: flex; flex-direction: column; gap: 10px; }
.install-section { display: flex; flex-direction: column; gap: 4px; }
.install-distro { font-size: 10px; font-weight: 700; color: var(--text-sec); text-transform: uppercase; letter-spacing: .5px; }
.install-cmd { font-family: monospace; font-size: 11px; color: var(--text); background: var(--bg-input); padding: 6px 8px; border-radius: 4px; border: 1px solid var(--border); display: block; word-break: break-all; }
.badge-beta { font-size: 10px; font-weight: 700; background: var(--accent); color: #fff; padding: 2px 6px; border-radius: 4px; letter-spacing: .4px; }
.toggle-btn {
  width: 40px; height: 22px; border-radius: 11px; border: none; cursor: pointer;
  background: var(--bg-deep); border: 1px solid var(--border); position: relative;
  transition: background .2s, border-color .2s; flex-shrink: 0;
}
.toggle-btn.on { background: var(--accent); border-color: var(--accent); }
.toggle-knob {
  position: absolute; top: 2px; left: 2px; width: 16px; height: 16px;
  background: #fff; border-radius: 50%; transition: left .2s;
}
.toggle-btn.on .toggle-knob { left: 20px; }

/* ── Buttons ── */
.btn {
  display: inline-flex; align-items: center; gap: 6px;
  padding: 7px 14px; border: 1px solid var(--border); border-radius: var(--radius);
  background: var(--bg-card); color: var(--text-sec); font-size: 12px; cursor: pointer;
  white-space: nowrap; transition: background .12s, color .12s, border-color .12s;
}
.btn svg { width: 14px; height: 14px; }
.btn:hover { background: var(--bg-hover); color: var(--text); }
.btn:disabled { opacity: .45; cursor: not-allowed; }
.btn-accent { border-color: var(--accent); color: var(--accent); }
.btn-accent:hover { background: color-mix(in srgb, var(--accent) 10%, transparent); }
.btn-warn { border-color: var(--danger); color: var(--danger); }
.btn-warn:hover { background: color-mix(in srgb, var(--danger) 10%, transparent); }
.btn-ghost { border-color: transparent; color: var(--accent); font-size: 12px; padding: 5px 0; }
.btn-ghost:hover { background: transparent; color: color-mix(in srgb, var(--accent) 70%, white); }

.btn-icon {
  width: 26px; height: 26px; display: flex; align-items: center; justify-content: center;
  background: transparent; border: 1px solid transparent; border-radius: var(--radius);
  cursor: pointer; color: var(--text-muted); transition: all .12s;
}
.btn-icon svg { width: 13px; height: 13px; }
.btn-remove:hover { border-color: var(--danger); color: var(--danger); background: color-mix(in srgb, var(--danger) 8%, transparent); }

/* ── Color row (accent) ── */
.color-row { display: flex; gap: 8px; align-items: center; }
.color-picker { width: 38px; height: 34px; border: 1px solid var(--border); border-radius: var(--radius); background: none; cursor: pointer; padding: 2px; }
.color-hex { width: 88px; padding: 7px 10px; background: var(--bg-input); border: 1px solid var(--border); border-radius: var(--radius); color: var(--text); font-size: 12px; font-family: monospace; outline: none; color-scheme: dark; }

/* ── Toggles ── */
.toggle-row {
  display: flex; align-items: center; gap: 12px; padding: 9px 12px; margin-bottom: 5px;
  background: var(--bg-input); border: 1px solid var(--border); border-radius: var(--radius);
  cursor: pointer; font-size: 13px;
}
.toggle-row input { accent-color: var(--accent); width: 15px; height: 15px; flex-shrink: 0; }
.tname { font-weight: 600; min-width: 140px; }
.tdesc { color: var(--text-sec); flex: 1; font-size: 12px; }
/* ── Daemon toggle rows (On/Off switch style) ── */
.daemon-toggle-row {
  display: flex; align-items: center; justify-content: space-between;
  padding: 10px 0; border-bottom: 1px solid var(--border);
}
.daemon-toggle-row:last-child { border-bottom: none; padding-bottom: 0; }
.daemon-toggle-info { display: flex; flex-direction: column; gap: 3px; flex: 1; margin-right: 16px; }
.daemon-toggle-info .tname { font-weight: 600; font-size: 13px; color: var(--text); min-width: unset; }
.daemon-toggle-info .tdesc { color: var(--text-sec); font-size: 12px; line-height: 1.4; }
.mode-toggle-row {
  display: inline-flex; align-items: center; gap: 8px; padding: 7px 10px;
  background: var(--bg-input); border: 1px solid var(--border); border-radius: var(--radius);
  cursor: pointer; font-size: 13px; font-weight: 600;
}
.mode-toggle-row input { accent-color: var(--accent); width: 15px; height: 15px; }

/* ── About ── */
.about-row { font-size: 13px; color: var(--text-sec); margin-bottom: 4px; line-height: 1.8; }
.about-row strong { color: var(--text); }
.about-row.muted { color: var(--text-muted); }
.about-row.saved { font-size: 11px; color: var(--success); }
.link { color: var(--accent); text-decoration: none; }
.link:hover { text-decoration: underline; }

/* ── Language ── */
.lang-list { display: flex; flex-direction: column; gap: 6px; }
.lang-btn {
  display: flex; align-items: center; justify-content: space-between;
  padding: 12px 14px; width: 100%;
  background: var(--bg-input); border: 1px solid var(--border);
  border-radius: var(--radius); cursor: pointer; color: var(--text-sec);
  transition: all .12s; text-align: start;
}
.lang-btn:hover { border-color: var(--accent); color: var(--text); }
.lang-btn.active { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 10%, transparent); color: var(--text); }
.lang-left { display: flex; align-items: center; gap: 10px; }
.lang-code { font-size: 11px; font-weight: 800; color: var(--accent); min-width: 26px; }
.lang-label { font-size: 14px; font-weight: 600; }
.lang-dir-badge { font-size: 10px; color: var(--text-muted); background: var(--bg-deep); padding: 2px 6px; border-radius: 3px; flex-shrink: 0; }
.lang-rtl-btn { padding: 6px 16px; border-radius: 20px; border: 1px solid var(--border); background: var(--bg-surface); color: var(--text-sec); font-size: 13px; font-weight: 600; cursor: pointer; transition: all .15s; }
.lang-rtl-btn:hover { color: var(--text); border-color: var(--accent); }
.lang-rtl-btn.active { background: color-mix(in srgb, var(--accent) 15%, transparent); color: var(--accent); border-color: var(--accent); }
.path-hint { font-size: 11px; color: var(--text-muted); font-family: monospace; margin-inline-start: 8px; max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.lang-actions { display: flex; align-items: center; gap: 4px; margin-inline-start: auto; }
/* Icon button tooltip (for lang-actions buttons) */
.theme-icon-btn { position: relative; }
.info-tooltip-wrap { pointer-events: none; }
.btn-tooltip {
  position: absolute; bottom: calc(100% + 6px); left: 50%;
  transform: translateX(-50%);
  white-space: normal; max-width: 220px; text-align: center;
  background: var(--bg-card); border: 1px solid var(--border); border-radius: 6px;
  padding: 5px 8px; font-size: 11px; color: var(--text-sec);
  box-shadow: 0 4px 12px rgba(0,0,0,.4);
  opacity: 0; transition: opacity .15s; z-index: 9999;
  font-weight: 400;
}
.theme-icon-btn:hover .btn-tooltip { opacity: 1; }
.lang-actions .btn-tooltip { left: auto; right: 0; transform: none; text-align: end; }
[data-tooltip-pos="below"] .btn-tooltip { bottom: auto; top: calc(100% + 6px); }
.spinning { animation: spin .7s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }

/* ── Shortcuts ── */
.shortcut-hdr { display:flex; align-items:center; justify-content:space-between; gap:12px; margin-bottom:16px; }
.btn-reset-sc {
  padding:5px 12px; border-radius:6px; border:1px solid var(--border);
  background:var(--bg-deep); color:var(--text-sec); font-size:12px; cursor:pointer; white-space:nowrap;
  transition:all .15s;
}
.btn-reset-sc:hover:not(:disabled) { border-color:var(--accent); color:var(--accent); }
.btn-reset-sc:disabled { opacity:.4; cursor:default; }
.shortcut-list { display: flex; flex-direction: column; }
.shortcut-row {
  display: flex; align-items: center; justify-content: space-between; gap: 12px;
  padding: 11px 0; border-bottom: 1px solid var(--border); font-size: 13px;
}
.shortcut-row:last-child { border-bottom: none; }
.shortcut-action { flex: 1; color: var(--text-sec); display: flex; align-items: center; gap: 6px; }
.sc-info {
  display: inline-flex; align-items: center; justify-content: center;
  width: 15px; height: 15px; border-radius: 50%; flex-shrink: 0;
  background: var(--bg-deep); border: 1px solid var(--border);
  color: var(--text-muted); font-size: 9px; font-weight: 800;
  cursor: help; transition: border-color .12s, color .12s;
}
.sc-info:hover { border-color: var(--accent); color: var(--accent); }
.shortcut-key {
  display: flex; align-items: center; gap: 6px;
  padding: 5px 14px; min-width: 130px; justify-content: center;
  background: var(--bg-input); border: 1px solid var(--border);
  border-radius: var(--radius); font-size: 12px; font-weight: 600;
  color: var(--text-sec); cursor: pointer; font-family: monospace;
  transition: border-color .12s, color .12s, background .12s;
}
.shortcut-key:hover { border-color: var(--accent); color: var(--accent); }
.shortcut-key.recording {
  border-color: var(--accent); color: var(--accent);
  background: color-mix(in srgb, var(--accent) 8%, transparent);
  font-family: inherit; font-size: 11px;
}
.rec-dot {
  width: 7px; height: 7px; border-radius: 50%;
  background: var(--accent); flex-shrink: 0;
  animation: blink 1s infinite;
}
@keyframes blink { 0%,100%{opacity:1} 50%{opacity:.2} }

/* ── Capture tracks ── */
.capture-tracks { display: flex; flex-direction: column; gap: 8px; margin-bottom: 12px; }
.capture-row { display: flex; align-items: center; gap: 10px; }
.capture-track-name {
  width: 65px; flex-shrink: 0; font-size: 12px; font-weight: 700;
  color: var(--text-sec); text-transform: uppercase; letter-spacing: .5px;
}
.capture-select-wrap { flex: 1; }
.add-row { margin-top: 4px; }

/* ── Track colors ── */
.color-tracks { display: flex; flex-direction: column; gap: 6px; margin-bottom: 12px; }
/* Strict 5-column grid: swatch | name | color-picker | hex | remove-or-placeholder
   Every row has exactly 5 cells so columns stay vertically aligned. */
.color-track-row {
  display: grid;
  grid-template-columns: 20px 1fr 36px 64px 26px;
  align-items: center;
  gap: 10px;
}
.track-swatch { width: 20px; height: 20px; border-radius: 4px; border: 1px solid rgba(255,255,255,.1); }
.track-name-lbl { font-size: 13px; color: var(--text-sec); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.track-color-input {
  width: 36px; height: 32px; padding: 2px; border: 1px solid var(--border);
  border-radius: var(--radius); background: transparent; cursor: pointer;
}
.track-hex { font-size: 11px; font-family: monospace; color: var(--text-muted); }
/* Invisible placeholder — same dimensions as .btn-icon — keeps grid aligned */
.btn-placeholder { width: 26px; height: 26px; }

/* ── Action row ── */
.action-row { display: flex; align-items: center; gap: 10px; }
.cache-msg { font-size: 11px; color: var(--text-muted); }

/* ── Folder row ── */
.folder-row { display: flex; gap: 8px; align-items: center; }
.source-row { display: flex; align-items: center; gap: 8px; padding: 6px 8px; background: var(--bg-deep); border-radius: 6px; margin-bottom: 6px; }
.source-path { flex: 1; font-size: 12px; color: var(--text-sec); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.btn-icon-sm { width: 22px; height: 22px; border: 1px solid var(--border); border-radius: 4px; background: transparent; color: var(--text-muted); font-size: 11px; cursor: pointer; flex-shrink: 0; display: flex; align-items: center; justify-content: center; } .btn-icon-sm:hover { background: rgba(220,38,38,.15); color: var(--danger); border-color: var(--danger); }
.badge-count { display: inline-block; background: var(--accent); color: #fff; font-size: 9px; font-weight: 700; padding: 1px 5px; border-radius: 10px; margin-left: 6px; vertical-align: middle; }
.folder-input {
  flex: 1; padding: 8px 12px; background: var(--bg-input);
  border: 1px solid var(--border); border-radius: var(--radius);
  color: var(--text); outline: none; font-size: 13px; color-scheme: dark;
}
.folder-btn {
  display: inline-flex; align-items: center; gap: 8px;
  padding: 8px 12px; border: 1px solid var(--border); border-radius: var(--radius);
  background: var(--bg-input); color: var(--text-sec); font-size: 13px; cursor: pointer;
  transition: background .12s, border-color .12s, color .12s;
  text-align: left; flex: 1; font-family: inherit;
}
.folder-btn:hover { background: var(--bg-hover); border-color: var(--accent); color: var(--text); }
.folder-path { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; user-select: text; -webkit-user-select: text; }
.folder-chevron { width: 14px; height: 14px; flex-shrink: 0; opacity: .5; }

/* ── Media Directories card ── */
.media-dir-row { display: flex; flex-direction: column; gap: 6px; }
.media-dir-label { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: .5px; color: var(--text-muted); }
.media-dir-extra { display: flex; flex-direction: column; gap: 4px; margin-top: 4px; }
.media-dir-divider { height: 1px; background: var(--border); margin: 10px 0; }
.btn-sm { font-size: 11px; padding: 4px 10px; }

/* ── Storage stats ── */
.storage-stats { display: flex; gap: 10px; flex-wrap: wrap; margin-bottom: 16px; }
.stat-pill {
  display: flex; flex-direction: column; gap: 3px; padding: 10px 16px;
  background: var(--bg-input); border: 1px solid var(--border); border-radius: var(--radius); min-width: 90px;
}
.stat-label { font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: .8px; color: var(--text-muted); }
.stat-val { font-size: 18px; font-weight: 800; color: var(--text); font-variant-numeric: tabular-nums; }
.stat-val.accent { color: var(--accent); }
.progress-bar-wrap { height: 8px; background: var(--bg-deep); border-radius: 4px; overflow: hidden; border: 1px solid var(--border); margin-bottom: 8px; }
.progress-bar { height: 100%; border-radius: 4px; transition: width .4s ease; }
.progress-label { font-size: 11px; color: var(--text-muted); }

/* ── Placeholder ── */
.placeholder-box {
  display: flex; flex-direction: column; align-items: center; justify-content: center;
  gap: 12px; padding: 40px; color: var(--text-muted); font-size: 13px;
}
.placeholder-box svg { width: 40px; height: 40px; opacity: .3; }

/* ── Extensions ── */
.ext-toggle-row {
  display: flex; align-items: center; gap: 16px;
  padding: 14px 0; border-bottom: 1px solid var(--border); cursor: default;
}
.ext-toggle-row:last-child { border-bottom: none; padding-bottom: 0; }
.ext-toggle-info { flex: 1; display: flex; flex-direction: column; gap: 3px; }
.ext-name { font-size: 13px; font-weight: 600; color: var(--text); }
.ext-desc { font-size: 12px; color: var(--text-sec); line-height: 1.4; }
.ext-switch-wrap { flex-shrink: 0; }
.ext-restart-hint { margin-top: 4px; font-size: 11px; color: var(--text-muted); }

/* iOS-style toggle switch */
.switch { position: relative; display: inline-block; width: 40px; height: 22px; cursor: pointer; }
.switch input { opacity: 0; width: 0; height: 0; position: absolute; }
.switch-track {
  position: absolute; inset: 0; border-radius: 11px;
  background: var(--bg-deep); border: 1px solid var(--border);
  transition: background .18s, border-color .18s;
}
.switch-track::after {
  content: ''; position: absolute; left: 3px; top: 50%; transform: translateY(-50%);
  width: 14px; height: 14px; border-radius: 50%;
  background: var(--text-muted); transition: left .18s, background .18s;
}
.switch input:checked + .switch-track {
  background: color-mix(in srgb, var(--accent) 20%, transparent);
  border-color: var(--accent);
}
.switch input:checked + .switch-track::after {
  left: calc(100% - 17px); background: var(--accent);
}

/* ── Extension card rows (professional card layout) ── */
.ext-card-row {
  display: flex; align-items: center; gap: 14px;
  padding: 14px 0;
  border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
}
.ext-card-row:last-child { border-bottom: none; padding-bottom: 0; }

.ext-card-icon-wrap {
  flex-shrink: 0;
  width: 42px; height: 42px;
  border-radius: 10px;
  background: color-mix(in srgb, var(--ext-clr, var(--accent)) 18%, var(--bg-deep));
  border: 1px solid color-mix(in srgb, var(--ext-clr, var(--accent)) 35%, transparent);
  display: flex; align-items: center; justify-content: center;
  color: var(--ext-clr, var(--accent));
}
.ext-card-icon-wrap svg { width: 20px; height: 20px; }
.plugin-icon { width: 24px; height: 24px; object-fit: contain; }

.ext-card-info {
  flex: 1;
  display: flex; flex-direction: column; gap: 3px;
  min-width: 0;
}

.ext-card-title-row {
  display: flex; align-items: center; gap: 6px;
}

.ext-gear-btn {
  flex-shrink: 0;
  width: 22px; height: 22px;
  display: flex; align-items: center; justify-content: center;
  border: none; background: transparent;
  color: var(--text-muted); border-radius: 4px;
  cursor: pointer; transition: all .15s;
  padding: 0;
}
.ext-gear-btn svg { width: 13px; height: 13px; }
.ext-gear-btn:hover { background: color-mix(in srgb, var(--accent) 10%, transparent); color: var(--accent); }

.ext-card-switch { flex-shrink: 0; }

.plugin-ver { font-size: 10px; color: var(--text-muted); margin-left: 2px; }

/* ── Plugin list ── */
.plugin-row { padding: 10px 0; border-bottom: 1px solid var(--border); }
.plugin-row:last-child { border-bottom: none; padding-bottom: 0; }
.plugin-info { display: flex; align-items: baseline; gap: 8px; margin-bottom: 3px; }
.plugin-name { font-size: 13px; font-weight: 600; color: var(--text); }
.plugin-ver { font-size: 10px; color: var(--text-muted); }
.plugin-desc { font-size: 11px; color: var(--text-sec); margin: 0; line-height: 1.4; }
.btn-sm { padding: 4px 10px; font-size: 11px; }

/* ── Timeline Tracks ── */
.tdef-list { display: flex; flex-direction: column; gap: 8px; margin-bottom: 14px; }
.tdef-row {
  display: grid;
  grid-template-columns: 28px 20px 1fr auto 28px;
  align-items: center;
  gap: 8px;
}
.tdef-swatch {
  width: 20px; height: 20px; border-radius: 4px; border: 1px solid rgba(255,255,255,.1);
  flex-shrink: 0; cursor: pointer; transition: transform .1s;
}
.tdef-swatch:hover { transform: scale(1.15); }
.tdef-color-hidden { position: absolute; left: -9999px; opacity: 0; width: 0; height: 0; pointer-events: none; }
.tdef-name-input {
  padding: 6px 10px; background: var(--bg-input); border: 1px solid var(--border);
  border-radius: var(--radius); color: var(--text); font-size: 12px;
  outline: none; width: 100%; color-scheme: dark;
}
.tdef-name-input:focus { border-color: var(--accent); }

/* ── Live Timeline Preview ── */
.tl-preview {
  background: var(--bg-deep); border: 1px solid var(--border);
  border-radius: var(--radius); overflow: hidden;
}
.tl-preview-row {
  display: flex; align-items: center; gap: 8px;
  height: 32px; padding: 0 10px;
  border-bottom: 1px solid color-mix(in srgb, var(--border) 50%, transparent);
  transition: background .1s;
}
.tl-preview-row:last-child { border-bottom: none; }
.tl-preview-row:hover { background: var(--bg-hover); }
.tl-pv-accent { width: 3px; height: 18px; border-radius: 2px; background: var(--pv); flex-shrink: 0; }
.tl-pv-id { font-size: 9px; font-weight: 800; color: var(--text-muted); letter-spacing: .5px; min-width: 22px; font-family: monospace; }
.tl-pv-icon { width: 12px; height: 12px; color: var(--pv); opacity: .8; flex-shrink: 0; }
.tl-pv-name { font-size: 11px; font-weight: 600; min-width: 80px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex-shrink: 0; }
.tl-pv-track-body { flex: 1; height: 14px; background: color-mix(in srgb, var(--pv) 10%, transparent); border-radius: 3px; overflow: hidden; position: relative; }
.tl-pv-bar { position: absolute; left: 0; top: 0; bottom: 0; width: 70%; background: color-mix(in srgb, var(--pv) 28%, transparent); border-radius: 3px; }

/* ── Danger Zone ── */
.danger-zone-card { border-color: color-mix(in srgb, var(--danger) 35%, var(--border)); }
.danger-head { color: var(--danger) !important; display: flex; align-items: center; gap: 8px; }
.danger-action-row { display: flex; align-items: flex-start; gap: 16px; }
.danger-info { flex: 1; display: flex; flex-direction: column; gap: 4px; }
.danger-label { font-size: 13px; font-weight: 600; color: var(--text); display: flex; align-items: center; }
.btn-danger {
  display: inline-flex; align-items: center; gap: 6px; flex-shrink: 0;
  padding: 8px 16px; border: 1px solid var(--danger);
  border-radius: var(--radius); background: transparent;
  color: var(--danger); font-size: 12px; font-weight: 600; cursor: pointer;
  transition: background .12s; white-space: nowrap;
}
.btn-danger svg { width: 13px; height: 13px; }
.btn-danger:hover { background: color-mix(in srgb, var(--danger) 10%, transparent); }
.btn-danger:disabled { opacity: .45; cursor: not-allowed; }
.danger-msg { margin-top: 12px; font-size: 12px; color: var(--danger); padding: 8px 12px; background: color-mix(in srgb, var(--danger) 8%, transparent); border-radius: var(--radius); border: 1px solid color-mix(in srgb, var(--danger) 25%, transparent); }
.danger-ok  { color: var(--success); background: color-mix(in srgb, var(--success) 8%, transparent); border-color: color-mix(in srgb, var(--success) 25%, transparent); }

/* ── Theme icon buttons ── */
.theme-actions { display: flex; align-items: center; gap: 4px; margin-inline-start: auto; }
.theme-icon-btn {
  width: 28px; height: 28px; border-radius: 6px;
  border: 1px solid var(--border); background: var(--bg-deep);
  color: var(--text-muted); cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  transition: all .15s;
}
.theme-icon-btn svg { width: 14px; height: 14px; }
.theme-icon-btn:hover { background: color-mix(in srgb, var(--accent) 12%, transparent); color: var(--accent); }
.theme-icon-btn.active { background: color-mix(in srgb, var(--accent) 18%, transparent); color: var(--accent); border-color: color-mix(in srgb, var(--accent) 50%, transparent); }
.theme-icon-btn:disabled { opacity: .4; cursor: not-allowed; }

/* ── Storage side-by-side grid ── */
.storage-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; margin-bottom: 16px; }
.steam-storage-head { display:flex; align-items:flex-start; justify-content:space-between; gap:12px; margin-bottom:12px; }
.steam-storage-copy { min-width:0; }
.steam-storage-title { font-size:13px; font-weight:700; color:var(--text); }
.steam-storage-body { margin-top:4px; font-size:12px; color:var(--text-sec); line-height:1.45; }
.steam-storage-btn { flex-shrink:0; }
.steam-storage-list { display:flex; flex-direction:column; gap:8px; max-height:280px; overflow:auto; padding-right:4px; }
.steam-storage-row { display:flex; align-items:center; gap:10px; padding:8px 10px; border:1px solid var(--border); border-radius:8px; background:var(--bg-deep); }
.steam-storage-icon { width:24px; height:24px; border-radius:6px; object-fit:cover; flex-shrink:0; }
.steam-storage-icon--fallback { display:flex; align-items:center; justify-content:center; background:color-mix(in srgb, var(--accent) 18%, var(--bg-card)); color:var(--accent); font-size:11px; font-weight:800; }
.steam-storage-name { font-size:12px; color:var(--text); }

/* ── Disabled track delete button ── */
.btn-icon:disabled { opacity: .35; cursor: not-allowed; }

/* ── Shortcut header label ── */
.shortcut-hdr-label { font-size: 12px; color: var(--text-sec); display: flex; align-items: center; }

/* ── About page ── */
.hero-card {
  background: var(--bg-card); border: 1px solid var(--border);
  border-radius: var(--radius); padding: 32px 28px;
  display: flex; flex-direction: column; align-items: center; text-align: center; gap: 8px;
}
.logo-svg { margin-bottom: 4px; }
.about-app-name { font-size: 28px; font-weight: 800; color: var(--text); letter-spacing: -0.5px; margin: 0; }
.version-badge {
  font-size: 12px; font-weight: 600; color: var(--accent);
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
  padding: 3px 12px; border-radius: 20px; margin: 0;
}
.about-tagline { font-size: 14px; font-weight: 600; color: var(--text-sec); margin: 4px 0 0; }
.about-desc { font-size: 13px; color: var(--text-muted); line-height: 1.6; margin: 0; max-width: 420px; }
.goals-card { background: var(--bg-card); border: 1px solid var(--border); border-radius: var(--radius); padding: 20px 24px; }
.goals-list { list-style: none; padding: 0; margin: 8px 0 0; display: flex; flex-direction: column; gap: 12px; }
.goals-list li { font-size: 13px; color: var(--text-sec); line-height: 1.5; padding-left: 18px; position: relative; }
.goals-list li::before { content: '▸'; position: absolute; left: 0; color: var(--accent); font-size: 12px; top: 2px; }
.connect-card { background: var(--bg-card); border: 1px solid var(--border); border-radius: var(--radius); padding: 20px 24px; }
.social-links { display: flex; flex-direction: column; gap: 10px; margin-top: 8px; }
.social-btn {
  display: flex; align-items: center; gap: 12px; padding: 10px 16px;
  background: var(--bg-deep); border: 1px solid var(--border); border-radius: var(--radius);
  color: var(--text-sec); font-size: 13px; font-weight: 500; cursor: pointer; transition: all .15s; text-align: left;
}
.social-btn:hover { background: color-mix(in srgb, var(--accent) 8%, transparent); color: var(--text); border-color: var(--accent); }
.social-btn svg { flex-shrink: 0; opacity: .8; }
.social-btn:hover svg { opacity: 1; color: var(--accent); }
.about-contributors { font-size: 12px; color: var(--text-muted); text-align: center; margin: 0; padding: 8px 0 16px; }

/* ── Notifications page ── */
.section-desc { font-size: 13px; color: var(--text-sec); margin: -8px 0 16px; line-height: 1.5; }
.notif-duration-field { margin-top: 16px; }
.notif-style-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 10px; margin-top: 4px; }
.notif-option {
  display: flex; align-items: center; gap: 10px; padding: 10px 14px;
  background: var(--bg-deep); border: 1px solid var(--border); border-radius: var(--radius);
  cursor: pointer; transition: all .15s;
}
.notif-option:hover { background: color-mix(in srgb, var(--accent) 8%, transparent); border-color: var(--accent); }
.notif-option.active { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 8%, transparent); }
.notif-option-icon { width: 28px; height: 28px; display: flex; align-items: center; justify-content: center; color: var(--text-sec); flex-shrink: 0; }
.notif-option.active .notif-option-icon { color: var(--accent); }
.notif-option-icon svg { width: 18px; height: 18px; }
.notif-option-label { font-size: 12px; font-weight: 600; color: var(--text-sec); }
.notif-option.active .notif-option-label { color: var(--accent); }
.notif-duration-slider {
  width: 100%; margin-top: 8px;
  -webkit-appearance: none; appearance: none;
  height: 4px; background: var(--border); border-radius: 2px; outline: none;
}
.notif-duration-slider::-webkit-slider-thumb {
  -webkit-appearance: none; appearance: none;
  width: 16px; height: 16px; border-radius: 50%;
  background: var(--accent); cursor: pointer; border: 2px solid var(--bg-card);
}
.notif-duration-slider::-moz-range-thumb {
  width: 16px; height: 16px; border-radius: 50%;
  background: var(--accent); cursor: pointer; border: 2px solid var(--bg-card);
}

/* ── Track visibility button ── */
.track-vis-btn {
  width: 26px; height: 26px; flex-shrink: 0;
  display: flex; align-items: center; justify-content: center;
  background: transparent; border: none; cursor: pointer;
  color: var(--text-muted); transition: color .12s; border-radius: 4px;
}
.track-vis-btn:hover { color: var(--accent); background: color-mix(in srgb, var(--accent) 10%, transparent); }
.track-vis-btn svg { width: 14px; height: 14px; }
.track-vis-btn.active { color: var(--accent); }

/* ── Screenshot dirs list ── */
.media-dirs-list { display: flex; flex-direction: column; gap: 6px; }
.dir-icon { width: 16px; height: 16px; color: var(--text-muted); flex-shrink: 0; }
.folder-add-btn { align-self: flex-start; margin-top: 4px; }

/* ── Extensions unified header ── */
.ext-section-head {
  display: flex; align-items: center; justify-content: space-between;
}
.ext-head-actions {
  display: flex; align-items: center; gap: 4px; margin-inline-start: auto;
}
.ext-icon-btn {
  width: 28px; height: 28px; border-radius: 6px;
  border: 1px solid var(--border); background: var(--bg-deep);
  color: var(--text-muted); cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  transition: all .15s; flex-shrink: 0;
}
.ext-icon-btn svg { width: 14px; height: 14px; }
.ext-icon-btn:hover { background: color-mix(in srgb, var(--accent) 10%, transparent); color: var(--accent); border-color: var(--accent); }
.ext-icon-btn:disabled { opacity: .4; cursor: not-allowed; }

/* ── Store placeholder ── */
.store-coming-soon {
  display: flex; flex-direction: column; align-items: center;
  justify-content: center; gap: 12px; padding: 48px 28px; text-align: center;
}
.store-icon {
  width: 56px; height: 56px; opacity: .3; color: var(--text-sec);
}
.store-coming-soon-title {
  font-size: 18px; font-weight: 700; color: var(--text); margin: 0;
}
</style>

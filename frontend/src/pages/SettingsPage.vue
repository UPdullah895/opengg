<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { ask, open as openDialog } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { usePersistenceStore, DEFAULTS } from '../stores/persistence'
import { useReplayStore } from '../stores/replay'
import { loadTheme, saveTheme, getCurrentTheme, applyThemeMode, getThemeMode } from '../utils/theme'
import { LANGUAGES, registerLocale } from '../i18n'
import SelectField from '../components/SelectField.vue'
import IconPicker from '../components/IconPicker.vue'
import InfoIcon from '../components/InfoIcon.vue'

const { t, locale } = useI18n()
const persist = usePersistenceStore()
const replay = useReplayStore()
onMounted(async () => {
  if (!persist.loaded) await persist.load()
  syncLocale()
  // ★ Epic 4: Sync autostart UI with actual OS state on every open
  try { settings.value.runAtStartup = await invoke<boolean>('get_autostart') } catch { /* ignore */ }
  // ★ Epic 4: Push saved run-in-background flag to Rust state
  try { await invoke('set_run_in_background', { val: settings.value.runInBackground }) } catch { /* ignore */ }
})

const settings = computed(() => persist.state.settings)

// ─── RTL: NEVER touch <html dir>. Only the .settings-content wrapper flips. ───
const contentDir = ref<'ltr' | 'rtl'>('ltr')
function syncLocale() {
  if (settings.value.language) locale.value = settings.value.language
  const entry = LANGUAGES.find(l => l.code === settings.value.language)
  contentDir.value = (entry?.dir ?? 'ltr') as 'ltr' | 'rtl'
}
function setLanguage(code: string) {
  settings.value.language = code
  locale.value = code
  const entry = LANGUAGES.find(l => l.code === code)
  contentDir.value = (entry?.dir ?? 'ltr') as 'ltr' | 'rtl'
}

// ─── Nav ───
type Section = 'general' | 'language' | 'shortcuts' | 'mixerRouting' | 'eqAutoFlatten' | 'captureSound' | 'trackManagement' | 'storage' | 'extensions'
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
      { key: 'eqAutoFlatten' as Section, label: t('settings.sections.eqAutoFlatten') } as NavItem,
    ],
  },
  {
    key: 'moments', label: t('settings.groups.moments'),
    items: [
      { key: 'captureSound'    as Section, label: t('settings.sections.captureSound') } as NavItem,
      { key: 'trackManagement' as Section, label: 'Timeline Tracks'                   } as NavItem,
      { key: 'storage'         as Section, label: t('settings.sections.storage')      } as NavItem,
    ],
  },
  {
    key: 'extensions', label: t('settings.groups.extensions'),
    items: [
      // ★ Epic 5: Beta badge
      { key: 'extensions' as Section, label: t('settings.sections.extensions'), badge: 'Beta' } as NavItem,
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
const gsrQualityOptions = [
  { value: 'High',   label: 'High (CRF 23)'   },
  { value: 'Medium', label: 'Medium (CRF 28)'  },
  { value: 'Low',    label: 'Low (CRF 35)'     },
]
const gsrFpsOptions    = [30, 60, 120].map(v => ({ value: v, label: `${v} FPS` }))
const gsrReplayOptions = [15, 30, 60, 120, 180].map(v => ({ value: v, label: `${v}s` }))

async function toggleGsr() {
  const outputDir = (settings.value.clip_directories?.[0] ?? '~/Videos/OpenGG').replace('~', '')
  try {
    if (settings.value.gsrEnabled) {
      await invoke('stop_gsr_replay')
    } else {
      await invoke('start_gsr_replay', {
        outputDir,
        replaySecs: settings.value.gsrReplaySecs,
        fps: settings.value.gsrFps,
        quality: settings.value.gsrQuality,
      })
    }
    settings.value.gsrEnabled = !settings.value.gsrEnabled
  } catch (e) { console.error('GSR toggle:', e) }
}

async function restartGsr() {
  if (!settings.value.gsrEnabled) return
  const outputDir = (settings.value.clip_directories?.[0] ?? '~/Videos/OpenGG').replace('~', '')
  try {
    await invoke('stop_gsr_replay')
    await invoke('start_gsr_replay', {
      outputDir,
      replaySecs: settings.value.gsrReplaySecs,
      fps: settings.value.gsrFps,
      quality: settings.value.gsrQuality,
    })
  } catch (e) { console.error('GSR restart:', e) }
}

const isDefaultShortcuts = computed(() =>
  JSON.stringify(settings.value.shortcuts) === JSON.stringify(DEFAULTS.settings.shortcuts)
)
function resetShortcuts() { persist.resetShortcuts() }

// ─── Capture tracks ───
const CAPTURE_SOURCES = ['Game', 'Chat', 'Media', 'Aux', 'Mic']
const captureSourceOptions = computed(() =>
  CAPTURE_SOURCES.map(s => ({ value: s, label: t(`settings.captureSound.sources.${s}`) }))
)
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
  settings.value.trackDefs.push({ id: `A${idx}`, name: `Audio ${idx}`, color: '#64748b', icon: 'game' })
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
  const confirmed = await ask('This will unload all OpenGG virtual audio sinks and restart PipeWire. Your audio routing will be reset. Continue?', { title: 'Danger Zone', kind: 'warning' })
  if (!confirmed) return
  dangerLoading.value = true; dangerMsg.value = ''
  try {
    await invoke('remove_virtual_audio')
    dangerMsg.value = '✓ Virtual audio removed. Select your devices below.'
    setTimeout(() => {
      window.dispatchEvent(new CustomEvent('openOnboarding', { detail: { step: 2 } }))
    }, 600)
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

// ─── Screenshot directory ───
async function pickScreenshotDir() {
  try {
    const s = await openDialog({ directory: true, multiple: false, title: 'Screenshot Save Location' })
    if (s && typeof s === 'string') settings.value.screenshotDir = s
  } catch {}
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
async function loadStorage() {
  storageLoading.value = true
  try { storageInfo.value = await invoke<StorageInfo>('get_storage_info', { clipDirectories: settings.value.clip_directories ?? ['~/Videos/OpenGG'] }) }
  catch { storageInfo.value = null }
  finally { storageLoading.value = false }
}
watch(active, v => { if (v === 'storage') loadStorage() })
onMounted(() => { if (active.value === 'storage') loadStorage() })

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
const clickOptions = [{ value:'preview', label: 'Quick Preview' }, { value:'editor', label: 'Advanced Editor' }]
const dateFormatOptions = [
  { value: 'YMD', label: 'YYYY/MM/DD' },
  { value: 'YDM', label: 'YYYY/DD/MM' },
]

// ─── Extensions: plugin scanning ───
interface ExtensionInfo { id: string; name: string; description: string; version: string; path: string }
const scannedPlugins = ref<ExtensionInfo[]>([])
const pluginScanLoading = ref(false)

async function scanPlugins() {
  pluginScanLoading.value = true
  try { scannedPlugins.value = await invoke<ExtensionInfo[]>('scan_extensions') } catch { scannedPlugins.value = [] }
  finally { pluginScanLoading.value = false }
}

async function openExtensionsFolder() {
  try { await invoke('open_extensions_folder') } catch (e) { console.error(e) }
  await scanPlugins()
}

// Scan whenever the extensions section becomes active
watch(active, v => { if (v === 'extensions') scanPlugins() })
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

    <!-- ── Content ── dir is set per-section, NEVER on <html> ── -->
    <div class="settings-content" :dir="contentDir">

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
          <div class="card-head">Clip Preferences</div>
          <div class="form-grid">
            <div class="field">
              <label>{{ t('settings.clipSettings.defaultClick') }}</label>
              <SelectField v-model="settings.defaultClickAction" :options="clickOptions" />
            </div>
            <div class="field">
              <label>Search date format<InfoIcon title="How the clip search parses typed dates. YYYY/MM/DD uses month-day order; YYYY/DD/MM uses day-month order. Month names (january, feb, …) work in both." /></label>
              <SelectField v-model="settings.dateFormat" :options="dateFormatOptions" />
            </div>
          </div>
        </div>

        <!-- ★ Epic 4: Daemon & Startup toggles -->
        <div class="card">
          <div class="card-head">Daemon &amp; Startup</div>
          <label class="toggle-row">
            <input type="checkbox" v-model="settings.runAtStartup" @change="onRunAtStartupChange">
            <span class="tname">Run OpenGG when my computer starts</span>
            <span class="tdesc">Automatically launch OpenGG on login via XDG autostart</span>
          </label>
          <label class="toggle-row">
            <input type="checkbox" v-model="settings.runInBackground" @change="onRunInBackgroundChange">
            <span class="tname">Keep running in background when closed</span>
            <span class="tdesc">Closing the window hides it to the system tray instead of quitting</span>
          </label>
        </div>

        <!-- ★ Epic 2: Diagnostics / crash log -->
        <div class="card">
          <div class="card-head">Diagnostics <InfoIcon title="Crash and error logs are stored locally for debugging. Share them when reporting issues." /></div>
          <div class="action-row">
            <button class="btn btn-accent" @click="openCrashLogsFolder">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>
              Open Logs Folder
            </button>
          </div>
        </div>

        <div class="card">
          <div class="card-head">{{ t('settings.general.about') }}</div>
          <div class="about-row"><strong>OpenGG</strong> {{ t('settings.general.version') }}</div>
          <div class="about-row muted">{{ t('settings.general.openSource') }}</div>
          <div class="about-row saved" v-if="persist.loaded">✓ {{ t('settings.saved') }}</div>
          <div class="about-row" style="margin-top:8px">
            <a href="#" class="link" @click.prevent="openExternal('https://github.com/UPdullah895/opengg')">{{ t('settings.general.github') }}</a>
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
                <span class="info-tooltip-wrap"><span class="btn-tooltip">Reload languages</span></span>
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
            <button class="btn-reset-sc" :disabled="isDefaultShortcuts" @click="resetShortcuts">Reset to Defaults</button>
          </div>
          <div class="shortcut-list">
            <div
              v-for="action in shortcutActions" :key="action.key"
              class="shortcut-row"
            >
              <span class="shortcut-action">
                {{ action.label }}
                <span class="sc-info" :title="action.hint">?</span>
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
            <span>Mixer routing config — coming soon</span>
          </div>
        </div>

        <!-- ★ Epic 3: Danger Zone -->
        <div class="card danger-zone-card">
          <div class="card-head danger-head">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:15px;height:15px;flex-shrink:0"><path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
            Danger Zone
          </div>
          <p class="hint" style="color:color-mix(in srgb,var(--danger) 80%,var(--text-sec))">These actions are destructive. Use only if your audio routing is broken and needs a full reset.</p>
          <div class="danger-action-row">
            <div class="danger-info">
              <span class="danger-label">Remove Virtual Audio &amp; Restore OS Defaults</span>
              <span class="danger-desc">Unloads all OpenGG virtual sinks and restarts PipeWire + WirePlumber. Your physical hardware routes will be restored. The onboarding wizard will guide re-setup on next launch.</span>
            </div>
            <button class="btn-danger" :disabled="dangerLoading" @click="removeVirtualAudio">
              <svg v-if="!dangerLoading" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6m3 0V4a1 1 0 011-1h4a1 1 0 011 1v2"/></svg>
              <span>{{ dangerLoading ? 'Removing…' : 'Remove Virtual Audio & Restore OS Defaults' }}</span>
            </button>
          </div>
          <div v-if="dangerMsg" class="danger-msg" :class="{ 'danger-ok': dangerMsg.startsWith('✓') }">{{ dangerMsg }}</div>
        </div>
      </section>

      <!-- ════════════════════ EQ AUTO-FLATTEN ════════════════════ -->
      <section v-if="active === 'eqAutoFlatten'">
        <h2 class="sec-title">{{ t('settings.sections.eqAutoFlatten') }}</h2>
        <div class="card">
          <div class="placeholder-box">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 12h4l3-9 4 18 3-9h4"/></svg>
            <span>EQ presets &amp; auto-flatten — coming soon</span>
          </div>
        </div>
      </section>

      <!-- ════════════════════ CAPTURE & SOUND ════════════════════ -->
      <section v-if="active === 'captureSound'">
        <h2 class="sec-title">{{ t('settings.captureSound.title') }}</h2>

        <!-- GPU Screen Recorder panel (top) -->
        <div class="card">
          <div class="card-head gsr-head">
            <span>GPU Screen Recorder</span>
            <span class="badge-beta">Beta</span>
            <InfoIcon title="Uses gpu-screen-recorder for low-latency hardware-encoded replay buffer (NVENC/VAAPI). Must be installed separately." />
          </div>
          <div v-if="settings.gsrEnabled" class="form-grid gsr-grid">
            <div class="field">
              <label>Quality</label>
              <SelectField v-model="settings.gsrQuality" :options="gsrQualityOptions" @update:modelValue="restartGsr" />
            </div>
            <div class="field">
              <label>FPS</label>
              <SelectField v-model="settings.gsrFps" :options="gsrFpsOptions" @update:modelValue="restartGsr" />
            </div>
            <div class="field">
              <label>Replay Buffer</label>
              <SelectField v-model="settings.gsrReplaySecs" :options="gsrReplayOptions" @update:modelValue="restartGsr" />
            </div>
          </div>
          <div v-else class="hint" style="margin-top:8px">Enable GPU Screen Recorder in <strong>Extensions</strong> to configure it here.</div>
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
        <h2 class="sec-title">Timeline Tracks</h2>

        <div class="card">
          <div class="card-head">Track Icons</div>
          <label class="ext-toggle-row" style="border-bottom:none;padding-bottom:0">
            <div class="ext-toggle-info">
              <span class="ext-name">Show icons in editor track headers</span>
              <span class="ext-desc">Display a small icon next to each track label in the editor timeline</span>
            </div>
            <div class="ext-switch-wrap">
              <label class="switch">
                <input type="checkbox" v-model="settings.showTrackIcons" />
                <span class="switch-track"></span>
              </label>
            </div>
          </label>
        </div>

        <div class="card">
          <div class="card-head">Track List <InfoIcon title="Customize the name, color, and icon for each editor timeline track. Changes apply live — open the editor to see them instantly." /></div>
          <div class="tdef-list">
            <div v-for="(def, idx) in settings.trackDefs" :key="def.id" class="tdef-row">
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
          <button class="btn btn-ghost add-row" @click="addTrackDef">+ Add Audio Track</button>
        </div>

        <!-- Live Preview -->
        <div class="card">
          <div class="card-head">Live Preview <InfoIcon title="Exactly how the editor timeline headers will look. Updates as you type." /></div>
          <div class="tl-preview">
            <div v-for="def in settings.trackDefs" :key="def.id" class="tl-preview-row" :style="{ '--pv': def.color }">
              <div class="tl-pv-accent"></div>
              <span class="tl-pv-id">{{ def.id }}</span>
              <svg v-if="settings.showTrackIcons" class="tl-pv-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
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

        <!-- Clip directories -->
        <div class="card">
          <div class="card-head">
            Clip Directories <span class="badge-count">{{ (settings.clip_directories || []).length }}</span>
            <InfoIcon title="OpenGG watches these folders for new clips. Files in any directory appear instantly." />
          </div>
          <div v-for="(src, i) in (settings.clip_directories || [])" :key="i" class="source-row">
            <span class="source-path">{{ src }}</span>
            <button class="btn-icon-sm" @click="removeClipSource(i)" title="Remove">✕</button>
          </div>
          <div v-if="!(settings.clip_directories || []).length" class="hint">No directories configured. Add one below.</div>
          <button class="btn" style="margin-top:10px" @click="addClipSource">+ Add Directory</button>
        </div>

        <!-- Screenshot location -->
        <div class="card">
          <div class="card-head">Screenshot Save Location <InfoIcon title="Where screenshots taken from the Editor are saved." /></div>
          <div class="folder-row">
            <input type="text" :value="settings.screenshotDir || '~/Pictures (default)'" readonly class="folder-input" />
            <button class="btn" @click="pickScreenshotDir">Change</button>
            <button v-if="settings.screenshotDir" class="btn" @click="settings.screenshotDir = ''">Reset</button>
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
            <div v-else class="hint">Could not read storage info.</div>
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
            <span>GPU Screen Recorder</span>
            <span class="badge-beta">Beta</span>
            <InfoIcon title="Uses gpu-screen-recorder for low-latency hardware-encoded replay buffer (NVENC/VAAPI). Must be installed separately." />
          </div>
          <button class="gsr-install-toggle" @click="gsrInstallOpen = !gsrInstallOpen">{{ gsrInstallOpen ? '▼ Hide install guide' : '▶ How to install?' }}</button>
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

        <!-- Editor Features -->
        <div class="card">
          <div class="card-head">Editor Features</div>
          <label class="ext-toggle-row">
            <div class="ext-toggle-info">
              <span class="ext-name">{{ t('settings.extensions.overlays') }}</span>
              <span class="ext-desc">{{ t('settings.extensions.overlaysDesc') }}</span>
            </div>
            <div class="ext-switch-wrap">
              <label class="switch">
                <input type="checkbox" v-model="persist.state.extensions.overlays" />
                <span class="switch-track"></span>
              </label>
            </div>
          </label>

          <label class="ext-toggle-row">
            <div class="ext-toggle-info">
              <span class="ext-name">{{ t('settings.extensions.tiktokExport') }}</span>
              <span class="ext-desc">{{ t('settings.extensions.tiktokExportDesc') }}</span>
            </div>
            <div class="ext-switch-wrap">
              <label class="switch">
                <input type="checkbox" v-model="persist.state.extensions.tiktokExport" />
                <span class="switch-track"></span>
              </label>
            </div>
          </label>
        </div>

        <p class="hint ext-restart-hint">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:12px;height:12px;vertical-align:middle;margin-right:4px"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
          {{ t('settings.extensions.restartHint') }}
        </p>

        <!-- ─── Installed Plugins ─── -->
        <div class="card">
          <div class="card-head" style="display:flex;align-items:center;justify-content:space-between">
            <span>Installed Plugins</span>
            <button class="btn btn-sm" @click="openExtensionsFolder">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:11px;height:11px"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>
              Open Folder
            </button>
          </div>
          <div v-if="pluginScanLoading" class="hint" style="padding:8px 0">Scanning…</div>
          <div v-else-if="!scannedPlugins.length" class="hint" style="padding:8px 0">
            No plugins installed. Drop a plugin folder with a <code>manifest.json</code> into the plugins directory.
          </div>
          <div v-else>
            <div v-for="p in scannedPlugins" :key="p.id" class="plugin-row">
              <div class="plugin-info">
                <span class="plugin-name">{{ p.name }}</span>
                <span class="plugin-ver">v{{ p.version }}</span>
              </div>
              <p class="plugin-desc">{{ p.description }}</p>
            </div>
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
.nav-item {
  display: flex; align-items: center; gap: 6px;
  width: 100%; padding: 8px 16px;
  background: transparent; border: none; color: var(--text-sec);
  font-size: 13px; text-align: left; cursor: pointer;
  transition: background .12s, color .12s, border-color .12s;
  border-right: 2px solid transparent;
}
.nav-item:hover { background: var(--bg-hover); color: var(--text); }
.nav-item.active {
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  color: var(--accent); border-right-color: var(--accent);
}
/* ★ Epic 5: Beta badge */
.nav-badge {
  margin-left: auto;
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
.gsr-head { display: flex; align-items: center; gap: 8px; }
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
.path-hint { font-size: 11px; color: var(--text-muted); font-family: monospace; margin-inline-start: 8px; max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.lang-actions { display: flex; align-items: center; gap: 4px; margin-inline-start: auto; }
/* Icon button tooltip (for lang-actions buttons) */
.theme-icon-btn { position: relative; }
.info-tooltip-wrap { pointer-events: none; }
.btn-tooltip {
  position: absolute; bottom: calc(100% + 6px); left: 50%;
  transform: translateX(-50%);
  white-space: nowrap;
  background: var(--bg-card); border: 1px solid var(--border); border-radius: 6px;
  padding: 5px 8px; font-size: 11px; color: var(--text-sec);
  box-shadow: 0 4px 12px rgba(0,0,0,.4);
  opacity: 0; transition: opacity .15s; z-index: 9999;
  font-weight: 400;
}
.theme-icon-btn:hover .btn-tooltip { opacity: 1; }
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
  grid-template-columns: 20px 1fr 32px 26px;
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
.danger-label { font-size: 13px; font-weight: 600; color: var(--text); }
.danger-desc { font-size: 12px; color: var(--text-sec); line-height: 1.45; }
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
.theme-actions { display: flex; align-items: center; gap: 4px; margin-left: auto; }
.theme-icon-btn {
  width: 28px; height: 28px; border-radius: 6px;
  border: 1px solid var(--border); background: var(--bg-deep);
  color: var(--text-muted); cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  transition: all .15s;
}
.theme-icon-btn svg { width: 14px; height: 14px; }
.theme-icon-btn:hover { background: var(--bg-hover); color: var(--text); }
.theme-icon-btn:disabled { opacity: .4; cursor: not-allowed; }

/* ── Storage side-by-side grid ── */
.storage-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; margin-bottom: 16px; }

/* ── Disabled track delete button ── */
.btn-icon:disabled { opacity: .35; cursor: not-allowed; }

/* ── Shortcut header label ── */
.shortcut-hdr-label { font-size: 12px; color: var(--text-sec); display: flex; align-items: center; }
</style>

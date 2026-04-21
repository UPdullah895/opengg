import { defineStore } from 'pinia'
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export type PlaybackMode = 'run_once' | 'hold_repeat' | 'toggle'

export interface MacroAction {
  type: 'key_down' | 'key_up' | 'delay'
  key?: string
  delayMs?: number
}

export interface MouseMacro {
  button: string
  name: string
  actions: MacroAction[]
  playback: PlaybackMode
}

export interface TrackDef {
  id: string    // 'V1', 'A1', 'A2', … 'O1'
  name: string  // user-editable display name
  color: string // hex color
  icon: string  // 'video' | 'game' | 'chat' | 'mic' | 'media' | 'overlay'
  visible: boolean // whether track is visible in the editor timeline
}

export interface PersistedState {
  mixer: { volumes: Record<string, number>; mutes: Record<string, boolean>; devices: Record<string, string>; appRules: Record<string, string> }
  settings: {
    fps: number; quality: string; replayDuration: number
    defaultClickAction: 'preview' | 'editor'
    clipsPerRow: 2 | 3 | 4 | 5
    // Date format used by the clip search box when parsing typed dates.
    // YMD = YYYY/MM/DD, YDM = YYYY/DD/MM. Clips are still stored in ISO YMD internally.
    dateFormat: 'YMD' | 'YDM'
    shortcuts: {
      saveReplay: string; toggleRecording: string; screenshot: string
      splitClip: string; exportClip: string; toggleMic: string
      undo: string; redo: string
    }
    videoQuality: 'High' | 'Medium' | 'Low'
    videoResolution: '1080p' | '720p' | '480p'
    language: string
    trackDefs: TrackDef[]
    captureTracks: Array<{ name: string; source: string }>
    // ★ Epic 4: daemon settings
    runAtStartup: boolean
    runInBackground: boolean
    // Clip directories (watched for new clips)
    clip_directories: string[]
    screenshotDirs: string[]
    // ★ GPU Screen Recorder
    gsrEnabled: boolean
    gsrFps: number
    gsrQuality: 'cbr' | 'medium' | 'high' | 'very_high' | 'ultra'
    gsrCbrBitrate: number        // kbps, used only when gsrQuality === 'cbr'
    gsrReplaySecs: number
    gsrReplayPreset: '15' | '30' | '60' | '90' | '120' | 'custom'
    gsrRestartOnSave: boolean
    gsrMonitorTarget: string     // 'screen' | 'DP-1' | 'HDMI-1' | ...
    gsrAutoStart: boolean        // start replay buffer automatically on app launch
    steamLibraryAccess: 'unknown' | 'granted' | 'denied'
    enableClipNotifications: boolean  // show overlay toast when a clip is saved
    notificationStyle: 'auto' | 'gsr-notify' | 'x11-overlay' | 'system' | 'disabled'
    notificationPosition: 'top-right' | 'top-left' | 'bottom-right' | 'bottom-left'
    notificationDuration: number  // 1-10 seconds
    rtlMode: boolean
  }
  modules: { audio: boolean; device: boolean; replay: boolean }
  /** Keyed by extension id (e.g. 'overlays-system'). true = enabled. */
  extensions: Record<string, boolean>
  /** Mouse macros keyed by device ID. */
  macros: Record<string, MouseMacro[]>
}

export const DEFAULTS: PersistedState = {
  mixer: { volumes: { Game:100, Chat:100, Media:100, Aux:100, Mic:100 }, mutes:{}, devices:{}, appRules:{} },
  settings: {
    fps: 60, quality: 'High', replayDuration: 30,
    defaultClickAction: 'preview', clipsPerRow: 4,
    dateFormat: 'YMD',
    shortcuts: {
      saveReplay: 'Alt+F10', toggleRecording: 'Alt+F9', screenshot: 'Alt+F12',
      splitClip: 'S', exportClip: 'Ctrl+E', toggleMic: 'Alt+M',
      undo: 'Ctrl+Z', redo: 'Ctrl+Shift+Z',
    },
    videoQuality: 'High', videoResolution: '1080p', language: 'en',
    trackDefs: [
      { id: 'O1', name: 'Overlays', color: '#f97316', icon: 'overlay', visible: true },
      { id: 'V1', name: 'Video',    color: '#E94560', icon: 'video',   visible: true },
      { id: 'A1', name: 'Audio 1',  color: '#10b981', icon: 'game',    visible: true },
      { id: 'A2', name: 'Audio 2',  color: '#3b82f6', icon: 'chat',    visible: true },
      { id: 'A3', name: 'Audio 3',  color: '#f59e0b', icon: 'mic',     visible: true },
      { id: 'A4', name: 'Audio 4',  color: '#8b5cf6', icon: 'media',   visible: true },
      { id: 'A5', name: 'Audio 5',  color: '#ec4899', icon: 'media',   visible: true },
    ],
    captureTracks: [
      { name: 'Track 1', source: 'Game' },
      { name: 'Track 2', source: 'Chat' },
      { name: 'Track 3', source: 'Mic'  },
    ],
    runAtStartup:      false,
    runInBackground:   true,
    clip_directories:  ['~/Videos/OpenGG'],
    screenshotDirs:    ['~/Pictures'],
    gsrEnabled:        false,
    gsrFps:            60,
    gsrQuality:        'cbr',
    gsrCbrBitrate:     8000,
    gsrReplaySecs:     30,
    gsrReplayPreset:   '30',
    gsrRestartOnSave:  false,
    gsrMonitorTarget:  'screen',
    gsrAutoStart:      true,
    steamLibraryAccess: 'unknown',
    enableClipNotifications: true,
    notificationStyle: 'auto' as 'auto' | 'gsr-notify' | 'x11-overlay' | 'system' | 'disabled',
    notificationPosition: 'top-right' as 'top-right' | 'top-left' | 'bottom-right' | 'bottom-left',
    notificationDuration: 4,
    rtlMode:               false,
  },
  modules: { audio: true, device: true, replay: true },
  extensions: { 'overlays-system': true, 'tiktok-export': true },
  macros: {},
}

export const usePersistenceStore = defineStore('persistence', () => {
  const state = ref<PersistedState>(structuredClone(DEFAULTS))
  const loaded = ref(false)

  async function load() {
    try {
      const j = await invoke<string>('load_ui_settings')
      if (j && j !== 'null') {
        const parsed = JSON.parse(j)
        // Migration: clipsFolder → clip_directories
        if (parsed?.settings?.clipsFolder && !parsed?.settings?.clip_directories?.length) {
          parsed.settings.clip_directories = [parsed.settings.clipsFolder]
        }
        delete parsed?.settings?.clipsFolder
        delete parsed?.settings?.clipSources
        // Migration: screenshotDir (string) → screenshotDirs (array)
        if (parsed?.settings?.screenshotDir && !parsed?.settings?.screenshotDirs?.length) {
          parsed.settings.screenshotDirs = [parsed.settings.screenshotDir]
        }
        delete parsed?.settings?.screenshotDir
        // Migration: drop stale showTrackIcons (replaced by per-track visible toggle)
        delete parsed?.settings?.showTrackIcons
        state.value = deepMerge(structuredClone(DEFAULTS), parsed)
        // Migration: stale gsrMonitorTarget values from the old Tauri monitor API were
        // EDID model names or resolution strings (e.g. "1920x1080", "BenQ GW2780").
        // GSR only accepts connector names ("screen", "DP-1", "HDMI-A-1").
        // Reset to "screen" if the saved value looks like a resolution or contains spaces.
        const target = state.value.settings.gsrMonitorTarget
        if (target && (/^\d+x\d+/.test(target) || target.includes(' '))) {
          state.value.settings.gsrMonitorTarget = 'screen'
        }
        // Migration: ensure O1 (Overlays) exists and is positioned before V1 (Video)
        const defs = state.value.settings.trackDefs
        const o1Def = DEFAULTS.settings.trackDefs.find(d => d.id === 'O1')!
        const o1i = defs.findIndex(d => d.id === 'O1')
        const v1i = defs.findIndex(d => d.id === 'V1')
        if (o1i === -1 && o1Def) {
          // O1 missing entirely — insert it before V1 (or at 0)
          defs.splice(v1i !== -1 ? v1i : 0, 0, { ...o1Def })
        } else if (o1i !== -1 && v1i !== -1 && o1i > v1i) {
          // O1 exists but is below V1 — move it above
          const [o1] = defs.splice(o1i, 1)
          defs.splice(v1i, 0, o1)
        }
        // Migration: ensure A1 exists (deepMerge replaces arrays wholesale; saved settings
        // predating A1 will be missing it, causing a ghost "Track 1" in the editor)
        const a1Def = DEFAULTS.settings.trackDefs.find(d => d.id === 'A1')!
        const a1i = defs.findIndex(d => d.id === 'A1')
        if (a1i === -1 && a1Def) {
          const v1Pos = defs.findIndex(d => d.id === 'V1')
          defs.splice(v1Pos !== -1 ? v1Pos + 1 : defs.length, 0, { ...a1Def })
        }
        // Migration: extensions changed from { overlays, tiktokExport } to Record<string, boolean>
        if (typeof state.value.extensions?.['overlays'] === 'boolean') {
          state.value.extensions['overlays-system'] = state.value.extensions['overlays'] as boolean
          delete state.value.extensions['overlays']
        }
        if (typeof state.value.extensions?.['tiktokExport'] === 'boolean') {
          state.value.extensions['tiktok-export'] = state.value.extensions['tiktokExport'] as boolean
          delete state.value.extensions['tiktokExport']
        }
        // Migration: ensure all existing tracks have the visible field
        for (const def of defs) {
          if (def.visible === undefined) def.visible = true
        }
      }
    } catch (e) { console.warn('load settings:', e) }
    loaded.value = true
  }

  async function save() {
    try { await invoke('save_ui_settings', { settingsJson: JSON.stringify(state.value, null, 2) }) }
    catch (e) { console.error('save:', e) }
  }

  let t: ReturnType<typeof setTimeout> | null = null
  watch(state, () => { if (!loaded.value) return; if (t) clearTimeout(t); t = setTimeout(() => save(), 500) }, { deep: true })

  function setChannelVolume(ch: string, v: number) { state.value.mixer.volumes[ch] = v }
  function setChannelMute(ch: string, m: boolean) { state.value.mixer.mutes[ch] = m }
  function setChannelDevice(ch: string, d: string) { state.value.mixer.devices[ch] = d }
  function setAppRule(bin: string, ch: string) {
    if (ch === 'default' || ch === 'Master') delete state.value.mixer.appRules[bin]
    else state.value.mixer.appRules[bin] = ch
  }
  function getAppRule(bin: string) { return state.value.mixer.appRules[bin] }

  function resetShortcuts() { state.value.settings.shortcuts = structuredClone(DEFAULTS.settings.shortcuts) }

  return { state, loaded, load, save, setChannelVolume, setChannelMute, setChannelDevice, setAppRule, getAppRule, resetShortcuts }
})

function deepMerge(a: any, b: any): any {
  if (b == null) return a
  if (typeof a !== 'object' || typeof b !== 'object') return b
  if (Array.isArray(a)) return b
  const r = { ...a }
  for (const k of Object.keys(b)) {
    r[k] = (k in r && typeof r[k] === 'object' && typeof b[k] === 'object')
      ? deepMerge(r[k], b[k]) : b[k]
  }
  return r
}

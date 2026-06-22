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
  /** Internal schema version for automated migrations. */
  _schemaVersion: number
  mixer: { volumes: Record<string, number>; mutes: Record<string, boolean>; devices: Record<string, string>; appRules: Record<string, string>; earBlast: { enabled: boolean; channels: string[]; threshold: number; target: number } }
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
      undo: string; redo: string; toggleEarBlast: string
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
    // ★ Mixer app-box layout (global, applies to every channel's connected-apps box)
    appBoxCount: number          // number of apps shown before the box scrolls
    appBoxPerRow: 1 | 2          // apps per row (1 = single column, 2 = two columns)
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
    gsrAutoRestart: boolean      // auto-restart GSR if it crashes unexpectedly
    steamLibraryAccess: 'unknown' | 'granted' | 'denied'
    enableClipNotifications: boolean  // show overlay toast when a clip is saved
    notificationStyle: 'auto' | 'gsr-notify' | 'x11-overlay' | 'system' | 'disabled'
    notificationPosition: 'top-right' | 'top-left' | 'bottom-right' | 'bottom-left'
    notificationDuration: number  // 1-10 seconds
    rtlMode: boolean
    tutorialSeen: boolean
    languagePicked: boolean
  }
  modules: { audio: boolean; device: boolean; replay: boolean }
  /** Keyed by extension id (e.g. 'overlays-system'). true = enabled. */
  extensions: Record<string, boolean>
  /** Keyed by extension id. true = user has consented to daemon-part first-run. */
  extensionConsents: Record<string, boolean>
  /** Mouse macros keyed by device ID. */
  macros: Record<string, MouseMacro[]>
}

const CURRENT_SCHEMA_VERSION = 12

export const DEFAULTS: PersistedState = {
  _schemaVersion: CURRENT_SCHEMA_VERSION,
  mixer: { volumes: { Game:100, Chat:100, Media:100, Aux:100, Mic:100 }, mutes:{}, devices:{}, appRules:{}, earBlast: { enabled: false, channels: ['Game'], threshold: 85, target: 60 } },
  settings: {
    fps: 60, quality: 'High', replayDuration: 30,
    defaultClickAction: 'preview', clipsPerRow: 4,
    dateFormat: 'YMD',
    shortcuts: {
      saveReplay: 'Alt+F10', toggleRecording: 'Alt+F9', screenshot: 'Alt+F12',
      splitClip: 'S', exportClip: 'Ctrl+E', toggleMic: 'Alt+M',
      undo: 'Ctrl+Z', redo: 'Ctrl+Shift+Z', toggleEarBlast: '',
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
      { name: 'Track 1', source: 'OpenGG_Game.monitor' },
      { name: 'Track 2', source: 'OpenGG_Chat.monitor' },
      { name: 'Track 3', source: 'OpenGG_Mic.monitor'  },
    ],
    runAtStartup:      false,
    runInBackground:   true,
    clip_directories:  ['~/Videos/OpenGG'],
    screenshotDirs:    ['~/Pictures'],
    appBoxCount:       3,
    appBoxPerRow:      1,
    gsrEnabled:        false,
    gsrFps:            60,
    gsrQuality:        'cbr',
    gsrCbrBitrate:     8000,
    gsrReplaySecs:     30,
    gsrReplayPreset:   '30',
    gsrRestartOnSave:  false,
    gsrMonitorTarget:  'screen',
    gsrAutoStart:      true,
    gsrAutoRestart:    true,
    steamLibraryAccess: 'unknown',
    enableClipNotifications: true,
    notificationStyle: 'auto' as 'auto' | 'gsr-notify' | 'x11-overlay' | 'system' | 'disabled',
    notificationPosition: 'top-right' as 'top-right' | 'top-left' | 'bottom-right' | 'bottom-left',
    notificationDuration: 4,
    rtlMode:               false,
    tutorialSeen:          false,
    languagePicked:        false,
  },
  modules: { audio: true, device: true, replay: true },
  extensions: {},
  extensionConsents: {},
  macros: {},
}

// ══════════════════════════════════════════════════════════════════════
//  Versioned Migrations
// ══════════════════════════════════════════════════════════════════════
// Each key is the target schema version. The function mutates the parsed
// state object in-place. Migrations run sequentially from (savedVersion+1)
// up to CURRENT_SCHEMA_VERSION.

const MIGRATIONS: Record<number, (state: any) => void> = {
  1: (s) => {
    // clipsFolder → clip_directories
    if (s?.settings?.clipsFolder && !s?.settings?.clip_directories?.length) {
      s.settings.clip_directories = [s.settings.clipsFolder]
    }
    delete s?.settings?.clipsFolder
    delete s?.settings?.clipSources
  },
  2: (s) => {
    // screenshotDir (string) → screenshotDirs (array)
    if (s?.settings?.screenshotDir && !s?.settings?.screenshotDirs?.length) {
      s.settings.screenshotDirs = [s.settings.screenshotDir]
    }
    delete s?.settings?.screenshotDir
  },
  3: (s) => {
    // drop stale showTrackIcons
    delete s?.settings?.showTrackIcons
  },
  4: (s) => {
    // fix stale gsrMonitorTarget values
    const target = s?.settings?.gsrMonitorTarget
    if (target && (/^\d+x\d+/.test(target) || target.includes(' '))) {
      s.settings.gsrMonitorTarget = 'screen'
    }
  },
  5: (s) => {
    // ensure O1 (Overlays) exists and is positioned before V1 (Video)
    const defs = s?.settings?.trackDefs
    if (!Array.isArray(defs)) return
    const o1Def = DEFAULTS.settings.trackDefs.find((d: TrackDef) => d.id === 'O1')
    const o1i = defs.findIndex((d: TrackDef) => d.id === 'O1')
    const v1i = defs.findIndex((d: TrackDef) => d.id === 'V1')
    if (o1i === -1 && o1Def) {
      defs.splice(v1i !== -1 ? v1i : 0, 0, { ...o1Def })
    } else if (o1i !== -1 && v1i !== -1 && o1i > v1i) {
      const [o1] = defs.splice(o1i, 1)
      defs.splice(v1i, 0, o1)
    }
  },
  6: (s) => {
    // ensure A1 exists
    const defs = s?.settings?.trackDefs
    if (!Array.isArray(defs)) return
    const a1Def = DEFAULTS.settings.trackDefs.find((d: TrackDef) => d.id === 'A1')
    const a1i = defs.findIndex((d: TrackDef) => d.id === 'A1')
    if (a1i === -1 && a1Def) {
      const v1Pos = defs.findIndex((d: TrackDef) => d.id === 'V1')
      defs.splice(v1Pos !== -1 ? v1Pos + 1 : defs.length, 0, { ...a1Def })
    }
  },
  7: (s) => {
    // ensure all tracks have the visible field
    const defs = s?.settings?.trackDefs
    if (!Array.isArray(defs)) return
    for (const def of defs) {
      if (def.visible === undefined) def.visible = true
    }
  },
  8: (s) => {
    // schema version field introduced — no data changes, just bookkeeping
    s._schemaVersion = CURRENT_SCHEMA_VERSION
  },
  9: (s) => {
    // ★ Ear Blast Protection defaults
    if (!s?.mixer?.earBlast) {
      s.mixer = s.mixer || {}
      s.mixer.earBlast = { enabled: false, channels: ['Game'], threshold: 85, target: 60 }
    }
  },
  10: (s) => {
    // Normalize legacy "connector|resolution" gsrMonitorTarget composites
    // (e.g. "DP-1|1920x1080"). GSR's -w only accepts the bare connector name —
    // passing the composite makes GSR crash with `display "1920x1080" not found`.
    // Migration 4 only caught pure resolution/space values, not this composite.
    const t = s?.settings?.gsrMonitorTarget
    if (typeof t === 'string') {
      const connector = t.split('|')[0].trim()
      if (!connector || /^\d/.test(connector) || connector.includes(' ')) {
        s.settings.gsrMonitorTarget = 'screen'
      } else if (connector !== t) {
        s.settings.gsrMonitorTarget = connector
      }
    }
  },
  11: (s) => {
    // captureTracks now store the real PipeWire source node.name (what the recorder
    // captures from) so the live "Audio Capture Devices" dropdown can select any source.
    // Convert legacy friendly labels ("Game"/"Chat"/"Media"/"Aux"/"Mic") to the real
    // OpenGG channel monitor node names. Values already containing '.'/'_' are left as-is.
    const tracks = s?.settings?.captureTracks
    if (Array.isArray(tracks)) {
      const LEGACY = ['Game', 'Chat', 'Media', 'Aux', 'Mic']
      for (const tr of tracks) {
        if (tr && typeof tr.source === 'string' && LEGACY.includes(tr.source)) {
          tr.source = `OpenGG_${tr.source}.monitor`
        }
      }
    }
  },
  12: (s) => {
    // Mark existing users as having already picked a language (skip the first-launch picker)
    if (s?.settings) s.settings.languagePicked = true
  },
}

export function runMigrations(parsed: any): void {
  const savedVersion = typeof parsed?._schemaVersion === 'number' ? parsed._schemaVersion : 0
  for (let v = savedVersion + 1; v <= CURRENT_SCHEMA_VERSION; v++) {
    const migrate = MIGRATIONS[v]
    if (migrate) {
      try {
        migrate(parsed)
      } catch (e) {
        console.warn(`Migration ${v} failed:`, e)
      }
    }
  }
  parsed._schemaVersion = CURRENT_SCHEMA_VERSION
}

export const usePersistenceStore = defineStore('persistence', () => {
  const state = ref<PersistedState>(structuredClone(DEFAULTS))
  const loaded = ref(false)

  async function load() {
    try {
      const j = await invoke<string>('load_ui_settings')
      if (j && j !== 'null') {
        const parsed = JSON.parse(j)
        runMigrations(parsed)
        state.value = deepMerge(structuredClone(DEFAULTS), parsed)
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

  function setEarBlastEnabled(v: boolean) { state.value.mixer.earBlast.enabled = v }
  function setEarBlastChannels(chs: string[]) { state.value.mixer.earBlast.channels = chs }
  function setEarBlastThreshold(v: number) { state.value.mixer.earBlast.threshold = Math.max(1, Math.min(100, v)) }
  function setEarBlastTarget(v: number) { state.value.mixer.earBlast.target = Math.max(0, Math.min(100, v)) }

  function resetShortcuts() { state.value.settings.shortcuts = structuredClone(DEFAULTS.settings.shortcuts) }

  function resetAllSettings() {
    state.value = structuredClone(DEFAULTS)
    save()
  }

  return { state, loaded, load, save, setChannelVolume, setChannelMute, setChannelDevice, setAppRule, getAppRule, setEarBlastEnabled, setEarBlastChannels, setEarBlastThreshold, setEarBlastTarget, resetShortcuts, resetAllSettings }
})

export function deepMerge(a: any, b: any): any {
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

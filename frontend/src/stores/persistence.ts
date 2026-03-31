import { defineStore } from 'pinia'
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface TrackDef {
  id: string    // 'V1', 'A1', 'A2', … 'O1'
  name: string  // user-editable display name
  color: string // hex color
  icon: string  // 'video' | 'game' | 'chat' | 'mic' | 'media' | 'overlay'
}

export interface PersistedState {
  mixer: { volumes: Record<string, number>; mutes: Record<string, boolean>; devices: Record<string, string>; appRules: Record<string, string> }
  settings: {
    fps: number; quality: string; replayDuration: number
    defaultClickAction: 'preview' | 'editor'
    clipsPerRow: 2 | 3 | 4 | 5
    shortcuts: {
      saveReplay: string; toggleRecording: string; screenshot: string
      splitClip: string; exportClip: string; toggleMic: string
      undo: string; redo: string
    }
    videoQuality: 'High' | 'Medium' | 'Low'
    videoResolution: '1080p' | '720p' | '480p'
    language: string
    trackDefs: TrackDef[]
    showTrackIcons: boolean
    captureTracks: Array<{ name: string; source: string }>
    // ★ Epic 4: daemon settings
    runAtStartup: boolean
    runInBackground: boolean
    // Clip directories (watched for new clips)
    clip_directories: string[]
    screenshotDir: string
    // ★ GPU Screen Recorder
    gsrEnabled: boolean
    gsrFps: number
    gsrQuality: 'High' | 'Medium' | 'Low'
    gsrReplaySecs: number
  }
  modules: { audio: boolean; device: boolean; replay: boolean }
  extensions: { overlays: boolean; tiktokExport: boolean }
}

export const DEFAULTS: PersistedState = {
  mixer: { volumes: { Game:100, Chat:100, Media:100, Aux:100, Mic:100 }, mutes:{}, devices:{}, appRules:{} },
  settings: {
    fps: 60, quality: 'High', replayDuration: 30,
    defaultClickAction: 'preview', clipsPerRow: 4,
    shortcuts: {
      saveReplay: 'Alt+F10', toggleRecording: 'Alt+F9', screenshot: 'Alt+F12',
      splitClip: 'S', exportClip: 'Ctrl+E', toggleMic: 'Alt+M',
      undo: 'Ctrl+Z', redo: 'Ctrl+Shift+Z',
    },
    videoQuality: 'High', videoResolution: '1080p', language: 'en',
    trackDefs: [
      { id: 'V1', name: 'Video',    color: '#E94560', icon: 'video'   },
      { id: 'A1', name: 'Audio 1',  color: '#10b981', icon: 'game'    },
      { id: 'A2', name: 'Audio 2',  color: '#3b82f6', icon: 'chat'    },
      { id: 'A3', name: 'Audio 3',  color: '#f59e0b', icon: 'mic'     },
      { id: 'A4', name: 'Audio 4',  color: '#8b5cf6', icon: 'media'   },
      { id: 'A5', name: 'Audio 5',  color: '#ec4899', icon: 'media'   },
      { id: 'O1', name: 'Overlays', color: '#f97316', icon: 'overlay' },
    ],
    showTrackIcons: true,
    captureTracks: [
      { name: 'Track 1', source: 'Game' },
      { name: 'Track 2', source: 'Chat' },
      { name: 'Track 3', source: 'Mic'  },
    ],
    runAtStartup:      false,
    runInBackground:   true,
    clip_directories:  ['~/Videos/OpenGG'],
    screenshotDir:     '',
    gsrEnabled:        false,
    gsrFps:            60,
    gsrQuality:        'High',
    gsrReplaySecs:     30,
  },
  modules: { audio: true, device: true, replay: true },
  extensions: { overlays: false, tiktokExport: false },
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

import { defineStore } from 'pinia'
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface PersistedState {
  mixer: { volumes: Record<string, number>; mutes: Record<string, boolean>; devices: Record<string, string>; appRules: Record<string, string> }
  settings: {
    clipsFolder: string; fps: number; quality: string; replayDuration: number
    defaultClickAction: 'preview' | 'editor'
    clipsPerRow: 3 | 4 | 5
    shortcuts: { saveReplay: string; toggleRecording: string; screenshot: string }
  }
  modules: { audio: boolean; device: boolean; replay: boolean }
}

const DEFAULTS: PersistedState = {
  mixer: { volumes: { Game:100, Chat:100, Media:100, Aux:100, Mic:100 }, mutes:{}, devices:{}, appRules:{} },
  settings: {
    clipsFolder: '~/Videos/OpenGG', fps: 60, quality: 'High', replayDuration: 30,
    defaultClickAction: 'preview', clipsPerRow: 4,
    shortcuts: { saveReplay: 'Alt+F10', toggleRecording: 'Alt+F9', screenshot: 'Alt+F12' }
  },
  modules: { audio: true, device: true, replay: true },
}

export const usePersistenceStore = defineStore('persistence', () => {
  const state = ref<PersistedState>(structuredClone(DEFAULTS))
  const loaded = ref(false)

  async function load() {
    try {
      const j = await invoke<string>('load_ui_settings')
      if (j && j !== 'null') state.value = deepMerge(structuredClone(DEFAULTS), JSON.parse(j))
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

  return { state, loaded, load, save, setChannelVolume, setChannelMute, setChannelDevice, setAppRule, getAppRule }
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

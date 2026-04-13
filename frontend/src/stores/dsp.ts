import { defineStore } from 'pinia'
import { reactive } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface EqBands    { enabled: boolean; preamp: number; bands: number[] }
export interface NrState    { enabled: boolean; intensity: number }
export interface GateState  { enabled: boolean; threshold: number; auto: boolean }
export interface CompState  { enabled: boolean; level: number }
export interface DspChannel { nr: NrState; gate: GateState; comp: CompState }

const mkEq  = (): EqBands    => ({ enabled: false, preamp: 0, bands: Array(10).fill(0) })
const mkDsp = (): DspChannel => ({
  nr:   { enabled: false, intensity: 50 },
  gate: { enabled: false, threshold: -40, auto: false },
  comp: { enabled: false, level: 50 },
})

// ── EQ presets (10 bands: 32 64 125 250 500 1k 2k 4k 8k 16k) ──────────────
const EQ_PRESETS: Record<string, number[]> = {
  Default: Array(10).fill(0),
  Gaming:  [3, 2, 0, 0, 0, 0, 2, 2, 1, 1],
  Movies:  [2, 2, 1, 0, 1, 2, 2, 1, 0, 0],
  Music:   [3, 2, 0, 0, -1, 0, 1, 2, 2, 1],
}

// ── DSP presets ──────────────────────────────────────────────────────────────
interface DspPreset { nr: Partial<NrState>; gate: Partial<GateState>; comp: Partial<CompState> }
const DSP_PRESETS: Record<string, DspPreset> = {
  Default: {
    nr: { enabled: false, intensity: 50 }, gate: { enabled: false, threshold: -40, auto: false }, comp: { enabled: false, level: 50 },
  },
  Broadcast: {
    nr: { enabled: true, intensity: 50 }, gate: { enabled: false, threshold: -40, auto: false }, comp: { enabled: true, level: 70 },
  },
  Podcaster: {
    nr: { enabled: true, intensity: 30 }, gate: { enabled: true, threshold: -30, auto: false }, comp: { enabled: true, level: 60 },
  },
  'Noise-Reduction Focus': {
    nr: { enabled: true, intensity: 80 }, gate: { enabled: true, threshold: -40, auto: true }, comp: { enabled: false, level: 50 },
  },
}

export const EQ_PRESET_NAMES  = Object.keys(EQ_PRESETS)
export const DSP_PRESET_NAMES = Object.keys(DSP_PRESETS)

export const useDspStore = defineStore('dsp', () => {
  // EQ channels: Game, Chat, Media, Aux
  const eq = reactive<Record<string, EqBands>>({
    Game: mkEq(), Chat: mkEq(), Media: mkEq(), Aux: mkEq(),
  })
  // DSP channels: Chat, Mic
  const dsp = reactive<Record<string, DspChannel>>({
    Chat: mkDsp(), Mic: mkDsp(),
  })

  // Active preset label per channel — set to 'Custom' on any manual tweak
  const activeEqPreset  = reactive<Record<string, string>>({ Game: 'Default', Chat: 'Default', Media: 'Default', Aux: 'Default' })
  const activeDspPreset = reactive<Record<string, string>>({ Chat: 'Default', Mic: 'Default' })

  // ── EQ ───────────────────────────────────────────────────────────────────
  async function setEqEnabled(ch: string, val: boolean) {
    if (!eq[ch]) return
    eq[ch].enabled = val
    if (val) {
      try {
        await invoke('start_eq_engine', { channel: ch })
        await invoke('apply_eq', { channel: ch, bands: [...eq[ch].bands] })
      } catch (e) { console.error('[opengg] start_eq_engine failed:', e) }
    } else {
      try { await invoke('stop_eq_engine', { channel: ch }) } catch (e) { console.error('[opengg] stop_eq_engine failed:', e) }
    }
  }
  function setEqBand(ch: string, i: number, val: number) {
    if (!eq[ch]) return
    eq[ch].bands[i] = val
    activeEqPreset[ch] = 'Custom'
    if (eq[ch].enabled) invoke('apply_eq', { channel: ch, bands: [...eq[ch].bands] }).catch(e => console.error('[opengg] apply_eq failed:', e))
  }
  function setPreamp(ch: string, val: number) {
    if (!eq[ch]) return
    eq[ch].preamp = val
    activeEqPreset[ch] = 'Custom'
    if (eq[ch].enabled) invoke('apply_eq', { channel: ch, bands: [...eq[ch].bands] }).catch(e => console.error('[opengg] apply_eq failed:', e))
  }
  function resetEq(ch: string) {
    if (!eq[ch]) return
    eq[ch].bands = Array(10).fill(0)
    eq[ch].preamp = 0
    activeEqPreset[ch] = 'Default'
    if (eq[ch].enabled) invoke('apply_eq', { channel: ch, bands: Array(10).fill(0) }).catch(e => console.error('[opengg] apply_eq failed:', e))
  }
  function setEqPreset(ch: string, name: string) {
    if (!eq[ch] || !EQ_PRESETS[name]) return
    eq[ch].bands = [...EQ_PRESETS[name]]
    eq[ch].preamp = 0
    activeEqPreset[ch] = name
    if (eq[ch].enabled) invoke('apply_eq', { channel: ch, bands: [...eq[ch].bands] }).catch(e => console.error('[opengg] apply_eq failed:', e))
  }

  // ── DSP ──────────────────────────────────────────────────────────────────
  function setNr(ch: string, patch: Partial<NrState>) {
    if (!dsp[ch]) return
    Object.assign(dsp[ch].nr, patch)
    activeDspPreset[ch] = 'Custom'
    invoke('apply_noise_reduction', { channel: ch, enabled: dsp[ch].nr.enabled, intensity: dsp[ch].nr.intensity }).catch(() => {})
  }
  function setGate(ch: string, patch: Partial<GateState>) {
    if (!dsp[ch]) return
    Object.assign(dsp[ch].gate, patch)
    activeDspPreset[ch] = 'Custom'
    invoke('apply_noise_gate', { channel: ch, enabled: dsp[ch].gate.enabled, threshold: dsp[ch].gate.threshold, autoDetect: dsp[ch].gate.auto }).catch(() => {})
  }
  function setComp(ch: string, patch: Partial<CompState>) {
    if (!dsp[ch]) return
    Object.assign(dsp[ch].comp, patch)
    activeDspPreset[ch] = 'Custom'
    invoke('apply_compressor', { channel: ch, enabled: dsp[ch].comp.enabled, level: dsp[ch].comp.level }).catch(() => {})
  }
  function setDspPreset(ch: string, name: string) {
    if (!dsp[ch] || !DSP_PRESETS[name]) return
    const p = DSP_PRESETS[name]
    Object.assign(dsp[ch].nr,   p.nr)
    Object.assign(dsp[ch].gate, p.gate)
    Object.assign(dsp[ch].comp, p.comp)
    activeDspPreset[ch] = name
    invoke('apply_noise_reduction', { channel: ch, enabled: dsp[ch].nr.enabled, intensity: dsp[ch].nr.intensity }).catch(() => {})
    invoke('apply_noise_gate',      { channel: ch, enabled: dsp[ch].gate.enabled, threshold: dsp[ch].gate.threshold, autoDetect: dsp[ch].gate.auto }).catch(() => {})
    invoke('apply_compressor',      { channel: ch, enabled: dsp[ch].comp.enabled, level: dsp[ch].comp.level }).catch(() => {})
  }

  return {
    eq, dsp, activeEqPreset, activeDspPreset,
    EQ_PRESET_NAMES, DSP_PRESET_NAMES,
    setEqEnabled, setEqBand, setPreamp, resetEq, setEqPreset,
    setNr, setGate, setComp, setDspPreset,
  }
})

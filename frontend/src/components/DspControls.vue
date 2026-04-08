<script setup lang="ts">
import { computed } from 'vue'
import { useDspStore } from '../stores/dsp'

const props = defineProps<{ channel: string; color: string }>()
const store = useDspStore()
const ch = computed(() => store.dsp[props.channel])
</script>

<template>
  <div class="dsp-wrap">
    <div class="dsp-page-hdr">
      <span class="dsp-title">DSP Controls</span>
      <span class="dsp-ch" :style="{ color }">{{ channel }}</span>

      <!-- Preset dropdown -->
      <div class="preset-wrap">
        <select
          class="preset-sel"
          :value="store.activeDspPreset[channel]"
          @change="store.setDspPreset(channel, ($event.target as HTMLSelectElement).value)"
        >
          <option v-for="p in store.DSP_PRESET_NAMES" :key="p" :value="p">{{ p }}</option>
          <option v-if="store.activeDspPreset[channel] === 'Custom'" value="Custom">Custom</option>
        </select>
        <svg class="preset-arrow" viewBox="0 0 10 6" fill="currentColor"><path d="M0 0l5 6 5-6z"/></svg>
      </div>
    </div>

    <div class="dsp-cards">

      <!-- Noise Reduction -->
      <div class="dsp-card" :class="{ 'dsp-card--on': ch?.nr.enabled }">
        <div class="card-hdr">
          <div class="card-icon" :style="ch?.nr.enabled ? { background: color + '18', color } : {}">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 1a3 3 0 00-3 3v8a3 3 0 006 0V4a3 3 0 00-3-3z"/><path d="M19 10v2a7 7 0 01-14 0v-2"/></svg>
          </div>
          <div class="card-info">
            <span class="card-name">Noise Reduction</span>
            <span class="card-desc">Suppresses background hiss and room noise</span>
          </div>
          <label class="tog-wrap">
            <input type="checkbox" :checked="ch?.nr.enabled" @change="store.setNr(channel, { enabled: ($event.target as HTMLInputElement).checked })" />
            <span class="tog-track"><span class="tog-thumb"></span></span>
          </label>
        </div>
        <!-- Always rendered; disabled state via CSS when off -->
        <div class="card-body" :class="{ 'card-body--off': !ch?.nr.enabled }">
          <div class="ctrl-row">
            <span class="ctrl-lbl">Intensity</span>
            <span class="ctrl-val" :style="{ color }">{{ ch?.nr.intensity ?? 50 }}%</span>
          </div>
          <input type="range" class="ctrl-sl" min="0" max="100" step="1"
            :value="ch?.nr.intensity ?? 50" :style="{ '--ch': color }"
            @input="store.setNr(channel, { intensity: parseInt(($event.target as HTMLInputElement).value) })" />
        </div>
      </div>

      <!-- Noise Gate -->
      <div class="dsp-card" :class="{ 'dsp-card--on': ch?.gate.enabled }">
        <div class="card-hdr">
          <div class="card-icon" :style="ch?.gate.enabled ? { background: color + '18', color } : {}">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="9" y1="3" x2="9" y2="21"/><line x1="15" y1="3" x2="15" y2="21"/></svg>
          </div>
          <div class="card-info">
            <span class="card-name">Noise Gate</span>
            <span class="card-desc">Silences signal below a threshold level</span>
          </div>
          <label class="tog-wrap">
            <input type="checkbox" :checked="ch?.gate.enabled" @change="store.setGate(channel, { enabled: ($event.target as HTMLInputElement).checked })" />
            <span class="tog-track"><span class="tog-thumb"></span></span>
          </label>
        </div>
        <div class="card-body" :class="{ 'card-body--off': !ch?.gate.enabled }">
          <div class="ctrl-row">
            <span class="ctrl-lbl">Threshold</span>
            <span class="ctrl-val" :style="{ color }">{{ ch?.gate.threshold ?? -40 }} dB</span>
          </div>
          <input type="range" class="ctrl-sl" min="-80" max="0" step="1"
            :value="ch?.gate.threshold ?? -40" :style="{ '--ch': color }"
            @input="store.setGate(channel, { threshold: parseInt(($event.target as HTMLInputElement).value) })" />
          <label class="check-row">
            <input type="checkbox" :checked="ch?.gate.auto" @change="store.setGate(channel, { auto: ($event.target as HTMLInputElement).checked })" />
            <span>Auto-detect threshold</span>
          </label>
        </div>
      </div>

      <!-- Compressor -->
      <div class="dsp-card" :class="{ 'dsp-card--on': ch?.comp.enabled }">
        <div class="card-hdr">
          <div class="card-icon" :style="ch?.comp.enabled ? { background: color + '18', color } : {}">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/></svg>
          </div>
          <div class="card-info">
            <span class="card-name">Compressor</span>
            <span class="card-desc">Evens out loud and quiet parts of the signal</span>
          </div>
          <label class="tog-wrap">
            <input type="checkbox" :checked="ch?.comp.enabled" @change="store.setComp(channel, { enabled: ($event.target as HTMLInputElement).checked })" />
            <span class="tog-track"><span class="tog-thumb"></span></span>
          </label>
        </div>
        <div class="card-body" :class="{ 'card-body--off': !ch?.comp.enabled }">
          <div class="ctrl-row">
            <span class="ctrl-lbl">Level</span>
            <span class="ctrl-val" :style="{ color }">{{ ch?.comp.level ?? 50 }}%</span>
          </div>
          <input type="range" class="ctrl-sl" min="0" max="100" step="1"
            :value="ch?.comp.level ?? 50" :style="{ '--ch': color }"
            @input="store.setComp(channel, { level: parseInt(($event.target as HTMLInputElement).value) })" />
        </div>
      </div>

    </div>
  </div>
</template>

<style scoped>
.dsp-wrap { display: flex; flex-direction: column; gap: 16px; width: 100%; }

.dsp-page-hdr { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
.dsp-title { font-size: 14px; font-weight: 700; }
.dsp-ch    { font-size: 12px; font-weight: 600; text-transform: uppercase; letter-spacing: 1px; }

/* Preset dropdown */
.preset-wrap {
  position: relative; display: flex; align-items: center; margin-left: auto;
}
.preset-sel {
  appearance: none; padding: 4px 28px 4px 10px;
  border-radius: 6px; border: 1px solid var(--border);
  background: var(--bg-deep); color: var(--text);
  font-size: 11px; font-weight: 600; cursor: pointer;
  transition: border-color .15s;
}
.preset-sel:hover { border-color: var(--text-muted); }
.preset-sel:focus { outline: none; border-color: var(--accent); }
.preset-arrow {
  position: absolute; right: 9px; width: 8px; height: 5px;
  color: var(--text-muted); pointer-events: none;
}

/* Cards grid — 1 col by default, 2 cols on wide screens */
.dsp-cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: 10px;
}

.dsp-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
  transition: border-color .2s;
}
.dsp-card--on { border-color: color-mix(in srgb, currentColor 30%, var(--border)); }

.card-hdr {
  display: flex; align-items: center; gap: 12px;
  padding: 14px 16px;
}
.card-icon {
  width: 36px; height: 36px; border-radius: 8px;
  background: var(--bg-deep); color: var(--text-muted);
  display: flex; align-items: center; justify-content: center;
  flex-shrink: 0; transition: background .2s, color .2s;
}
.card-icon svg { width: 18px; height: 18px; }
.card-info { flex: 1; display: flex; flex-direction: column; gap: 2px; }
.card-name { font-size: 13px; font-weight: 600; }
.card-desc { font-size: 11px; color: var(--text-muted); }

/* Toggle */
.tog-wrap  { display: flex; align-items: center; cursor: pointer; flex-shrink: 0; }
.tog-wrap input { display: none; }
.tog-track {
  width: 38px; height: 20px; border-radius: 10px;
  background: var(--bg-deep); border: 1px solid var(--border);
  position: relative; transition: background .2s, border-color .2s;
}
.tog-wrap input:checked ~ .tog-track { background: var(--accent); border-color: var(--accent); }
.tog-thumb {
  position: absolute; top: 3px; left: 3px;
  width: 12px; height: 12px; border-radius: 50%;
  background: #fff; transition: left .2s; box-shadow: 0 1px 3px rgba(0,0,0,.4);
}
.tog-wrap input:checked ~ .tog-track .tog-thumb { left: 21px; }

/* Card body — always rendered, dimmed when off */
.card-body {
  padding: 0 16px 14px;
  display: flex; flex-direction: column; gap: 8px;
  border-top: 1px solid var(--border);
  transition: opacity .2s;
}
.card-body--off {
  opacity: .4;
  pointer-events: none;
}

.ctrl-row { display: flex; justify-content: space-between; align-items: center; padding-top: 12px; }
.ctrl-lbl { font-size: 11px; color: var(--text-muted); }
.ctrl-val { font-size: 12px; font-weight: 700; font-variant-numeric: tabular-nums; }

.ctrl-sl {
  width: 100%; cursor: pointer;
  accent-color: var(--ch, var(--accent));
}

.check-row {
  display: flex; align-items: center; gap: 6px;
  font-size: 11px; color: var(--text-sec); cursor: pointer; user-select: none;
}
.check-row input { accent-color: var(--accent); cursor: pointer; }
</style>

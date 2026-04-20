<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { useI18n } from 'vue-i18n'
import type { Channel, AudioDevice } from '../stores/audio'

const { t } = useI18n()

const props = defineProps<{
  channel: Channel; color: string; type: 'output' | 'input' | 'master'
  vuLevel?: number; devices?: AudioDevice[]; selectedDevice?: string
  overdrive?: boolean   // ★ Epic 2: when true, fader max extends to 150%
}>()
const emit = defineEmits<{
  'update:volume': [number]; 'update:mute': [boolean]; 'update:device': [string]
}>()

const deviceOpen = ref(false)
const dropdownRef = ref<HTMLElement | null>(null)

function toggleDropdown() { deviceOpen.value = !deviceOpen.value }
function onClickOutside(e: MouseEvent) {
  if (!deviceOpen.value) return
  if (dropdownRef.value && !dropdownRef.value.contains(e.target as Node)) deviceOpen.value = false
}
onMounted(() => document.addEventListener('mousedown', onClickOutside))
onBeforeUnmount(() => document.removeEventListener('mousedown', onClickOutside))

// VU — rAF lerp for butter-smooth attack + peak hold
const peakLevel     = ref(0)
const vu            = computed(() => {
  const db = props.vuLevel ?? -60
  return ((Math.max(-60, Math.min(0, db)) + 60) / 60) * 100
})
const currentVuHeight = ref(0)   // lerped display value (drives DOM)

// Peak ballistics still driven by the raw vu value
watch(vu, (newVal) => {
  if (newVal > peakLevel.value) peakLevel.value = newVal
  else peakLevel.value = Math.max(0, peakLevel.value - 1.2)
})

// rAF lerp loop — fast attack (α=0.4), slow release (α=0.12)
let vuRaf = 0
onMounted(() => {
  const tick = () => {
    const target = vu.value
    const alpha  = target > currentVuHeight.value ? 0.4 : 0.12
    currentVuHeight.value = currentVuHeight.value + (target - currentVuHeight.value) * alpha
    vuRaf = requestAnimationFrame(tick)
  }
  vuRaf = requestAnimationFrame(tick)
})
onBeforeUnmount(() => cancelAnimationFrame(vuRaf))

const vuColor = computed(() => {
  const db = props.vuLevel ?? -60
  return db > -3 ? '#ef4444' : db > -12 ? '#f59e0b' : props.color
})
const vuDbText = computed(() => {
  const db = props.vuLevel ?? -60
  return db <= -59.9 ? '-∞' : db.toFixed(1)
})

// ★ Epic 2 — Overdrive: max volume cap
const maxVol = computed(() => props.overdrive ? 150 : 100)

// Fader fill height as % of the track (0–100 visual)
const fillH = computed(() =>
  Math.min(props.channel.volume, maxVol.value) / maxVol.value * 100
)

// ★ Epic 2 — Dragging tooltip state
const isDragging = ref(false)

function faderInteract(e: MouseEvent, track: HTMLElement) {
  const rect = track.getBoundingClientRect()
  const vol  = Math.round(
    Math.max(0, Math.min(maxVol.value, (1 - (e.clientY - rect.top) / rect.height) * maxVol.value))
  )
  emit('update:volume', vol)
}

function onFaderDown(e: MouseEvent) {
  e.preventDefault()
  isDragging.value = true
  const t  = e.currentTarget as HTMLElement
  faderInteract(e, t)
  const mv = (ev: MouseEvent) => faderInteract(ev, t)
  const up = () => {
    isDragging.value = false
    document.removeEventListener('mousemove', mv)
    document.removeEventListener('mouseup',   up)
  }
  document.addEventListener('mousemove', mv)
  document.addEventListener('mouseup',   up)
}
</script>

<template>
  <div class="strip" :class="[`strip--${type}`, { muted: channel.muted }]" :style="{ '--ch': color }">
    <div class="accent-bar" :style="{ background: color }"></div>
    <div class="hdr">
      <div class="icon-box" :style="{ background: color + '18', color }"><slot name="icon" /></div>
      <span class="name">{{ channel.name }}</span>
    </div>

    <div class="fader-row">
      <!-- VU meter -->
      <div class="vu">
        <div class="vu-track">
          <div class="vu-fill" :style="{ height:currentVuHeight+'%', background:`linear-gradient(to top,${color}50,${vuColor})` }"></div>
          <div class="vu-peak" :style="{ bottom:peakLevel+'%' }" v-show="peakLevel > 2"></div>
        </div>
        <div class="vu-ticks"><span>0</span><span>-20</span><span>-40</span><span>-60</span></div>
      </div>

      <!-- Custom fader -->
      <div class="fader" @mousedown="onFaderDown">
        <div class="fader-track">
          <div class="fader-fill" :style="{ height:fillH+'%', background:color }"></div>

          <!-- ★ Epic 3: Tooltip — always visible while dragging, floats above the thumb -->
          <div
            class="fader-tip"
            :class="{ overdrive: channel.volume > 100 }"
            v-show="isDragging"
            :style="{ bottom: `calc(${fillH}% + 16px)` }"
          >{{ channel.volume }}<span class="tip-u">%</span></div>

          <div class="fader-thumb" :style="{ bottom:`calc(${fillH}% - 7px)` }">
            <div class="grip"></div>
          </div>
        </div>
      </div>
    </div>

    <!-- Static vol label — always visible; tooltip also shows during drag -->
    <div class="vol" :style="{ color: channel.muted ? 'var(--text-muted)' : color }">
      {{ channel.volume }}<span class="vol-u">%</span>
    </div>
    <div class="vol-db">{{ vuDbText }}<span class="voldb-u"> dB</span></div>

    <!-- Mute + Device row — mute always visible, dropdown only when devices present -->
    <div class="dev-wrap" ref="dropdownRef">
      <div class="dev-row">
        <button class="cb mute dev-mute" :class="{ active: channel.muted }" @click="emit('update:mute', !channel.muted)" title="Mute">
          <svg v-if="channel.muted" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>
          <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 010 7.07"/></svg>
        </button>
        <button v-if="devices && devices.length > 0" class="dev-btn" @click="toggleDropdown">
          <svg v-if="type!=='input'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="dev-ic"><path d="M3 18v-6a9 9 0 0118 0v6"/><path d="M21 19a2 2 0 01-2 2h-1a2 2 0 01-2-2v-3a2 2 0 012-2h3zM3 19a2 2 0 002 2h1a2 2 0 002-2v-3a2 2 0 00-2-2H3z"/></svg>
          <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="dev-ic"><path d="M12 1a3 3 0 00-3 3v8a3 3 0 006 0V4a3 3 0 00-3-3z"/><path d="M19 10v2a7 7 0 01-14 0v-2"/></svg>
          <span class="dev-name">{{ selectedDevice || t('devices.defaultDevice') }}</span>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="dev-chev"><path d="M6 9l6 6 6-6"/></svg>
        </button>
      </div>
      <div class="dev-drop" v-show="deviceOpen && devices && devices.length > 0">
        <button v-for="d in devices" :key="d.name" class="dev-opt"
          :class="{ active: d.name===selectedDevice || (!selectedDevice && d.is_default) }"
          @click="emit('update:device',d.name); deviceOpen=false">{{ d.description }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.strip {
  background: var(--bg-card); border: 1px solid var(--border); border-radius: 10px;
  padding: 0 10px 10px; display: flex; flex-direction: column; align-items: center; gap: 5px;
  transition: border-color .2s, opacity .2s; position: relative;
  min-height: 0; height: 100%;
}
.strip:hover { border-color: color-mix(in srgb, var(--ch) 40%, transparent); }
.strip.muted { opacity: .45; }
.strip--input  { border-style: solid; }
.accent-bar { width: calc(100% + 2px); height: 3px; border-radius: 10px 10px 0 0; margin: -1px -1px 4px; opacity: .8; }
.hdr { display: flex; flex-direction: column; align-items: center; gap: 3px; flex-shrink: 0; }
.icon-box { width: 28px; height: 28px; border-radius: 6px; display: flex; align-items: center; justify-content: center; }
.icon-box :deep(svg) { width: 15px; height: 15px; }
.name { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 1.2px; }

.fader-row {
  display: flex; gap: 5px; align-items: stretch; width: 100%; justify-content: center;
  flex: 1 1 0%; min-height: 120px; /* grows to fill strip, never below 120px */
  padding: 10px 0; /* breathing room above/below the fader group */
}

/* VU */
.vu { display: flex; gap: 2px; }
.vu-track { width: 5px; background: var(--bg-deep); border-radius: 3px; position: relative; overflow: hidden; border: 1px solid color-mix(in srgb, var(--border) 50%, transparent); }
.vu-fill  { position: absolute; bottom: 0; left: 0; right: 0; border-radius: 3px; opacity: .85; /* no CSS transition — height driven by rAF lerp */ }
.vu-peak  { position: absolute; left: -1px; right: -1px; height: 2px; background: #fff; border-radius: 1px; transition: bottom 0.08s ease-out; box-shadow: 0 0 4px rgba(255,255,255,.5); }
.vu-ticks { display: flex; flex-direction: column; justify-content: space-between; font-size: 7px; color: var(--text-muted); width: 14px; text-align: right; opacity: .5; }

/* Fader */
.fader       { cursor: pointer; display: flex; padding: 4px 6px; /* 4px top/bottom within fader column */ }
.fader-track { width: 12px; background: var(--bg-deep); border-radius: 6px; position: relative; border: 1px solid var(--border); }
.fader-fill  { position: absolute; bottom: 0; left: 0; right: 0; border-radius: 6px; transition: height 40ms ease; opacity: .7; }
.fader-thumb {
  position: absolute; left: 50%; transform: translateX(-50%);
  width: 24px; height: 13px;
  /* Thumb uses the track accent color — lighter tint on top, darker on bottom for 3D depth */
  background: linear-gradient(
    180deg,
    color-mix(in srgb, var(--ch) 55%, #fff),
    color-mix(in srgb, var(--ch) 85%, #000 15%)
  );
  border-radius: 4px;
  box-shadow: 0 1px 3px rgba(0,0,0,.5), inset 0 1px 0 rgba(255,255,255,.2);
  cursor: grab; display: flex; align-items: center; justify-content: center;
}
.fader-thumb:active { cursor: grabbing; filter: brightness(1.2); }
.grip { width: 10px; height: 2px; background: rgba(0,0,0,.25); border-radius: 1px; box-shadow: 0 3px 0 rgba(0,0,0,.15), 0 -3px 0 rgba(0,0,0,.15); }

/* Drag tooltip — floats above the thumb */
.fader-tip {
  position: absolute;
  left: 50%; transform: translateX(-50%);
  background: var(--bg-card);
  border: 1px solid var(--border);
  color: var(--text);
  font-size: 14px; font-weight: 700; font-variant-numeric: tabular-nums;
  padding: 3px 7px; border-radius: 5px;
  white-space: nowrap; pointer-events: none;
  box-shadow: 0 2px 8px rgba(0,0,0,.4);
  z-index: 10;
  /* Fade in */
  animation: tip-in .08s ease-out;
}
.fader-tip::after {
  content: ''; position: absolute; top: 100%; left: 50%; transform: translateX(-50%);
  border: 4px solid transparent; border-top-color: var(--border);
}
/* Accent colour when in overdrive range */
.fader-tip.overdrive { color: #f59e0b; border-color: #f59e0b55; }
.fader-tip.overdrive::after { border-top-color: #f59e0b55; }
.tip-u { font-size: 11px; opacity: .6; }

@keyframes tip-in { from { opacity: 0; transform: translateX(-50%) translateY(3px); } to { opacity: 1; transform: translateX(-50%) translateY(0); } }

/* Volume label */
.vol { font-size: 18px; font-weight: 800; font-variant-numeric: tabular-nums; text-align: center; line-height: 1; transition: color .2s; flex-shrink: 0; }
.vol-u { font-size: 10px; font-weight: 600; opacity: .5; margin-left: 1px; }
.vol-db { font-size: 9px; font-weight: 600; font-variant-numeric: tabular-nums; color: var(--text-muted); opacity: .55; text-align: center; flex-shrink: 0; margin-top: -3px; }
.voldb-u { font-size: 7px; opacity: .7; }

/* Buttons */
.cb { height: 26px; border-radius: 5px; border: 1px solid var(--border); background: var(--bg-deep); color: var(--text-muted); cursor: pointer; display: flex; align-items: center; justify-content: center; transition: all .15s; font-size: 9px; font-weight: 700; letter-spacing: .5px; }
.cb:hover { border-color: var(--ch); color: var(--ch); background: color-mix(in srgb, var(--ch) 8%, transparent); }
.mute svg { width: 13px; height: 13px; }
.mute.active { background: var(--danger); border-color: var(--danger); color: #fff; }

/* Device + Mute row */
.dev-wrap { width: 100%; position: relative; flex-shrink: 0; }
.dev-row  { display: flex; gap: 4px; align-items: center; width: 100%; }

/* Mute inside dev-row — fixed width, same height as dev-btn */
.dev-mute { width: 28px; flex-shrink: 0; }

/* Device trigger — takes remaining width, same height as mute */
.dev-btn  { flex: 1; min-width: 0; height: 26px; display: flex; align-items: center; gap: 4px; padding: 0 6px; background: var(--bg-deep); border: 1px solid var(--border); border-radius: 5px; color: var(--text-sec); font-size: 9px; cursor: pointer; transition: all .15s; }
.dev-btn:hover { border-color: var(--ch); }
.dev-ic   { width: 11px; height: 11px; flex-shrink: 0; }
.dev-name { flex: 1; text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.dev-chev { width: 9px; height: 9px; flex-shrink: 0; opacity: .4; }
.dev-drop { position: absolute; bottom: 100%; left: 0; right: 0; background: var(--bg-card); border: 1px solid var(--border); border-radius: 6px; padding: 2px; z-index: 100; box-shadow: 0 -4px 16px rgba(0,0,0,.4); margin-bottom: 3px; max-height: 180px; overflow-y: auto; }
.dev-opt  { width: 100%; padding: 5px 8px; background: transparent; border: none; border-radius: 4px; color: var(--text-sec); font-size: 10px; text-align: left; cursor: pointer; }
.dev-opt:hover  { background: var(--bg-hover); color: var(--text); }
.dev-opt.active { color: var(--ch); font-weight: 600; }
</style>

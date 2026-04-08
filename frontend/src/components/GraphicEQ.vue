<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted } from 'vue'
import { useDspStore } from '../stores/dsp'

const props = defineProps<{ channel: string; color: string }>()
const store = useDspStore()

const BAND_HZ    = [32, 64, 125, 250, 500, 1000, 2000, 4000, 8000, 16000]
const BAND_LABEL = ['32', '64', '125', '250', '500', '1k', '2k', '4k', '8k', '16k']
const DB_LINES   = [12, 6, 0, -6, -12]

// SVG canvas dimensions
const W = 560
const H = 160
const PAD_L = 28  // left padding for dB labels
const PAD_R = 8
const PAD_T = 10
const PAD_B = 24  // bottom padding for freq labels
const INNER_W = W - PAD_L - PAD_R
const INNER_H = H - PAD_T - PAD_B

const eq = computed(() => store.eq[props.channel])

// Map dB [-12, +12] → SVG Y coordinate within the inner area
function dbToY(db: number): number {
  return PAD_T + ((12 - db) / 24) * INNER_H
}

// Map band index [0..9] → SVG X coordinate (log-scale)
function bandToX(i: number): number {
  // Distribute 10 bands evenly across inner width with equal log spacing
  const t = i / (BAND_HZ.length - 1)
  return PAD_L + t * INNER_W
}

// Build a smooth cubic bezier path through all band points
const curvePath = computed(() => {
  if (!eq.value) return ''
  const pts = BAND_HZ.map((_, i) => ({
    x: bandToX(i),
    y: dbToY(eq.value.bands[i] ?? 0),
  }))
  if (pts.length < 2) return ''
  let d = `M ${pts[0].x} ${pts[0].y}`
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[i]
    const p1 = pts[i + 1]
    const cx = (p0.x + p1.x) / 2
    d += ` C ${cx} ${p0.y}, ${cx} ${p1.y}, ${p1.x} ${p1.y}`
  }
  return d
})

// Build the filled area path (curve + close to zero-line)
const fillPath = computed(() => {
  if (!eq.value) return ''
  const y0 = dbToY(0)
  const pts = BAND_HZ.map((_, i) => ({
    x: bandToX(i),
    y: dbToY(eq.value.bands[i] ?? 0),
  }))
  if (pts.length < 2) return ''
  let d = `M ${pts[0].x} ${y0} L ${pts[0].x} ${pts[0].y}`
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[i]
    const p1 = pts[i + 1]
    const cx = (p0.x + p1.x) / 2
    d += ` C ${cx} ${p0.y}, ${cx} ${p1.y}, ${p1.x} ${p1.y}`
  }
  d += ` L ${pts[pts.length - 1].x} ${y0} Z`
  return d
})

// ── Drag interaction ──────────────────────────────────────────────────────────
const svgRef = ref<SVGSVGElement | null>(null)
const dragging = ref<number | null>(null)  // band index being dragged

// ★ FIX: use getScreenCTM().inverse() to map client → SVG coordinates.
// Manual scale math fails when preserveAspectRatio adds letterbox offsets.
function clientToSvgY(clientY: number): number {
  if (!svgRef.value) return 0
  const ctm = svgRef.value.getScreenCTM()
  if (!ctm) return 0
  const pt = svgRef.value.createSVGPoint()
  pt.y = clientY
  return pt.matrixTransform(ctm.inverse()).y
}

function yToDb(y: number): number {
  const db = 12 - ((y - PAD_T) / INNER_H) * 24
  return Math.max(-12, Math.min(12, db))
}

function onNodeMouseDown(e: MouseEvent, i: number) {
  if (!eq.value?.enabled) return
  e.preventDefault()
  dragging.value = i
}

function onNodeTouchStart(e: TouchEvent, i: number) {
  if (!eq.value?.enabled) return
  e.preventDefault()
  dragging.value = i
}

function onMouseMove(e: MouseEvent) {
  if (dragging.value === null) return
  const y = clientToSvgY(e.clientY)
  const db = Math.round(yToDb(y) * 2) / 2  // snap to 0.5dB steps
  store.setEqBand(props.channel, dragging.value, db)
}

function onTouchMove(e: TouchEvent) {
  if (dragging.value === null) return
  e.preventDefault()
  const touch = e.touches[0]
  const y = clientToSvgY(touch.clientY)
  const db = Math.round(yToDb(y) * 2) / 2
  store.setEqBand(props.channel, dragging.value, db)
}

function onDragEnd() { dragging.value = null }

onMounted(() => {
  window.addEventListener('mousemove', onMouseMove)
  window.addEventListener('mouseup', onDragEnd)
  window.addEventListener('touchmove', onTouchMove, { passive: false })
  window.addEventListener('touchend', onDragEnd)
})
onUnmounted(() => {
  window.removeEventListener('mousemove', onMouseMove)
  window.removeEventListener('mouseup', onDragEnd)
  window.removeEventListener('touchmove', onTouchMove)
  window.removeEventListener('touchend', onDragEnd)
})

// Preamp: separate small fader on the left
const preampY = computed(() => eq.value ? dbToY(eq.value.preamp ?? 0) : dbToY(0))
const draggingPreamp = ref(false)

function onPreampMouseDown(e: MouseEvent) {
  if (!eq.value?.enabled) return
  e.preventDefault()
  draggingPreamp.value = true
}
function onPreampMouseMove(e: MouseEvent) {
  if (!draggingPreamp.value) return
  const y = clientToSvgY(e.clientY)
  const db = Math.round(yToDb(y) * 2) / 2
  store.setPreamp(props.channel, db)
}
function onPreampEnd() { draggingPreamp.value = false }
onMounted(() => {
  window.addEventListener('mousemove', onPreampMouseMove)
  window.addEventListener('mouseup', onPreampEnd)
})
onUnmounted(() => {
  window.removeEventListener('mousemove', onPreampMouseMove)
  window.removeEventListener('mouseup', onPreampEnd)
})
</script>

<template>
  <div class="eq-wrap">
    <!-- Header -->
    <div class="eq-hdr">
      <div class="eq-left">
        <span class="eq-title">Graphic EQ</span>
        <span class="eq-ch" :style="{ color }">{{ channel }}</span>
      </div>
      <div class="eq-right">
        <div class="preset-wrap">
          <select
            class="preset-sel"
            :value="store.activeEqPreset[channel]"
            @change="store.setEqPreset(channel, ($event.target as HTMLSelectElement).value)"
          >
            <option v-for="p in store.EQ_PRESET_NAMES" :key="p" :value="p">{{ p }}</option>
            <option v-if="store.activeEqPreset[channel] === 'Custom'" value="Custom">Custom</option>
          </select>
          <svg class="preset-arrow" viewBox="0 0 10 6" fill="currentColor"><path d="M0 0l5 6 5-6z"/></svg>
        </div>
        <button class="eq-reset" :disabled="!eq?.enabled" @click="store.resetEq(channel)">Flat</button>
        <label class="tog-wrap">
          <input type="checkbox" :checked="eq?.enabled" @change="store.setEqEnabled(channel, ($event.target as HTMLInputElement).checked)" />
          <span class="tog-track"><span class="tog-thumb"></span></span>
          <span class="tog-lbl">{{ eq?.enabled ? 'ON' : 'OFF' }}</span>
        </label>
      </div>
    </div>

    <!-- EQ Graph Canvas -->
    <div class="eq-canvas-wrap" :class="{ 'eq-off': !eq?.enabled }">
      <svg
        ref="svgRef"
        class="eq-svg"
        :viewBox="`0 0 ${W} ${H}`"
        preserveAspectRatio="xMidYMid meet"
      >
        <!-- dB grid lines -->
        <g class="grid-lines">
          <line
            v-for="db in DB_LINES"
            :key="db"
            :x1="PAD_L" :x2="W - PAD_R"
            :y1="dbToY(db)" :y2="dbToY(db)"
            :class="db === 0 ? 'grid-zero' : 'grid-minor'"
          />
        </g>

        <!-- dB axis labels -->
        <g class="db-labels">
          <text
            v-for="db in DB_LINES"
            :key="db"
            :x="PAD_L - 4"
            :y="dbToY(db) + 3"
            class="db-label"
          >{{ db > 0 ? '+' + db : db }}</text>
        </g>

        <!-- Filled area under/over the curve -->
        <path
          :d="fillPath"
          class="eq-fill"
          :fill="color"
          fill-opacity="0.12"
          stroke="none"
        />

        <!-- The EQ curve -->
        <path
          :d="curvePath"
          class="eq-curve"
          :stroke="color"
          stroke-width="2"
          fill="none"
          stroke-linecap="round"
          stroke-linejoin="round"
        />

        <!-- Band nodes -->
        <g v-for="(hz, i) in BAND_HZ" :key="hz">
          <!-- Value label above/below node -->
          <text
            :x="bandToX(i)"
            :y="(eq?.bands[i] ?? 0) >= 0 ? dbToY(eq?.bands[i] ?? 0) - 10 : dbToY(eq?.bands[i] ?? 0) + 16"
            class="band-val-label"
            :fill="(eq?.bands[i] ?? 0) !== 0 ? color : 'var(--text-muted)'"
            text-anchor="middle"
          >{{ (eq?.bands[i] ?? 0) >= 0 ? '+' : '' }}{{ (eq?.bands[i] ?? 0).toFixed(1) }}</text>

          <!-- Node hit area (transparent, larger) -->
          <circle
            :cx="bandToX(i)"
            :cy="dbToY(eq?.bands[i] ?? 0)"
            r="14"
            fill="transparent"
            class="node-hit"
            @mousedown="onNodeMouseDown($event, i)"
            @touchstart.passive="onNodeTouchStart($event, i)"
          />

          <!-- Node visual circle -->
          <circle
            :cx="bandToX(i)"
            :cy="dbToY(eq?.bands[i] ?? 0)"
            r="4"
            class="band-node"
            :fill="color"
            :class="{ 'node-active': dragging === i, 'node-zero': (eq?.bands[i] ?? 0) === 0 }"
          />
        </g>

        <!-- Frequency labels along the bottom -->
        <g class="freq-labels">
          <text
            v-for="(label, i) in BAND_LABEL"
            :key="label"
            :x="bandToX(i)"
            :y="H - 4"
            class="freq-label"
            text-anchor="middle"
          >{{ label }}</text>
        </g>

        <!-- Pre-amp divider -->
        <line
          :x1="PAD_L - 14" :x2="PAD_L - 14"
          :y1="PAD_T" :y2="H - PAD_B"
          class="preamp-div"
        />

        <!-- Pre-amp track -->
        <line
          :x1="PAD_L - 20" :x2="PAD_L - 20"
          :y1="PAD_T" :y2="H - PAD_B"
          class="preamp-track"
        />
        <!-- Pre-amp fill from zero -->
        <line
          :x1="PAD_L - 20" :x2="PAD_L - 20"
          :y1="Math.min(dbToY(0), preampY)"
          :y2="Math.max(dbToY(0), preampY)"
          class="preamp-fill"
          :stroke="color"
          stroke-width="3"
          stroke-linecap="round"
        />
        <!-- Pre-amp node -->
        <circle
          :cx="PAD_L - 20"
          :cy="preampY"
          r="14"
          fill="transparent"
          class="node-hit"
          @mousedown="onPreampMouseDown"
        />
        <circle
          :cx="PAD_L - 20"
          :cy="preampY"
          r="3"
          :fill="color"
          class="band-node"
          :class="{ 'node-active': draggingPreamp }"
        />
        <!-- Pre-amp label -->
        <text
          :x="PAD_L - 20"
          :y="H - 4"
          class="freq-label"
          text-anchor="middle"
        >Pre</text>
        <text
          :x="PAD_L - 20"
          :y="(eq?.preamp ?? 0) >= 0 ? preampY - 8 : preampY + 14"
          class="band-val-label"
          :fill="(eq?.preamp ?? 0) !== 0 ? color : 'var(--text-muted)'"
          text-anchor="middle"
        >{{ (eq?.preamp ?? 0) >= 0 ? '+' : '' }}{{ (eq?.preamp ?? 0).toFixed(1) }}</text>
      </svg>
    </div>
  </div>
</template>

<style scoped>
.eq-wrap {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 18px 20px;
  display: flex;
  flex-direction: column;
  gap: 14px;
  width: 100%;
}

/* ── Header ── */
.eq-hdr  { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 8px; }
.eq-left { display: flex; align-items: center; gap: 8px; }
.eq-title { font-size: 14px; font-weight: 700; }
.eq-ch    { font-size: 12px; font-weight: 600; text-transform: uppercase; letter-spacing: 1px; }
.eq-right { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }

.preset-wrap { position: relative; display: flex; align-items: center; }
.preset-sel {
  appearance: none; padding: 4px 28px 4px 10px;
  border-radius: 6px; border: 1px solid var(--border);
  background: var(--bg-deep); color: var(--text);
  font-size: 11px; font-weight: 600; cursor: pointer; transition: border-color .15s;
}
.preset-sel:hover { border-color: var(--text-muted); }
.preset-sel:focus { outline: none; border-color: var(--accent); }
.preset-arrow { position: absolute; right: 9px; width: 8px; height: 5px; color: var(--text-muted); pointer-events: none; }

.eq-reset {
  font-size: 11px; padding: 4px 12px; border-radius: 5px;
  border: 1px solid var(--border); background: var(--bg-deep);
  color: var(--text-sec); cursor: pointer; transition: all .15s;
}
.eq-reset:hover:not(:disabled) { border-color: var(--text-sec); color: var(--text); }
.eq-reset:disabled { opacity: .35; cursor: not-allowed; }

.tog-wrap  { display: flex; align-items: center; gap: 6px; cursor: pointer; user-select: none; }
.tog-wrap input { display: none; }
.tog-track {
  width: 34px; height: 18px; border-radius: 9px;
  background: var(--bg-deep); border: 1px solid var(--border);
  position: relative; transition: background .2s, border-color .2s; flex-shrink: 0;
}
.tog-wrap input:checked ~ .tog-track { background: var(--accent); border-color: var(--accent); }
.tog-thumb {
  position: absolute; top: 2px; left: 2px;
  width: 12px; height: 12px; border-radius: 50%;
  background: #fff; transition: left .2s; box-shadow: 0 1px 3px rgba(0,0,0,.4);
}
.tog-wrap input:checked ~ .tog-track .tog-thumb { left: 18px; }
.tog-lbl { font-size: 11px; font-weight: 700; color: var(--text-muted); width: 26px; }
.tog-wrap input:checked ~ .tog-lbl { color: var(--accent); }

/* ── Canvas ── */
.eq-canvas-wrap {
  background: var(--bg-deep);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 4px 6px;
  transition: opacity .2s;
  overflow: hidden;
}
.eq-off { opacity: .35; pointer-events: none; }

.eq-svg {
  display: block;
  width: 100%;
  height: auto;
  user-select: none;
}

/* Grid */
.grid-minor { stroke: var(--border); stroke-width: 1; opacity: .5; }
.grid-zero  { stroke: var(--text-muted); stroke-width: 1; opacity: .4; stroke-dasharray: 3 3; }

/* Axis text */
.db-label  { font-size: 8px; fill: var(--text-muted); font-family: inherit; }
.freq-label { font-size: 8px; fill: var(--text-muted); font-family: inherit; }
.band-val-label { font-size: 7.5px; font-family: inherit; font-variant-numeric: tabular-nums; }

/* Curve */
.eq-curve { pointer-events: none; transition: d .08s; }
.eq-fill  { pointer-events: none; }

/* Nodes */
.band-node {
  transition: r .1s, opacity .1s;
  filter: drop-shadow(0 0 3px color-mix(in srgb, currentColor 40%, transparent));
}
.node-zero { opacity: .45; }
.node-active { r: 6; filter: drop-shadow(0 0 6px currentColor); }
.node-hit { cursor: grab; }
.node-hit:active { cursor: grabbing; }

/* Pre-amp */
.preamp-div { stroke: var(--border); stroke-width: 1; opacity: .4; }
.preamp-track { stroke: var(--border); stroke-width: 2; stroke-linecap: round; opacity: .5; }
</style>

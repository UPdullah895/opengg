<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useReplayStore } from '../stores/replay'
import { usePersistenceStore } from '../stores/persistence'
import { useToast } from '../composables/useToast'
import SelectField from './SelectField.vue'

const replay  = useReplayStore()
const persist = usePersistenceStore()
const settings = computed(() => persist.state.settings)
const toast   = useToast()

const open = ref(false)
const root = ref<HTMLElement | null>(null)

// ── Live timer ──
const elapsed = ref(0)  // seconds since GSR was started / replay began
let timerHandle: ReturnType<typeof setInterval> | null = null

function startTimer() {
  elapsed.value = 0
  timerHandle = setInterval(() => { elapsed.value++ }, 1000)
}
function stopTimer() {
  if (timerHandle) { clearInterval(timerHandle); timerHandle = null }
  elapsed.value = 0
}
function fmtElapsed(s: number) {
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const sec = s % 60
  if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}`
  return `${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}`
}

// ── Status label ──
const isGsr      = computed(() => settings.value.gsrEnabled)
const isRunning  = computed(() => replay.status !== 'idle')
const isRecording = computed(() => replay.status === 'recording')

const statusLabel = computed(() => {
  if (!isRunning.value) return 'Idle'
  if (isGsr.value) return `Replay Buffer · ${fmtElapsed(elapsed.value)}`
  if (isRecording.value) return `Recording · ${fmtElapsed(elapsed.value)}`
  return `Replay · ${replay.replayDuration}s`
})

// ── Quick-settings options ──
const gsrQualityOptions = [
  { value: 'cbr',       label: 'Constant bitrate' },
  { value: 'medium',    label: 'Medium'    },
  { value: 'high',      label: 'High'      },
  { value: 'very_high', label: 'Very high' },
  { value: 'ultra',     label: 'Ultra'     },
]
const gsrFpsOptions = [30, 60, 120].map(v => ({ value: v, label: `${v} FPS` }))
const gsrReplayOptions = [
  { value: 15,  label: '15s'  },
  { value: 30,  label: '30s'  },
  { value: 60,  label: '60s'  },
  { value: 90,  label: '90s'  },
  { value: 120, label: '120s' },
]
const targetOptions = ref<Array<{ value: string; label: string }>>([
  { value: 'screen', label: 'Primary Monitor' },
])

// ── GSR invoke helper ──
function gsrParams() {
  return {
    outputDir:     settings.value.clip_directories?.[0] ?? '~/Videos/OpenGG',
    replaySecs:    settings.value.gsrReplaySecs,
    fps:           settings.value.gsrFps,
    quality:       settings.value.gsrQuality,
    bitrateKbps:   settings.value.gsrQuality === 'cbr' ? settings.value.gsrCbrBitrate : null,
    monitorTarget: settings.value.gsrMonitorTarget || 'screen',
    audioSources:  settings.value.captureTracks.map((t: { source: string }) => t.source),
  }
}

async function toggleRecording() {
  try {
    if (isGsr.value) {
      if (isRunning.value) {
        await invoke('stop_gsr_replay')
        replay.status = 'idle'
        stopTimer()
      } else {
        await invoke('start_gsr_replay', gsrParams())
        replay.status = 'replay'
        startTimer()
      }
    } else {
      if (isRunning.value) {
        await replay.stopRecorder()
        stopTimer()
      } else {
        await replay.startReplay(settings.value.gsrReplaySecs)
        startTimer()
      }
    }
  } catch (e) {
    toast.error(`Recording failed: ${e}`)
  }
}

async function saveClip() {
  await replay.saveReplay()
}

async function restartGsr() {
  if (!isRunning.value) return
  try { await invoke('restart_gsr_replay', gsrParams()) } catch {}
}

// ── Close on outside click ──
function onDocClick(e: MouseEvent) {
  if (root.value && !root.value.contains(e.target as Node)) open.value = false
}
onMounted(async () => {
  document.addEventListener('mousedown', onDocClick)
  // Hydrate recording status from backend so the pill shows the correct state after route change
  try { await replay.fetchStatus() } catch { /* daemon may not be running */ }
  // If status came back as running, start the elapsed timer
  if (isRunning.value) startTimer()
  // Populate monitor target dropdown from Tauri's native monitor API
  try {
    const sessionType = await invoke<string>('get_session_type')
    const monitors = await invoke<Array<{ name: string; label: string }>>('list_monitors')
    const opts = monitors.map(m => ({ value: m.name, label: m.label }))
    if (sessionType !== 'wayland') {
      opts.push({ value: 'focused', label: 'Fullscreen Application' })
    }
    targetOptions.value = opts
  } catch { /* keep default */ }
})
onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onDocClick)
  stopTimer()
})
</script>

<template>
  <div class="rec-dd" ref="root">
    <!-- Trigger pill -->
    <button
      class="rec-pill"
      :class="{ active: isRunning, recording: isRecording }"
      @click="open = !open"
    >
      <span class="rec-dot" :class="{ on: isRunning, recording: isRecording }"></span>
      <span class="rec-pill-label">{{ statusLabel }}</span>
      <svg class="rec-chevron" :class="{ flipped: open }" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" width="12" height="12"><polyline points="6 9 12 15 18 9"/></svg>
    </button>

    <!-- Dropdown panel -->
    <div v-if="open" class="rec-panel">
      <!-- Status row -->
      <div class="rec-status-row">
        <span class="rec-status-dot" :class="{ on: isRunning, recording: isRecording }"></span>
        <span class="rec-status-label">{{ isRunning ? (isGsr ? 'GSR Replay Buffer Running' : (isRecording ? 'Recording' : 'Replay Buffer')) : 'Stopped' }}</span>
      </div>

      <!-- Action buttons -->
      <div class="rec-actions">
        <button class="rec-btn" :class="{ danger: isRunning }" @click="toggleRecording">
          {{ isRunning ? 'Stop' : (isGsr ? 'Start Replay Buffer' : 'Start Replay') }}
        </button>
        <button
          class="rec-btn rec-btn-save"
          :disabled="!isRunning"
          @click="saveClip"
          title="Save current replay buffer as a clip"
        >
          Save Clip
        </button>
      </div>

      <!-- Quick settings (only relevant for GSR) -->
      <div v-if="isGsr" class="rec-qs">
        <div class="rec-qs-title">Quick Settings</div>
        <div class="rec-qs-row">
          <label>Quality</label>
          <SelectField
            v-model="settings.gsrQuality"
            :options="gsrQualityOptions"
            @update:modelValue="restartGsr"
          />
        </div>
        <div class="rec-qs-row">
          <label>FPS</label>
          <SelectField
            v-model="settings.gsrFps"
            :options="gsrFpsOptions"
            @update:modelValue="restartGsr"
          />
        </div>
        <div class="rec-qs-row">
          <label>Buffer</label>
          <SelectField
            v-model="settings.gsrReplaySecs"
            :options="gsrReplayOptions"
            @update:modelValue="restartGsr"
          />
        </div>
        <div class="rec-qs-row">
          <label>Target</label>
          <SelectField
            v-model="settings.gsrMonitorTarget"
            :options="targetOptions"
            @update:modelValue="restartGsr"
          />
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.rec-dd { position: relative; }

/* ── Pill trigger ── */
.rec-pill {
  display: flex; align-items: center; gap: 6px;
  padding: 7px 10px; border-radius: 6px;
  border: 1px solid var(--border); background: var(--bg-card);
  color: var(--text-sec); font-size: 13px; cursor: pointer;
  transition: border-color .15s, background .15s;
}
.rec-pill:hover { background: var(--bg-hover); }
.rec-pill.active { border-color: var(--danger); }
.rec-pill-label { white-space: nowrap; }

.rec-dot {
  width: 7px; height: 7px; border-radius: 50%;
  background: var(--text-muted); flex-shrink: 0;
  transition: background .2s;
}
.rec-dot.on { background: var(--danger); animation: pulse 1.2s infinite; }

.rec-chevron { color: var(--text-muted); transition: transform .15s; flex-shrink: 0; }
.rec-chevron.flipped { transform: rotate(180deg); }

@keyframes pulse { 0%,100%{opacity:1} 50%{opacity:.4} }

/* ── Panel ── */
.rec-panel {
  position: absolute; top: calc(100% + 6px); left: 0;
  width: 240px; background: var(--bg-card);
  border: 1px solid var(--border); border-radius: 10px;
  box-shadow: 0 8px 24px rgba(0,0,0,.35);
  z-index: 9999; padding: 12px;
  display: flex; flex-direction: column; gap: 10px;
}

/* ── Status row ── */
.rec-status-row { display: flex; align-items: center; gap: 8px; }
.rec-status-dot {
  width: 8px; height: 8px; border-radius: 50%;
  background: var(--text-muted); flex-shrink: 0;
}
.rec-status-dot.on { background: var(--danger); animation: pulse 1.2s infinite; }
.rec-status-label { font-size: 12px; color: var(--text-sec); font-weight: 600; }

/* ── Buttons ── */
.rec-actions { display: flex; gap: 6px; }
.rec-btn {
  flex: 1; padding: 6px 0; border-radius: 6px; font-size: 12px; font-weight: 600;
  border: 1px solid var(--border); background: var(--bg-input);
  color: var(--text); cursor: pointer; transition: background .15s;
}
.rec-btn:hover { background: var(--bg-hover); }
.rec-btn.danger { border-color: var(--danger); color: var(--danger); }
.rec-btn.danger:hover { background: color-mix(in srgb, var(--danger) 12%, transparent); }
.rec-btn-save { background: color-mix(in srgb, var(--accent) 15%, transparent); border-color: var(--accent); color: var(--accent); }
.rec-btn-save:hover:not(:disabled) { background: color-mix(in srgb, var(--accent) 25%, transparent); }
.rec-btn-save:disabled { opacity: .4; cursor: not-allowed; }

/* ── Quick settings ── */
.rec-qs { display: flex; flex-direction: column; gap: 7px; border-top: 1px solid var(--border); padding-top: 10px; }
.rec-qs-title { font-size: 10px; font-weight: 700; letter-spacing: .5px; text-transform: uppercase; color: var(--text-muted); }
.rec-qs-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.rec-qs-row label { font-size: 12px; color: var(--text-sec); min-width: 44px; flex-shrink: 0; }
.rec-qs-row :deep(.sf-root) { flex: 1; min-width: 0; }
.rec-qs-input {
  flex: 1; padding: 4px 7px; border-radius: 5px;
  border: 1px solid var(--border); background: var(--bg-input);
  color: var(--text); font-size: 12px;
}
.rec-qs-input:focus { outline: none; border-color: var(--accent); }
</style>

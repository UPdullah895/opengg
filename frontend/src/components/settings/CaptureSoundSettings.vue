<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { usePersistenceStore } from '../../stores/persistence'
import { useToast } from '../../composables/useToast'
import { missing } from '../../composables/useDependencyStatus'
import SelectField from '../SelectField.vue'
import InfoIcon from '../InfoIcon.vue'
import ToggleSwitch from '../ToggleSwitch.vue'
import './settings-shared.css'

const { t } = useI18n()
const toast = useToast()
const persist = usePersistenceStore()

const settings = computed(() => persist.state.settings)

// ─── GPU Screen Recorder ───
const gsrQualityOptions = computed(() => [
  { value: 'cbr',       label: t('settings.captureGsr.qualityCbr')      },
  { value: 'medium',    label: t('settings.captureGsr.qualityMedium')   },
  { value: 'high',      label: t('settings.captureGsr.qualityHigh')     },
  { value: 'very_high', label: t('settings.captureGsr.qualityVeryHigh') },
  { value: 'ultra',     label: t('settings.captureGsr.qualityUltra')    },
])
const gsrFpsOptions = computed(() => [30, 60, 120].map(v => ({ value: v, label: t(`dashboard.gsrFps.${v}`) })))
const gsrReplayPresets = computed(() => [
  { value: '15',     label: t('dashboard.gsrReplay.15')       },
  { value: '30',     label: t('dashboard.gsrReplay.30')       },
  { value: '60',     label: t('dashboard.gsrReplay.60')       },
  { value: '90',     label: t('dashboard.gsrReplay.90')       },
  { value: '120',    label: t('dashboard.gsrReplay.120')      },
  { value: 'custom', label: t('settings.captureGsr.replayCustom') },
])

function onReplayPresetChange(preset: string | number) {
  settings.value.gsrReplayPreset = String(preset) as any
  if (preset !== 'custom') {
    settings.value.gsrReplaySecs = Number(preset)
    restartGsr()
  }
}

function onCustomReplaySecs(e: Event) {
  const raw = Number((e.target as HTMLInputElement).value)
  settings.value.gsrReplaySecs = Math.max(5, Math.min(600, raw || 30))
  restartGsr()
}

const gsrAudioSources = computed(() =>
  settings.value.captureTracks.map((t: { source: string }) => t.source)
)

function gsrInvokeParams() {
  const outputDir = settings.value.clip_directories?.[0] ?? '~/Videos/OpenGG'
  return {
    outputDir,
    replaySecs:    settings.value.gsrReplaySecs,
    fps:           settings.value.gsrFps,
    quality:       settings.value.gsrQuality,
    bitrateKbps:   settings.value.gsrQuality === 'cbr' ? settings.value.gsrCbrBitrate : null,
    monitorTarget: settings.value.gsrMonitorTarget || 'screen',
    audioSources:  gsrAudioSources.value,
  }
}

// Note: toggleGsr moved to ExtensionsSettings for GSR enable/disable

async function restartGsr() {
  if (!settings.value.gsrEnabled) return
  try {
    await invoke('restart_gsr_replay', gsrInvokeParams())
    toast.success('Recording restarted with new settings')
  } catch (e) {
    console.error('GSR restart:', e)
    toast.error(`Failed to restart recording: ${e}`)
  }
}

// ─── GSR Diagnostics ───
interface DiagnosticFix {
  command: string
  description: string
}
interface DiagnosticItem {
  message: string
  severity: 'error' | 'warning'
  fix?: DiagnosticFix
}
interface GsrDiagnosticResult {
  ok: boolean
  gsr_installed: boolean
  gsr_version: string | null
  in_render_group: boolean
  in_video_group: boolean
  gpu_encoder_available: boolean
  audio_sources_ok: boolean
  missing_audio_sources: string[]
  items: DiagnosticItem[]
  report: string
}
const diagLoading = ref(false)
const diagResult = ref<GsrDiagnosticResult | null>(null)
const copiedCommand = ref<string | null>(null)
let copiedTimer: ReturnType<typeof setTimeout> | null = null

async function runDiagnostics() {
  diagLoading.value = true
  try {
    const res = await invoke<GsrDiagnosticResult>('gsr_diagnostics', {
      audioSources: gsrAudioSources.value,
      monitorTarget: settings.value.gsrMonitorTarget || 'screen',
    })
    diagResult.value = res
  } catch (e) {
    toast.error(`Diagnostics failed: ${e}`)
  } finally {
    diagLoading.value = false
  }
}

async function doCopy(text: string) {
  if (!text) return
  try {
    await invoke('write_clipboard', { text })
  } catch (e2) {
    toast.error(`Copy failed: ${e2}`)
    return
  }
  copiedCommand.value = text
  if (copiedTimer) clearTimeout(copiedTimer)
  copiedTimer = setTimeout(() => { copiedCommand.value = null }, 1500)
  toast.success(t('settings.captureGsr.copied'))
}

// ─── Resource estimation ───
const gsrEstFileMb = computed(() => {
  const kbps = settings.value.gsrQuality === 'cbr'
    ? (settings.value.gsrCbrBitrate ?? 8000)
    : ({ medium: 4000, high: 6000, very_high: 12000, ultra: 20000 } as Record<string, number>)[settings.value.gsrQuality] ?? 8000
  return ((kbps * settings.value.gsrReplaySecs) / 8 / 1024).toFixed(0)
})
const gsrEstRamMb = computed(() => {
  return Math.ceil(Number(gsrEstFileMb.value) * 1.2)
})

watch(
  () => settings.value.captureTracks.map((t: { source: string }) => t.source).join(','),
  () => restartGsr(),
)

// ─── Capture tracks ───
const CAPTURE_SOURCES = ['Game', 'Chat', 'Media', 'Aux', 'Mic']
// Live, curated capture sources (OpenGG channel monitors + real hardware inputs /
// output monitors), populated from the backend in onMounted. The fallback uses the
// real OpenGG channel monitor node names so values stay valid before enumeration runs.
const captureSourceOptions = ref<Array<{ value: string; label: string }>>(
  CAPTURE_SOURCES.map(s => ({ value: `OpenGG_${s}.monitor`, label: s })),
)

const audioSinkOptions = ref<Array<{ value: string; label: string }>>([])
const sessionType = ref<'x11' | 'wayland' | 'unknown'>('unknown')
const monitorOptions = ref<Array<{ value: string; label: string }>>([
  { value: 'screen', label: 'Primary Monitor' },
])
const isWayland = computed(() => sessionType.value === 'wayland')

onMounted(async () => {
  try {
    const sinks = await invoke<string[]>('list_audio_sinks')
    audioSinkOptions.value = sinks.map(name => ({ value: name, label: name }))
  } catch {
    audioSinkOptions.value = CAPTURE_SOURCES.map(s => ({ value: `OpenGG_${s}`, label: `OpenGG_${s}` }))
  }
  try {
    const sources = await invoke<Array<{ value: string; label: string }>>('list_capture_sources')
    if (sources.length) captureSourceOptions.value = sources
  } catch { /* keep fallback */ }
  try {
    sessionType.value = (await invoke<string>('get_session_type')) as typeof sessionType.value
  } catch { /* ignore */ }
  try {
    const monitors = await invoke<Array<{ name: string; label: string }>>('list_monitors')
    const opts = monitors.map(m => ({ value: m.name, label: m.label }))
    if (isWayland.value) {
      // Wayland: relabel "screen" to "portal" for UI clarity.
      // ★ Connector names (DP-1, HDMI-A-1, etc.) remain selectable for direct KMS capture.
      // The label change does NOT affect the persisted value — "screen" stays "screen".
      // Mapping to "portal" happens only for X11-only values (screen/focused/empty) at spawn time.
      opts.forEach((opt) => {
        if (opt.value === 'screen') {
          opt.label = t('settings.captureGsr.portalOption')
          opt.value = 'screen' // persisted value; GSR mapping happens at command spawn time
        }
      })
    } else {
      // X11: add "Fullscreen Application" option
      opts.push({ value: 'focused', label: 'Fullscreen Application' })
    }
    monitorOptions.value = opts
  } catch { /* keep default */ }
})

function addCaptureTrack() {
  const n = settings.value.captureTracks.length + 1
  const def = captureSourceOptions.value[0]?.value ?? 'OpenGG_Game.monitor'
  settings.value.captureTracks.push({ name: `Track ${n}`, source: def })
}

// ── Drag-to-reorder track rows (drag index held in a ref, NOT dataTransfer —
// dataTransfer.getData returns '' on WebKitGTK per CLAUDE.md) ──
const dragFrom = ref<number | null>(null)
const dragOver = ref<number | null>(null)
function onTrackDragStart(i: number, e: DragEvent) {
  dragFrom.value = i
  if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move'
}
function onTrackDragOver(i: number) { dragOver.value = i }
function onTrackDrop(i: number) {
  const from = dragFrom.value
  dragFrom.value = null
  dragOver.value = null
  if (from === null || from === i) return
  const arr = settings.value.captureTracks
  const [moved] = arr.splice(from, 1)
  arr.splice(i, 0, moved)
}
function onTrackDragEnd() { dragFrom.value = null; dragOver.value = null }

function removeCaptureTrack(i: number) {
  settings.value.captureTracks.splice(i, 1)
}

defineEmits<{ navigate: [page: string] }>()
</script>

<template>
  <section class="settings-section">
    <h2 class="sec-title">{{ t('settings.captureSound.title') }}</h2>

    <!-- GPU Screen Recorder panel (top) -->
    <div class="card">
      <!-- Missing gpu-screen-recorder warning -->
      <div v-if="missing('recording')" class="dep-warn dep-warn-recording">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:16px;height:16px;flex-shrink:0">
          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"/>
        </svg>
        <span>{{ t('settings.deps.missingGsr') }}</span>
      </div>

      <div class="card-head gsr-head">
        <span>{{ t('settings.captureGsr.title') }}</span>
        <span class="badge-beta">Beta</span>
        <InfoIcon :title="t('settings.captureGsr.hint')" />
        <span v-if="settings.gsrEnabled" class="gsr-est">
          Est. RAM: ~{{ gsrEstRamMb }} MB &nbsp;|&nbsp; File: ~{{ gsrEstFileMb }} MB
        </span>
      </div>
      <div v-if="settings.gsrEnabled" class="form-grid gsr-grid">
        <div class="field">
          <label>{{ t('settings.captureGsr.quality') }}</label>
          <SelectField v-model="settings.gsrQuality" :options="gsrQualityOptions" @update:modelValue="restartGsr" />
          <input
            v-if="settings.gsrQuality === 'cbr'"
            type="number"
            class="gsr-custom-secs"
            :value="settings.gsrCbrBitrate"
            min="500"
            max="100000"
            step="500"
            placeholder="Bitrate (kbps)"
            title="Target bitrate in kbps (e.g. 8000 = 8 Mbps)"
            @change="e => { settings.gsrCbrBitrate = Math.max(500, Math.min(100000, Number((e.target as HTMLInputElement).value) || 8000)); restartGsr() }"
          />
        </div>
        <div class="field">
          <label>{{ t('settings.captureGsr.fps') }}</label>
          <SelectField v-model="settings.gsrFps" :options="gsrFpsOptions" @update:modelValue="restartGsr" />
        </div>
        <div class="field">
          <label>{{ t('settings.captureGsr.replayBuffer') }}</label>
          <SelectField
            :modelValue="settings.gsrReplayPreset"
            :options="gsrReplayPresets"
            @update:modelValue="onReplayPresetChange"
          />
          <input
            v-if="settings.gsrReplayPreset === 'custom'"
            type="number"
            class="gsr-custom-secs"
            :value="settings.gsrReplaySecs"
            min="5"
            max="600"
            placeholder="Seconds"
            @change="onCustomReplaySecs"
          />
        </div>
        <div class="field">
          <label>{{ t('settings.captureGsr.monitorTarget') }}</label>
          <SelectField
            v-model="settings.gsrMonitorTarget"
            :options="monitorOptions"
            @update:modelValue="restartGsr"
          />
          <!-- Only warn about the X11-only "Fullscreen Application" mode when it is actually
               offered in the dropdown. On Wayland that option is never added, so this stays
               hidden instead of describing an unavailable choice. -->
          <span v-if="isWayland && monitorOptions.some(o => o.value === 'focused')" class="hint" style="color:var(--warn,#f59e0b);margin-top:4px;font-size:11px">
            {{ t('settings.captureGsr.waylandHint') }}
          </span>
        </div>
      </div>
      <div v-if="settings.gsrEnabled" class="gsr-toggle-row">
        <span class="gsr-label">{{ t('settings.captureGsr.autoStart') }}
          <InfoIcon :title="t('settings.captureGsr.autoStartTooltip')" />
        </span>
        <ToggleSwitch v-model="settings.gsrAutoStart" />
      </div>
      <div v-if="settings.gsrEnabled" class="gsr-toggle-row">
        <span class="gsr-label">{{ t('settings.captureGsr.autoRestart') }}
          <InfoIcon :title="t('settings.captureGsr.autoRestartTooltip')" />
        </span>
        <ToggleSwitch v-model="settings.gsrAutoRestart" />
      </div>

      <!-- ★ GSR Diagnostics -->
      <div v-if="settings.gsrEnabled" class="gsr-diagnostics" style="margin-top:12px">
        <button class="btn btn-sm" :disabled="diagLoading" @click="runDiagnostics">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:14px;height:14px"><path d="M22 11.08V12a10 10 0 11-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>
          {{ diagLoading ? 'Running…' : t('settings.captureGsr.runDiagnostics') }}
        </button>
        <div v-if="diagResult" class="diag-results" :class="{ 'diag-ok': diagResult.ok, 'diag-fail': !diagResult.ok }">
          <div class="diag-summary">{{ diagResult.ok ? t('settings.captureGsr.diagnosticsOk') : t('settings.captureGsr.diagnosticsFail') }}</div>
          <div v-for="(item, i) in diagResult.items" :key="i" class="diag-item" :class="`diag-sev-${item.severity}`">
            <div class="diag-msg">{{ item.severity === 'error' ? '✗' : '⚠' }} {{ item.message }}</div>
            <div v-if="item.fix" class="diag-fix">
              <div class="diag-fix-desc">{{ item.fix.description }}</div>
              <div v-if="item.fix.command" class="diag-fix-row">
                <code class="diag-fix-cmd">{{ item.fix.command }}</code>
                <button
                  class="copy-btn"
                  :class="{ copied: copiedCommand === item.fix.command }"
                  :title="t('settings.captureGsr.copyTooltip')"
                  @click="doCopy(item.fix.command)"
                >
                  <svg v-if="copiedCommand !== item.fix.command" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:13px;height:13px"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
                  <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:13px;height:13px"><polyline points="20 6 9 17 4 12"/></svg>
                </button>
              </div>
            </div>
          </div>
          <button
            v-if="diagResult.report"
            class="btn btn-sm diag-copy-btn"
            :class="{ copied: copiedCommand === diagResult.report }"
            style="margin-top:10px"
            @click="doCopy(diagResult.report)"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:13px;height:13px"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
            {{ copiedCommand === diagResult.report ? t('settings.captureGsr.diagnosticsCopied') : t('settings.captureGsr.copyDiagnostics') }}
          </button>
        </div>
      </div>

      <div v-else class="hint" style="margin-top:8px">{{ t('settings.captureGsr.extensionsHint') }}</div>
    </div>

    <!-- OBS-style Audio Capture Devices -->
    <div class="card">
      <!-- Missing ffmpeg warning -->
      <div v-if="missing('export')" class="dep-warn dep-warn-export">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:16px;height:16px;flex-shrink:0">
          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"/>
        </svg>
        <span>{{ t('settings.deps.missingFfmpeg') }}</span>
      </div>

      <div class="card-head">{{ t('settings.captureSound.captureDevices') }} <InfoIcon :title="t('settings.captureSound.captureHint')" /></div>
      <div class="capture-tracks">
        <div
          v-for="(track, i) in settings.captureTracks"
          :key="i"
          class="capture-row"
          :class="{ 'drag-over': dragOver === i && dragFrom !== null && dragFrom !== i }"
          @dragover.prevent="onTrackDragOver(i)"
          @drop.prevent="onTrackDrop(i)"
        >
          <button
            class="capture-grip"
            draggable="true"
            :title="t('settings.captureSound.reorderTrack')"
            @dragstart="onTrackDragStart(i, $event)"
            @dragend="onTrackDragEnd"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="8" y1="7" x2="16" y2="7"/><line x1="8" y1="12" x2="16" y2="12"/><line x1="8" y1="17" x2="16" y2="17"/>
            </svg>
          </button>
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
</template>

<style scoped>
.dep-warn {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  margin-bottom: 12px;
  background-color: color-mix(in srgb, var(--danger, #ef4444) 10%, var(--bg-card));
  border-left: 3px solid var(--danger, #ef4444);
  border-radius: 4px;
  font-size: 13px;
  color: var(--text);
  line-height: 1.4;
}

.dep-warn svg {
  color: var(--danger, #ef4444);
}
</style>

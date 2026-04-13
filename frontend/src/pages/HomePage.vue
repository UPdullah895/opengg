<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { usePersistenceStore } from '../stores/persistence'
import { useReplayStore } from '../stores/replay'
import { useAudioStore } from '../stores/audio'
import { getMediaPort, mediaUrl } from '../utils/assets'
import SelectField from '../components/SelectField.vue'
import { useToast } from '../composables/useToast'
import { settingsTargetTab } from '../composables/useNavSignal'
import { viewMode } from '../composables/useViewMode'

const emit = defineEmits<{ navigate: [page: string] }>()
const { t, tm } = useI18n()
const persist = usePersistenceStore()
const replay  = useReplayStore()
const audio   = useAudioStore()
const toast   = useToast()

const recorderStatus = computed(() => replay.status)
const clipCount = ref(0)
const gsrWarning = ref(false)
const mediaPort = ref(0)
const appVersion = ref('')

// ── Active popover ──
type CardKey = 'clips' | 'recorder' | 'mixer' | 'devices'
const activeCard = ref<CardKey | null>(null)
const rootEl = ref<HTMLElement | null>(null)

function toggleCard(key: CardKey) {
  if (activeCard.value === key) { activeCard.value = null; return }
  activeCard.value = key
  if (key === 'clips') loadRecentClips()
  if (key === 'mixer') { audio.fetchChannels(); audio.fetchApps() }
}

function closePopovers() { activeCard.value = null }

function onDocClick(e: MouseEvent) {
  if (rootEl.value && !rootEl.value.contains(e.target as Node)) closePopovers()
}

// ── Recent clips (max 3 — 4th slot in grid is the "More Clips" button-card) ──
const recentClips = computed(() =>
  [...replay.clips].filter(c => !c.isSkeleton)
    .sort((a, b) => b.created.localeCompare(a.created))
    .slice(0, 3)
)

// ── Recorder status label (translatable) ──
const recorderStatusLabel = computed(() => {
  const s = recorderStatus.value
  if (s === 'idle') return t('dashboard.recorderStatus.idle')
  if (s.startsWith('replay')) return t('dashboard.recorderStatus.replay')
  if (s === 'recording') return t('dashboard.recorderStatus.recording')
  return s
})

async function loadRecentClips() {
  const dirs = persist.state.settings.clip_directories?.length
    ? persist.state.settings.clip_directories : ['']
  await replay.fetchClips(dirs[0], true)
}

// ── Recorder quick settings ──
const settings = computed(() => persist.state.settings)
const isRunning = computed(() => replay.status !== 'idle')

const gsrQualityOptions = computed(() => [
  { value: 'cbr',       label: t('dashboard.gsrQuality.cbr')      },
  { value: 'medium',    label: t('dashboard.gsrQuality.medium')    },
  { value: 'high',      label: t('dashboard.gsrQuality.high')      },
  { value: 'very_high', label: t('dashboard.gsrQuality.very_high') },
  { value: 'ultra',     label: t('dashboard.gsrQuality.ultra')     },
])
const gsrFpsOptions = computed(() =>
  [30, 60, 120].map(v => ({ value: v, label: t(`dashboard.gsrFps.${v}`) }))
)
const gsrReplayOptions = computed(() =>
  [15, 30, 60, 90, 120].map(v => ({ value: v, label: t(`dashboard.gsrReplay.${v}`) }))
)
const targetOptions = computed(() => [
  { value: 'screen',  label: t('dashboard.gsrTarget.screen')  },
  { value: 'focused', label: t('dashboard.gsrTarget.focused') },
])

function gsrParams() {
  return {
    outputDir:     settings.value.clip_directories?.[0] ?? '~/Videos/OpenGG',
    replaySecs:    settings.value.gsrReplaySecs,
    fps:           settings.value.gsrFps,
    quality:       settings.value.gsrQuality,
    bitrateKbps:   settings.value.gsrQuality === 'cbr' ? settings.value.gsrCbrBitrate : null,
    monitorTarget: settings.value.gsrMonitorTarget || 'screen',
    audioSources:  settings.value.captureTracks?.map((t: { source: string }) => t.source) ?? [],
  }
}

async function toggleRecording() {
  try {
    const isGsr = settings.value.gsrEnabled
    if (isGsr) {
      if (isRunning.value) {
        await invoke('stop_gsr_replay')
        replay.status = 'idle'
      } else {
        await invoke('start_gsr_replay', gsrParams())
        replay.status = 'replay'
        toast.success('Recording started')
      }
    } else {
      if (isRunning.value) {
        await replay.stopRecorder()
      } else {
        await replay.startReplay(settings.value.gsrReplaySecs)
        toast.success('Recording started')
      }
    }
  } catch (e) {
    toast.error(`Recording failed: ${e}`)
  }
}

// ── Mixer quick strips ──
const mixerChannels = ['Game', 'Chat', 'Media', 'Aux', 'Mic']
const draggingVolumes = ref<Record<string, number | null>>({})
function onStripInput(ch: string, e: Event) {
  const val = Number((e.target as HTMLInputElement).value)
  draggingVolumes.value[ch] = val
  audio.setVolume(ch, val)
}
function onStripChange(ch: string) {
  draggingVolumes.value[ch] = null
}

// ── Resource estimation (mirrors SettingsPage formula exactly) ──
const gsrEstFileMb = computed(() => {
  const kbps = settings.value.gsrQuality === 'cbr'
    ? (settings.value.gsrCbrBitrate ?? 8000)
    : ({ medium: 4000, high: 6000, very_high: 12000, ultra: 20000 } as Record<string, number>)[settings.value.gsrQuality] ?? 8000
  return ((kbps * settings.value.gsrReplaySecs) / 8 / 1024).toFixed(0)
})
const gsrEstRamMb = computed(() => Math.ceil(Number(gsrEstFileMb.value) * 1.2))

// ── Clip click: navigate to clips page and auto-open preview ──
function openClipPreview(clip: { id: string }) {
  replay.previewTargetClipId = clip.id
  emit('navigate', 'clips')
  closePopovers()
}

// ── Clips context menu (navigate-first: set state then navigate so ClipsPage shows it on mount) ──
function openClipContextMenu(clip: { id: string }, e: MouseEvent) {
  e.preventDefault()
  replay.activeMenuClipId = clip.id
  replay.activeMenuPos = { x: e.clientX, y: e.clientY }
  emit('navigate', 'clips')
  closePopovers()
}

onMounted(async () => {
  document.addEventListener('mousedown', onDocClick)
  if (!persist.loaded) await persist.load()
  mediaPort.value = await getMediaPort()
  try { appVersion.value = await getVersion() } catch { appVersion.value = '0.1.1' }
  try { await replay.fetchStatus() } catch { /* daemon may not be running */ }
  try {
    const dirs = persist.state.settings.clip_directories?.length
      ? persist.state.settings.clip_directories : ['']
    const counts = await Promise.all(
      dirs.map(d => invoke<number>('get_clips_count', { folder: d }).catch(() => 0))
    )
    clipCount.value = counts.reduce((a, b) => a + b, 0)
  } catch { /* clips folder may not exist yet */ }
  try {
    const installed = await invoke<boolean>('check_gsr_installed')
    gsrWarning.value = !installed
  } catch { gsrWarning.value = false }
})

onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onDocClick)
})

const showAllUpdates = ref(false)
interface ChangelogEntry { version: string; date: string; items: string[] }
const updates = computed(() => tm('dashboard.changelog') as ChangelogEntry[])
</script>

<template>
  <div class="home" ref="rootEl">
    <h1 class="page-title">{{ t('dashboard.title') }}</h1>

    <!-- GSR warning banner -->
    <div v-if="gsrWarning" class="gsr-warn">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="gsr-warn-ic"><path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
      <span>{{ t('dashboard.gsrWarning') }}</span>
      <button class="gsr-warn-dismiss" @click="gsrWarning=false">{{ t('dashboard.gsrDismiss') }}</button>
    </div>

    <div class="grid">
      <!-- AUDIO MIXER card -->
      <div class="card" :class="{ 'card-active': activeCard === 'mixer' }" @click="toggleCard('mixer')">
        <div class="card-head">
          <span class="card-label">{{ t('dashboard.audioMixer') }}</span>
          <div class="card-icon accent">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="4" y1="21" x2="4" y2="14"/><line x1="4" y1="10" x2="4" y2="3"/><line x1="12" y1="21" x2="12" y2="12"/><line x1="12" y1="8" x2="12" y2="3"/><line x1="20" y1="21" x2="20" y2="16"/><line x1="20" y1="12" x2="20" y2="3"/></svg>
          </div>
        </div>
        <div class="card-val">{{ t('dashboard.channelSummary') }}</div>
        <div class="card-sub">{{ t('dashboard.channelList') }}</div>

        <!-- Mixer popover -->
        <div v-if="activeCard === 'mixer'" class="popover" @click.stop>
          <div class="pop-title">{{ t('dashboard.quickMixer') }}</div>
          <div class="mixer-strips">
            <div v-for="ch in mixerChannels" :key="ch" class="mixer-strip">
              <div class="strip-label">{{ ch }}</div>
              <div class="strip-slider-wrap">
                <span class="strip-vol" :class="{ visible: draggingVolumes[ch] !== null && draggingVolumes[ch] !== undefined }">
                  {{ draggingVolumes[ch] ?? (audio.channelVolumes[ch] ?? 100) }}%
                </span>
                <input
                  type="range" min="0" max="100" step="1"
                  :value="audio.channelVolumes[ch] ?? 100"
                  class="strip-slider"
                  @input="onStripInput(ch, $event)"
                  @change="onStripChange(ch)"
                  @click.stop
                />
              </div>
              <button
                class="strip-mute"
                :class="{ muted: audio.channelMutes[ch] }"
                @click.stop="audio.setMute(ch, !audio.channelMutes[ch])"
                :title="audio.channelMutes[ch] ? t('dashboard.mute.unmute') : t('dashboard.mute.mute')"
              >
                <svg v-if="!audio.channelMutes[ch]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M19.07 4.93a10 10 0 010 14.14M15.54 8.46a5 5 0 010 7.07"/></svg>
                <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>
              </button>
            </div>
          </div>
          <button class="pop-nav-btn" @click.stop="emit('navigate', 'mixer'); closePopovers()">{{ t('dashboard.openMixer') }}</button>
        </div>
      </div>

      <!-- RECORDER card -->
      <div class="card" :class="{ 'card-active': activeCard === 'recorder' }" @click="toggleCard('recorder')">
        <div class="card-head">
          <span class="card-label">{{ t('dashboard.recorder') }}</span>
          <div class="card-icon red">
            <svg viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="12" r="8"/></svg>
          </div>
        </div>
        <div class="card-val">{{ recorderStatus === 'idle' ? t('dashboard.idle') : t('dashboard.active') }}</div>
        <div class="card-sub">{{ recorderStatusLabel }}</div>

        <!-- Recorder popover -->
        <div v-if="activeCard === 'recorder'" class="popover" @click.stop>
          <div class="pop-header">
            <span class="pop-title">{{ t('dashboard.recordingSettings') }}</span>
            <button class="pop-gear" :title="t('dashboard.captureSettings')" @click.stop="settingsTargetTab = 'captureSound'; emit('navigate', 'settings'); closePopovers()">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/></svg>
            </button>
          </div>
          <div class="pop-rows">
            <div class="pop-row">
              <label>{{ t('dashboard.quality') }}</label>
              <SelectField v-model="settings.gsrQuality" :options="gsrQualityOptions" />
            </div>
            <div class="pop-row">
              <label>{{ t('dashboard.fps') }}</label>
              <SelectField v-model="settings.gsrFps" :options="gsrFpsOptions" />
            </div>
            <div class="pop-row">
              <label>{{ t('dashboard.buffer') }}</label>
              <SelectField v-model="settings.gsrReplaySecs" :options="gsrReplayOptions" />
            </div>
            <div class="pop-row">
              <label>{{ t('dashboard.target') }}</label>
              <SelectField v-model="settings.gsrMonitorTarget" :options="targetOptions" />
            </div>
          </div>
          <button
            class="pop-action-btn"
            :class="{ danger: isRunning }"
            @click.stop="toggleRecording"
          >
            <span class="rec-status-dot" :class="{ on: isRunning, recording: replay.status === 'recording' }"></span>
            {{ isRunning ? t('dashboard.stop') : (settings.gsrEnabled ? t('dashboard.startReplayBuffer') : t('dashboard.startReplay')) }}
          </button>
          <div v-if="settings.gsrEnabled" class="gsr-est-row">
            ~{{ gsrEstRamMb }} {{ t('dashboard.gsr.estRam') }} &nbsp;·&nbsp; ~{{ gsrEstFileMb }} {{ t('dashboard.gsr.estFile') }}
          </div>
        </div>
      </div>

      <!-- CLIPS card -->
      <div class="card" :class="{ 'card-active': activeCard === 'clips' }" @click="toggleCard('clips')">
        <div class="card-head">
          <span class="card-label">{{ t('dashboard.clipsCard') }}</span>
          <div class="card-icon green">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2"/></svg>
          </div>
        </div>
        <div class="card-val">{{ clipCount }}</div>
        <div class="card-sub">{{ t('dashboard.videoClipsSaved') }}</div>

        <!-- Clips popover -->
        <div v-if="activeCard === 'clips'" class="popover popover-clips" @click.stop>
          <div class="pop-title">{{ t('dashboard.recentClips') }}</div>
          <div v-if="replay.loading" class="pop-empty">{{ t('dashboard.loading') }}</div>
          <div v-else-if="recentClips.length === 0" class="pop-empty">{{ t('dashboard.noClips') }}</div>
          <template v-else>
            <!-- Grid mode: 2×2 grid, 4th slot is "More Clips" button-card -->
            <div v-if="viewMode === 'grid'" class="clip-mini-grid">
              <div
                v-for="clip in recentClips"
                :key="clip.id"
                class="clip-mini"
                @click.stop="openClipPreview(clip)"
                @contextmenu.stop.prevent="openClipContextMenu(clip, $event)"
              >
                <div class="clip-mini-thumb">
                  <img
                    v-if="clip.thumbnail && mediaPort"
                    :src="mediaUrl(clip.thumbnail, mediaPort)"
                    class="clip-thumb-img"
                    loading="lazy"
                  />
                  <div v-else class="clip-thumb-placeholder">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="20" height="20"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2"/></svg>
                  </div>
                </div>
                <div class="clip-mini-name">{{ clip.custom_name || clip.filename }}</div>
              </div>
              <!-- 4th slot: More Clips button-card -->
              <button class="clip-mini clip-mini-more" @click.stop="emit('navigate', 'clips'); closePopovers()">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="18" height="18"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2"/></svg>
                <span>{{ t('dashboard.moreClips') }}</span>
              </button>
            </div>
            <!-- List mode: vertical list + More Clips button below -->
            <div v-else class="clip-mini-list">
              <div
                v-for="clip in recentClips"
                :key="clip.id"
                class="clip-mini-row"
                @click.stop="openClipPreview(clip)"
                @contextmenu.stop.prevent="openClipContextMenu(clip, $event)"
              >
                <div class="clip-row-thumb">
                  <img
                    v-if="clip.thumbnail && mediaPort"
                    :src="mediaUrl(clip.thumbnail, mediaPort)"
                    class="clip-thumb-img"
                    loading="lazy"
                  />
                  <div v-else class="clip-thumb-placeholder">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="14" height="14"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2"/></svg>
                  </div>
                </div>
                <div class="clip-row-name">{{ clip.custom_name || clip.filename }}</div>
              </div>
            </div>
            <button v-if="viewMode === 'list'" class="pop-nav-btn" @click.stop="emit('navigate', 'clips'); closePopovers()">{{ t('dashboard.moreClips') }}</button>
          </template>
        </div>
      </div>

      <!-- DEVICES card -->
      <div class="card" :class="{ 'card-active': activeCard === 'devices' }" @click="toggleCard('devices')">
        <div class="card-head">
          <span class="card-label">{{ t('dashboard.devices') }}</span>
          <div class="card-icon purple">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 18v-6a9 9 0 0118 0v6"/><path d="M21 19a2 2 0 01-2 2h-1a2 2 0 01-2-2v-3a2 2 0 012-2h3zM3 19a2 2 0 002 2h1a2 2 0 002-2v-3a2 2 0 00-2-2H3z"/></svg>
          </div>
        </div>
        <div class="card-val">{{ t('dashboard.scan') }}</div>
        <div class="card-sub">{{ t('dashboard.devicesSub') }}</div>

        <!-- Devices popover -->
        <div v-if="activeCard === 'devices'" class="popover popover-devices" @click.stop>
          <div class="devices-coming-soon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="32" height="32" class="cs-icon"><path d="M3 18v-6a9 9 0 0118 0v6"/><path d="M21 19a2 2 0 01-2 2h-1a2 2 0 01-2-2v-3a2 2 0 012-2h3zM3 19a2 2 0 002 2h1a2 2 0 002-2v-3a2 2 0 00-2-2H3z"/></svg>
            <div class="cs-title">{{ t('dashboard.comingSoon') }}</div>
            <div class="cs-sub">{{ t('dashboard.comingSoonDesc') }}</div>
          </div>
        </div>
      </div>
    </div>

    <h2 class="section-title">{{ t('dashboard.latestUpdates') }}</h2>
    <div class="updates">
      <div class="update-card" v-for="(u, i) in updates" :key="u.version" v-show="i === 0 || showAllUpdates">
        <div class="update-hdr">
          <span class="update-ver">{{ u.version }}</span>
          <span class="update-date">{{ u.date }}</span>
        </div>
        <ul class="update-list">
          <li v-for="item in u.items" :key="item">{{ item }}</li>
        </ul>
      </div>
      <button class="updates-toggle" @click="showAllUpdates = !showAllUpdates">
        {{ showAllUpdates ? t('dashboard.showLess') : t('dashboard.moreUpdates', updates.length - 1) }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.home { display: flex; flex-direction: column; gap: 20px; }
.gsr-warn {
  display: flex; align-items: center; gap: 10px; padding: 10px 14px;
  background: rgba(245,158,11,.1); border: 1px solid rgba(245,158,11,.3);
  border-radius: 8px; font-size: 12px; color: #f59e0b; line-height: 1.4;
}
.gsr-warn-ic { width: 16px; height: 16px; flex-shrink: 0; }
.gsr-warn span { flex: 1; }
.gsr-warn-dismiss { background: transparent; border: none; color: rgba(245,158,11,.6); cursor: pointer; font-size: 14px; padding: 0 2px; line-height: 1; }
.gsr-warn-dismiss:hover { color: #f59e0b; }
.page-title { font-size: 22px; font-weight: 700; }
.grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 14px; }
.card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 16px;
  cursor: pointer;
  transition: border-color .15s;
  position: relative;
  overflow: visible;
}
.card:hover { border-color: var(--text-muted); }
.card-active { border-color: var(--accent) !important; }
.card-head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px; }
.card-label { font-size: 13px; font-weight: 600; text-transform: uppercase; letter-spacing: .8px; color: var(--text-sec); }
.card-icon { width: 36px; height: 36px; border-radius: var(--radius); display: flex; align-items: center; justify-content: center; }
.card-icon svg { width: 18px; height: 18px; }
.card-icon.accent { background: rgba(59,130,246,.1); color: #3b82f6; }
.card-icon.red { background: rgba(239,68,68,.1); color: #EF4444; }
.card-icon.green { background: rgba(16,185,129,.1); color: var(--success); }
.card-icon.purple { background: rgba(168,85,247,.1); color: var(--purple); }
.card-val { font-size: 24px; font-weight: 700; line-height: 1.2; }
.card-sub { font-size: 12px; color: var(--text-muted); margin-top: 3px; }

/* ── Popover ── */
.popover {
  position: absolute;
  top: calc(100% + 8px);
  left: 0;
  width: 100%;
  min-width: 280px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 10px;
  box-shadow: 0 8px 28px rgba(0,0,0,.4);
  z-index: 9999;
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.pop-header {
  display: flex; align-items: center; justify-content: space-between;
}
.pop-title {
  font-size: 10px; font-weight: 700; letter-spacing: .6px;
  text-transform: uppercase; color: var(--text-muted);
}
.pop-gear {
  background: none; border: none; cursor: pointer; padding: 2px;
  color: var(--text-muted); display: flex; align-items: center; border-radius: 4px;
  transition: color .12s, background .12s;
}
.pop-gear:hover { color: var(--text); background: var(--bg-hover); }
.pop-empty { font-size: 12px; color: var(--text-muted); text-align: center; padding: 12px 0; }

/* ── Recorder popover ── */
.pop-rows { display: flex; flex-direction: column; gap: 7px; }
.pop-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.pop-row label { font-size: 12px; color: var(--text-sec); min-width: 44px; flex-shrink: 0; }
.pop-row :deep(.sf-root) { flex: 1; min-width: 0; }

.pop-action-btn {
  width: 100%; padding: 7px 0; border-radius: 6px; font-size: 12px; font-weight: 600;
  border: 1px solid var(--accent); background: color-mix(in srgb, var(--accent) 15%, transparent);
  color: var(--accent); cursor: pointer; transition: background .15s;
}
.pop-action-btn:hover { background: color-mix(in srgb, var(--accent) 25%, transparent); }
.pop-action-btn.danger { border-color: var(--danger); color: var(--danger); background: color-mix(in srgb, var(--danger) 12%, transparent); }
.pop-action-btn.danger:hover { background: color-mix(in srgb, var(--danger) 22%, transparent); }
.pop-action-btn { display: flex; align-items: center; justify-content: center; gap: 6px; }

/* Recording status dot inside button */
.rec-status-dot {
  width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0;
  background: var(--text-muted); transition: background .2s;
}
.rec-status-dot.on { background: #22c55e; animation: pulse-dot 1.2s infinite; }
.rec-status-dot.recording { background: var(--danger); }
@keyframes pulse-dot { 0%,100%{opacity:1} 50%{opacity:.35} }

/* Resource estimation */
.gsr-est-row {
  font-size: 10px; color: var(--text-muted); text-align: center;
  border-top: 1px solid var(--border); padding-top: 8px; margin-top: -2px;
}

.pop-nav-btn {
  width: 100%; padding: 7px 0; border-radius: 6px; font-size: 12px; font-weight: 600;
  border: 1px solid var(--border); background: var(--bg-input);
  color: var(--text-sec); cursor: pointer; transition: background .15s;
}
.pop-nav-btn:hover { background: var(--bg-hover); color: var(--text); }

/* ── Mixer strips ── */
.mixer-strips { display: flex; flex-direction: column; gap: 8px; }
.mixer-strip { display: flex; align-items: center; gap: 8px; flex-wrap: nowrap; }
.strip-label { font-size: 11px; font-weight: 600; color: var(--text-sec); width: 34px; flex-shrink: 0; white-space: nowrap; }
.strip-slider-wrap { flex: 1; min-width: 0; overflow: hidden; position: relative; display: flex; align-items: center; }
.strip-slider {
  flex: 1; height: 5px; cursor: pointer;
  -webkit-appearance: none; appearance: none;
  background: var(--border); border-radius: 3px; outline: none;
}
.strip-slider::-webkit-slider-thumb {
  -webkit-appearance: none; appearance: none;
  width: 6px; height: 18px; border-radius: 3px;
  background: var(--accent); cursor: pointer;
}
.strip-slider::-moz-range-thumb {
  width: 6px; height: 18px; border-radius: 3px;
  background: var(--accent); cursor: pointer; border: none;
}
.strip-vol {
  position: absolute; top: -24px; left: 50%; transform: translateX(-50%);
  font-size: 10px; color: var(--text); background: var(--bg-card);
  border: 1px solid var(--border); border-radius: 4px;
  padding: 1px 5px; white-space: nowrap; pointer-events: none;
  opacity: 0; transition: opacity .1s;
  font-variant-numeric: tabular-nums;
}
.strip-vol.visible { opacity: 1; }
.strip-mute {
  width: 24px; height: 24px; border-radius: 5px; flex-shrink: 0;
  border: 1px solid var(--border); background: var(--bg-input);
  color: var(--text-sec); cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  transition: background .12s, color .12s, border-color .12s;
}
.strip-mute:hover { background: var(--bg-hover); }
.strip-mute.muted { background: color-mix(in srgb, var(--danger) 15%, transparent); border-color: var(--danger); color: var(--danger); }

/* ── Clips mini grid ── */
.popover-clips { }
.clip-mini-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
.clip-mini {
  border-radius: 6px; overflow: hidden; cursor: pointer;
  border: 1px solid var(--border); transition: border-color .12s;
}
.clip-mini:hover { border-color: var(--accent); }
.clip-mini-thumb { aspect-ratio: 16/9; background: var(--bg-input); overflow: hidden; }
.clip-thumb-img { width: 100%; height: 100%; object-fit: cover; display: block; }
.clip-thumb-placeholder {
  width: 100%; height: 100%; display: flex; align-items: center; justify-content: center;
  color: var(--text-muted);
}
.clip-mini-name {
  font-size: 10px; color: var(--text-sec); padding: 4px 6px;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
/* 4th slot: More Clips button-card */
.clip-mini-more {
  display: flex; flex-direction: column; align-items: center; justify-content: center;
  gap: 5px; aspect-ratio: unset; min-height: 60px;
  background: var(--bg-input); color: var(--text-muted);
  font-size: 10px; font-weight: 600; text-align: center;
  border: 1px dashed var(--border);
  transition: border-color .12s, color .12s, background .12s;
}
.clip-mini-more:hover { border-color: var(--accent); color: var(--accent); background: color-mix(in srgb, var(--accent) 8%, transparent); }

/* ── Clips list layout (when ClipsPage is in list mode) ── */
.clip-mini-list { display: flex; flex-direction: column; gap: 6px; }
.clip-mini-row {
  display: flex; align-items: center; gap: 8px; cursor: pointer;
  border-radius: 6px; padding: 5px 6px;
  border: 1px solid var(--border); transition: border-color .12s;
}
.clip-mini-row:hover { border-color: var(--accent); }
.clip-row-thumb {
  width: 52px; height: 32px; flex-shrink: 0;
  border-radius: 4px; overflow: hidden; background: var(--bg-input);
}
.clip-row-thumb .clip-thumb-img { width: 100%; height: 100%; object-fit: cover; display: block; }
.clip-row-thumb .clip-thumb-placeholder { width: 100%; height: 100%; display: flex; align-items: center; justify-content: center; color: var(--text-muted); }
.clip-row-name {
  flex: 1; min-width: 0; font-size: 11px; color: var(--text-sec);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}

/* ── Devices coming soon ── */
.popover-devices { }
.devices-coming-soon {
  display: flex; flex-direction: column; align-items: center;
  gap: 8px; padding: 10px 4px;
  text-align: center;
}
.cs-icon { color: var(--purple); opacity: .7; }
.cs-title { font-size: 14px; font-weight: 700; color: var(--text); }
.cs-sub { font-size: 11px; color: var(--text-muted); line-height: 1.5; }

.section-title { font-size: 15px; font-weight: 700; color: var(--text-sec); letter-spacing: .3px; }

.updates { display: flex; flex-direction: column; gap: 10px; }
.update-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 14px 16px;
}
.update-hdr { display: flex; align-items: center; gap: 10px; margin-bottom: 8px; }
.update-ver { font-size: 13px; font-weight: 700; color: var(--accent); }
.update-date { font-size: 11px; color: var(--text-muted); margin-left: auto; }
.update-list { margin: 0; padding-left: 18px; display: flex; flex-direction: column; gap: 4px; }
.update-list li { font-size: 12px; color: var(--text-sec); line-height: 1.5; }
.updates-toggle {
  align-self: flex-start; background: none; border: none; cursor: pointer;
  font-size: 12px; color: var(--accent); padding: 0; font-weight: 600;
  opacity: .8; transition: opacity .12s;
}
.updates-toggle:hover { opacity: 1; }
</style>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, inject, watch, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { mediaUrl } from '../utils/assets'
import type { Clip, SteamGame } from '../stores/replay'
import { useReplayStore } from '../stores/replay'
import type { Ref } from 'vue'
import CustomVideoPlayer from './CustomVideoPlayer.vue'
import { useI18n } from 'vue-i18n'
import GameTagDropdown from './GameTagDropdown.vue'
import { clipDisplayTitle } from '../utils/format'

const props = defineProps<{ clip: Clip; mode: 'preview' | 'trim' }>()
const emit = defineEmits<{ 'close': []; 'saved': [string]; 'toast': [string]; 'open-editor': [] }>()
const { t } = useI18n()
const replay = useReplayStore()
const playerComp = ref<InstanceType<typeof CustomVideoPlayer> | null>(null)

const mediaPort = inject<Ref<number>>('mediaPort', ref(0))
const videoSrc = computed(() => mediaUrl(props.clip.filepath, mediaPort.value))

const duration = ref(props.clip.duration || 0)
const currentTime = ref(0)
const playing = ref(false)

// ═══ Multi-track audio support ═══
interface MInfo { duration: number; width: number; height: number; fps: number; video_codec: string; streams: { index: number; codec_type: string; title: string }[]; audio_streams: number }
interface AudioTrack { id: string; label: string; streamIndex: number; volume: number; muted: boolean }
const info = ref<MInfo | null>(null)
const audioTracks = ref<AudioTrack[]>([])
const audioEls = ref<Record<string, HTMLAudioElement>>({})
const hasMultipleAudioTracks = computed(() => audioTracks.value.length > 1)
const masterVol = ref(1)

function onVolumeChange(v: number) { masterVol.value = v }

function buildAudioUrl(streamIndex: number) {
  const port = mediaPort.value
  if (!port) return ''
  const encoded = encodeURIComponent(props.clip.filepath)
  return `http://localhost:${port}/audio?file=${encoded}&stream=${streamIndex}`
}

function initAudioElements() {
  for (const el of Object.values(audioEls.value)) { el.pause(); el.src = ''; el.load() }
  audioEls.value = {}
  for (const t of audioTracks.value) {
    const el = new Audio()
    el.src = buildAudioUrl(t.streamIndex)
    el.preload = 'auto'
    el.volume = t.muted ? 0 : t.volume / 100
    audioEls.value[t.id] = el
  }
}

function applyAudioVolumes() {
  for (const t of audioTracks.value) {
    const el = audioEls.value[t.id]
    if (el) el.volume = (t.muted || t.volume <= 0) ? 0 : Math.min(1, (t.volume / 100) * masterVol.value)
  }
}
watch(audioTracks, applyAudioVolumes, { deep: true })
watch(masterVol, applyAudioVolumes)

// Sync audio elements to video currentTime
let syncRaf = 0
function syncAudioToVideo() {
  const tick = () => {
    const video = playerComp.value?.videoRef
    if (video) {
      const vt = video.currentTime
      const vPlaying = !video.paused
      for (const el of Object.values(audioEls.value)) {
        if (Math.abs(el.currentTime - vt) > 0.15) el.currentTime = vt
        if (vPlaying && el.paused) el.play().catch(() => {})
        if (!vPlaying && !el.paused) el.pause()
      }
    }
    syncRaf = requestAnimationFrame(tick)
  }
  syncRaf = requestAnimationFrame(tick)
}

// Re-sync audio when video seeks
function onTimeUpdate(ct: number) {
  currentTime.value = ct
  const video = playerComp.value?.videoRef
  if (video && hasMultipleAudioTracks.value) {
    for (const el of Object.values(audioEls.value)) {
      if (Math.abs(el.currentTime - video.currentTime) > 0.15) el.currentTime = video.currentTime
    }
  }
}

function toggleTrackMute(t: AudioTrack) { t.muted = !t.muted }
function setTrackVolume(t: AudioTrack, v: number) { t.volume = v; if (v > 0) t.muted = false }
const editTitle = ref(clipDisplayTitle(props.clip.custom_name || '', props.clip.game || '', props.clip.filename))
const titleDirty = ref(false)
const trimStart = ref(0)
const trimEnd = ref(props.clip.duration || 0)
const trimDuration = computed(() => Math.max(0, trimEnd.value - trimStart.value))
const exporting = ref(false)
const exportMenuOpen = ref(false)
const exportProgress = ref(0)
const exportStage = ref('')
const exportSpeed = ref('')
const exportResult = ref('')
let progressUnsub: UnlistenFn | null = null

function onMeta(dur: number) { duration.value = dur; if (trimEnd.value <= 0) trimEnd.value = dur }
function seekTo(s: number) {
  playerComp.value?.seekTo(s)
  currentTime.value = s
  if (hasMultipleAudioTracks.value) {
    for (const el of Object.values(audioEls.value)) el.currentTime = s
  }
}

const timelineRef = ref<HTMLElement | null>(null)
function pxToSec(px: number) { if (!timelineRef.value || !duration.value) return 0; const r = timelineRef.value.getBoundingClientRect(); return Math.max(0, Math.min(duration.value, ((px - r.left) / r.width) * duration.value)) }
function onTlClick(e: MouseEvent) { seekTo(pxToSec(e.clientX)) }
function dragHandle(h: 'start' | 'end', e: MouseEvent) {
  e.preventDefault(); e.stopPropagation()
  const mv = (ev: MouseEvent) => { const s = pxToSec(ev.clientX); if (h === 'start') trimStart.value = Math.min(s, trimEnd.value - 0.5); else trimEnd.value = Math.max(s, trimStart.value + 0.5) }
  const up = () => { document.removeEventListener('mousemove', mv); document.removeEventListener('mouseup', up); saveTrimState() }
  document.addEventListener('mousemove', mv); document.addEventListener('mouseup', up)
}
function togglePlay() { playerComp.value?.togglePlay() }

const GAMES_STATIC = ['Counter-Strike 2','League of Legends','Valorant','Overwatch 2','Apex Legends','Fortnite','Minecraft','Dota 2','Rocket League','Elden Ring',"Baldur's Gate 3",'Cyberpunk 2077','Honkai: Star Rail','Genshin Impact','Helldivers 2','Path of Exile 2']
const GAMES = computed(() => {
  const s = new Set([...GAMES_STATIC, ...replay.steamGames.map(game => game.name)])
  return Array.from(s).sort()
})
const steamGameLookup = computed<Record<string, SteamGame>>(() => replay.steamGameMap)
const gameTag = ref('')
function steamIcon(game: string) { return steamGameLookup.value[game.toLowerCase()]?.icon_url || '' }
function steamIconUrl(game: string) {
  const icon = steamIcon(game)
  if (!icon) return ''
  return icon.startsWith('/') ? mediaUrl(icon, mediaPort.value) : icon
}

async function saveTrimState() {
  try {
    await invoke('save_trim_state', { filepath: props.clip.filepath, trimStart: trimStart.value, trimEnd: trimEnd.value })
    window.dispatchEvent(new CustomEvent('clip-trim-updated', { detail: { filepath: props.clip.filepath, trimStart: trimStart.value, trimEnd: trimEnd.value } }))
  } catch {}
}
onMounted(async () => {
  try { const s = await invoke<{ trim_start: number; trim_end: number } | null>('get_trim_state', { filepath: props.clip.filepath }); if (s && s.trim_end > 0) { trimStart.value = s.trim_start; trimEnd.value = s.trim_end } } catch {}
  try { const m = await invoke<string>('get_clip_meta', { filepath: props.clip.filepath }); if (m && m !== 'null') { const d = JSON.parse(m); if (d.game_tag) gameTag.value = d.game_tag } } catch {}
  try {
    info.value = await invoke<MInfo>('analyze_media', { filepath: props.clip.filepath })
    const streams = info.value?.streams.filter(s => s.codec_type === 'audio') || []
    audioTracks.value = streams.map((s, i) => ({
      id: `A${i + 1}`,
      label: s.title || `Audio ${i + 1}`,
      streamIndex: s.index,
      volume: 100,
      muted: false,
    }))
    await nextTick()
    if (audioTracks.value.length > 1) initAudioElements()
    syncAudioToVideo()
  } catch {}
})

onBeforeUnmount(() => {
  cancelAnimationFrame(syncRaf)
  for (const el of Object.values(audioEls.value)) { el.pause(); el.src = '' }
})

async function saveTitle() {
  const name = editTitle.value.trim()
  const fallbackTitle = clipDisplayTitle('', gameTag.value || props.clip.game || '', props.clip.filename)
  const customName = name && name !== fallbackTitle ? name : ''
  replay.updateClipMeta(props.clip.filepath, { custom_name: customName, game: gameTag.value })
  try { await invoke('set_clip_meta', { update: { filepath: props.clip.filepath, custom_name: customName, favorite: props.clip.favorite, game_tag: gameTag.value } }) } catch {}
  titleDirty.value = false
}

async function toggleFavorite() {
  const newFav = !props.clip.favorite
  replay.updateClipMeta(props.clip.filepath, { favorite: newFav })
  const fallbackTitle = clipDisplayTitle('', gameTag.value || props.clip.game || '', props.clip.filename)
  const customName = editTitle.value.trim() && editTitle.value.trim() !== fallbackTitle ? editTitle.value.trim() : ''
  try { await invoke('set_clip_meta', { update: { filepath: props.clip.filepath, custom_name: customName, favorite: newFav, game_tag: gameTag.value } }) } catch {}
}

function selectGame(g: string) { gameTag.value = g; saveTitle() }

async function doExport(targetMb: number) {
  exportMenuOpen.value = false; exporting.value = true; exportProgress.value = 0; exportStage.value = ''; exportSpeed.value = ''; exportResult.value = ''
  progressUnsub = await listen<{ percent: number; speed: string; stage: string }>('export-progress', e => {
    if (e.payload.percent >= 0) exportProgress.value = e.payload.percent
    if (e.payload.speed) exportSpeed.value = e.payload.speed
    if (e.payload.stage) exportStage.value = e.payload.stage
  })
  try {
    const out = targetMb <= 0
      ? await invoke<string>('trim_clip', { inputPath: props.clip.filepath, startSec: trimStart.value, endSec: trimEnd.value, outputPath: '', codec: 'libx264' })
      : await invoke<string>('export_clip_sized', { inputPath: props.clip.filepath, startSec: trimStart.value, endSec: trimEnd.value, targetMb, outputPath: '', codec: 'libx264', audioStartSec: null, audioEndSec: null })
    exportResult.value = out
    emit('saved', out)
  } catch (e: any) { emit('toast', t('clips.toast.exportFailed', { error: String(e) })) }
  finally {
    exporting.value = false
    progressUnsub?.(); progressUnsub = null
  }
}

function startFileDrag(e: DragEvent) {
  if (!exportResult.value || !e.dataTransfer) return
  e.dataTransfer.setData('text/uri-list', `file://${exportResult.value}`)
  e.dataTransfer.setData('text/plain', exportResult.value)
  e.dataTransfer.effectAllowed = 'copy'
}

async function openFolder() {
  if (!exportResult.value) return
  try {
    await invoke('open_file_location', { filepath: exportResult.value })
  } catch (e) {
    emit('toast', t('clips.toast.openFolderFailed', { error: String(e) }))
  }
}

async function copyPath() {
  if (!exportResult.value) return
  try {
    await invoke('write_clipboard', { text: exportResult.value })
    emit('toast', t('editor.pathCopied'))
  } catch (e) {
    emit('toast', t('clips.toast.exportFailed', { error: String(e) }))
  }
}

function closeExportSuccess() { exportResult.value = '' }

function fmt(s: number) { return `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}.${Math.floor((s % 1) * 10)}` }
</script>

<template>
  <div class="overlay" @click.self="emit('close')">
    <div class="modal" :class="{ wide: mode === 'trim' }">
      <div class="modal-hdr">
        <input v-model="editTitle" class="title-input" @input="titleDirty = true" @blur="saveTitle" @keydown.enter="($event.target as HTMLInputElement).blur()" spellcheck="false" />
        <!-- ★ E2-P6: Game tag in Quick Preview -->
        <GameTagDropdown
          v-model="gameTag"
          :games="GAMES"
          :steam-icon-url="steamIconUrl"
          :max-items="8"
          @select="selectGame"
        />
        <button class="fav-btn" :class="{ on: clip.favorite }" @click="toggleFavorite" title="Toggle favorite">
          <svg viewBox="0 0 24 24" :fill="clip.favorite ? 'currentColor' : 'none'" stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
        </button>
        <button class="edit-btn" @click="emit('open-editor')" :title="t('clips.contextMenu.edit')">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
        </button>
        <button class="close-btn" @click="emit('close')"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg></button>
      </div>

      <!-- Custom video player -->
      <CustomVideoPlayer
        ref="playerComp"
        :src="videoSrc"
        :muted="hasMultipleAudioTracks"
        :capture-keyboard="true"
        :native-controls="false"
        :show-controls="true"
        @loadedmetadata="onMeta"
        @timeupdate="onTimeUpdate"
        @play="playing = true"
        @pause="playing = false"
        @volume-change="onVolumeChange"
        @ended="playing = false"
      />

      <!-- Trim controls -->
      <div v-if="mode === 'trim'" class="trim-panel">
        <div class="time-row"><span>{{ fmt(trimStart) }}</span><span class="cur">{{ fmt(currentTime) }}</span><span>{{ fmt(trimEnd) }}</span></div>
        <div class="tl" ref="timelineRef" @click="onTlClick">
          <div class="tl-track"></div>
          <div class="tl-range" :style="{ left:(trimStart/duration*100)+'%', width:((trimEnd-trimStart)/duration*100)+'%' }"></div>
          <div class="tl-head" :style="{ left:(currentTime/duration*100)+'%' }"></div>
          <div class="tl-h" :style="{ left:(trimStart/duration*100)+'%' }" @mousedown="dragHandle('start',$event)"><div class="hg">◀</div></div>
          <div class="tl-h" :style="{ left:(trimEnd/duration*100)+'%' }" @mousedown="dragHandle('end',$event)"><div class="hg">▶</div></div>
        </div>

        <!-- Multi-track audio controls (trim mode only) -->
        <div v-if="audioTracks.length > 1" class="audio-tracks">
          <div v-for="t in audioTracks" :key="t.id" class="audio-track">
            <button class="track-mute" :class="{ muted: t.muted }" @click="toggleTrackMute(t)" :title="t.muted ? 'Unmute' : 'Mute'">
              <svg v-if="!t.muted" viewBox="0 0 24 24" fill="currentColor" style="width:14px;height:14px"><path d="M11 5L6 9H2v6h4l5 4V5z"/><path d="M15.54 8.46a5 5 0 010 7.07" stroke="currentColor" fill="none" stroke-width="2"/></svg>
              <svg v-else viewBox="0 0 24 24" fill="currentColor" style="width:14px;height:14px"><path d="M11 5L6 9H2v6h4l5 4V5z"/><line x1="23" y1="9" x2="17" y2="15" stroke="currentColor" stroke-width="2"/><line x1="17" y1="9" x2="23" y2="15" stroke="currentColor" stroke-width="2"/></svg>
            </button>
            <span class="track-label">{{ t.label }}</span>
            <input type="range" min="0" max="100" :value="t.volume" @input="setTrackVolume(t, +($event.target as HTMLInputElement).value)" class="track-vol" />
          </div>
        </div>
        <div class="transport">
          <button class="tb" @click="togglePlay"><svg v-if="playing" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg><svg v-else viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg></button>
          <button class="tb" @click="seekTo(trimStart)">⏮</button>
          <button class="tb" @click="seekTo(trimEnd)">⏭</button>
          <span class="trim-info">{{ fmt(trimDuration) }}</span>

          <!-- Export success: share actions -->
          <template v-if="exportResult">
            <div class="export-success-row">
              <span class="file-name-text">{{ exportResult.split('/').pop() }}</span>
              <button class="tb" @click="copyPath" :title="t('editor.copyPath')">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
              </button>
              <button class="tb" @click="openFolder" :title="t('editor.showInFolder')">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/>
                </svg>
              </button>
              <div class="tb drag-btn" draggable="true" @dragstart="startFileDrag" :title="t('editor.dragReady')">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/>
                  <line x1="8.59" y1="13.51" x2="15.42" y2="17.49"/><line x1="15.41" y1="6.51" x2="8.59" y2="10.49"/>
                </svg>
              </div>
              <button class="tb" @click="closeExportSuccess">✕</button>
            </div>
          </template>

          <!-- Export progress -->
          <template v-else-if="exporting">
            <div class="progress-bar-wrap">
              <div class="progress-track">
                <div class="progress-fill" :class="{ shimmer: exportProgress <= 0 }" :style="{ width: Math.max(exportProgress, 4) + '%' }"></div>
              </div>
              <span class="progress-text">{{ exportStage === 'copying' || exportStage === 'pass1' || exportStage === 'pass2' ? t('editor.exporting') : `${exportProgress.toFixed(0)}%` }} {{ exportSpeed ? `(${exportSpeed})` : '' }}</span>
            </div>
          </template>

          <!-- Export menu -->
          <template v-else>
            <div class="exp-wrap">
              <button class="exp-btn" @click="exportMenuOpen=!exportMenuOpen">{{ t('editor.exportClip') }} <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:12px;height:12px"><path d="M6 9l6 6 6-6"/></svg></button>
              <div v-if="exportMenuOpen" class="exp-menu"><button @click="doExport(0)">{{ t('editor.original') }}</button><button @click="doExport(10)">10 MB</button><button @click="doExport(50)">50 MB</button><button @click="doExport(100)">100 MB</button></div>
            </div>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.overlay { position:fixed; inset:0; z-index:1000; background:rgba(0,0,0,.85); display:flex; align-items:center; justify-content:center; backdrop-filter:blur(4px); }

/* ★ FIX 2: Responsive modal with viewport-based sizing and strict bounds */
.modal {
  width: 90vw;
  max-width: 1100px;
  min-width: 500px;
  max-height: 90vh;
  background: var(--bg-surface);
  border: 1px solid var(--border);
  border-radius: 12px;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.modal.wide {
  max-width: 1200px;
}

.modal-hdr { display:flex; align-items:center; justify-content:space-between; padding:12px 20px; border-bottom:1px solid var(--border); gap:12px; flex-shrink:0; }
.title-input { flex:1; padding:6px 10px; background:var(--bg-input); border:1px solid var(--border); border-radius:6px; color:var(--text); font-size:15px; font-weight:700; outline:none; color-scheme:dark; } .title-input:focus { border-color:var(--accent); }
/* Game tag input sizing to match title input */
:deep(.gtd-input) { padding: 5px 10px; font-size: 13px; }

.fav-btn, .edit-btn { width:32px; height:32px; border:none; background:transparent; color:var(--text-sec); cursor:pointer; border-radius:6px; display:flex; align-items:center; justify-content:center; flex-shrink:0; }
.fav-btn svg, .edit-btn svg { width:16px; height:16px; }
.fav-btn:hover { background:var(--bg-hover); color:var(--accent); }
.fav-btn.on { color:#E94560; }
.edit-btn:hover { background:var(--bg-hover); color:var(--accent); }
.close-btn { width:32px; height:32px; border:none; background:transparent; color:var(--text-sec); cursor:pointer; border-radius:6px; display:flex; align-items:center; justify-content:center; } .close-btn svg { width:18px; height:18px; } .close-btn:hover { background:var(--bg-hover); }

/* Player sizing — CustomVideoPlayer handles the video/controls */
:deep(.cvp-wrap) {
  flex-shrink: 1;
  min-height: 0;
  min-width: 0;
  width: 100%;
  height: auto;
  max-height: 100%;
  overflow: hidden;
}

.trim-panel { padding:16px 20px; flex-shrink:0; }
.time-row { display:flex; justify-content:space-between; font-size:11px; color:var(--text-muted); margin-bottom:6px; } .cur { color:var(--accent); font-weight:600; }
.tl { position:relative; height:36px; cursor:pointer; user-select:none; }
.tl-track { position:absolute; top:50%; left:0; right:0; height:4px; transform:translateY(-50%); background:var(--bg-deep); border-radius:2px; }
.tl-range { position:absolute; top:50%; height:4px; transform:translateY(-50%); background:var(--accent); opacity:.6; border-radius:2px; }
.tl-head { position:absolute; top:4px; bottom:4px; width:2px; background:#fff; z-index:3; }
.tl-h { position:absolute; top:2px; bottom:2px; width:14px; transform:translateX(-7px); z-index:4; cursor:ew-resize; display:flex; align-items:center; justify-content:center; }
.hg { width:14px; height:24px; background:var(--accent); border-radius:3px; display:flex; align-items:center; justify-content:center; font-size:8px; color:#fff; }
.transport { display:flex; align-items:center; gap:8px; margin-top:14px; }
.tb { width:36px; height:36px; border:1px solid var(--border); border-radius:8px; background:var(--bg-card); color:var(--text-sec); cursor:pointer; display:flex; align-items:center; justify-content:center; } .tb svg { width:16px; height:16px; } .tb:hover { background:var(--bg-hover); }
.trim-info { font-size:11px; color:var(--text-muted); margin-left:auto; margin-right:8px; }
.exp-wrap { position:relative; }
.exp-btn { padding:8px 18px; border:none; border-radius:8px; background:var(--accent); color:#fff; font-weight:600; font-size:13px; cursor:pointer; display:flex; align-items:center; gap:4px; } .exp-btn:disabled { opacity:.5; }
.exp-menu { position:absolute; bottom:100%; right:0; margin-bottom:6px; background:var(--bg-card); border:1px solid var(--border); border-radius:8px; padding:4px; min-width:140px; box-shadow:0 -4px 16px rgba(0,0,0,.4); }
.exp-menu button { width:100%; padding:8px 12px; border:none; background:transparent; color:var(--text-sec); font-size:12px; text-align:left; cursor:pointer; border-radius:5px; } .exp-menu button:hover { background:var(--bg-hover); color:var(--text); }

/* Progress bar */
.progress-bar-wrap { flex:1; display:flex; align-items:center; gap:8px; min-width:0; }
.progress-track { flex:1; height:6px; background:var(--bg-deep); border-radius:3px; overflow:hidden; }
.progress-fill { height:100%; background:var(--accent); border-radius:3px; transition:width .2s; }
.progress-fill.shimmer {
  background: linear-gradient(90deg, var(--accent) 25%, color-mix(in srgb, var(--accent) 60%, #fff) 50%, var(--accent) 75%);
  background-size: 200% 100%;
  animation: shimmer 1.2s infinite linear;
}
@keyframes shimmer {
  0% { background-position: 200% 0; }
  100% { background-position: -200% 0; }
}
.progress-text { font-size:11px; color:var(--text-muted); white-space:nowrap; }

/* Export success row */
.export-success-row { flex:1; display:flex; align-items:center; gap:6px; min-width:0; justify-content:flex-end; }
.export-success-row .file-name-text { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:11px; color:var(--text-sec); max-width:160px; }
.drag-btn { cursor:grab; }
.drag-btn:active { cursor:grabbing; }

/* Multi-track audio controls */
.audio-tracks { padding:12px 20px; display:flex; flex-direction:column; gap:8px; flex-shrink:0; }
.audio-track { display:flex; align-items:center; gap:8px; padding:6px 10px; background:var(--bg-card); border:1px solid var(--border); border-radius:8px; }
.track-mute { width:28px; height:28px; border:none; background:transparent; color:var(--text-sec); cursor:pointer; border-radius:6px; display:flex; align-items:center; justify-content:center; flex-shrink:0; }
.track-mute:hover { background:var(--bg-hover); }
.track-mute.muted { color:#E94560; }
.track-label { font-size:12px; color:var(--text-sec); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; max-width:120px; }
.track-vol { flex:1; height:4px; accent-color:var(--accent); cursor:pointer; }
</style>

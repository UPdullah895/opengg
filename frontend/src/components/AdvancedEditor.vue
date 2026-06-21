<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, watch, inject, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { useI18n } from 'vue-i18n'
import { mediaUrl } from '../utils/assets'
import { useReplayStore } from '../stores/replay'
import type { Clip } from '../stores/replay'
import type { Ref } from 'vue'
import { usePersistenceStore } from '../stores/persistence'
import { resumeAudioContext } from '../utils/audio'
import CustomVideoPlayer from './CustomVideoPlayer.vue'
import GameTagDropdown from './GameTagDropdown.vue'
import VolumeSlider from './VolumeSlider.vue'
import TimelineTrackRow from './TimelineTrackRow.vue'
import { clipDisplayTitle } from '../utils/format'

// ── Types ──
interface MInfo { duration: number; width: number; height: number; fps: number; video_codec: string; streams: { index: number; codec_type: string; title: string }[]; audio_streams: number }
interface Track { id: string; label: string; type: 'video' | 'audio'; color: string; volume: number; muted: boolean; streamIndex: number; peaks: number[]; volOpen: boolean; title?: string }

const props = defineProps<{ clip: Clip }>()
const emit = defineEmits<{ 'close': []; 'toast': [string]; 'saved': [string] }>()
const mediaPort = inject<Ref<number>>('mediaPort', ref(0))
const mediaToken = inject<Ref<string>>('mediaToken', ref(''))
const videoSrc = computed(() => mediaUrl(props.clip.filepath, mediaPort.value, mediaToken.value))
const persist = usePersistenceStore()
const replay  = useReplayStore()
const { t }   = useI18n()

const SKIP = 5
const GAMES = ['Counter-Strike 2', 'League of Legends', 'Valorant', 'Overwatch 2', 'Apex Legends', 'Fortnite', 'Minecraft', 'Dota 2', 'Rocket League', 'Elden Ring', "Baldur's Gate 3", 'Cyberpunk 2077', 'Honkai: Star Rail', 'Genshin Impact', 'Helldivers 2', 'Path of Exile 2']

// ── Core State ──
const playerComp = ref<InstanceType<typeof CustomVideoPlayer> | null>(null)
const videoRef   = ref<HTMLVideoElement | null>(null)  // synced from playerComp in onMounted
const dur = ref(props.clip.duration || 30)

// ★ Epic 2: Local monitor volume (preview only — NOT exported) and hover-mute tracking
const localVol        = ref(1)
const hoveredTrackId  = ref<string | null>(null)
const ct = ref(0)
const playing = ref(false)
const info = ref<MInfo | null>(null)
const trimS = ref(0)
const trimE = ref(dur.value)
const trimDur = computed(() => Math.max(0, trimE.value - trimS.value))
const magnetMode = ref(true)
const audioTrimS = ref(0)
const audioTrimE = ref(0)
const editName = ref(clipDisplayTitle(props.clip.custom_name || '', props.clip.game || '', props.clip.filename))
const gameTag = ref('')

// ── Tracks (E6: colors via CSS vars, sourced from persisted Settings) ──
const tracks = ref<Track[]>([])
const hasMultipleAudioTracks = computed(() => tracks.value.filter(t => t.type === 'audio').length > 1)

const TRACK_FALLBACKS: Record<string, string> = {
  V1: '#E94560', A1: '#10B981', A2: '#3b82f6', A3: '#f59e0b', A4: '#8b5cf6', A5: '#ec4899',
}

function getTrackDef(id: string) {
  return persist.state.settings.trackDefs?.find(d => d.id === id)
}
function getTrackColor(id: string): string {
  return getTrackDef(id)?.color ?? TRACK_FALLBACKS[id] ?? '#64748b'
}
// Default label when no name is configured — never render blank.
function defaultTrackName(id: string): string {
  if (id === 'V1') return 'Video'
  if (id.startsWith('A')) return `Audio ${id.slice(1)}`
  if (id.startsWith('O')) return `Overlay ${id.slice(1)}`
  return id
}
function getTrackName(id: string): string {
  // `||` (not `??`) so a cleared/empty name falls back to the default instead of rendering blank.
  return getTrackDef(id)?.name?.trim() || defaultTrackName(id)
}
function showIcons(trackId: string): boolean {
  return getTrackDef(trackId)?.visible ?? true
}
// Role badge text: video tracks → "Video"; mic capture → "Input"; everything else
// (desktop/game/chat/media/aux output capture) → "Output". Uses the track's name/embedded
// title so it follows the actual captured source, not a positional guess.
function trackKindLabel(tk: Track): string {
  if (tk.type === 'video') return t('editor.trackKind.video')
  const hay = `${tk.label} ${tk.title ?? ''}`.toLowerCase()
  return /\bmic\b|microphone|ميكروف|مايك/.test(hay) ? t('editor.trackKind.input') : t('editor.trackKind.output')
}

// AudioContext resume is handled by the shared singleton in utils/audio.ts.
// installAudioUnlocker() in App.vue covers the global one-shot unlock.
// resumeAudioContext() is called immediately before every play() as a safeguard.

// ── Layout ──
const sideOpen = ref(true)
const sideWidth = ref(200) // E4: resizable
const previewFlex = ref(3)
const tlFlex = ref(1.2)
const undoStack = ref<{ s: number; e: number; as: number; ae: number }[]>([])
const redoStack = ref<{ s: number; e: number; as: number; ae: number }[]>([])
const sideTab = ref('info')


// ── Export ──
const exportModal = ref(false)
const exportName = ref('')
const exportDir = ref('')
const exportTarget = ref(0)
const exportSettings = ref('')
const exporting = ref(false)
const exportProgress = ref(0)
const exportSpeed = ref('')
const exportStage = ref('')
const exportCodec = ref('libx264')
const exportAdvancedOpen = ref(false)
const codecOptions = computed(() => [
  { value: 'libx264', label: 'H.264' },
  { value: 'libx265', label: 'H.265' },
  { value: 'libvpx-vp9', label: 'VP9' },
  { value: 'libsvtav1', label: 'AV1' },
  { value: 'copy', label: t('editor.codecOriginal') },
])
let progressUnsub: UnlistenFn | null = null
const toast = ref('')
const toastFile = ref('')
function showToast(m: string, f = '') { toast.value = m; toastFile.value = f; setTimeout(() => { toast.value = ''; toastFile.value = '' }, 5000) }
function unmuteMediaStreams() { invoke('unmute_media_streams').catch(() => {}) }

// ── Video ──
function onMeta() { if (videoRef.value) { dur.value = videoRef.value.duration; if (trimE.value <= 0 || trimE.value > dur.value) trimE.value = dur.value; if (audioTrimE.value <= 0 || audioTrimE.value > dur.value) { audioTrimS.value = trimS.value; audioTrimE.value = trimE.value } } }
// Intercept 'play' event to resume AudioContext (handles native controls too)
async function onVideoPlay() { await resumeAudioContext(); playing.value = true }
async function togglePlay() {
  if (!videoRef.value) return
  await resumeAudioContext()
  if (playing.value) { videoRef.value.pause() }
  else {
    unmuteMediaStreams()
    if (videoRef.value.currentTime < trimS.value || videoRef.value.currentTime >= trimE.value) {
      const seekTo = trimS.value > 0 ? trimS.value : 0.01
      videoRef.value.currentTime = seekTo
      // Re-seek audio elements immediately so they're aligned when play() fires
      for (const el of Object.values(audioEls.value)) el.currentTime = seekTo
    }
    videoRef.value.play().catch(e => { if (import.meta.env.DEV) console.warn('play blocked:', e) })
  }
  playing.value = !playing.value
}
function seekTo(s: number) { if (videoRef.value) { videoRef.value.currentTime = Math.max(0, Math.min(dur.value, s)); ct.value = videoRef.value.currentTime } }
function skip(d: number) { seekTo(ct.value + d) }
watch(ct, t => { if (playing.value && t >= trimE.value && videoRef.value) { videoRef.value.pause(); playing.value = false } })

// RAF playhead
let raf = 0
function startRAF() { const tick = () => { if (videoRef.value) ct.value = videoRef.value.currentTime; raf = requestAnimationFrame(tick) }; raf = requestAnimationFrame(tick) }
onMounted(startRAF)
// The AudioContext singleton lives for the app session; don't close it on unmount.
onBeforeUnmount(() => { cancelAnimationFrame(raf) })

// ═══ Epic 1: Multi-Track Audio via hidden <audio> elements ═══
// HTML5 <video> only plays the first audio stream.
// Solution: Mute the <video>, create per-track <audio> elements
// fed from the /audio endpoint, synced to video.currentTime.
const audioEls = ref<Record<string, HTMLAudioElement>>({})

function buildAudioUrl(streamIndex: number) {
  const port = mediaPort.value
  const token = mediaToken.value
  if (!port || !token) return ''
  const encoded = encodeURIComponent(props.clip.filepath)
  return `http://127.0.0.1:${port}/audio?file=${encoded}&stream=${streamIndex}&token=${encodeURIComponent(token)}`
}

function initAudioElements() {
  // Destroy old audio elements
  for (const el of Object.values(audioEls.value)) { el.pause(); el.src = ''; el.load() }
  audioEls.value = {}

  const audioTracks = tracks.value.filter(t => t.type === 'audio')
  if (audioTracks.length <= 1) {
    // Single-track: native audio plays through the video element
    if (videoRef.value) { videoRef.value.volume = audioTracks[0] ? (audioTracks[0].muted ? 0 : audioTracks[0].volume / 100) : 1 }
    return
  }

  // Multi-track: video is muted via :muted prop; use separate <audio> elements

  for (const t of audioTracks) {
    const el = new Audio()
    el.src = buildAudioUrl(t.streamIndex)
    el.preload = 'auto'
    el.volume = t.muted ? 0 : t.volume / 100
    audioEls.value[t.id] = el
  }
}

async function syncPlayerVideoRef() {
  await nextTick()
  videoRef.value = playerComp.value?.videoRef ?? null
}

async function refreshAudioRouting() {
  await syncPlayerVideoRef()
  if (!videoRef.value) return
  initAudioElements()
  applyAudioVolumes()
}

// Sync audio elements to video currentTime
let syncRaf = 0
function syncAudioToVideo() {
  const tick = () => {
    if (videoRef.value) {
      const vt = videoRef.value.currentTime
      const vPlaying = !videoRef.value.paused
      if (vPlaying) unmuteMediaStreams()
      for (const [_id, el] of Object.entries(audioEls.value)) {
        if (Math.abs(el.currentTime - vt) > 0.15) el.currentTime = vt
        if (vPlaying && el.paused) el.play().catch(() => {})
        if (!vPlaying && !el.paused) el.pause()
      }
    }
    syncRaf = requestAnimationFrame(tick)
  }
  syncRaf = requestAnimationFrame(tick)
}
onMounted(syncAudioToVideo)
onBeforeUnmount(() => {
  cancelAnimationFrame(syncRaf)
  for (const el of Object.values(audioEls.value)) { el.pause(); el.src = '' }
})

// Per-track volume/mute → update the corresponding <audio> element
// localVol is applied as a master monitor gain (preview only — not in export)
function applyAudioVolumes() {
  const audioTracks = tracks.value.filter(t => t.type === 'audio')
  const hasMulti = Object.keys(audioEls.value).length > 0
  if (hasMulti) {
    for (const t of audioTracks) {
      const el = audioEls.value[t.id]
      if (el) {
        const silent = t.muted || t.volume <= 0
        el.volume = silent ? 0 : Math.min(1, (t.volume / 100) * localVol.value)
        // True mute: volume 0 alone can be undone by element/UA quirks — set muted too.
        // A 0% track (even if not flagged muted) must be dead silent.
        el.muted = localVol.value <= 0 || silent
      }
    }
  } else if (videoRef.value) {
    const t = audioTracks[0]
    if (t) {
      const silent = t.muted || t.volume <= 0
      videoRef.value.volume = silent ? 0 : Math.min(1, (t.volume / 100) * localVol.value)
      videoRef.value.muted = localVol.value <= 0 || silent
    }
  }
}
watch(tracks, applyAudioVolumes, { deep: true })
watch(localVol, applyAudioVolumes)
watch(
  () => tracks.value.filter(t => t.type === 'audio').map(t => `${t.id}:${t.streamIndex}`).join('|'),
  () => { refreshAudioRouting().catch(() => {}) },
)

// ── Timeline ──
const tlRef = ref<HTMLElement | null>(null)
const scrubbing = ref(false)
function pxToSec(e: MouseEvent) { if (!tlRef.value) return 0; const r = tlRef.value.getBoundingClientRect(); return Math.max(0, Math.min(dur.value, ((e.clientX - r.left) / r.width) * dur.value)) }
function tlClick(e: MouseEvent) { if ((e.target as HTMLElement).closest('.no-seek')) return; seekTo(pxToSec(e)) }
function pushUndo() { undoStack.value.push({ s: trimS.value, e: trimE.value, as: audioTrimS.value, ae: audioTrimE.value }); redoStack.value = [] }
function undo() { const p = undoStack.value.pop(); if (p) { redoStack.value.push({ s: trimS.value, e: trimE.value, as: audioTrimS.value, ae: audioTrimE.value }); trimS.value = p.s; trimE.value = p.e; audioTrimS.value = p.as; audioTrimE.value = p.ae; saveMeta() } }
function redo() { const r = redoStack.value.pop(); if (r) { undoStack.value.push({ s: trimS.value, e: trimE.value, as: audioTrimS.value, ae: audioTrimE.value }); trimS.value = r.s; trimE.value = r.e; audioTrimS.value = r.as; audioTrimE.value = r.ae; saveMeta() } }
function dragTrimBoth(w: 'start' | 'end', e: MouseEvent) { e.preventDefault(); e.stopPropagation(); pushUndo(); const mv = (ev: MouseEvent) => { const s = pxToSec(ev); if (w === 'start') { const ns = Math.min(s, trimE.value - 0.3); trimS.value = ns; audioTrimS.value = ns } else { const ne = Math.max(s, trimS.value + 0.3); trimE.value = ne; audioTrimE.value = ne } }; const up = () => { document.removeEventListener('mousemove', mv); document.removeEventListener('mouseup', up); saveMeta() }; document.addEventListener('mousemove', mv); document.addEventListener('mouseup', up) }
function dragTrimVideo(w: 'start' | 'end', e: MouseEvent) { e.preventDefault(); e.stopPropagation(); pushUndo(); const mv = (ev: MouseEvent) => { const s = pxToSec(ev); if (w === 'start') trimS.value = Math.min(s, trimE.value - 0.3); else trimE.value = Math.max(s, trimS.value + 0.3) }; const up = () => { document.removeEventListener('mousemove', mv); document.removeEventListener('mouseup', up); saveMeta() }; document.addEventListener('mousemove', mv); document.addEventListener('mouseup', up) }
function dragTrimAudio(w: 'start' | 'end', e: MouseEvent) { e.preventDefault(); e.stopPropagation(); pushUndo(); const mv = (ev: MouseEvent) => { const s = pxToSec(ev); if (w === 'start') audioTrimS.value = Math.max(trimS.value, Math.min(s, audioTrimE.value - 0.3)); else audioTrimE.value = Math.min(trimE.value, Math.max(s, audioTrimS.value + 0.3)) }; const up = () => { document.removeEventListener('mousemove', mv); document.removeEventListener('mouseup', up); saveMeta() }; document.addEventListener('mousemove', mv); document.addEventListener('mouseup', up) }
function phDown(e: MouseEvent) { e.preventDefault(); scrubbing.value = true; seekTo(pxToSec(e)); document.addEventListener('mousemove', scrubMv); document.addEventListener('mouseup', phUp) }
function scrubMv(e: MouseEvent) { if (scrubbing.value) seekTo(pxToSec(e)) }
function phUp() { scrubbing.value = false; document.removeEventListener('mousemove', scrubMv); document.removeEventListener('mouseup', phUp) }
function pct(v: number) { return `${(v / dur.value * 100).toFixed(3)}%` }


// Splitter
function splitterDown(e: MouseEvent) { e.preventDefault(); const sy = e.clientY; const sp = previewFlex.value; const st = tlFlex.value; const mv = (ev: MouseEvent) => { const d = ev.clientY - sy; const r = d / 200; previewFlex.value = Math.max(1, sp + r); tlFlex.value = Math.max(0.5, st - r) }; const up = () => { document.removeEventListener('mousemove', mv); document.removeEventListener('mouseup', up) }; document.addEventListener('mousemove', mv); document.addEventListener('mouseup', up) }

// E4: Sidebar resize
function sideResizeDown(e: MouseEvent) {
  e.preventDefault(); const startX = e.clientX; const startW = sideWidth.value
  const mv = (ev: MouseEvent) => { sideWidth.value = Math.max(140, Math.min(400, startW - (ev.clientX - startX))) }
  const up = () => { document.removeEventListener('mousemove', mv); document.removeEventListener('mouseup', up) }
  document.addEventListener('mousemove', mv); document.addEventListener('mouseup', up)
}

// ── Init ──
function getCaptureTrackName(index: number, embeddedTitle?: string): string {
  // Priority 1: the per-recording EMBEDDED track title (baked in at record time from the
  // actual capture source via remux_with_track_titles). This is correct across machines and
  // for old recordings, where global trackDefs (positional) would mismatch.
  const title = embeddedTitle?.trim()
  if (title) return title
  // Priority 2: trackDefs name (Settings → Timeline Tracks)
  const def = getTrackDef(`A${index + 1}`)
  if (def?.name?.trim()) return def.name.trim()
  // Priority 3: captureTracks override name
  const ct = persist.state.settings.captureTracks?.[index]
  if (ct?.name?.trim()) return ct.name.trim()
  return `Audio ${index + 1}`
}

function initTracks() {
  const t: Track[] = []
  t.push({ id: 'V1', label: getTrackName('V1'), type: 'video', color: getTrackColor('V1'), volume: 100, muted: false, streamIndex: 0, peaks: [], volOpen: false })
  const audioStreams = info.value?.streams.filter(s => s.codec_type === 'audio') || []
  audioStreams.forEach((s, i) => {
    const id = `A${i + 1}`
    t.push({ id, label: getCaptureTrackName(i, s.title), type: 'audio', color: getTrackColor(id), volume: 100, muted: false, streamIndex: s.index, peaks: [], volOpen: false, title: s.title })
  })
  if (!audioStreams.length) t.push({ id: 'A1', label: getCaptureTrackName(0), type: 'audio', color: getTrackColor('A1'), volume: 100, muted: false, streamIndex: 1, peaks: [], volOpen: false })
  tracks.value = t
}

// Live-update colors + names when Settings → Track Defs or Capture Tracks change
watch(() => persist.state.settings.trackDefs, () => {
  tracks.value.forEach(t => {
    t.color = getTrackColor(t.id)
    // Update labels for all track types — trackDefs is the authoritative name source
    if (t.type === 'audio') {
      const i = parseInt(t.id.slice(1), 10) - 1
      t.label = getCaptureTrackName(i, t.title)
    } else {
      t.label = getTrackName(t.id)
    }
  })
}, { deep: true })
watch(() => persist.state.settings.captureTracks, () => {
  tracks.value.forEach((t, _) => {
    if (t.type !== 'audio') return
    const i = parseInt(t.id.slice(1), 10) - 1
    t.label = getCaptureTrackName(i, t.title)
  })
}, { deep: true })

async function loadWaveforms() { for (const t of tracks.value) { if (t.type !== 'audio') continue; try { t.peaks = await invoke<number[]>('generate_waveform', { filepath: props.clip.filepath, streamIndex: t.streamIndex, numPeaks: 200 }) } catch { t.peaks = Array(200).fill(0.3) } } }
onMounted(async () => {
  try {
    info.value = await invoke<MInfo>('analyze_media', { filepath: props.clip.filepath })
    if (info.value.duration > 0) dur.value = info.value.duration
  } catch {}
  initTracks()
  await loadMeta()
  await loadWaveforms()
  await refreshAudioRouting()  // This calls initAudioElements() and applyAudioVolumes()
  applyAudioVolumes()  // Ensure master volume is applied to all elements at mount
})

// Persistence
// ★ E2-P4: Persistence with game_tag
async function saveMeta() {
  try {
    await invoke('save_trim_state', { filepath: props.clip.filepath, trimStart: trimS.value, trimEnd: trimE.value })
    window.dispatchEvent(new CustomEvent('clip-trim-updated', { detail: { filepath: props.clip.filepath, trimStart: trimS.value, trimEnd: trimE.value } }))
  } catch {}
  try {
    const fallbackTitle = clipDisplayTitle('', gameTag.value || props.clip.game || '', props.clip.filename)
    const customName = editName.value.trim() && editName.value.trim() !== fallbackTitle ? editName.value.trim() : ''
    await invoke('set_clip_meta', { update: { filepath: props.clip.filepath, custom_name: customName, favorite: props.clip.favorite, game_tag: gameTag.value } })
    replay.updateClipMeta(props.clip.filepath, { custom_name: customName, game: gameTag.value, favorite: props.clip.favorite })
  } catch {}
}
async function loadMeta() {
  try { const s = await invoke<{ trim_start: number; trim_end: number } | null>('get_trim_state', { filepath: props.clip.filepath }); if (s && s.trim_end > 0) { trimS.value = s.trim_start; trimE.value = s.trim_end; audioTrimS.value = s.trim_start; audioTrimE.value = s.trim_end } } catch {}
  try {
    const m = await invoke<string>('get_clip_meta', { filepath: props.clip.filepath })
    if (m && m !== 'null') {
      const d = JSON.parse(m)
      if (d.game_tag) gameTag.value = d.game_tag
    }
  } catch {}
}

// ★ E3-P5: Screenshot — respects the user-configured save directory
async function takeScreenshot() {
  try {
    const outputDir = persist.state.settings.screenshotDirs?.[0] || ''
    const path = await invoke<string>('take_screenshot', {
      filepath: props.clip.filepath,
      timeSec: ct.value,
      outputDir,
    })
    showToast(t('clips.toast.screenshotSaved', { file: path.split('/').pop() }), path)
  } catch (e: any) { showToast(t('clips.toast.screenshotFailed', { error: String(e) })) }
}

/** Returns true when a KeyboardEvent matches a shortcut string like "Alt+F12" or "Ctrl+Shift+S". */
function shortcutMatches(e: KeyboardEvent, combo: string): boolean {
  const parts = combo.split('+').map(p => p.trim().toLowerCase())
  const key = parts[parts.length - 1]
  const needCtrl  = parts.includes('ctrl')
  const needShift = parts.includes('shift')
  const needAlt   = parts.includes('alt')
  const evKey = e.key.toLowerCase()
  const evCode = e.code.toLowerCase().replace('key', '').replace('digit', '')
  return (e.ctrlKey === needCtrl) && (e.shiftKey === needShift) && (e.altKey === needAlt)
      && (evKey === key || evCode === key)
}

// Click-outside for track volume popovers
function onDocClick(e: MouseEvent) {
  tracks.value.forEach(t => { if (!(e.target as HTMLElement).closest('.vol-zone')) t.volOpen = false })
}
onMounted(() => document.addEventListener('mousedown', onDocClick)); onBeforeUnmount(() => document.removeEventListener('mousedown', onDocClick))
function selectGame(g: string) { gameTag.value = g; saveMeta() }

// Export
async function openExport() { exportModal.value = true; exportName.value = editName.value || 'export'; exportDir.value = props.clip.filepath.replace(/\/[^/]+$/, ''); exportTarget.value = 0; exportProgress.value = 0; exportSpeed.value = ''; exportStage.value = ''; exportCodec.value = 'libx264'; exportAdvancedOpen.value = false; await updateProj() }
const codecDisplayName: Record<string, string> = { libx264: 'H.264', libx265: 'H.265', 'libvpx-vp9': 'VP9', libsvtav1: 'AV1', copy: 'Original' }
async function updateProj() { const cname = codecDisplayName[exportCodec.value] || 'H.264'; if (exportTarget.value <= 0) { exportSettings.value = `Stream copy • ${cname} • ${info.value?.width || 1920}x${info.value?.height || 1080}`; return } try { const j = await invoke<string>('calc_export_settings', { durationSec: trimDur.value, targetMb: exportTarget.value, width: info.value?.width || 1920, height: info.value?.height || 1080 }); const s = JSON.parse(j); exportSettings.value = `${s.resolution} • ${s.video_bitrate_kbps}kbps video • ${s.audio_bitrate_kbps}kbps audio • ${cname}` } catch { exportSettings.value = '...' } }
watch([exportTarget, exportCodec], updateProj)
const exportResult = ref('')

async function pickExportDir() {
  const dir = await openDialog({ defaultPath: exportDir.value, directory: true, multiple: false, title: t('editor.selectExportDir') })
  if (dir) exportDir.value = dir as string
}

async function openFolder() {
  if (!exportResult.value) return
  try {
    await invoke('open_file_location', { filepath: exportResult.value })
  } catch (e) {
    showToast(t('clips.toast.openFolderFailed', { error: String(e) }))
  }
}

async function doExport() {
  exporting.value = true; exportProgress.value = 0; exportResult.value = ''; exportStage.value = ''
  progressUnsub = await listen<{ percent: number; speed: string; stage: string }>('export-progress', e => {
    if (e.payload.percent >= 0) exportProgress.value = e.payload.percent
    if (e.payload.speed) exportSpeed.value = e.payload.speed
    if (e.payload.stage) exportStage.value = e.payload.stage
  })
  try {
    const p = `${exportDir.value}/${exportName.value.replace(/[<>:"/\\|?*\x00]/g, '').trim()}.mp4`
    const audioTracks = tracks.value.filter(t => t.type === 'audio').map(t => ({
      // A 0% track is treated as muted so the backend filter drops it entirely (true
      // silence) — otherwise float dust below 1% leaks through as faint audio.
      stream_index: t.streamIndex, volume: t.volume / 100, muted: t.muted || t.volume <= 0,
    }))
    const hasFilters = audioTracks.some(t => t.muted || t.volume < 1)
    let out: string
    if (hasFilters) {
      out = await invoke<string>('export_clip_with_filters', {
        inputPath: props.clip.filepath, startSec: trimS.value, endSec: trimE.value,
        audioTracks, overlays: [], targetMb: exportTarget.value, outputPath: p,
        codec: exportCodec.value,
        audioStartSec: audioTrimS.value,
        audioEndSec: audioTrimE.value,
      })
    } else {
      out = await invoke<string>('export_with_progress', {
        inputPath: props.clip.filepath, startSec: trimS.value, endSec: trimE.value,
        targetMb: exportTarget.value, outputPath: p,
        codec: exportCodec.value,
      })
    }
    // ★ Epic 3: Stay open on success — show drag zone
    exportResult.value = out
    emit('saved', out)
  } catch (e) {
    showToast(t('clips.toast.exportFailed', { error: String(e) }))
    exportModal.value = false
  } finally {
    exporting.value = false
    progressUnsub?.(); progressUnsub = null
  }
}
function closeExportModal() { exportModal.value = false; exportResult.value = ''; exportStage.value = '' }

// ★ Epic 3: Cancel running export
async function cancelExport() {
  try { await invoke('cancel_export') } catch {}
  exporting.value = false
  exportProgress.value = 0
  exportSpeed.value = ''
  exportStage.value = ''
  showToast(t('clips.toast.exportCancelled'))
}
function onToastDrag(e: DragEvent) { if (e.dataTransfer && toastFile.value) { e.dataTransfer.setData('text/uri-list', `file://${toastFile.value}`); e.dataTransfer.effectAllowed = 'copy' } }
function startFileDrag(e: DragEvent) {
  if (!exportResult.value) return
  e.dataTransfer!.setData('text/uri-list', `file://${exportResult.value}`)
  e.dataTransfer!.setData('text/plain', exportResult.value)
  e.dataTransfer!.effectAllowed = 'copy'
}

// Hotkeys — all configurable shortcuts read from settings store dynamically
function onKey(e: KeyboardEvent) {
  if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return
  const sc = persist.state.settings.shortcuts
  if (e.code === 'Space') { e.preventDefault(); togglePlay() }
  else if (e.code === 'ArrowRight') { e.preventDefault(); skip(SKIP) }
  else if (e.code === 'ArrowLeft')  { e.preventDefault(); skip(-SKIP) }
  else if (e.code === 'ArrowUp')    { e.preventDefault(); setPreviewVol(Math.min(1, localVol.value + 0.1)) }
  else if (e.code === 'ArrowDown')  { e.preventDefault(); setPreviewVol(Math.max(0, localVol.value - 0.1)) }
  else if (e.code === 'Escape') { if (exportModal.value) closeExportModal(); else emit('close') }
  else if (shortcutMatches(e, sc.undo))       { e.preventDefault(); undo() }
  else if (shortcutMatches(e, sc.redo))       { e.preventDefault(); redo() }
  else if (shortcutMatches(e, sc.screenshot)) { e.preventDefault(); takeScreenshot() }
  else if (shortcutMatches(e, sc.exportClip)) { e.preventDefault(); openExport() }
  else if (shortcutMatches(e, sc.splitClip))  { e.preventDefault(); pushUndo(); trimE.value = Math.max(ct.value, trimS.value + 0.1); saveMeta() }
  else if (e.code === 'KeyM' && hoveredTrackId.value) {
    const t = tracks.value.find(t => t.id === hoveredTrackId.value)
    if (t) { t.muted = !t.muted; applyAudioVolumes() }
  }
  else if (shortcutMatches(e, sc.toggleMic))  {
    e.preventDefault()
    if (hoveredTrackId.value) {
      const t = tracks.value.find(t => t.id === hoveredTrackId.value)
      if (t) { t.muted = !t.muted; applyAudioVolumes() }
    } else if (videoRef.value && !hasMultipleAudioTracks.value) {
      videoRef.value.muted = !videoRef.value.muted
    }
  }
}
function togglePreviewFullscreen() { playerComp.value?.toggleFullscreen() }
onMounted(() => {
  document.addEventListener('keydown', onKey)
  refreshAudioRouting().catch(() => {})
})
onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKey)
})

function fmt(s: number) { return `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}.${Math.floor((s % 1) * 10)}` }
function setPreviewVol(v: number) {
  localVol.value = Math.max(0, Math.min(1, v))  // Clamp 0..1
  applyAudioVolumes()
}
function drawWave(canvas: HTMLCanvasElement | null, t: Track) { if (!canvas || !t.peaks.length) return; const ctx = canvas.getContext('2d'); if (!ctx) return; const w = canvas.clientWidth; const h = canvas.clientHeight; canvas.width = w; canvas.height = h; ctx.clearRect(0, 0, w, h); const bw = w / t.peaks.length; const color = t.muted ? '#555' : t.color; const grad = ctx.createLinearGradient(0, 0, 0, h); grad.addColorStop(0, color + '80'); grad.addColorStop(0.5, color + 'DD'); grad.addColorStop(1, color + '80'); ctx.fillStyle = grad; for (let i = 0; i < t.peaks.length; i++) { const bh = Math.max(2, t.peaks[i] * h * 1.6); ctx.fillRect(i * bw, (h - bh) / 2, Math.max(1, bw - 0.5), bh) } }

</script>

<template>
<div class="editor">
  <!-- ═══ Top Bar ═══ -->
  <div class="bar">
    <button class="btn" @click="emit('close')"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="ic"><polyline points="15 18 9 12 15 6"/></svg></button>
    <input v-model="editName" class="name-in" spellcheck="false" @blur="saveMeta" @keydown.enter="($event.target as HTMLInputElement).blur()" />
    <GameTagDropdown
      v-model="gameTag"
      :games="GAMES"
      :show-custom="true"
      :max-items="10"
      @select="selectGame"
    />
    <div style="flex:1"></div>
    <span v-if="info" class="tag">{{ info.video_codec }}</span>
    <span v-if="info" class="tag">{{ info.width }}×{{ info.height }}</span>
    <span v-if="info" class="tag">{{ info.fps.toFixed(0) }}fps</span>
    <span v-if="undoStack.length" class="tag">↩{{ undoStack.length }}</span>
    <button class="btn accent" :disabled="exporting" @click="openExport">{{ t('editor.exportClip') }}</button>
  </div>

  <!-- ═══ Main: Preview + Sidebar ═══ -->
  <div class="main" :style="{ flex: previewFlex }">
    <div class="preview">
      <CustomVideoPlayer
        ref="playerComp"
        :src="videoSrc"
        :muted="hasMultipleAudioTracks"
        :master-volume="localVol"
        :show-controls="false"
        :capture-keyboard="false"
        @loadedmetadata="onMeta"
        @play="onVideoPlay"
        @pause="playing = false"
        @volume-change="setPreviewVol"
        @ended="playing = false"
      >
        <div v-if="!playing" class="play-ov" @click.stop="togglePlay"><svg viewBox="0 0 24 24" fill="currentColor" style="width:40px;height:40px;color:#fff;opacity:.7"><polygon points="5 3 19 12 5 21"/></svg></div>
      </CustomVideoPlayer>
    </div>

    <!-- ═══ E4: Resizable Sidebar ═══ -->
    <div class="sidebar" data-tour="editor-filters" :class="{ shut: !sideOpen }" :style="sideOpen ? { width: sideWidth + 'px' } : {}">
      <div v-if="sideOpen" class="side-resize" @mousedown="sideResizeDown"></div>
      <button class="side-toggle" @click="sideOpen = !sideOpen">{{ sideOpen ? '›' : '‹' }}</button>
      <div v-if="sideOpen" class="side-inner">
        <div class="side-tabs">
          <button :class="{ active: sideTab === 'info' }" :style="sideTab === 'info' ? { backgroundColor: 'color-mix(in srgb, var(--accent) 20%, transparent)' } : {}" @click="sideTab = 'info'">Info</button>
        </div>

        <!-- Info tab -->
        <div v-if="sideTab === 'info'" class="tab-content">
          <div class="info-path">{{ clip.filepath }}</div>
          <div class="info-row"><span>Duration</span><span>{{ dur.toFixed(1) }}s</span></div>
          <div class="info-row"><span>Resolution</span><span>{{ info?.width }}×{{ info?.height }}</span></div>
          <div class="info-row"><span>FPS</span><span>{{ info?.fps?.toFixed(1) }}</span></div>
          <div class="info-row"><span>Codec</span><span>{{ info?.video_codec }}</span></div>
          <div class="info-row"><span>Audio</span><span>{{ info?.audio_streams || 1 }} tracks</span></div>
          <div class="info-row"><span>Trim</span><span>{{ fmt(trimS) }} → {{ fmt(trimE) }}</span></div>
          <div class="info-row"><span>Output</span><span>{{ trimDur.toFixed(1) }}s</span></div>
          <div class="keys-section">
            <div class="key-row"><kbd>Space</kbd><span>Play/Pause</span></div>
            <div class="key-row"><kbd>← →</kbd><span>Skip ±{{ SKIP }}s</span></div>
            <div class="key-row"><kbd>⌘Z</kbd><span>Undo trim</span></div>
          </div>
        </div>

        <!-- Track colors: Moved to global Settings page (SettingsPage.vue)
             Implementation: Store in ~/.config/opengg/theme.json under
             "trackColors": { "V1": "#3B82F6", "A1": "#10B981", ... }
             Load via loadTheme() and apply via CSS vars -->
      </div>
    </div>
  </div>

  <!-- ═══ Splitter ═══ -->
  <div class="splitter" @mousedown="splitterDown"><div class="splitter-grip"></div></div>

  <!-- ═══ Transport ═══ -->
  <div class="transport">
    <button class="tr-btn" @click="skip(-SKIP)"><svg viewBox="0 0 24 24" fill="currentColor" class="ic"><polygon points="11 19 2 12 11 5"/><polygon points="22 19 13 12 22 5"/></svg></button>
    <button class="tr-btn play-btn" @click="togglePlay">
      <svg v-if="playing" viewBox="0 0 24 24" fill="currentColor" style="width:18px;height:18px"><rect x="6" y="4" width="4" height="16" rx="1"/><rect x="14" y="4" width="4" height="16" rx="1"/></svg>
      <svg v-else viewBox="0 0 24 24" fill="currentColor" style="width:18px;height:18px"><polygon points="5 3 19 12 5 21"/></svg>
    </button>
    <button class="tr-btn" @click="skip(SKIP)"><svg viewBox="0 0 24 24" fill="currentColor" class="ic"><polygon points="13 19 22 12 13 5"/><polygon points="2 19 11 12 2 5"/></svg></button>
    <span class="tr-time dim">{{ fmt(trimS) }}</span>
    <span class="tr-sep">│</span>
    <span class="tr-time cur">{{ fmt(ct) }}</span>
    <span class="tr-sep">│</span>
    <span class="tr-time dim">{{ fmt(trimE) }}</span>
    <div style="flex:1"></div>
    <!-- ★ Epic 2: Local monitor volume (preview only — not exported) -->
    <div class="vol-monitor" title="Monitor volume (preview only)">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="ic" style="opacity:.5">
        <polygon points="11 5 6 9 2 9 2 15 6 15 11 19"/>
        <path v-if="localVol > 0" d="M15.54 8.46a5 5 0 010 7.07"/>
        <line v-else x1="23" y1="9" x2="17" y2="15"/><line v-if="localVol === 0" x1="17" y1="9" x2="23" y2="15"/>
      </svg>
      <VolumeSlider
        :model-value="Math.round(localVol * 100)"
        color="var(--accent)"
        :min="0"
        :max="100"
        unit=""
        compact
        @update:model-value="v => localVol = v / 100"
      />
    </div>
    <!-- Magnet Mode toggle -->
    <button class="tr-btn" :class="{ active: magnetMode }" @click="magnetMode = !magnetMode" title="Magnet Mode — sync video/audio trim handles">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="ic">
        <path d="M6 15V9a6 6 0 0112 0v6"/>
        <path d="M6 9H3a3 3 0 000 6h3"/>
        <path d="M18 9h3a3 3 0 010 6h-3"/>
        <rect x="5" y="15" width="14" height="4" rx="1"/>
      </svg>
    </button>
    <!-- ★ E3-P5: Screenshot button -->
    <button class="tr-btn" @click="takeScreenshot" title="Screenshot (save to Pictures)">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="ic"><path d="M23 19a2 2 0 01-2 2H3a2 2 0 01-2-2V8a2 2 0 012-2h4l2-3h6l2 3h4a2 2 0 012 2z"/><circle cx="12" cy="13" r="4"/></svg>
    </button>
    <!-- ★ Epic 4: Fullscreen button -->
    <button class="tr-btn" @click="togglePreviewFullscreen()" title="Fullscreen (click again to exit)">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="ic"><path d="M8 3H5a2 2 0 00-2 2v3m18 0V5a2 2 0 00-2-2h-3m0 18h3a2 2 0 002-2v-3M3 16v3a2 2 0 002 2h3"/></svg>
    </button>
    <span class="tr-dur">{{ fmt(trimDur) }}</span>
  </div>

  <!-- ═══ E1: Timeline with PROPER row layout ═══ -->
  <div class="tl-wrap" data-tour="editor-timeline" :style="{ flex: tlFlex }">
    <div class="tl-container">
      <!--  Left: Track Headers (fixed width, gap from canvas) -->
      <div class="tl-headers">
        <TimelineTrackRow
          v-for="t in tracks"
          :key="'h' + t.id"
          part="header"
          :color="t.color"
          :label="t.label"
          :kind-label="trackKindLabel(t)"
          :icon="getTrackDef(t.id)?.icon"
          :show-icon="showIcons(t.id)"
          @mouseenter="hoveredTrackId = t.id"
          @mouseleave="hoveredTrackId = null"
        >
          <template v-if="t.type === 'audio'" #actions>
            <div class="vol-zone" style="margin-left:auto">
              <button class="hdr-speaker" :class="{ off: t.muted }" @click.stop="t.volOpen = !t.volOpen; tracks.filter(x => x.id !== t.id).forEach(x => x.volOpen = false)">
                <svg v-if="t.muted" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" class="ic-s"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>
                <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" class="ic-s"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19"/><path d="M15.54 8.46a5 5 0 010 7.07"/></svg>
              </button>
              <div v-if="t.volOpen" class="vol-popover no-seek" @click.stop>
                <VolumeSlider
                  :model-value="t.volume"
                  :color="t.color"
                  :min="0"
                  :max="100"
                  @update:model-value="v => t.volume = v"
                />
              </div>
            </div>
          </template>
        </TimelineTrackRow>
      </div>

      <!-- Right: Timeline Canvas — each track is a FIXED HEIGHT ROW -->
      <div class="tl-canvas" ref="tlRef" @click="tlClick">
        <!-- Dim zones outside trim -->
        <div class="dim-left" :style="{ width: pct(trimS) }"></div>
        <div class="dim-right" :style="{ width: pct(dur - trimE) }"></div>

        <!-- E1: Each track is a distinct fixed-height row -->
        <div class="tl-rows">
          <TimelineTrackRow
            v-for="t in tracks"
            :key="'r' + t.id"
            part="body"
            :color="t.color"
            @mouseenter="hoveredTrackId = t.id"
            @mouseleave="hoveredTrackId = null"
          >
            <!-- Audio track → waveform -->
            <canvas v-if="t.type === 'audio' && t.peaks.length" :ref="el => drawWave(el as HTMLCanvasElement, t)" class="waveform"></canvas>
          </TimelineTrackRow>
        </div>

        <!-- Trim handles — Magnet ON: unified handles span all tracks -->
        <template v-if="magnetMode">
          <div class="trim-handle unified" :style="{ left: pct(trimS), height: tracks.length * 36 + 'px' }" @mousedown="dragTrimBoth('start', $event)"><div class="trim-grip">‹</div></div>
          <div class="trim-handle unified" :style="{ left: pct(trimE), height: tracks.length * 36 + 'px' }" @mousedown="dragTrimBoth('end', $event)"><div class="trim-grip">›</div></div>
        </template>
        <!-- Magnet OFF: per-track handles -->
        <template v-else>
          <div class="trim-handle" :style="{ left: pct(trimS) }" @mousedown="dragTrimVideo('start', $event)"><div class="trim-grip">‹</div></div>
          <div class="trim-handle" :style="{ left: pct(trimE) }" @mousedown="dragTrimVideo('end', $event)"><div class="trim-grip">›</div></div>
          <div class="trim-handle audio-handle" :style="{ left: pct(audioTrimS) }" @mousedown="dragTrimAudio('start', $event)"><div class="trim-grip audio">‹</div></div>
          <div class="trim-handle audio-handle" :style="{ left: pct(audioTrimE) }" @mousedown="dragTrimAudio('end', $event)"><div class="trim-grip audio">›</div></div>
        </template>

        <!-- Playhead -->
        <div class="playhead" :style="{ left: `calc(${pct(ct)} - 10px)` }" @mousedown="phDown">
          <div class="ph-line"></div>
          <div class="ph-dot"></div>
        </div>
      </div>
    </div>
  </div>

  <!-- ═══ Export Modal ═══ -->
  <Teleport to="body">
    <div v-if="exportModal" class="modal-overlay" @click.self="closeExportModal">
      <div class="modal-box">
        <!-- ★ Success state: draggable file row + open folder -->
        <template v-if="exportResult">
          <h2>✓ {{ t('editor.exportComplete') }}</h2>
          <div class="export-success-row">
            <div
              class="export-file-name"
              :title="t('editor.dragReady')"
              draggable="true"
              @dragstart="startFileDrag"
            >
              <svg class="file-drag-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/>
                <line x1="8.59" y1="13.51" x2="15.42" y2="17.49"/>
                <line x1="15.41" y1="6.51" x2="8.59" y2="10.49"/>
              </svg>
              <span class="file-name-text">{{ exportResult.split('/').pop() }}</span>
            </div>
            <button class="btn btn-icon" @click="openFolder" :title="t('editor.showInFolder')">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/>
              </svg>
            </button>
          </div>
          <div class="modal-actions">
            <button class="btn" @click="closeExportModal">{{ t('common.cancel') }}</button>
          </div>
        </template>
        <!-- Normal export form -->
        <template v-else>
        <h2>{{ t('editor.exportClip') }}</h2>
        <div class="modal-field">
          <label>{{ t('editor.filename') }}</label>
          <div class="dir-row">
            <input v-model="exportName" class="modal-input" />
            <button class="btn btn-icon" @click="pickExportDir" :title="t('editor.changeFolder')">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/>
              </svg>
            </button>
          </div>
        </div>
        <div class="modal-field">
          <label>{{ t('editor.directory') }}</label>
          <div class="dir-row">
            <input v-model="exportDir" class="modal-input" readonly />
            <button class="btn btn-icon" @click="pickExportDir" :title="t('editor.selectExportDir')">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/>
              </svg>
            </button>
          </div>
        </div>
        <div class="modal-field">
          <label>{{ t('editor.targetSize') }}</label>
          <div class="radio-row">
            <label v-for="v in [0, 100, 50, 10]" :key="v" :class="{ active: exportTarget === v }">
              <input type="radio" v-model.number="exportTarget" :value="v" />{{ v === 0 ? t('editor.original') : `${v}MB` }}
            </label>
          </div>
        </div>
        <div class="modal-field">
          <button class="adv-toggle" @click="exportAdvancedOpen = !exportAdvancedOpen">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" :class="{ rotated: exportAdvancedOpen }">
              <polyline points="9 18 15 12 9 6"/>
            </svg>
            {{ t('editor.advancedSettings') }}
          </button>
          <div v-if="exportAdvancedOpen" class="adv-panel">
            <div class="adv-row">
              <span class="adv-label">{{ t('editor.codec') }}</span>
              <div class="radio-row">
                <label v-for="c in codecOptions" :key="c.value" :class="{ active: exportCodec === c.value }">
                  <input type="radio" v-model="exportCodec" :value="c.value" />{{ c.label }}
                </label>
              </div>
            </div>
          </div>
        </div>
        <div class="proj-box">{{ exportSettings }}</div>
        <div v-if="exporting" class="progress-bar-wrap">
          <div class="progress-track"><div class="progress-fill" :style="{ width: exportProgress + '%' }"></div></div>
          <span class="progress-text">{{ exportStage === 'copying' ? 'Copying...' : `${exportProgress.toFixed(0)}%` }} {{ exportSpeed ? `(${exportSpeed})` : '' }}</span>
        </div>
        <div class="modal-actions">
          <button v-if="exporting" class="btn btn-cancel" @click="cancelExport">✕ {{ t('editor.cancelExport') }}</button>
          <button v-else class="btn" @click="closeExportModal">{{ t('common.cancel') }}</button>
          <button class="btn accent" @click="doExport" :disabled="exporting">{{ exporting ? t('editor.exporting') : t('editor.exportClip') }}</button>
        </div>
        </template>
      </div>
    </div>
  </Teleport>

  <!-- Toast -->
  <Transition name="toast-anim">
    <div v-if="toast" class="toast-box">
      <span>{{ toast }}</span>
      <div v-if="toastFile" class="toast-drag" draggable="true" @dragstart="onToastDrag">📎 Drag to share</div>
    </div>
  </Transition>
</div>
</template>

<style scoped>
/* ═══ Layout Shell ═══ */
.editor { position: fixed; top: var(--titlebar-h, 40px); left: 0; right: 0; bottom: 0; z-index: 900; background: var(--bg-surface); display: flex; flex-direction: column; }
.ic { width: 14px; height: 14px; }
.ic-s { width: 12px; height: 12px; }

/* ═══ Top Bar ═══ */
.bar { display: flex; align-items: center; gap: 8px; padding: 5px 12px; border-bottom: 1px solid var(--border); background: var(--bg-card); flex-shrink: 0; }
.btn { padding: 4px 10px; border: 1px solid var(--border); border-radius: 5px; background: var(--bg-card); color: var(--text-sec); font-size: 11px; cursor: pointer; display: flex; align-items: center; gap: 3px; }
.btn:hover { background: var(--bg-hover); }
.btn:disabled { opacity: .4; }
.accent { background: var(--accent); border-color: var(--accent); color: #fff; }
.name-in { flex: 0 1 200px; padding: 3px 8px; background: var(--bg-input); border: 1px solid var(--border); border-radius: 5px; color: var(--text); font-size: 12px; font-weight: 700; outline: none; }
.name-in:focus { border-color: var(--accent); }
/* Match game tag input height to title input */
:deep(.gtd-input) { padding: 3px 8px; font-size: 12px; }
.tag { font-size: 8px; background: var(--bg-deep); border: 1px solid var(--border); padding: 1px 5px; border-radius: 3px; color: var(--text-muted); }

/* ═══ Main Area ═══ */
.main { display: flex; min-height: 0; min-width: 0; }
.preview { flex: 1; background: #000; position: relative; display: flex; align-items: center; justify-content: center; cursor: pointer; min-width: 0; min-height: 0; overflow: hidden; }
.vid { max-width: 100%; max-height: 100%; object-fit: contain; }
.play-ov { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; background: rgba(0,0,0,.15); }
/* ★ Epic 4: Fullscreen fix — container fills top layer, video letterboxes cleanly */
.preview:fullscreen,
.preview:-webkit-full-screen {
  width: 100vw !important;
  height: 100vh !important;
  aspect-ratio: auto !important;
  display: flex !important;
  align-items: center;
  justify-content: center;
  background: #000;
  position: relative;
}
.preview:fullscreen .vid,
.preview:-webkit-full-screen .vid {
  max-width: 100%;
  max-height: 100%;
  width: auto;
  height: auto;
  object-fit: contain;
}
.preview:fullscreen .play-ov,
.preview:-webkit-full-screen .play-ov {
  position: absolute;
  inset: 0;
}

/* ═══ E4: Resizable Sidebar ═══ */
.sidebar { border-left: 1px solid var(--border); background: var(--bg-card); flex-shrink: 0; position: relative; transition: width .15s; }
.sidebar.shut { width: 20px !important; }
.side-resize { position: absolute; left: -3px; top: 0; bottom: 0; width: 6px; cursor: ew-resize; z-index: 6; }
.side-resize:hover { background: var(--accent); opacity: .3; }
.side-toggle { position: absolute; left: -11px; top: 50%; transform: translateY(-50%); width: 22px; height: 44px; border: 1px solid var(--border); border-radius: 5px; background: var(--bg-card); color: var(--text-muted); font-size: 13px; cursor: pointer; z-index: 5; display: flex; align-items: center; justify-content: center; }
.side-inner { padding: 10px; overflow-y: auto; height: 100%; font-size: 10px; }
.side-tabs { display: flex; border-bottom: 1px solid var(--border); margin-bottom: 8px; }
.side-tabs button { flex: 1; padding: 5px 2px; border: none; background: transparent; color: var(--text-muted); font-size: 8px; font-weight: 700; cursor: pointer; border-bottom: 2px solid transparent; text-transform: uppercase; letter-spacing: .5px; }
.side-tabs button.active { color: var(--accent); border-bottom-color: var(--accent); }
.tab-badge { margin-left: 2px; background: var(--accent); color: #fff; font-size: 7px; padding: 0 4px; border-radius: 8px; }
.tab-content { }

/* Info tab */
.info-path { font-size: 8px; color: var(--text-muted); word-break: break-all; margin-bottom: 6px; padding: 4px; background: var(--bg-deep); border-radius: 3px; }
.info-row { display: flex; justify-content: space-between; color: var(--text-sec); padding: 2px 0; font-size: 10px; }
.keys-section { margin-top: 10px; border-top: 1px solid var(--border); padding-top: 6px; }
.key-row { display: flex; align-items: center; gap: 6px; font-size: 9px; color: var(--text-muted); padding: 1px 0; }
kbd { padding: 1px 4px; background: var(--bg-deep); border: 1px solid var(--border); border-radius: 2px; font-family: monospace; font-size: 7px; }

/* E6: Colors tab */
.color-row { display: flex; align-items: center; gap: 6px; padding: 3px 0; }
.color-label { font-size: 10px; font-weight: 700; color: var(--text-sec); min-width: 24px; }
.color-picker { width: 24px; height: 20px; border: 1px solid var(--border); border-radius: 3px; cursor: pointer; padding: 0; background: none; }
.color-hex { font-size: 9px; color: var(--text-muted); font-family: monospace; }

/* ═══ Splitter ═══ */
.splitter { height: 5px; background: var(--border); cursor: ns-resize; flex-shrink: 0; display: flex; align-items: center; justify-content: center; }
.splitter:hover { background: var(--accent); }
.splitter-grip { width: 30px; height: 2px; background: var(--text-muted); border-radius: 1px; opacity: .4; }

/* ═══ Transport ═══ */
.transport { display: flex; align-items: center; gap: 6px; padding: 4px 12px; border-top: 1px solid var(--border); background: var(--bg-card); flex-shrink: 0; }
.tr-btn { width: 28px; height: 28px; border: 1px solid var(--border); border-radius: 5px; background: var(--bg-card); color: var(--text-sec); cursor: pointer; display: flex; align-items: center; justify-content: center; }
.tr-btn:hover { background: var(--bg-hover); }
.tr-btn.active { border-color: var(--accent); color: var(--accent); background: color-mix(in srgb, var(--accent) 10%, transparent); }
.play-btn { width: 34px; height: 34px; background: var(--accent); border-color: var(--accent); color: #fff; }
.tr-time { font-size: 11px; font-variant-numeric: tabular-nums; }
.tr-time.dim { color: var(--text-sec); }
.tr-time.cur { font-weight: 700; color: var(--text); }
.tr-sep { color: var(--text-muted); font-size: 9px; }
.tr-dur { font-size: 11px; color: var(--accent); font-weight: 600; }

/* ★ Epic 2: Local monitor volume slider */
.vol-monitor { display: flex; align-items: center; gap: 4px; padding: 0 4px; }

/* ═══ E1: Timeline — FIXED HEIGHT ROWS ═══ */
.tl-wrap { min-height: 60px; overflow: hidden; flex-shrink: 0; }
.tl-container { display: flex; height: 100%; }

/* Track Headers — flush against canvas (no gap) */
.tl-headers { width: 110px; flex-shrink: 0; display: flex; flex-direction: column; background: var(--bg-deep); z-index: 4; }

/* Speaker icon on right edge */
.vol-zone { position: relative; }
.hdr-speaker { border: none; background: none; cursor: pointer; padding: 2px; color: var(--tc); opacity: .7; }
.hdr-speaker:hover { opacity: 1; }
.hdr-speaker.off { opacity: .3; }
.vol-popover { position: absolute; right: 0; top: 100%; margin-top: 4px; background: var(--bg-card); border: 1px solid var(--border); border-radius: 6px; padding: 6px 8px; display: flex; align-items: center; gap: 6px; box-shadow: 0 4px 12px rgba(0,0,0,.4); z-index: 20; white-space: nowrap; }

/* Timeline Canvas */
.tl-canvas { flex: 1; position: relative; overflow: hidden; cursor: crosshair; user-select: none; }
.dim-left, .dim-right { position: absolute; top: 0; bottom: 0; background: rgba(0,0,0,.45); z-index: 3; pointer-events: none; }
.dim-left { left: 0; }
.dim-right { right: 0; }

/* ★ E1: Each track row has FIXED height with visible borders */
.tl-rows { display: flex; flex-direction: column; height: 100%; }

/* Waveform canvas */
.waveform { width: 100%; height: 100%; display: block; }

/* Trim handles */
.trim-handle { position: absolute; top: 0; bottom: 0; width: 18px; transform: translateX(-9px); z-index: 6; cursor: ew-resize; display: flex; align-items: center; justify-content: center; }
.trim-handle.unified { cursor: ew-resize; }
.trim-grip { width: 14px; height: 30px; background: var(--accent); border-radius: 3px; display: flex; align-items: center; justify-content: center; font-size: 11px; color: #fff; font-weight: 700; box-shadow: 0 0 6px rgba(0,0,0,.3); transition: transform .1s; }
.trim-grip.audio { background: #10B981; }
.trim-handle:hover .trim-grip { transform: scaleX(1.15); }
.trim-handle.audio-handle { z-index: 7; }

/* Playhead */
.playhead { position: absolute; top: 0; bottom: 0; width: 20px; z-index: 5; cursor: col-resize; display: flex; flex-direction: column; align-items: center; }
.ph-line { width: 2px; flex: 1; background: #fff; box-shadow: 0 0 5px rgba(255,255,255,.4); pointer-events: none; }
.ph-dot { width: 8px; height: 8px; background: #fff; border-radius: 50%; margin-top: -2px; box-shadow: 0 0 4px rgba(255,255,255,.4); pointer-events: none; }

/* ═══ Export Modal ═══ */
.modal-overlay { position: fixed; inset: 0; z-index: 2000; background: rgba(0,0,0,.7); display: flex; align-items: center; justify-content: center; }
.modal-box { background: var(--bg-surface); border: 1px solid var(--border); border-radius: 12px; padding: 24px; width: 460px; }
.modal-box h2 { font-size: 16px; font-weight: 700; margin-bottom: 16px; }
.modal-field { margin-bottom: 14px; }
.modal-field label { display: block; font-size: 10px; font-weight: 600; color: var(--text-sec); margin-bottom: 4px; text-transform: uppercase; letter-spacing: .5px; }
.modal-input { width: 100%; padding: 7px 10px; background: var(--bg-input); border: 1px solid var(--border); border-radius: 5px; color: var(--text); font-size: 12px; outline: none; }
.modal-input:focus { border-color: var(--accent); }
.radio-row { display: flex; gap: 6px; flex-wrap: wrap; }
.radio-row label { padding: 5px 12px; border: 1px solid var(--border); border-radius: 5px; font-size: 11px; color: var(--text-sec); cursor: pointer; }
.radio-row label.active { border-color: var(--accent); background: var(--accent); color: #fff; }
.radio-row input { display: none; }
.proj-box { padding: 8px; background: var(--bg-deep); border: 1px solid var(--border); border-radius: 5px; font-size: 10px; color: var(--text-sec); font-family: monospace; margin-bottom: 14px; }
.progress-bar-wrap { margin-bottom: 14px; }
.progress-track { height: 8px; background: var(--bg-deep); border-radius: 4px; overflow: hidden; }
.progress-fill { height: 100%; background: var(--accent); transition: width .3s; border-radius: 4px; }
.progress-text { font-size: 10px; color: var(--text-sec); margin-top: 4px; display: block; }
.modal-actions { display: flex; gap: 8px; justify-content: flex-end; }
.btn-cancel { border-color: var(--danger); color: var(--danger); }
.btn-cancel:hover { background: color-mix(in srgb, var(--danger) 10%, transparent); }
.export-success-row {
  display: flex; align-items: center; gap: 8px;
  margin-bottom: 16px;
}
.export-success-row {
  display: flex; align-items: center; gap: 8px; margin-bottom: 16px;
}
.export-file-name {
  flex: 1; min-width: 0;
  display: flex; align-items: center; gap: 10px;
  padding: 10px 12px;
  background: var(--bg-deep); border: 1.5px solid var(--border);
  border-radius: 8px;
  cursor: grab;
  transition: border-color .15s, box-shadow .15s;
  overflow: hidden;
  -webkit-app-region: no-drag;
  user-drag: none;
}
.export-file-name:active { cursor: grabbing; }
.export-file-name:hover {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 15%, transparent);
}
.file-drag-icon { width: 16px; height: 16px; color: var(--accent); flex-shrink: 0; }
.file-name-text { flex: 1; font-size: 13px; font-weight: 600; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.export-success-row .btn-icon {
  width: 38px; height: 38px; border-radius: 8px;
  border: 1px solid var(--border); background: var(--bg-card);
  color: var(--text-sec); cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  transition: border-color .12s, color .12s, background .12s;
  flex-shrink: 0;
}
.export-success-row .btn-icon:hover {
  border-color: var(--accent); color: var(--accent);
  background: color-mix(in srgb, var(--accent) 8%, transparent);
}
.export-success-row .btn-icon svg { width: 16px; height: 16px; }
.dir-row {
  display: flex; gap: 8px; align-items: center;
}
.dir-row .modal-input { flex: 1; min-width: 0; }
.dir-row .btn-icon {
  width: 36px; height: 36px; border-radius: 6px;
  border: 1px solid var(--border); background: var(--bg-input);
  color: var(--text-muted); cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  transition: border-color .12s, color .12s, background .12s;
  flex-shrink: 0;
}
.dir-row .btn-icon:hover {
  border-color: var(--accent); color: var(--accent);
  background: color-mix(in srgb, var(--accent) 8%, transparent);
}
.dir-row .btn-icon svg { width: 15px; height: 15px; }
.adv-toggle {
  display: flex; align-items: center; gap: 6px;
  background: none; border: none;
  color: var(--text-sec); font-size: 11px; font-weight: 600;
  cursor: pointer; padding: 4px 0;
  transition: color .12s;
}
.adv-toggle:hover { color: var(--accent); }
.adv-toggle svg { width: 14px; height: 14px; transition: transform .2s; }
.adv-toggle svg.rotated { transform: rotate(90deg); }
.adv-panel {
  margin-top: 10px; padding: 12px;
  background: var(--bg-deep); border: 1px solid var(--border);
  border-radius: 8px;
}
.adv-row { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
.adv-label { font-size: 10px; font-weight: 600; color: var(--text-sec); text-transform: uppercase; letter-spacing: .5px; white-space: nowrap; }

/* Toast */
.toast-box { position: fixed; bottom: 14px; right: 14px; z-index: 9999; background: var(--bg-card); border: 1px solid var(--accent); padding: 10px 14px; border-radius: 8px; box-shadow: 0 4px 16px rgba(0,0,0,.4); font-size: 11px; font-weight: 600; display: flex; flex-direction: column; gap: 5px; color: var(--text); }
.toast-drag { padding: 4px 8px; background: var(--bg-deep); border: 1px dashed var(--accent); border-radius: 4px; text-align: center; cursor: grab; font-size: 9px; color: var(--accent); }
.toast-drag:active { cursor: grabbing; opacity: .7; }
.toast-anim-enter-active, .toast-anim-leave-active { transition: transform .3s, opacity .3s; }
.toast-anim-enter-from, .toast-anim-leave-to { transform: translateY(14px); opacity: 0; }
</style>

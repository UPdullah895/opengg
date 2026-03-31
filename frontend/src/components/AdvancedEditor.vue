<script setup lang="ts">
/**
 * OpenGG Clip Editor — Clean rewrite addressing 6 epics:
 *   E1: Fixed-height track rows with visible borders
 *   E2: Dedicated O1 overlay track with draggable overlay clips
 *   E3: Accordion overlays in sidebar with number inputs + tooltips
 *   E4: Resizable right sidebar via drag handle
 *   E5: Audio playback fix — don't mute on init
 *   E6: Dynamic track colors via CSS variables
 */
import { ref, computed, onMounted, onBeforeUnmount, watch, inject } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { mediaUrl } from '../utils/assets'
import type { Clip } from '../stores/replay'
import type { Ref } from 'vue'
import { usePersistenceStore } from '../stores/persistence'
import { resumeAudioContext } from '../utils/audio'
import CustomVideoPlayer from './CustomVideoPlayer.vue'

// ── Types ──
interface MInfo { duration: number; width: number; height: number; fps: number; video_codec: string; streams: { index: number; codec_type: string; title: string }[]; audio_streams: number }
interface Track { id: string; label: string; type: 'video' | 'audio' | 'overlay'; color: string; volume: number; muted: boolean; streamIndex: number; peaks: number[]; volOpen: boolean }
interface Overlay { id: string; type: 'text' | 'image' | 'gif'; content: string; x: number; y: number; scale: number; startSec: number; durSec: number; open: boolean; fontName?: string }

const props = defineProps<{ clip: Clip }>()
const emit = defineEmits<{ 'close': []; 'toast': [string]; 'saved': [string] }>()
const mediaPort = inject<Ref<number>>('mediaPort', ref(0))
const videoSrc = computed(() => mediaUrl(props.clip.filepath, mediaPort.value))
const persist = usePersistenceStore()

const SKIP = 5
const GAMES = ['Counter-Strike 2', 'League of Legends', 'Valorant', 'Overwatch 2', 'Apex Legends', 'Fortnite', 'Minecraft', 'Dota 2', 'Rocket League', 'Elden Ring', "Baldur's Gate 3", 'Cyberpunk 2077', 'Honkai: Star Rail', 'Genshin Impact', 'Helldivers 2', 'Path of Exile 2']

// ── Core State ──
const playerComp = ref<InstanceType<typeof CustomVideoPlayer> | null>(null)
const videoRef   = ref<HTMLVideoElement | null>(null)  // synced from playerComp in onMounted
const dur = ref(props.clip.duration || 30)

// ★ Epic 2: Local monitor volume (preview only — NOT exported) and hover-mute tracking
const localVol        = ref(1)
const hoveredTrackId  = ref<string | null>(null)
const isFullscreen    = ref(false)
const ct = ref(0)
const playing = ref(false)
const info = ref<MInfo | null>(null)
const trimS = ref(0)
const trimE = ref(dur.value)
const trimDur = computed(() => Math.max(0, trimE.value - trimS.value))
const editName = ref(props.clip.custom_name || props.clip.filename.replace(/\.[^.]+$/, ''))
const gameTag = ref('')
const gameOpen = ref(false)
const gameRef = ref<HTMLElement | null>(null)
const gameFiltered = computed(() => { const q = gameTag.value.toLowerCase(); return q ? GAMES.filter(g => g.toLowerCase().includes(q)) : GAMES })

// ── Tracks (E6: colors via CSS vars, sourced from persisted Settings) ──
const tracks = ref<Track[]>([])

const TRACK_FALLBACKS: Record<string, string> = {
  V1: '#E94560', A1: '#10B981', A2: '#3b82f6', A3: '#f59e0b', A4: '#8b5cf6', A5: '#ec4899', O1: '#F97316',
}

function getTrackDef(id: string) {
  return persist.state.settings.trackDefs?.find(d => d.id === id)
}
function getTrackColor(id: string): string {
  return getTrackDef(id)?.color ?? TRACK_FALLBACKS[id] ?? '#64748b'
}
function getTrackName(id: string): string {
  return getTrackDef(id)?.name ?? (id.startsWith('A') ? `Audio ${id.slice(1)}` : id)
}
function showIcons(): boolean {
  return persist.state.settings.showTrackIcons ?? true
}

// AudioContext resume is handled by the shared singleton in utils/audio.ts.
// installAudioUnlocker() in App.vue covers the global one-shot unlock.
// resumeAudioContext() is called immediately before every play() as a safeguard.

// ── Extensions feature flags ──
const extOverlays = computed(() => persist.state.extensions?.overlays ?? true)
const extTiktok   = computed(() => persist.state.extensions?.tiktokExport ?? false)

// ── Layout ──
const sideOpen = ref(true)
const sideWidth = ref(200) // E4: resizable
const previewFlex = ref(3)
const tlFlex = ref(1.2)
const undoStack = ref<{ s: number; e: number }[]>([])
const redoStack = ref<{ s: number; e: number }[]>([])
const sideTab = ref<'info' | 'overlay'>('info')

// ── Overlays (E2+E3) ──
const overlays = ref<Overlay[]>([])
const selectedOverlay = ref<string | null>(null)

const OVERLAY_FONTS = ['Noto Sans', 'Impact', 'Arial', 'Tahoma', 'DejaVu Sans', 'Liberation Sans', 'Ubuntu', 'Roboto']

function addOverlay(type: 'text' | 'image' | 'gif') {
  const o: Overlay = { id: `ov_${Date.now()}`, type, content: type === 'text' ? 'Your Text' : '', x: 50, y: 50, scale: 100, startSec: ct.value, durSec: Math.min(5, dur.value - ct.value), open: true, fontName: 'Noto Sans' }
  overlays.value.push(o)
  selectedOverlay.value = o.id
  sideTab.value = 'overlay'
  if (!sideOpen.value) sideOpen.value = true
}
function removeOverlay(id: string) { overlays.value = overlays.value.filter(o => o.id !== id); if (selectedOverlay.value === id) selectedOverlay.value = null }
function clearOverlays() { overlays.value = []; selectedOverlay.value = null }

// ★ Epic 3: Clean display name — extract filename from full URL/path
function ovDisplayName(ov: Overlay): string {
  if (ov.type === 'text') return ov.content ? `"${ov.content.slice(0, 20)}"` : 'Empty text'
  if (!ov.content) return ov.type === 'image' ? 'No image' : 'No GIF'
  // Extract filename from URL or path: http://localhost:18500/media/home/.../photo.jpg → photo.jpg
  return ov.content.split('/').pop()?.split('?')[0] || ov.type
}

// ★ Epic 3: Overlay reordering — controls z-index AND ffmpeg filter chain order
function moveOverlay(id: string, dir: 'up' | 'down') {
  const idx = overlays.value.findIndex(o => o.id === id)
  if (idx < 0) return
  const newIdx = dir === 'up' ? idx - 1 : idx + 1
  if (newIdx < 0 || newIdx >= overlays.value.length) return
  const arr = [...overlays.value]
  ;[arr[idx], arr[newIdx]] = [arr[newIdx], arr[idx]]
  overlays.value = arr
}

// ★ Epic 3: File picker for image/gif overlays
async function pickOverlayFile(ov: Overlay) {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const filters = ov.type === 'gif'
      ? [{ name: 'GIF', extensions: ['gif'] }]
      : [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'] }]
    const path = await open({ multiple: false, filters })
    if (path && typeof path === 'string') {
      // Convert local path to media server URL for rendering
      ov.content = `http://localhost:${mediaPort.value}/media${path}`
    }
  } catch (e) { console.error('File picker:', e) }
}

// ── Export ──
const exportModal = ref(false)
const exportName = ref('')
const exportDir = ref('')
const exportTarget = ref(0)
const exportSettings = ref('')
const exporting = ref(false)
const exportProgress = ref(0)
const exportSpeed = ref('')
let progressUnsub: UnlistenFn | null = null
const toast = ref('')
const toastFile = ref('')
function showToast(m: string, f = '') { toast.value = m; toastFile.value = f; setTimeout(() => { toast.value = ''; toastFile.value = '' }, 5000) }

// ── Video ──
function onMeta() { if (videoRef.value) { dur.value = videoRef.value.duration; if (trimE.value <= 0 || trimE.value > dur.value) trimE.value = dur.value } }
// Intercept 'play' event to resume AudioContext (handles native controls too)
async function onVideoPlay() { await resumeAudioContext(); playing.value = true }
async function togglePlay() {
  if (!videoRef.value) return
  await resumeAudioContext()
  if (playing.value) { videoRef.value.pause() }
  else {
    if (videoRef.value.currentTime < trimS.value || videoRef.value.currentTime >= trimE.value) {
      const seekTo = trimS.value > 0 ? trimS.value : 0.01
      videoRef.value.currentTime = seekTo
      // Re-seek audio elements immediately so they're aligned when play() fires
      for (const el of Object.values(audioEls.value)) el.currentTime = seekTo
    }
    videoRef.value.play().catch(e => console.warn('play blocked:', e))
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
  if (!port) return ''
  const encoded = encodeURIComponent(props.clip.filepath)
  return `http://localhost:${port}/audio?file=${encoded}&stream=${streamIndex}`
}

function initAudioElements() {
  // Destroy old audio elements
  for (const el of Object.values(audioEls.value)) { el.pause(); el.src = ''; el.load() }
  audioEls.value = {}

  const audioTracks = tracks.value.filter(t => t.type === 'audio')
  if (audioTracks.length <= 1) {
    // Single-track: unmute the <video> so native audio plays
    if (videoRef.value) { videoRef.value.muted = false; videoRef.value.volume = audioTracks[0] ? (audioTracks[0].muted ? 0 : audioTracks[0].volume / 100) : 1 }
    return
  }

  // Multi-track: keep <video> muted and use separate <audio> elements
  if (videoRef.value) videoRef.value.muted = true

  for (const t of audioTracks) {
    const el = new Audio()
    el.src = buildAudioUrl(t.streamIndex)
    el.preload = 'auto'
    el.volume = t.muted ? 0 : t.volume / 100
    audioEls.value[t.id] = el
  }
}

// Sync audio elements to video currentTime
let syncRaf = 0
function syncAudioToVideo() {
  const tick = () => {
    if (videoRef.value) {
      const vt = videoRef.value.currentTime
      const vPlaying = !videoRef.value.paused
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
      if (el) el.volume = (t.muted || t.volume <= 0) ? 0 : Math.min(1, (t.volume / 100) * localVol.value)
    }
  } else if (videoRef.value) {
    const t = audioTracks[0]
    if (t) videoRef.value.volume = (t.muted || t.volume <= 0) ? 0 : Math.min(1, (t.volume / 100) * localVol.value)
  }
}
watch(tracks, applyAudioVolumes, { deep: true })
watch(localVol, applyAudioVolumes)

// ── Timeline ──
const tlRef = ref<HTMLElement | null>(null)
const scrubbing = ref(false)
function pxToSec(e: MouseEvent) { if (!tlRef.value) return 0; const r = tlRef.value.getBoundingClientRect(); return Math.max(0, Math.min(dur.value, ((e.clientX - r.left) / r.width) * dur.value)) }
function tlClick(e: MouseEvent) { if ((e.target as HTMLElement).closest('.no-seek')) return; seekTo(pxToSec(e)) }
function pushUndo() { undoStack.value.push({ s: trimS.value, e: trimE.value }); redoStack.value = [] }
function undo() { const p = undoStack.value.pop(); if (p) { redoStack.value.push({ s: trimS.value, e: trimE.value }); trimS.value = p.s; trimE.value = p.e; saveMeta() } }
function redo() { const r = redoStack.value.pop(); if (r) { undoStack.value.push({ s: trimS.value, e: trimE.value }); trimS.value = r.s; trimE.value = r.e; saveMeta() } }
function dragTrim(w: 'start' | 'end', e: MouseEvent) { e.preventDefault(); e.stopPropagation(); pushUndo(); const mv = (ev: MouseEvent) => { const s = pxToSec(ev); if (w === 'start') trimS.value = Math.min(s, trimE.value - 0.3); else trimE.value = Math.max(s, trimS.value + 0.3) }; const up = () => { document.removeEventListener('mousemove', mv); document.removeEventListener('mouseup', up); saveMeta() }; document.addEventListener('mousemove', mv); document.addEventListener('mouseup', up) }
function phDown(e: MouseEvent) { e.preventDefault(); scrubbing.value = true; seekTo(pxToSec(e)); document.addEventListener('mousemove', scrubMv); document.addEventListener('mouseup', phUp) }
function scrubMv(e: MouseEvent) { if (scrubbing.value) seekTo(pxToSec(e)) }
function phUp() { scrubbing.value = false; document.removeEventListener('mousemove', scrubMv); document.removeEventListener('mouseup', phUp) }
function pct(v: number) { return `${(v / dur.value * 100).toFixed(3)}%` }

// E2: Overlay clip drag on O1 track
function dragOverlayClip(ov: Overlay, e: MouseEvent) {
  e.preventDefault(); e.stopPropagation()
  const startX = e.clientX; const origStart = ov.startSec
  const mv = (ev: MouseEvent) => {
    const dx = ev.clientX - startX
    if (!tlRef.value) return
    const secPerPx = dur.value / tlRef.value.clientWidth
    ov.startSec = Math.max(0, Math.min(dur.value - ov.durSec, origStart + dx * secPerPx))
  }
  const up = () => { document.removeEventListener('mousemove', mv); document.removeEventListener('mouseup', up) }
  document.addEventListener('mousemove', mv); document.addEventListener('mouseup', up)
}

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
function initTracks() {
  const t: Track[] = []
  // Only add the overlay track if the extension is enabled
  if (extOverlays.value) {
    t.push({ id: 'O1', label: getTrackName('O1'), type: 'overlay', color: getTrackColor('O1'), volume: 100, muted: false, streamIndex: -1, peaks: [], volOpen: false })
  }
  t.push({ id: 'V1', label: getTrackName('V1'), type: 'video', color: getTrackColor('V1'), volume: 100, muted: false, streamIndex: 0, peaks: [], volOpen: false })
  const audioStreams = info.value?.streams.filter(s => s.codec_type === 'audio') || []
  audioStreams.forEach((s, i) => {
    const id = `A${i + 1}`
    t.push({ id, label: getTrackName(id), type: 'audio', color: getTrackColor(id), volume: 100, muted: false, streamIndex: s.index, peaks: [], volOpen: false })
  })
  if (!audioStreams.length) t.push({ id: 'A1', label: getTrackName('A1'), type: 'audio', color: getTrackColor('A1'), volume: 100, muted: false, streamIndex: 1, peaks: [], volOpen: false })
  tracks.value = t
}

// Live-update colors + names when Settings → Track Defs changes
watch(() => persist.state.settings.trackDefs, () => {
  tracks.value.forEach(t => { t.color = getTrackColor(t.id); t.label = getTrackName(t.id) })
}, { deep: true })

// Re-init tracks when the overlays extension is toggled (adds/removes O1 row)
watch(extOverlays, () => { initTracks() })
async function loadWaveforms() { for (const t of tracks.value) { if (t.type !== 'audio') continue; try { t.peaks = await invoke<number[]>('generate_waveform', { filepath: props.clip.filepath, streamIndex: t.streamIndex, numPeaks: 200 }) } catch { t.peaks = Array(200).fill(0.3) } } }
onMounted(async () => { try { info.value = await invoke<MInfo>('analyze_media', { filepath: props.clip.filepath }); if (info.value.duration > 0) dur.value = info.value.duration } catch {} initTracks(); await loadMeta(); await loadWaveforms(); initAudioElements() })

// Persistence
// ★ E2-P4: Persistence with game_tag
async function saveMeta() {
  try { await invoke('save_trim_state', { filepath: props.clip.filepath, trimStart: trimS.value, trimEnd: trimE.value }) } catch {}
  try { await invoke('set_clip_meta', { update: { filepath: props.clip.filepath, custom_name: editName.value, favorite: props.clip.favorite, game_tag: gameTag.value } }) } catch {}
  // ★ Epic 5: Persist overlays as JSON in the notes column
  if (overlays.value.length) {
    try {
      const ovJson = JSON.stringify(overlays.value.map(o => ({ ...o, open: false })))
      await invoke('set_clip_meta', { update: { filepath: props.clip.filepath, notes: ovJson } })
    } catch {}
  }
}
async function loadMeta() {
  try { const s = await invoke<{ trim_start: number; trim_end: number } | null>('get_trim_state', { filepath: props.clip.filepath }); if (s && s.trim_end > 0) { trimS.value = s.trim_start; trimE.value = s.trim_end } } catch {}
  try {
    const m = await invoke<string>('get_clip_meta', { filepath: props.clip.filepath })
    if (m && m !== 'null') {
      const d = JSON.parse(m)
      if (d.game_tag) gameTag.value = d.game_tag
      // ★ Epic 5: Restore overlays from notes
      if (d.notes) { try { const restored = JSON.parse(d.notes); if (Array.isArray(restored)) overlays.value = restored } catch {} }
    }
  } catch {}
}

// ★ E3-P5: Screenshot — respects the user-configured save directory
async function takeScreenshot() {
  try {
    const outputDir = persist.state.settings.screenshotDir || ''
    const path = await invoke<string>('take_screenshot', {
      filepath: props.clip.filepath,
      timeSec: ct.value,
      outputDir,
    })
    showToast(`Screenshot saved: ${path.split('/').pop()}`, path)
  } catch (e) { showToast(`Screenshot failed: ${e}`) }
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

// ★ E1-P2: Active overlays for rendering on video
const visibleOverlays = computed(() => overlays.value.filter(o => ct.value >= o.startSec && ct.value < o.startSec + o.durSec))

// ★ Epic 1: Auto-save overlays whenever they change (debounced)
let overlaySaveTimer: ReturnType<typeof setTimeout> | null = null
watch(overlays, () => {
  if (overlaySaveTimer) clearTimeout(overlaySaveTimer)
  overlaySaveTimer = setTimeout(() => saveMeta(), 800)
}, { deep: true })

// Click-outside
function onDocClick(e: MouseEvent) {
  if (gameRef.value && !gameRef.value.contains(e.target as Node)) {
    if (gameOpen.value && gameTag.value) saveMeta() // ★ Bug 3: Save custom tag on close
    gameOpen.value = false
  }
  tracks.value.forEach(t => { if (!(e.target as HTMLElement).closest('.vol-zone')) t.volOpen = false })
}
onMounted(() => document.addEventListener('mousedown', onDocClick)); onBeforeUnmount(() => document.removeEventListener('mousedown', onDocClick))
function selectGame(g: string) { gameTag.value = g; gameOpen.value = false; saveMeta() }

// Export
async function openExport() { exportModal.value = true; exportName.value = editName.value || 'export'; exportDir.value = props.clip.filepath.replace(/\/[^/]+$/, ''); exportTarget.value = 0; exportProgress.value = 0; exportSpeed.value = ''; await updateProj() }
async function updateProj() { if (exportTarget.value <= 0) { exportSettings.value = `Stream copy • ${info.value?.width || 1920}x${info.value?.height || 1080}`; return } try { const j = await invoke<string>('calc_export_settings', { durationSec: trimDur.value, targetMb: exportTarget.value, width: info.value?.width || 1920, height: info.value?.height || 1080 }); const s = JSON.parse(j); exportSettings.value = `${s.resolution} • ${s.video_bitrate_kbps}kbps video • ${s.audio_bitrate_kbps}kbps audio • H.264` } catch { exportSettings.value = '...' } }
watch(exportTarget, updateProj)
const exportResult = ref('')

async function doExport() {
  exporting.value = true; exportProgress.value = 0; exportResult.value = ''
  progressUnsub = await listen<{ percent: number; speed: string }>('export-progress', e => {
    if (e.payload.percent >= 0) exportProgress.value = e.payload.percent
    if (e.payload.speed) exportSpeed.value = e.payload.speed
  })
  try {
    const p = `${exportDir.value}/${exportName.value.replace(/[<>:"/\\|?*\x00]/g, '').trim()}.mp4`
    const audioTracks = tracks.value.filter(t => t.type === 'audio').map(t => ({
      stream_index: t.streamIndex, volume: t.volume / 100, muted: t.muted,
    }))
    // Only include overlays in the export if the extension is enabled
    const exportOverlays = extOverlays.value
      ? overlays.value.map(o => ({
          overlay_type: o.type, content: o.type === 'text' ? o.content : (o.content?.replace(/^http:\/\/localhost:\d+\/media/, '') || ''),
          x: o.x, y: o.y, scale: o.scale, start_sec: o.startSec, dur_sec: o.durSec,
          font_name: o.fontName || null,
        }))
      : []
    const hasFilters = exportOverlays.length > 0 || audioTracks.some(t => t.muted || t.volume < 1)
    let out: string
    if (hasFilters) {
      out = await invoke<string>('export_clip_with_filters', {
        inputPath: props.clip.filepath, startSec: trimS.value, endSec: trimE.value,
        audioTracks, overlays: exportOverlays, targetMb: exportTarget.value, outputPath: p,
      })
    } else {
      out = await invoke<string>('export_with_progress', {
        inputPath: props.clip.filepath, startSec: trimS.value, endSec: trimE.value,
        targetMb: exportTarget.value, outputPath: p,
      })
    }
    // ★ Epic 3: Stay open on success — show drag zone
    exportResult.value = out
    emit('saved', out)
  } catch (e) {
    showToast(`Export failed: ${e}`)
    exportModal.value = false
  } finally {
    exporting.value = false
    progressUnsub?.(); progressUnsub = null
  }
}
function closeExportModal() { exportModal.value = false; exportResult.value = '' }

// ★ Epic 3: Cancel running export
async function cancelExport() {
  try { await invoke('cancel_export') } catch {}
  exporting.value = false
  exportProgress.value = 0
  exportSpeed.value = ''
  showToast('Export cancelled')
}
function onToastDrag(e: DragEvent) { if (e.dataTransfer && toastFile.value) { e.dataTransfer.setData('text/uri-list', `file://${toastFile.value}`); e.dataTransfer.effectAllowed = 'copy' } }

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
  else if (shortcutMatches(e, sc.toggleMic))  {
    e.preventDefault()
    if (hoveredTrackId.value) {
      const t = tracks.value.find(t => t.id === hoveredTrackId.value)
      if (t) { t.muted = !t.muted; applyAudioVolumes() }
    } else if (videoRef.value) {
      videoRef.value.muted = !videoRef.value.muted
    }
  }
}
const onFsChange = () => { isFullscreen.value = !!document.fullscreenElement }
function togglePreviewFullscreen() { playerComp.value?.toggleFullscreen() }
onMounted(() => {
  videoRef.value = playerComp.value?.videoRef ?? null
  document.addEventListener('keydown', onKey)
  document.addEventListener('fullscreenchange', onFsChange)
})
onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKey)
  document.removeEventListener('fullscreenchange', onFsChange)
})

function fmt(s: number) { return `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}.${Math.floor((s % 1) * 10)}` }
function onPreviewProgressClick(e: MouseEvent) {
  const el = e.currentTarget as HTMLElement
  const r = el.getBoundingClientRect()
  const t = ((e.clientX - r.left) / r.width) * dur.value
  if (videoRef.value) { videoRef.value.currentTime = t; ct.value = t }
}
function setPreviewVol(v: number) { localVol.value = v; if (videoRef.value) videoRef.value.volume = v }
function drawWave(canvas: HTMLCanvasElement | null, t: Track) { if (!canvas || !t.peaks.length) return; const ctx = canvas.getContext('2d'); if (!ctx) return; const w = canvas.clientWidth; const h = canvas.clientHeight; canvas.width = w; canvas.height = h; ctx.clearRect(0, 0, w, h); const bw = w / t.peaks.length; const color = t.muted ? '#555' : t.color; const grad = ctx.createLinearGradient(0, 0, 0, h); grad.addColorStop(0, color + '80'); grad.addColorStop(0.5, color + 'DD'); grad.addColorStop(1, color + '80'); ctx.fillStyle = grad; for (let i = 0; i < t.peaks.length; i++) { const bh = Math.max(2, t.peaks[i] * h * 1.6); ctx.fillRect(i * bw, (h - bh) / 2, Math.max(1, bw - 0.5), bh) } }

</script>

<template>
<div class="editor">
  <!-- ═══ Top Bar ═══ -->
  <div class="bar">
    <button class="btn" @click="emit('close')"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="ic"><polyline points="15 18 9 12 15 6"/></svg></button>
    <input v-model="editName" class="name-in" spellcheck="false" @blur="saveMeta" @keydown.enter="($event.target as HTMLInputElement).blur()" />
    <div class="game-wrap" ref="gameRef">
      <input v-model="gameTag" class="game-in" placeholder="Game..." @focus="gameOpen = true" />
      <div v-if="gameOpen && gameFiltered.length" class="game-drop">
        <button v-for="g in gameFiltered.slice(0, 10)" :key="g" @mousedown.prevent="selectGame(g)">{{ g }}</button>
        <div v-if="gameTag && !GAMES.some(g => g.toLowerCase() === gameTag.toLowerCase())" class="game-custom" @mousedown.prevent="selectGame(gameTag)">+ "{{ gameTag }}"</div>
      </div>
    </div>
    <div style="flex:1"></div>
    <span v-if="info" class="tag">{{ info.video_codec }}</span>
    <span v-if="info" class="tag">{{ info.width }}×{{ info.height }}</span>
    <span v-if="info" class="tag">{{ info.fps.toFixed(0) }}fps</span>
    <span v-if="undoStack.length" class="tag">↩{{ undoStack.length }}</span>
    <button v-if="extTiktok" class="btn tiktok-btn" :disabled="exporting" title="TikTok Vertical Export (9:16) — coming soon" @click.prevent>
      <svg viewBox="0 0 24 24" fill="currentColor" class="ic" style="width:13px;height:13px"><path d="M19.59 6.69a4.83 4.83 0 01-3.77-4.25V2h-3.45v13.67a2.89 2.89 0 01-2.88 2.5 2.89 2.89 0 01-2.89-2.89 2.89 2.89 0 012.89-2.89c.28 0 .54.04.79.1V9.01a6.27 6.27 0 00-.79-.05 6.34 6.34 0 00-6.34 6.34 6.34 6.34 0 006.34 6.34 6.34 6.34 0 006.33-6.34V8.69a8.14 8.14 0 004.77 1.52V6.74a4.85 4.85 0 01-1-.05z"/></svg>
      9:16
    </button>
    <button class="btn accent" :disabled="exporting" @click="openExport">Export</button>
  </div>

  <!-- ═══ Main: Preview + Sidebar ═══ -->
  <div class="main" :style="{ flex: previewFlex }">
    <div class="preview">
      <CustomVideoPlayer
        ref="playerComp"
        :src="videoSrc"
        :show-controls="false"
        :capture-keyboard="false"
        @loadedmetadata="onMeta"
        @play="onVideoPlay"
        @pause="playing = false"
        @ended="playing = false"
      >
        <!-- ★ E1-P2: Overlay rendering ON the video (only when extension is enabled) -->
        <div v-if="extOverlays" class="overlay-layer">
          <div v-for="ov in visibleOverlays" :key="ov.id" class="overlay-item"
            :style="{ left: ov.x + '%', top: ov.y + '%', transform: `translate(-50%,-50%) scale(${ov.scale / 100})` }"
            @click.stop="selectedOverlay = ov.id; sideTab = 'overlay'">
            <span v-if="ov.type === 'text'" class="overlay-text">{{ ov.content }}</span>
            <img v-else-if="ov.type === 'image' && ov.content" :src="ov.content" class="overlay-img" />
            <img v-else-if="ov.type === 'gif' && ov.content" :src="ov.content" class="overlay-img" />
            <span v-else class="overlay-placeholder">{{ ov.type === 'image' ? '🖼' : '🎞' }}</span>
          </div>
        </div>
        <div v-if="!playing && !isFullscreen" class="play-ov" @click.stop="togglePlay"><svg viewBox="0 0 24 24" fill="currentColor" style="width:40px;height:40px;color:#fff;opacity:.7"><polygon points="5 3 19 12 5 21"/></svg></div>
        <!-- Custom ctrl-bar — only visible in fullscreen mode; transport bar handles normal editing -->
        <div class="prev-ctrl-bar" :class="{ 'prev-ctrl-vis': isFullscreen }">
          <div class="prev-prog" @click.stop="onPreviewProgressClick">
            <div class="prev-prog-fill" :style="{ width: dur ? (ct/dur*100)+'%' : '0%' }">
              <div class="prev-prog-thumb"></div>
            </div>
          </div>
          <div class="prev-ctrl-row">
            <button class="prev-cb" @click.stop="skip(-5)" title="-5s">
              <svg viewBox="0 0 24 24" fill="currentColor" style="width:12px;height:12px"><polygon points="11 19 2 12 11 5"/><polygon points="22 19 13 12 22 5"/></svg>
            </button>
            <button class="prev-cb" @click.stop="togglePlay">
              <svg v-if="playing" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>
              <svg v-else viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21"/></svg>
            </button>
            <button class="prev-cb" @click.stop="skip(5)" title="+5s">
              <svg viewBox="0 0 24 24" fill="currentColor" style="width:12px;height:12px"><polygon points="13 19 22 12 13 5"/><polygon points="2 19 11 12 2 5"/></svg>
            </button>
            <span class="prev-time">{{ fmt(ct) }} / {{ fmt(dur) }}</span>
            <div class="prev-vol">
              <svg viewBox="0 0 24 24" fill="currentColor" class="prev-vol-ic" style="cursor:pointer" @click.stop="setPreviewVol(localVol > 0 ? 0 : 1)">
                <path d="M11 5L6 9H2v6h4l5 4V5z"/>
                <path v-if="localVol > 0" d="M15.54 8.46a5 5 0 010 7.07" stroke="currentColor" fill="none" stroke-width="2"/>
                <line v-else x1="23" y1="9" x2="17" y2="15" stroke="currentColor" stroke-width="2"/><line v-if="localVol === 0" x1="17" y1="9" x2="23" y2="15" stroke="currentColor" stroke-width="2"/>
              </svg>
              <input type="range" min="0" max="1" step="0.05" :value="localVol" @input="setPreviewVol(+($event.target as HTMLInputElement).value)" class="prev-vol-sl" />
            </div>
            <button class="prev-cb" @click.stop="togglePreviewFullscreen()" title="Fullscreen">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M8 3H5a2 2 0 00-2 2v3m18 0V5a2 2 0 00-2-2h-3m0 18h3a2 2 0 002-2v-3M3 16v3a2 2 0 002 2h3"/></svg>
            </button>
          </div>
        </div>
      </CustomVideoPlayer>
    </div>

    <!-- ═══ E4: Resizable Sidebar ═══ -->
    <div class="sidebar" :class="{ shut: !sideOpen }" :style="sideOpen ? { width: sideWidth + 'px' } : {}">
      <div v-if="sideOpen" class="side-resize" @mousedown="sideResizeDown"></div>
      <button class="side-toggle" @click="sideOpen = !sideOpen">{{ sideOpen ? '›' : '‹' }}</button>
      <div v-if="sideOpen" class="side-inner">
        <div class="side-tabs">
          <button :class="{ active: sideTab === 'info' }" :style="sideTab === 'info' ? { backgroundColor: 'color-mix(in srgb, var(--accent) 20%, transparent)' } : {}" @click="sideTab = 'info'">Info</button>
          <button v-if="extOverlays" :class="{ active: sideTab === 'overlay' }" :style="sideTab === 'overlay' ? { backgroundColor: 'color-mix(in srgb, var(--accent) 20%, transparent)' } : {}" @click="sideTab = 'overlay'">Overlays<span v-if="overlays.length" class="tab-badge">{{ overlays.length }}</span></button>
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

        <!-- E2+E3: Overlays tab (only rendered when extension is on) -->
        <div v-if="sideTab === 'overlay' && extOverlays" class="tab-content">
          <div class="ov-add-row">
            <button class="ov-add-btn" @click="addOverlay('text')">🔤 Text</button>
            <button class="ov-add-btn" @click="addOverlay('image')">🖼 Image</button>
            <button class="ov-add-btn" @click="addOverlay('gif')">🎞 GIF</button>
            <button v-if="overlays.length" class="ov-clear-btn" @click="clearOverlays" title="Remove all overlays">🗑</button>
          </div>
          <div v-if="!overlays.length" class="empty-hint">No overlays. Add one above.</div>
          <!-- E3: Accordion per overlay -->
          <div v-for="ov in overlays" :key="ov.id" class="ov-card" :class="{ sel: selectedOverlay === ov.id }">
            <div class="ov-header" @click="selectedOverlay = ov.id; ov.open = !ov.open">
              <span class="ov-icon">{{ ov.type === 'text' ? '🔤' : ov.type === 'image' ? '🖼' : '🎞' }}</span>
              <span class="ov-name">{{ ovDisplayName(ov) }}</span>
              <!-- ★ Epic 3: Reorder buttons -->
              <button class="ov-move" @click.stop="moveOverlay(ov.id, 'up')" title="Move up (behind)">▲</button>
              <button class="ov-move" @click.stop="moveOverlay(ov.id, 'down')" title="Move down (in front)">▼</button>
              <span class="ov-chevron">{{ ov.open ? '▾' : '▸' }}</span>
              <button class="ov-del" @click.stop="removeOverlay(ov.id)">×</button>
            </div>
            <div v-if="ov.open" class="ov-body">
              <div v-if="ov.type === 'text'" class="ov-field">
                <label>Text</label>
                <input v-model="ov.content" class="ov-input" placeholder="Your text..." />
              </div>
              <div v-if="ov.type === 'text'" class="ov-field">
                <label>Font</label>
                <select v-model="ov.fontName" class="ov-select">
                  <option v-for="f in OVERLAY_FONTS" :key="f" :value="f">{{ f }}</option>
                </select>
              </div>
              <!-- ★ Epic 3: File picker for image/gif overlays -->
              <div v-if="ov.type === 'image' || ov.type === 'gif'" class="ov-upload">
                <button class="ov-browse-btn" @click.stop="pickOverlayFile(ov)">📂 Browse File</button>
                <button class="ov-web-btn" disabled title="Coming Soon — Giphy/Tenor integration">🌐 From Web</button>
                <div v-if="ov.content" class="ov-preview-thumb">
                  <img :src="ov.content" class="ov-thumb-img" />
                </div>
                <div v-else class="ov-no-file">No file selected</div>
              </div>
              <div class="ov-field">
                <label>X <span class="tip" title="Horizontal position (0=left, 100=right)">?</span></label>
                <input type="number" v-model.number="ov.x" min="0" max="100" class="ov-num" />
              </div>
              <div class="ov-field">
                <label>Y <span class="tip" title="Vertical position (0=top, 100=bottom)">?</span></label>
                <input type="number" v-model.number="ov.y" min="0" max="100" class="ov-num" />
              </div>
              <div class="ov-field">
                <label>Scl <span class="tip" title="Scale percentage (20-200%)">?</span></label>
                <input type="number" v-model.number="ov.scale" min="20" max="200" class="ov-num" />
              </div>
              <div class="ov-field">
                <label>Start <span class="tip" title="Start time in seconds">?</span></label>
                <input type="number" v-model.number="ov.startSec" min="0" :max="dur" step="0.1" class="ov-num" />
              </div>
              <div class="ov-field">
                <label>Dur <span class="tip" title="Duration in seconds">?</span></label>
                <input type="number" v-model.number="ov.durSec" min="0.5" :max="dur" step="0.1" class="ov-num" />
              </div>
            </div>
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
      <input type="range" min="0" max="1" step="0.05" v-model.number="localVol" class="vol-monitor-range" />
      <span class="vol-monitor-val">{{ Math.round(localVol * 100) }}</span>
    </div>
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
  <div class="tl-wrap" :style="{ flex: tlFlex }">
    <div class="tl-container">
      <!--  Left: Track Headers (fixed width, gap from canvas) -->
      <div class="tl-headers">
        <div v-for="t in tracks" :key="'h' + t.id" class="tl-header" :style="{ '--tc': t.color }"
          @mouseenter="hoveredTrackId = t.id" @mouseleave="hoveredTrackId = null">
          <span class="hdr-id">{{ t.id }}</span>
          <svg v-if="showIcons() && getTrackDef(t.id)?.icon" class="hdr-icon-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path v-if="getTrackDef(t.id)?.icon === 'video'"   d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M3 6h10a2 2 0 012 2v8a2 2 0 01-2 2H3a2 2 0 01-2-2V8a2 2 0 012-2z"/>
            <path v-else-if="getTrackDef(t.id)?.icon === 'game'"    d="M6 11h4m-2-2v4m7-1h.01M18 11h.01M2 6a2 2 0 012-2h16a2 2 0 012 2v10a4 4 0 01-4 4H6a4 4 0 01-4-4V6z"/>
            <path v-else-if="getTrackDef(t.id)?.icon === 'mic'"     d="M12 1a3 3 0 00-3 3v8a3 3 0 006 0V4a3 3 0 00-3-3zM19 10v2a7 7 0 01-14 0v-2M12 19v3M8 23h8"/>
            <path v-else-if="getTrackDef(t.id)?.icon === 'chat'"    d="M3 18v-6a9 9 0 0118 0v6M21 19a2 2 0 01-2 2h-1a2 2 0 01-2-2v-3a2 2 0 012-2h3zM3 19a2 2 0 002 2h1a2 2 0 002-2v-3a2 2 0 00-2-2H3z"/>
            <path v-else-if="getTrackDef(t.id)?.icon === 'media'"   d="M9 18V5l12-2v13M9 19c0 1.1-1.34 2-3 2s-3-.9-3-2 1.34-2 3-2 3 .9 3 2zm12-3c0 1.1-1.34 2-3 2s-3-.9-3-2 1.34-2 3-2 3 .9 3 2z"/>
            <path v-else-if="getTrackDef(t.id)?.icon === 'overlay'" d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/>
          </svg>
          <span class="hdr-name">{{ t.label }}</span>
          <!-- Speaker icon on RIGHT edge of header for audio tracks -->
          <template v-if="t.type === 'audio'">
            <div class="vol-zone" style="margin-left:auto">
              <button class="hdr-speaker" :class="{ off: t.muted }" @click.stop="t.volOpen = !t.volOpen; tracks.filter(x => x.id !== t.id).forEach(x => x.volOpen = false)">
                <svg v-if="t.muted" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" class="ic-s"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>
                <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" class="ic-s"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19"/><path d="M15.54 8.46a5 5 0 010 7.07"/></svg>
              </button>
              <div v-if="t.volOpen" class="vol-popover no-seek" @click.stop>
                <input type="range" min="0" max="100" v-model.number="t.volume" class="vol-range" :style="{ accentColor: t.color }" />
                <span class="vol-val">{{ t.volume }}</span>
              </div>
            </div>
          </template>
        </div>
      </div>

      <!-- Right: Timeline Canvas — each track is a FIXED HEIGHT ROW -->
      <div class="tl-canvas" ref="tlRef" @click="tlClick">
        <!-- Dim zones outside trim -->
        <div class="dim-left" :style="{ width: pct(trimS) }"></div>
        <div class="dim-right" :style="{ width: pct(dur - trimE) }"></div>

        <!-- E1: Each track is a distinct fixed-height row -->
        <div class="tl-rows">
          <div v-for="t in tracks" :key="'r' + t.id" class="tl-row" :style="{ '--tc': t.color }"
            @mouseenter="hoveredTrackId = t.id" @mouseleave="hoveredTrackId = null">
            <!-- E2: Overlay track shows overlay clips -->
            <template v-if="t.type === 'overlay'">
              <div v-for="ov in overlays" :key="ov.id" class="ov-clip no-seek" :class="{ sel: selectedOverlay === ov.id }"
                :style="{ left: pct(ov.startSec), width: pct(ov.durSec) }"
                @mousedown="dragOverlayClip(ov, $event)" @click.stop="selectedOverlay = ov.id; sideTab = 'overlay'">
                <span>{{ ov.type === 'text' ? '🔤' : ov.type === 'image' ? '🖼' : '🎞' }}</span>
                <span class="ov-clip-label">{{ ovDisplayName(ov) }}</span>
              </div>
            </template>
            <!-- Audio track → waveform -->
            <canvas v-if="t.type === 'audio' && t.peaks.length" :ref="el => drawWave(el as HTMLCanvasElement, t)" class="waveform"></canvas>
          </div>
        </div>

        <!-- Trim handles -->
        <div class="trim-handle" :style="{ left: pct(trimS) }" @mousedown="dragTrim('start', $event)"><div class="trim-grip">‹</div></div>
        <div class="trim-handle" :style="{ left: pct(trimE) }" @mousedown="dragTrim('end', $event)"><div class="trim-grip">›</div></div>

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
        <!-- ★ Epic 3: Success state with drag zone -->
        <template v-if="exportResult">
          <h2>✓ Export Complete</h2>
          <div class="export-success-file">{{ exportResult.split('/').pop() }}</div>
          <div class="export-drag-zone" draggable="true" @dragstart="e => { if (e.dataTransfer) { e.dataTransfer.setData('text/uri-list', `file://${exportResult}`); e.dataTransfer.effectAllowed = 'copy' } }">
            📎 Drag this file to Discord, Telegram, or a folder
          </div>
          <div class="modal-actions">
            <button class="btn" @click="closeExportModal">Close</button>
          </div>
        </template>
        <!-- Normal export form -->
        <template v-else>
        <h2>Export Clip</h2>
        <div class="modal-field"><label>Filename</label><input v-model="exportName" class="modal-input" /></div>
        <div class="modal-field"><label>Directory</label><input v-model="exportDir" class="modal-input" readonly /></div>
        <div class="modal-field"><label>Target Size</label>
          <div class="radio-row">
            <label v-for="v in [0, 100, 50, 10]" :key="v" :class="{ active: exportTarget === v }">
              <input type="radio" v-model.number="exportTarget" :value="v" />{{ v === 0 ? 'Original' : `${v}MB` }}
            </label>
          </div>
        </div>
        <div class="proj-box">{{ exportSettings }}</div>
        <div v-if="exporting" class="progress-bar-wrap">
          <div class="progress-track"><div class="progress-fill" :style="{ width: exportProgress + '%' }"></div></div>
          <span class="progress-text">{{ exportProgress.toFixed(0) }}% {{ exportSpeed ? `(${exportSpeed})` : '' }}</span>
        </div>
        <div class="modal-actions">
          <button v-if="exporting" class="btn btn-cancel" @click="cancelExport">✕ Cancel Export</button>
          <button v-else class="btn" @click="closeExportModal">Cancel</button>
          <button class="btn accent" @click="doExport" :disabled="exporting">{{ exporting ? 'Exporting...' : 'Export' }}</button>
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
.tiktok-btn { border-color: #69C9D0; color: #69C9D0; gap: 4px; }
.tiktok-btn:hover { background: rgba(105,201,208,.1); }
.tiktok-btn:disabled { opacity: .5; cursor: not-allowed; }
.name-in { flex: 0 1 200px; padding: 3px 8px; background: var(--bg-input); border: 1px solid var(--border); border-radius: 5px; color: var(--text); font-size: 12px; font-weight: 700; outline: none; }
.name-in:focus { border-color: var(--accent); }
.game-wrap { position: relative; }
.game-in { width: 130px; padding: 3px 8px; background: var(--bg-input); border: 1px solid var(--border); border-radius: 5px; color: var(--text-sec); font-size: 10px; outline: none; }
.game-in:focus { border-color: var(--accent); }
.game-drop { position: absolute; top: 100%; left: 0; right: 0; margin-top: 2px; background: var(--bg-card); border: 1px solid var(--border); border-radius: 6px; padding: 2px; z-index: 20; max-height: 200px; overflow-y: auto; box-shadow: 0 4px 12px rgba(0,0,0,.3); }
.game-drop button { width: 100%; padding: 5px 8px; border: none; background: transparent; color: var(--text-sec); font-size: 10px; text-align: left; cursor: pointer; border-radius: 4px; }
.game-drop button:hover { background: var(--bg-hover); color: var(--text); }
.game-custom { padding: 5px 8px; color: var(--accent); font-size: 10px; cursor: pointer; }
.tag { font-size: 8px; background: var(--bg-deep); border: 1px solid var(--border); padding: 1px 5px; border-radius: 3px; color: var(--text-muted); }

/* ═══ Main Area ═══ */
.main { display: flex; min-height: 0; min-width: 0; }
.preview { flex: 1; background: #000; position: relative; display: flex; align-items: center; justify-content: center; cursor: pointer; min-width: 0; min-height: 0; overflow: hidden; }
.vid { max-width: 100%; max-height: 100%; object-fit: contain; }
.play-ov { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; background: rgba(0,0,0,.15); }
/* ── Custom preview ctrl-bar ── */
.prev-ctrl-bar {
  position:absolute; bottom:0; left:0; right:0;
  background:linear-gradient(transparent, rgba(0,0,0,.75));
  padding:12px 8px 6px; opacity:0; transition:opacity .2s; pointer-events:none;
}
.prev-ctrl-bar.prev-ctrl-vis { opacity:1; pointer-events:auto; }
.prev-prog {
  height:3px; background:rgba(255,255,255,.25); border-radius:2px; cursor:pointer; margin-bottom:6px;
  transition:height .15s;
}
.prev-prog:hover { height:5px; }
.prev-prog-fill { height:100%; background:var(--accent); border-radius:2px; pointer-events:none; position:relative; }
.prev-prog-thumb { position:absolute; right:-6px; top:50%; transform:translateY(-50%); width:13px; height:13px; border-radius:50%; background:#fff; box-shadow:0 0 5px rgba(0,0,0,.5); pointer-events:none; opacity:0; transition:opacity .15s; }
.prev-prog:hover .prev-prog-thumb { opacity:1; }
.prev-ctrl-row { display:flex; align-items:center; gap:6px; }
.prev-cb { width:26px; height:26px; border:none; background:transparent; color:#fff; cursor:pointer; display:flex; align-items:center; justify-content:center; border-radius:4px; flex-shrink:0; }
.prev-cb svg { width:14px; height:14px; }
.prev-cb:hover { background:rgba(255,255,255,.15); }
.prev-time { font-size:10px; color:rgba(255,255,255,.85); flex:1; white-space:nowrap; }
.prev-vol { display:flex; align-items:center; gap:4px; }
.prev-vol-ic { width:14px; height:14px; color:#fff; flex-shrink:0; }
.prev-vol-sl { width:55px; height:3px; accent-color:var(--accent); cursor:pointer; }
/* Fullscreen compat */
.preview:fullscreen .prev-ctrl-bar,
.preview:-webkit-full-screen .prev-ctrl-bar { position:absolute; }

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
.preview:fullscreen .overlay-layer,
.preview:-webkit-full-screen .overlay-layer {
  position: absolute;
  inset: 0;
  background: transparent;
}
.preview:fullscreen .play-ov,
.preview:-webkit-full-screen .play-ov {
  position: absolute;
  inset: 0;
}

/* ★ E1-P2: Overlay rendering on video */
.overlay-layer { position: absolute; inset: 0; pointer-events: none; overflow: hidden; }
.overlay-item { position: absolute; pointer-events: auto; cursor: move; }
.overlay-text { color: #fff; font-size: 24px; font-weight: 700; text-shadow: 2px 2px 4px rgba(0,0,0,.8); white-space: nowrap; }
.overlay-img { max-width: 200px; max-height: 200px; border-radius: 4px; }
.overlay-placeholder { font-size: 40px; }

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

/* E3: Overlay tab */
.ov-add-row { display: flex; gap: 4px; margin-bottom: 8px; }
.ov-add-btn { flex: 1; padding: 5px 4px; border: 1px dashed var(--border); border-radius: 5px; background: transparent; color: var(--text-sec); font-size: 9px; cursor: pointer; text-align: center; }
.ov-add-btn:hover { border-color: var(--accent); color: var(--accent); background: color-mix(in srgb, var(--accent) 5%, transparent); }
.ov-clear-btn { padding: 5px 7px; border: 1px dashed #ef4444; border-radius: 5px; background: transparent; color: #ef4444; font-size: 11px; cursor: pointer; }
.ov-clear-btn:hover { background: color-mix(in srgb, #ef4444 10%, transparent); }
.ov-select { width: 100%; background: var(--bg-input, #1a1a2e); color: var(--text); border: 1px solid var(--border); border-radius: 4px; padding: 3px 4px; font-size: 10px; }
.empty-hint { color: var(--text-muted); font-style: italic; font-size: 9px; padding: 8px 0; }
.ov-card { border: 1px solid var(--border); border-radius: 5px; margin-bottom: 6px; overflow: hidden; }
.ov-card.sel { border-color: var(--accent); }
.ov-header { display: flex; align-items: center; gap: 4px; padding: 5px 8px; cursor: pointer; background: var(--bg-deep); }
.ov-header:hover { background: var(--bg-hover); }
.ov-icon { font-size: 11px; }
.ov-name { flex: 1; font-size: 10px; color: var(--text-sec); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ov-chevron { font-size: 9px; color: var(--text-muted); }
.ov-del { border: none; background: none; color: var(--text-muted); font-size: 14px; cursor: pointer; padding: 0 2px; line-height: 1; }
.ov-del:hover { color: var(--danger); }
.ov-move { border: none; background: none; color: var(--text-muted); font-size: 8px; cursor: pointer; padding: 1px 2px; line-height: 1; opacity: .5; }
.ov-move:hover { opacity: 1; color: var(--accent); }
.ov-body { padding: 6px 8px; display: flex; flex-direction: column; gap: 5px; }
/* ★ Epic 3: Overlay file picker styles */
.ov-upload { display: flex; flex-direction: column; gap: 4px; }
.ov-browse-btn { padding: 6px 10px; border: 1px dashed var(--accent); border-radius: 5px; background: transparent; color: var(--accent); font-size: 10px; cursor: pointer; font-weight: 600; }
.ov-browse-btn:hover { background: color-mix(in srgb, var(--accent) 10%, transparent); }
.ov-web-btn { padding: 5px 8px; border: 1px solid var(--border); border-radius: 5px; background: var(--bg-deep); color: var(--text-muted); font-size: 9px; cursor: not-allowed; opacity: .5; }
.ov-preview-thumb { margin-top: 4px; }
.ov-thumb-img { max-width: 100%; max-height: 60px; border-radius: 3px; border: 1px solid var(--border); }
.ov-no-file { font-size: 9px; color: var(--text-muted); font-style: italic; }
.ov-field { display: flex; align-items: center; gap: 6px; }
.ov-field label { font-size: 9px; font-weight: 700; color: var(--text-muted); min-width: 28px; display: flex; align-items: center; gap: 2px; }
.tip { display: inline-flex; align-items: center; justify-content: center; width: 12px; height: 12px; border-radius: 50%; background: var(--bg-deep); border: 1px solid var(--border); font-size: 7px; color: var(--text-muted); cursor: help; }
.ov-input { flex: 1; padding: 3px 6px; background: var(--bg-input); border: 1px solid var(--border); border-radius: 4px; color: var(--text); font-size: 10px; outline: none; }
.ov-num { width: 52px; padding: 3px 4px; background: var(--bg-input); border: 1px solid var(--border); border-radius: 4px; color: var(--text); font-size: 10px; outline: none; text-align: center; }

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
.play-btn { width: 34px; height: 34px; background: var(--accent); border-color: var(--accent); color: #fff; }
.tr-time { font-size: 11px; font-variant-numeric: tabular-nums; }
.tr-time.dim { color: var(--text-sec); }
.tr-time.cur { font-weight: 700; color: var(--text); }
.tr-sep { color: var(--text-muted); font-size: 9px; }
.tr-dur { font-size: 11px; color: var(--accent); font-weight: 600; }

/* ★ Epic 2: Local monitor volume slider */
.vol-monitor { display: flex; align-items: center; gap: 4px; padding: 0 4px; }
.vol-monitor-range { width: 52px; height: 4px; accent-color: var(--accent); cursor: pointer; }
.vol-monitor-val { font-size: 9px; color: var(--text-muted); min-width: 22px; text-align: right; }

/* ═══ E1: Timeline — FIXED HEIGHT ROWS ═══ */
.tl-wrap { min-height: 60px; overflow: hidden; flex-shrink: 0; }
.tl-container { display: flex; height: 100%; }

/* Track Headers — flush against canvas (no gap) */
.tl-headers { width: 110px; flex-shrink: 0; display: flex; flex-direction: column; background: var(--bg-deep); z-index: 4; }
.tl-header {
  height: 36px; /* ★ E1: FIXED height per track */
  display: flex; align-items: center; gap: 4px; padding: 0 8px;
  border-bottom: 1px solid var(--border);
  border-left: 3px solid var(--tc);
  background: color-mix(in srgb, var(--tc) 8%, var(--bg-deep));
  position: relative;
}
.hdr-id { font-size: 10px; font-weight: 900; color: var(--tc); }
.hdr-icon-svg { width: 11px; height: 11px; color: var(--tc); opacity: .75; flex-shrink: 0; }
.hdr-name { font-size: 9px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

/* Speaker icon on right edge */
.vol-zone { position: relative; }
.hdr-speaker { border: none; background: none; cursor: pointer; padding: 2px; color: var(--tc); opacity: .7; }
.hdr-speaker:hover { opacity: 1; }
.hdr-speaker.off { opacity: .3; }
.vol-popover { position: absolute; right: 0; top: 100%; margin-top: 4px; background: var(--bg-card); border: 1px solid var(--border); border-radius: 6px; padding: 6px 8px; display: flex; align-items: center; gap: 6px; box-shadow: 0 4px 12px rgba(0,0,0,.4); z-index: 20; white-space: nowrap; }
.vol-range { width: 60px; height: 4px; }
.vol-val { font-size: 9px; font-weight: 700; color: var(--tc); min-width: 20px; }

/* Timeline Canvas */
.tl-canvas { flex: 1; position: relative; overflow: hidden; cursor: crosshair; user-select: none; }
.dim-left, .dim-right { position: absolute; top: 0; bottom: 0; background: rgba(0,0,0,.45); z-index: 3; pointer-events: none; }
.dim-left { left: 0; }
.dim-right { right: 0; }

/* ★ E1: Each track row has FIXED height with visible borders */
.tl-rows { display: flex; flex-direction: column; height: 100%; }
.tl-row {
  height: 36px; /* matches header */
  flex-shrink: 0;
  border-bottom: 1px solid var(--border);
  background: color-mix(in srgb, var(--tc) 6%, var(--bg-deep));
  position: relative;
  overflow: hidden;
}

/* E2: Overlay clips on O1 track */
.ov-clip {
  position: absolute; top: 3px; bottom: 3px;
  background: color-mix(in srgb, #F97316 20%, transparent);
  border: 1px solid #F9731660;
  border-radius: 4px; cursor: grab;
  display: flex; align-items: center; gap: 3px;
  padding: 0 5px; font-size: 9px; color: var(--text-sec);
  overflow: hidden; white-space: nowrap;
}
.ov-clip.sel { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent); }
.ov-clip:active { cursor: grabbing; }
.ov-clip-label { overflow: hidden; text-overflow: ellipsis; }

/* Waveform canvas */
.waveform { width: 100%; height: 100%; display: block; }

/* Trim handles */
.trim-handle { position: absolute; top: 0; bottom: 0; width: 18px; transform: translateX(-9px); z-index: 6; cursor: ew-resize; display: flex; align-items: center; justify-content: center; }
.trim-grip { width: 14px; height: 30px; background: var(--accent); border-radius: 3px; display: flex; align-items: center; justify-content: center; font-size: 11px; color: #fff; font-weight: 700; box-shadow: 0 0 6px rgba(0,0,0,.3); }
.trim-handle:hover .trim-grip { transform: scaleX(1.15); }

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
.export-success-file { padding: 10px; background: var(--bg-deep); border: 1px solid var(--border); border-radius: 6px; font-size: 13px; font-weight: 600; color: var(--text); margin-bottom: 12px; word-break: break-all; }
.export-drag-zone { padding: 20px; border: 2px dashed var(--accent); border-radius: 8px; text-align: center; cursor: grab; font-size: 13px; color: var(--accent); font-weight: 600; margin-bottom: 16px; transition: background .15s; }
.export-drag-zone:hover { background: color-mix(in srgb, var(--accent) 8%, transparent); }
.export-drag-zone:active { cursor: grabbing; opacity: .7; }

/* Toast */
.toast-box { position: fixed; bottom: 14px; right: 14px; z-index: 9999; background: var(--bg-card); border: 1px solid var(--accent); padding: 10px 14px; border-radius: 8px; box-shadow: 0 4px 16px rgba(0,0,0,.4); font-size: 11px; font-weight: 600; display: flex; flex-direction: column; gap: 5px; color: var(--text); }
.toast-drag { padding: 4px 8px; background: var(--bg-deep); border: 1px dashed var(--accent); border-radius: 4px; text-align: center; cursor: grab; font-size: 9px; color: var(--accent); }
.toast-drag:active { cursor: grabbing; opacity: .7; }
.toast-anim-enter-active, .toast-anim-leave-active { transition: transform .3s, opacity .3s; }
.toast-anim-enter-from, .toast-anim-leave-to { transform: translateY(14px); opacity: 0; }
</style>

<script setup lang="ts">
defineOptions({ name: 'ClipsPage' })
import { ref, computed, onMounted, onBeforeUnmount, onActivated, onDeactivated, inject, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { refDebounced } from '@vueuse/core'
import { invoke } from '@tauri-apps/api/core'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useReplayStore, type Clip } from '../stores/replay'
import { usePersistenceStore } from '../stores/persistence'
import ClipCard from '../components/ClipCard.vue'
import OverlayScrollbar from '../components/OverlayScrollbar.vue'
import ClipEditor from '../components/ClipEditor.vue'
import AdvancedEditor from '../components/AdvancedEditor.vue'
import SelectField from '../components/SelectField.vue'
import GameFilterDropdown from '../components/GameFilterDropdown.vue'
import RecordingDropdown from '../components/RecordingDropdown.vue'
import { mediaUrl } from '../utils/assets'
import { fmtDur, fmtSize, fmtRes, fmtDate } from '../utils/format'
import { viewMode } from '../composables/useViewMode'
import type { Ref } from 'vue'

const { t } = useI18n()
const replay = useReplayStore()
const persist = usePersistenceStore()
const _mediaPortRef = inject<Ref<number>>('mediaPort', ref(0))
const mediaPortNum  = computed(() => _mediaPortRef.value)

// ── Phase 4b: Debounce the raw search input by 150ms ──
const searchRaw = ref(replay.search)
const searchDebounced = refDebounced(searchRaw, 150)

// Keep store in sync with debounced value
watch(searchDebounced, (v) => { replay.search = v })
watch(() => replay.search, (v) => { if (v !== searchRaw.value) searchRaw.value = v })

// ── Phase 1d/4a: Use filteredClips directly — no more v-show isMatch() ──
const sortedSkeletons = computed(() => replay.filteredClips.filter((c: Clip) => c.isSkeleton))
const filteredRealClips = computed(() => replay.filteredClips.filter((c: Clip) => !c.isSkeleton))
// Phase 4a: cssVisibleCount derived from filteredClips (no legacy isMatch needed)
const cssVisibleCount = computed(() => filteredRealClips.value.length)
// Empty state helpers
const hasNoClipsAtAll = computed(() => totalClipCount.value === 0)
const isFilteredEmpty = computed(() => !hasNoClipsAtAll.value && cssVisibleCount.value === 0)

// ── Editor / modal state ──
const editorClip   = ref<Clip | null>(null)
const editorMode   = ref<'preview' | 'trim'>('preview')
const advancedClip = ref<Clip | null>(null)
const renameTarget = ref<Clip | null>(null)
const renameValue  = ref('')
const toast        = ref('')

function showToast(msg: string) { toast.value = msg; setTimeout(() => toast.value = '', 3500) }
function refreshClips() { replay.fetchClips(persist.state?.settings?.clip_directories?.[0] || '', true) }

async function importFolder() {
  try {
    const s = await openDialog({ directory: true, multiple: false, title: 'Import Clip Directory' })
    if (s && typeof s === 'string') {
      if (!persist.state!.settings!.clip_directories) persist.state!.settings!.clip_directories = []
      if (!persist.state!.settings!.clip_directories.includes(s)) {
        persist.state!.settings!.clip_directories.push(s)
      }
      // Save immediately — fetchClips passes the path to Rust which reads the settings
      // file on disk. The debounced watcher (500ms) is too slow; we need the file written first.
      await persist.save()
      await replay.fetchClips(s, true)
      try { await invoke('update_watch_dirs') } catch {}
    }
  } catch (e) { console.error('importFolder:', e) }
}

// ── View / sizing / grouping ──
// viewMode is a shared module-level ref (useViewMode.ts) — synced with Dashboard popover
const dateGrouped = ref(false)

interface DateGroup { date: string; label: string; clips: Clip[] }
const groupedClips = computed<DateGroup[]>(() => {
  if (!dateGrouped.value) return []
  const map = new Map<string, Clip[]>()
  for (const clip of filteredRealClips.value) {
    const dateKey = clip.created ? clip.created.split(' ')[0] : 'Unknown'
    if (!map.has(dateKey)) map.set(dateKey, [])
    map.get(dateKey)!.push(clip)
  }
  const today = new Date(); today.setHours(0, 0, 0, 0)
  const yesterday = new Date(today); yesterday.setDate(today.getDate() - 1)
  const groups: DateGroup[] = []
  for (const [dateKey, clips] of map) {
    let label = dateKey
    try {
      const d = new Date(dateKey)
      if (!isNaN(d.getTime())) {
        d.setHours(0, 0, 0, 0)
        if (d.getTime() === today.getTime()) label = 'Today'
        else if (d.getTime() === yesterday.getTime()) label = 'Yesterday'
        else label = d.toLocaleDateString('en-US', { weekday: 'long', month: 'long', day: 'numeric', year: 'numeric' })
      }
    } catch {}
    groups.push({ date: dateKey, label, clips })
  }
  return groups
})

// ── Group-by-date selection helpers ──
function groupCheckState(group: DateGroup): 'none' | 'some' | 'all' {
  const total = group.clips.length
  if (total === 0) return 'none'
  const sel = group.clips.filter(c => replay.selectedIds.has(c.id)).length
  if (sel === 0) return 'none'
  return sel === total ? 'all' : 'some'
}
function toggleGroupSelect(group: DateGroup) {
  const state = groupCheckState(group)
  if (state === 'all') {
    for (const c of group.clips) replay.selectedIds.delete(c.id)
  } else {
    for (const c of group.clips) replay.selectedIds.add(c.id)
  }
  replay.selectMode = replay.selectedIds.size > 0
}

// ★ Epic 2: Range slider (1–4) maps to column count (2–5)
// Local ref for instant visual feedback during drag; persistence only written on release (@change)
const gridSlider = ref(Math.max(1, Math.min(4, (persist.state?.settings?.clipsPerRow || 4) - 1)))
watch(() => persist.state?.settings?.clipsPerRow, (v) => {
  if (v != null) gridSlider.value = Math.max(1, Math.min(4, v - 1))
})
const gridCols = computed(() => gridSlider.value + 1)
function saveGridSlider() {
  if (persist.state?.settings) persist.state.settings.clipsPerRow = (gridSlider.value + 1) as 2 | 3 | 4 | 5
  // Reset measured row height when column count changes — the estimate
  // will be used until the ResizeObserver fires with the new card dimensions.
  measuredRowHeight.value = 0
}

// Native grid host scroll ref for OverlayScrollbar
const gridScrollRef = ref<HTMLElement | null>(null)
const listScrollRef = ref<HTMLElement | null>(null)
const groupedScrollRef = ref<HTMLElement | null>(null)

// Font scaling based on column count
const fontScale = computed(() => {
  const cols = gridCols.value
  const nameSize = cols <= 2 ? '17px' : cols === 3 ? '15px' : cols >= 5 ? '12px' : '14px'
  const metaSize = cols <= 2 ? '13px' : cols === 3 ? '12px' : cols >= 5 ? '10px' : '11px'
  return { nameSize, metaSize }
})

// Scan banner state from store
const scanActive = computed(() => replay.scanActive)
const scanCount = computed(() => replay.scanCount)

// ★ Epic 3: List view scales with the same slider — each size maps to thumb/font/padding values
const listStyles = computed(() => {
  const map: Record<number, { thumbW: string; thumbH: string; fontSize: string; padding: string }> = {
    1: { thumbW: '320px', thumbH: '180px', fontSize: '18px', padding: '16px' },
    2: { thumbW: '240px', thumbH: '135px', fontSize: '15px', padding: '12px' },
    3: { thumbW: '160px', thumbH: '90px',  fontSize: '13px', padding: '8px'  },
    4: { thumbW: '96px',  thumbH: '54px',  fontSize: '11px', padding: '6px'  },
  }
  return map[gridSlider.value] ?? map[3]
})

// ── Scroll-suppression: disable hover transitions while scrolling ──
// Also pauses prefetchThumbnails while the user is actively scrolling so the
// frame budget goes to paint/layout, not IPC/FFmpeg. Resumes ~200ms after the
// last scroll event.
const isScrolling = ref(false)
let scrollTimer: ReturnType<typeof setTimeout> | null = null
const gridScrollTop = ref(0)
function onScroll() {
  isScrolling.value = true
  replay.scrolling = true
  gridScrollTop.value = gridScrollRef.value?.scrollTop ?? 0
  if (scrollTimer) clearTimeout(scrollTimer)
  scrollTimer = setTimeout(() => {
    isScrolling.value = false
    replay.scrolling = false
    // Wake up the prefetch loop if it's sleeping
    if (prefetchWake) { prefetchWake(); prefetchWake = null }
  }, 200)
}
// Fires when prefetchThumbnails resumes after a scroll pause
let prefetchWake: (() => void) | null = null

// ── Virtual scroll: only render visible grid rows + buffer ──
// Without this, 420 ClipCards create 420 decoded thumbnail bitmaps (~1.6MB each)
// in WebKitGTK memory. Virtual scroll keeps only ~40-60 cards in the DOM.
const GRID_GAP = 16
const VIRTUAL_BUFFER_ROWS = 3

// Estimate card row height from container width + column count.
// Card = 16:9 thumbnail + ~80px info section + gap.
const estimatedRowHeight = computed(() => {
  const el = gridScrollRef.value
  if (!el) return 300
  const containerW = el.clientWidth - 32 // subtract horizontal padding
  const cols = gridCols.value
  const cardW = (containerW - GRID_GAP * (cols - 1)) / cols
  const thumbH = cardW * 9 / 16
  const infoH = cols <= 2 ? 90 : cols >= 5 ? 65 : 78
  return thumbH + infoH + GRID_GAP
})

// Measured row height (set after first render via ResizeObserver)
const measuredRowHeight = ref(0)
const rowHeight = computed(() => measuredRowHeight.value || estimatedRowHeight.value)

const virtualRange = computed(() => {
  const total = filteredRealClips.value.length
  const cols = gridCols.value
  const rh = rowHeight.value
  const el = gridScrollRef.value
  if (!el || !rh || total === 0) return { start: 0, end: total, padTop: 0, padBot: 0 }
  const clientH = el.clientHeight
  const totalRows = Math.ceil(total / cols)
  const startRow = Math.max(0, Math.floor(gridScrollTop.value / rh) - VIRTUAL_BUFFER_ROWS)
  const endRow = Math.min(totalRows, Math.ceil((gridScrollTop.value + clientH) / rh) + VIRTUAL_BUFFER_ROWS)
  return {
    start: startRow * cols,
    end: Math.min(endRow * cols, total),
    padTop: startRow * rh,
    padBot: Math.max(0, (totalRows - endRow) * rh),
  }
})

const visibleClips = computed(() => {
  const { start, end } = virtualRange.value
  return filteredRealClips.value.slice(start, end)
})

// Measure actual card height from first rendered card
let rowMeasureRO: ResizeObserver | null = null
function setupRowMeasure() {
  rowMeasureRO?.disconnect()
  rowMeasureRO = new ResizeObserver(() => {
    const grid = gridScrollRef.value?.querySelector('.native-grid')
    if (!grid) return
    const firstCard = grid.querySelector('.card') as HTMLElement
    if (firstCard) {
      measuredRowHeight.value = firstCard.offsetHeight + GRID_GAP
    }
  })
  if (gridScrollRef.value) rowMeasureRO.observe(gridScrollRef.value)
}

// ★ Epic 2: Slider drag tooltip
const SLIDER_LABELS: Record<number, string> = { 1: 'Extra Large', 2: 'Large', 3: 'Medium', 4: 'Small' }
const sliderLabel = computed(() => SLIDER_LABELS[gridSlider.value] ?? '')
const isDragging = ref(false)

// ── Filter options ──
const sortOptions = computed(() => [
  { value: 'newest',   label: t('clips.sortOptions.newest')   },
  { value: 'oldest',   label: t('clips.sortOptions.oldest')   },
  { value: 'longest',  label: t('clips.sortOptions.longest')  },
  { value: 'shortest', label: t('clips.sortOptions.shortest') },
])
// ★ Epic 3: game list and per-game counts for the multiselect dropdown
const gameNames = computed(() => replay.games.filter((g: string) => g !== 'all'))
const gameCounts = computed(() => {
  const counts: Record<string, number> = {}
  for (const c of replay.clips) {
    if (!c.isSkeleton && c.game) counts[c.game] = (counts[c.game] || 0) + 1
  }
  return counts
})
// kept for legacy SelectField usage (sort) — game SelectField is replaced by GameFilterDropdown
// Uses gameCounts (single-pass O(clips)) instead of re-filtering per game O(clips × games)
const gameOptions = computed(() =>
  replay.games.map((g: string) => {
    if (g === 'all') return { value: g, label: 'All Games' }
    return { value: g, label: `${g} (${gameCounts.value[g] || 0})` }
  })
)
// Total real (non-skeleton) clip count for display
const totalClipCount = computed(() => replay.clips.filter((c: Clip) => !c.isSkeleton).length)

// ── Card interactions ──
function onCardClick(clip: Clip) {
  if (clip.isSkeleton) return
  if (replay.selectMode) { replay.toggleSelect(clip.id); return }
  if (persist.state?.settings?.defaultClickAction === 'editor') openAdvanced(clip)
  else openPreview(clip)
}
function openPreview(clip: Clip) { editorClip.value = clip; editorMode.value = 'preview' }
function openAdvanced(clip: Clip) { advancedClip.value = clip }

function startRename(clip: Clip) { renameTarget.value = clip; renameValue.value = clip.custom_name || (clip.game !== 'Unknown' ? clip.game : clip.filename.replace(/\.[^.]+$/, '')) }
async function confirmRename() {
  if (!renameTarget.value) return
  const n = renameValue.value.trim()
  replay.updateClipMeta(renameTarget.value.filepath, { custom_name: n })
  try { await invoke('set_clip_meta', { update: { filepath: renameTarget.value.filepath, custom_name: n, favorite: renameTarget.value.favorite } }) } catch {}
  renameTarget.value = null
}

async function deleteClip(clip: Clip) {
  if (!confirm(`Delete "${clip.custom_name || (clip.game !== 'Unknown' ? clip.game : clip.filename)}"?`)) return
  try { await invoke('delete_clip', { filepath: clip.filepath }); replay.removeClip(clip.filepath); showToast('Clip deleted') } catch (e) { showToast(`Error: ${e}`) }
}

function openListMenu(clip: Clip, e: MouseEvent) {
  e.preventDefault(); e.stopPropagation()
  const menuW = 200, menuH = 270
  const x = e.clientX + menuW > window.innerWidth ? e.clientX - menuW : e.clientX
  const y = e.clientY + menuH > window.innerHeight ? e.clientY - menuH : e.clientY
  replay.activeMenuClipId = clip.id
  replay.activeMenuPos = { x, y }
}
async function toggleListFav(e: Event, clip: Clip) {
  e.stopPropagation()
  const v = !clip.favorite
  replay.updateClipMeta(clip.filepath, { favorite: v })
  try { await invoke('set_clip_meta', { update: { filepath: clip.filepath, custom_name: clip.custom_name, favorite: v } }) } catch {}
}

async function deleteSelected() {
  const ids = Array.from(replay.selectedIds)
  const clips = replay.clips.filter(c => ids.includes(c.id))
  if (!confirm(`Delete ${clips.length} clip(s)?`)) return
  for (const c of clips) { try { await invoke('delete_clip', { filepath: c.filepath }); replay.removeClip(c.filepath) } catch {} }
  replay.clearSelection(); showToast(`${clips.length} clip(s) deleted`)
}

// ── Live file-watcher (Epic 3 fix) ──
// Instead of a separate pendingPaths array (which caused the grid to hide),
// we inject a skeleton directly into replay.clips so the existing grid
// keeps rendering normally while the new file's metadata is fetched.
let unlistenAdded:   UnlistenFn | null = null
let unlistenRemoved: UnlistenFn | null = null

async function prefetchThumbnails() {
  // Re-entry guard. Also guards against the watch() below firing while we're already
  // running — since applyBulkProbe/applyProbeAndThumb mutate clips[], triggerRef can
  // cause the watch to re-fire on every flush.
  if (replay.isPrefetching) return
  // Include clips missing thumbnail OR missing duration/resolution — both need processing
  const needsWork = replay.filteredClips.filter((c: Clip) =>
    !c.isSkeleton && ((!c.thumbnail && !replay.liveThumbs.get(c.id)) || c.duration === 0)
  )
  if (!needsWork.length) return
  // Set the gate SYNCHRONOUSLY before the first await so ClipCard IO observers,
  // which also check replay.isPrefetching, can never race this function.
  replay.isPrefetching = true
  if (import.meta.env.DEV) console.debug(`[perf] prefetchThumbnails: ${needsWork.length} clips`)

  // ── Phase 1: BULK PROBE ──────────────────────────────────────────────
  // One probe_clips call for EVERY unprobed filepath. Rust's semaphore parallelizes
  // ffprobe 4-wide internally, so this is the fastest way to fill metadata.
  // Previously this was N separate 1-clip calls — 224 of them in a 22s session per
  // the perf log — each paying IPC + semaphore setup overhead and contending with
  // thumbnail ffmpegs. Now it's one call that yields duration/resolution for every
  // card at once, and the thumbnail loop only needs to spawn ffmpeg.
  const unprobedFps = needsWork.filter(c => c.duration === 0).map(c => c.filepath)
  const probeMap = new Map<string, { duration: number; width: number; height: number }>()
  if (unprobedFps.length) {
    try {
      const probed = await invoke<[string, number, number, number][]>('probe_clips', { filepaths: unprobedFps })
      for (const [fp, duration, width, height] of probed) {
        probeMap.set(fp, { duration, width, height })
      }
      // Single-shot bulk apply: one triggerRef for liveMeta + one rAF flush.
      // Duration badges + resolution pills appear on every probed card at once.
      replay.applyBulkProbe(probed)
    } catch (e) {
      if (import.meta.env.DEV) console.warn('[perf] bulk probe failed', e)
    }
  }

  // ── Phase 2: SEQUENTIAL THUMBNAIL LOOP (newest→oldest) ───────────────
  try {
    for (const clip of needsWork) {
      // Yield to scroll: if user is actively scrolling, wait for scroll to stop
      // before spawning the next ffmpeg. Keeps the frame budget clean.
      if (replay.scrolling) {
        if (import.meta.env.DEV) console.debug('[perf] prefetch paused (scrolling)')
        await new Promise<void>(resolve => { prefetchWake = resolve })
        if (import.meta.env.DEV) console.debug('[perf] prefetch resumed')
      }
      try {
        // Prefer freshly-probed values from Phase 1 over the (stale) captured clip fields.
        const probed = probeMap.get(clip.filepath)
        const duration = probed?.duration ?? clip.duration
        const width = probed?.width ?? clip.width
        const height = probed?.height ?? clip.height

        let thumbPath = clip.thumbnail || replay.liveThumbs.get(clip.id) || ''
        if (!thumbPath) {
          // Pass duration so Rust skips the redundant ffprobe inside generate_thumbnail
          thumbPath = await invoke<string>('generate_thumbnail', {
            filepath: clip.filepath,
            duration: duration > 0 ? duration : undefined,
          })
        }
        if (thumbPath) {
          replay.applyProbeAndThumb(clip.filepath, duration, width, height, clip.id, thumbPath)
        }
      } catch {}
    }
  } finally {
    replay.isPrefetching = false
    // Final authoritative trigger so sort-by-duration / "longest" etc. reflect
    // the freshly-probed values on the next natural recompute.
    replay.flushClipsNow()
  }
}

// Reset scroll to top when filters/search change so virtual range recalculates correctly
watch([searchDebounced, () => replay.sortMode, () => replay.filterFav, () => replay.selectedGames], () => {
  if (gridScrollRef.value) {
    gridScrollRef.value.scrollTop = 0
    gridScrollTop.value = 0
  }
})

// Re-run prefetchThumbnails every time fetchClips successfully populates the store.
// This handles the initial mount AND subsequent re-fetches (e.g., user imports clips in
// Settings after launching with the reset sentinel path). Without this, the prefetch only
// ran once at mount with 0 clips and all subsequent clips were handled by the IO fallback,
// firing 200+ single-clip probe_clips calls and killing scroll perf.
// flush: 'sync' ensures the gate is set before any ClipCard IO observers can fire.
// prefetchThumbnails has its own re-entry guard against the triggerRef storm inside itself.
watch(() => replay.clipsLoadedAt, () => { prefetchThumbnails() }, { flush: 'sync' })

onMounted(async () => {
  if (!persist.loaded) await persist.load()
  replay.fetchStatus()
  setupRowMeasure()
  await replay.fetchClips(persist.state?.settings?.clip_directories?.[0] || '')
  // The clipsLoadedAt watch above will have fired during fetchClips, kicking off prefetch.
  // No need to call it manually here.

  // ── Cross-page preview: open modal for clip navigated from Dashboard ──
  if (replay.previewTargetClipId) {
    const id = replay.previewTargetClipId
    replay.previewTargetClipId = null
    // Clips may not be loaded yet — wait for them then open
    const tryOpen = () => {
      const clip = replay.clips.find(c => c.id === id)
      if (clip) { openPreview(clip); return }
      // Retry once after fetch settles
      const stop = watch(() => replay.clips, () => {
        const c2 = replay.clips.find(c => c.id === id)
        if (c2) { openPreview(c2); stop() }
      })
    }
    tryOpen()
  }

  // ── Watch for subsequent same-route navigations (KeepAlive keeps this mounted) ──
  watch(() => replay.previewTargetClipId, (id) => {
    if (!id) return
    replay.previewTargetClipId = null
    const clip = replay.clips.find(c => c.id === id)
    if (clip) { openPreview(clip); return }
    // Clips may still be loading — retry when list updates
    const stop = watch(() => replay.clips, () => {
      const c2 = replay.clips.find(c => c.id === id)
      if (c2) { openPreview(c2); stop() }
    })
  })

  unlistenAdded = await listen<string>('clip_added', async (event) => {
    const fp = event.payload
    if (replay.clips.find(c => c.filepath === fp && !c.isSkeleton)) return
    const tempId = `skeleton_${fp}`
    replay.injectSkeleton(tempId, fp)  // grid stays visible with placeholder
    // Retry polling: probe the file up to 5 times with exponential backoff.
    // Handles both fast muxers (~500ms) and slow ones (large files, ~4s).
    let clip: Clip | null = null
    let delay = 500
    for (let attempt = 0; attempt < 5; attempt++) {
      await new Promise<void>(r => setTimeout(r, delay))
      try {
        clip = await invoke<Clip | null>('get_clip_by_path', { filepath: fp })
        if (clip && clip.duration > 0) break
      } catch { /* file not ready yet */ }
      delay = Math.min(delay * 2, 4000)
    }
    if (clip) replay.replaceSkeleton(tempId, clip)
    else replay.removeClip(tempId)
  })

  unlistenRemoved = await listen<string>('clip_removed', (event) => {
    replay.removeClip(event.payload)
  })
})

onBeforeUnmount(() => {
  unlistenAdded?.()
  unlistenRemoved?.()
  rowMeasureRO?.disconnect()
})

// ── RAM: Release decoded thumbnail bitmaps when navigating away from Clips ──
// KeepAlive preserves the full component tree. Without this, 420 decoded images
// (~170-670MB) stay in WebKitGTK memory while viewing Mixer/Settings/etc.
// Setting pageActive=false causes ClipCard's IO to clear thumbUrl (removing <img>
// from DOM). On reactivation, IO restores visible cards from resolvedThumbPath.
onDeactivated(() => { replay.pageActive = false })
onActivated(() => { replay.pageActive = true })

// ── Epic 1 Bug 1: Smart bulk favorite (toggle) ──
const allSelectedFavorited = computed(() => {
  const ids = Array.from(replay.selectedIds)
  const sel = replay.clips.filter(c => ids.includes(c.id) && !c.isSkeleton)
  return sel.length > 0 && sel.every(c => c.favorite)
})

async function bulkFavorite() {
  const ids = Array.from(replay.selectedIds)
  const clips = replay.clips.filter(c => ids.includes(c.id) && !c.isSkeleton)
  const newFav = !allSelectedFavorited.value
  for (const c of clips) {
    replay.updateClipMeta(c.filepath, { favorite: newFav })
    try { await invoke('set_clip_meta', { update: { filepath: c.filepath, custom_name: c.custom_name || '', favorite: newFav } }) } catch {}
  }
  showToast(`${newFav ? '❤' : '💔'} ${clips.length} clip(s) ${newFav ? 'favorited' : 'unfavorited'}`)
  replay.clearSelection()
}

// ── Epic 1 Bug 2: Bulk game change with search ──
const bulkGameOpen   = ref(false)
const bulkGameValue  = ref('all')
const bulkGameSearch = ref('')

const filteredBulkGames = computed(() => {
  if (!bulkGameSearch.value) return gameOptions.value
  const q = bulkGameSearch.value.toLowerCase()
  return gameOptions.value.filter(o => o.label.toLowerCase().includes(q))
})

async function bulkChangeGame() {
  const ids = Array.from(replay.selectedIds)
  const clips = replay.clips.filter(c => ids.includes(c.id) && !c.isSkeleton)
  const game_tag = bulkGameValue.value === 'all' ? '' : bulkGameValue.value
  for (const c of clips) {
    replay.updateClipMeta(c.filepath, { game: game_tag })
    try { await invoke('set_clip_meta', { update: { filepath: c.filepath, custom_name: c.custom_name || '', favorite: c.favorite, game_tag } }) } catch {}
  }
  bulkGameOpen.value = false; bulkGameSearch.value = ''
  showToast(`🎮 Game updated for ${clips.length} clip(s)`)
  replay.clearSelection()
}

// Close bulk-game drop when clicking outside
function onBulkOutside(e: MouseEvent) {
  if (!(e.target as HTMLElement).closest('.bulk-game-wrap')) bulkGameOpen.value = false
}
onMounted(() => document.addEventListener('mousedown', onBulkOutside))
onBeforeUnmount(() => document.removeEventListener('mousedown', onBulkOutside))

// Single page-level context menu + single global click listener
const contextMenuClip = computed(() =>
  replay.clips.find(c => c.id === replay.activeMenuClipId) ?? null
)

function ctxAction(action: string) {
  const clip = contextMenuClip.value
  replay.activeMenuClipId = ''
  if (!clip) return
  switch (action) {
    case 'preview': openPreview(clip); break
    case 'editor': openAdvanced(clip); break
    case 'select': replay.toggleSelect(clip.id); break
    case 'favorite': {
      const v = !clip.favorite
      replay.updateClipMeta(clip.filepath, { favorite: v })
      invoke('set_clip_meta', { update: { filepath: clip.filepath, custom_name: clip.custom_name, favorite: v } }).catch(() => {})
      break
    }
    case 'location': invoke('open_file_location', { filepath: clip.filepath }).catch(() => {}); break
    case 'rename': startRename(clip); break
    case 'delete': deleteClip(clip); break
  }
}

function closeContextMenu(e: MouseEvent) {
  const t = e.target as HTMLElement
  if (t.closest('.ctx-menu') || t.closest('.kebab')) return
  replay.activeMenuClipId = ''
}

onMounted(() => document.addEventListener('mousedown', closeContextMenu))
onBeforeUnmount(() => document.removeEventListener('mousedown', closeContextMenu))
</script>

<template>
  <div class="page">
    <!-- Header -->
    <div class="header">
      <h1 class="title">Clips <span class="title-count">{{ totalClipCount }}</span></h1>
    </div>

    <!-- Controls — left group + right group so toggle is always far right -->
    <div class="ctrl-bar">
      <div class="ctrl-left">
        <RecordingDropdown />
        <div class="search-wrap">
          <svg class="search-ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
          <!-- Phase 4b: bound to searchRaw; debounced 150ms before hitting store -->
          <input v-model="searchRaw" placeholder="Search clips…" class="search" />
        </div>
        <SelectField class="ctrl-sort" v-model="replay.sortMode" :options="sortOptions" />
        <!-- ★ Epic 3: Multiselect game filter dropdown -->
        <GameFilterDropdown
          v-model="replay.selectedGames"
          :games="gameNames"
          :clipCounts="gameCounts"
        />
        <button class="fav-btn" :class="{ active: replay.filterFav }" @click="replay.filterFav=!replay.filterFav">❤ {{ replay.favCount }}</button>
      </div>

      <!-- ★ Epic 2: Grid size slider + view toggle pinned to right -->
      <div class="ctrl-right">
        <div class="size-slider-wrap">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="size-ic"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/></svg>
          <div class="size-range-wrap" @wheel.prevent="e => { gridSlider = Math.max(1, Math.min(4, gridSlider + (e.deltaY > 0 ? 1 : -1))); saveGridSlider() }">
            <input
              type="range" min="1" max="4" step="1"
              v-model.number="gridSlider"
              @change="saveGridSlider"
              @mousedown="isDragging = true" @touchstart="isDragging = true"
              @mouseup="isDragging = false" @touchend="isDragging = false"
              @mouseleave="isDragging = false"
              class="size-range"
            />
            <Transition name="tip-fade">
              <div v-if="isDragging" class="size-tip" :style="{ left: ((gridSlider - 1) / 3 * 100) + '%' }">
                {{ sliderLabel }}
              </div>
            </Transition>
          </div>
        </div>
        <button class="vt-btn" :class="{ active: dateGrouped }" @click="dateGrouped = !dateGrouped" title="Group by date" style="border-radius:var(--radius); border:1px solid var(--border); margin-right:4px;">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="4" width="18" height="18" rx="2"/><line x1="16" y1="2" x2="16" y2="6"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="3" y1="10" x2="21" y2="10"/></svg>
        </button>
        <div class="view-toggle">
          <button class="vt-btn" :class="{ active: viewMode==='grid' }" @click="viewMode='grid'" title="Grid view">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/></svg>
          </button>
          <button class="vt-btn" :class="{ active: viewMode==='list' }" @click="viewMode='list'" title="List view">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="3" y1="6" x2="21" y2="6"/><line x1="3" y1="12" x2="21" y2="12"/><line x1="3" y1="18" x2="21" y2="18"/></svg>
          </button>
        </div>
      </div>
    </div>

    <div class="scroll-area">
      <!-- Initial loading skeleton -->
      <div v-if="replay.loading && !replay.loaded" class="clip-grid" :style="{ gridTemplateColumns: `repeat(${gridCols}, 1fr)` }">
        <div v-for="i in gridCols * 2" :key="'sk'+i" class="skeleton-card">
          <div class="skeleton-thumb animate-pulse"></div>
          <div class="skeleton-info">
            <div class="skeleton-line w70 animate-pulse"></div>
            <div class="skeleton-line w40 animate-pulse"></div>
          </div>
        </div>
      </div>

      <!-- View wrapper: fade when switching modes -->
      <Transition name="view-fade" mode="out-in">

      <!-- ═══ Date-grouped view ═══ -->
      <div v-if="dateGrouped" key="grouped" class="scroll-host">
        <div class="native-grid-host grouped-host" ref="groupedScrollRef" :class="{ scrolling: isScrolling }" @scroll.passive="onScroll">
          <div class="date-groups">
          <div v-for="group in groupedClips" :key="group.date" class="date-group">
            <div class="date-header">
              <div
                class="group-sel-box"
                :class="{ checked: groupCheckState(group) === 'all', indeterminate: groupCheckState(group) === 'some' }"
                @click.stop="toggleGroupSelect(group)"
              >
                <span v-if="groupCheckState(group) === 'all'">✓</span>
                <span v-else-if="groupCheckState(group) === 'some'">−</span>
              </div>
              <span class="date-label">{{ group.label }}</span>
              <span class="date-count">{{ group.clips.length }}</span>
            </div>
            <div v-if="viewMode === 'grid'" class="clip-grid" :style="{ gridTemplateColumns: `repeat(${gridCols}, 1fr)` }">
              <ClipCard
                v-for="clip in group.clips"
                :key="clip.id"
                :clip="clip"
                :selected="replay.isSelected(clip.id)"
                class="clip-stagger"
                @click="onCardClick(clip)"
                @preview="openPreview"
                @editor="openAdvanced"
                @rename="startRename"
                @delete="deleteClip"
              />
            </div>
            <div v-else class="clip-list" :style="{ '--list-thumb-w': listStyles.thumbW, '--list-thumb-h': listStyles.thumbH, '--list-font': listStyles.fontSize, '--list-pad': listStyles.padding }">
              <div
                v-for="clip in group.clips"
                :key="clip.id"
                class="list-row"
                :class="{ selected: replay.isSelected(clip.id) }"
                @click="onCardClick(clip)"
                @contextmenu.prevent="openListMenu(clip, $event)"
              >
                <div class="list-thumb-wrap">
                  <img v-if="(replay.liveThumbs.get(clip.id) || clip.thumbnail) && mediaPortNum" class="list-thumb" :src="mediaUrl(replay.liveThumbs.get(clip.id) || clip.thumbnail, mediaPortNum)" loading="lazy" decoding="async" @error="(e: Event) => ((e.target as HTMLImageElement).style.display='none')" />
                  <div v-else class="list-thumb list-thumb-empty">▶</div>
                  <span v-if="replay.liveMeta.get(clip.id)?.duration || clip.duration" class="list-badge">{{ fmtDur(replay.liveMeta.get(clip.id)?.duration || clip.duration) }}</span>
                  <button class="lt-heart" :class="{ on: clip.favorite }" @click.stop="toggleListFav($event, clip)" title="Favorite">
                    <svg viewBox="0 0 24 24" :fill="clip.favorite ? 'currentColor' : 'none'" stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
                  </button>
                  <div class="lt-sel-ov" :class="{ vis: replay.isSelected(clip.id) || replay.selectMode }" @click.stop="replay.toggleSelect(clip.id)">
                    <div class="lt-sel-box" :class="{ checked: replay.isSelected(clip.id) }">✓</div>
                  </div>
                </div>
                <div class="list-info">
                  <span class="list-name">{{ clip.custom_name || (clip.game !== 'Unknown' ? clip.game : clip.filename.replace(/\.[^.]+$/, '')) }}</span>
                  <span class="list-meta">
                    <span v-if="clip.game && clip.game !== 'Unknown'" class="lm-game">{{ clip.game }}</span>
                    <span v-if="clip.filesize" class="lm-pill">{{ fmtSize(clip.filesize) }}</span>
                    <span v-if="replay.liveMeta.get(clip.id)?.width || clip.width" class="lm-pill">{{ fmtRes(replay.liveMeta.get(clip.id)?.width || clip.width, replay.liveMeta.get(clip.id)?.height || clip.height) }}</span>
                    <span v-if="clip.created" class="lm-pill lm-date">{{ fmtDate(clip.created) }}</span>
                  </span>
                </div>
                <div class="list-actions">
                  <button class="list-act" @click.stop="openPreview(clip)">{{ t('clips.contextMenu.preview') }}</button>
                  <button class="list-act" @click.stop="openAdvanced(clip)">{{ t('clips.contextMenu.edit') }}</button>
                  <button class="list-act list-act-d" @click.stop="deleteClip(clip)">🗑</button>
                  <button class="list-kebab" @click.stop="openListMenu(clip, $event)" title="More options">
                    <svg viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="5" r="1.5"/><circle cx="12" cy="12" r="1.5"/><circle cx="12" cy="19" r="1.5"/></svg>
                  </button>
                </div>
              </div>
            </div>
          </div>
          <div v-if="groupedClips.length === 0" class="empty-state">
            <div class="empty-ic">🔍</div>
            <p>No clips found</p>
            <p class="empty-sub">Try again or adjust your filters</p>
          </div>
          </div><!-- /date-groups -->
        </div>
        <OverlayScrollbar :scroll-el="groupedScrollRef" />
      </div>

      <!-- ═══ Flat grid view — native CSS Grid ═══ -->
      <div v-else-if="viewMode === 'grid'" key="grid" class="scroll-host">
        <div class="native-grid-host" ref="gridScrollRef" :class="{ scrolling: isScrolling }" @scroll.passive="onScroll">
          <!-- Scan banner -->
          <div v-if="scanActive" class="scan-banner">
            <div class="scan-spinner"></div>
            <span>Scanning for new clips…</span>
            <span v-if="scanCount > 0" class="scan-count">+{{ scanCount }} found</span>
          </div>
          <!-- Skeleton cards from file watcher -->
          <div v-if="sortedSkeletons.length" class="clip-grid skeletons-row" :style="{ gridTemplateColumns: `repeat(${gridCols}, 1fr)` }">
            <div v-for="clip in sortedSkeletons" :key="clip.id" class="skeleton-card watcher-skeleton">
              <div class="skeleton-thumb animate-pulse"></div>
              <div class="skeleton-info">
                <div class="skeleton-line w70 animate-pulse"></div>
                <div class="skeleton-line w40 animate-pulse"></div>
              </div>
              <div class="watcher-label">New clip detected…</div>
            </div>
          </div>
          <!-- Virtual CSS grid — only visible rows + buffer rendered -->
          <div
            class="clip-grid native-grid"
            :style="{
              gridTemplateColumns: `repeat(${gridCols}, 1fr)`,
              '--name-size': fontScale.nameSize,
              '--meta-size': fontScale.metaSize,
              paddingTop: virtualRange.padTop + 'px',
              paddingBottom: virtualRange.padBot + 'px',
            }"
          >
            <ClipCard
              v-for="clip in visibleClips"
              :key="clip.id"
              :clip="clip"
              :selected="replay.isSelected(clip.id)"
              :class="{ 'clip-enter': clip._isNew }"
              @click="onCardClick(clip)"
              @preview="openPreview"
              @editor="openAdvanced"
              @rename="startRename"
              @delete="deleteClip"
            />
          </div>
        </div>
        <OverlayScrollbar :scroll-el="gridScrollRef" />
      </div>

      <!-- ═══ Flat list view — native scroll ═══ -->
      <div
        v-else
        key="list"
        class="scroll-host"
      >
        <div
          class="native-grid-host"
          ref="listScrollRef"
          :class="{ scrolling: isScrolling }"
          @scroll.passive="onScroll"
          :style="{
            '--list-thumb-w': listStyles.thumbW,
            '--list-thumb-h': listStyles.thumbH,
            '--list-font':    listStyles.fontSize,
            '--list-pad':     listStyles.padding,
          }"
        >
          <div class="clip-list">
            <div
              v-for="clip in filteredRealClips"
              :key="clip.id"
              class="list-row"
              :class="{ selected: replay.isSelected(clip.id) }"
              @click="onCardClick(clip)"
              @contextmenu.prevent="openListMenu(clip, $event)"
            >
              <div class="list-thumb-wrap">
                <img v-if="(replay.liveThumbs.get(clip.id) || clip.thumbnail) && mediaPortNum"
                     class="list-thumb"
                     :src="mediaUrl(replay.liveThumbs.get(clip.id) || clip.thumbnail, mediaPortNum)"
                     loading="lazy" decoding="async"
                     @error="(e: Event) => ((e.target as HTMLImageElement).style.display='none')" />
                <div v-else class="list-thumb list-thumb-empty">▶</div>
                <span v-if="replay.liveMeta.get(clip.id)?.duration || clip.duration" class="list-badge">{{ fmtDur(replay.liveMeta.get(clip.id)?.duration || clip.duration) }}</span>
                <button class="lt-heart" :class="{ on: clip.favorite }" @click.stop="toggleListFav($event, clip)" title="Favorite">
                  <svg viewBox="0 0 24 24" :fill="clip.favorite ? 'currentColor' : 'none'" stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
                </button>
                <div class="lt-sel-ov" :class="{ vis: replay.isSelected(clip.id) || replay.selectMode }" @click.stop="replay.toggleSelect(clip.id)">
                  <div class="lt-sel-box" :class="{ checked: replay.isSelected(clip.id) }">✓</div>
                </div>
              </div>
              <div class="list-info">
                <span class="list-name">{{ clip.custom_name || (clip.game !== 'Unknown' ? clip.game : clip.filename.replace(/\.[^.]+$/, '')) }}</span>
                <span class="list-meta">
                  <span v-if="clip.game && clip.game !== 'Unknown'" class="lm-game">{{ clip.game }}</span>
                  <span v-if="clip.filesize" class="lm-pill">{{ fmtSize(clip.filesize) }}</span>
                  <span v-if="replay.liveMeta.get(clip.id)?.width || clip.width" class="lm-pill">{{ fmtRes(replay.liveMeta.get(clip.id)?.width || clip.width, replay.liveMeta.get(clip.id)?.height || clip.height) }}</span>
                  <span v-if="clip.created" class="lm-pill lm-date">{{ fmtDate(clip.created) }}</span>
                </span>
              </div>
              <div class="list-actions">
                <button class="list-act" @click.stop="openPreview(clip)">Preview</button>
                <button class="list-act" @click.stop="openAdvanced(clip)">Edit</button>
                <button class="list-act list-act-d" @click.stop="deleteClip(clip)">🗑</button>
                <button class="list-kebab" @click.stop="openListMenu(clip, $event)" title="More options">
                  <svg viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="5" r="1.5"/><circle cx="12" cy="12" r="1.5"/><circle cx="12" cy="19" r="1.5"/></svg>
                </button>
              </div>
            </div>
          </div>
        </div>
        <OverlayScrollbar :scroll-el="listScrollRef" />
      </div>

      </Transition><!-- /view-fade -->

      <!-- Empty state A: no clips exist at all -->
      <div
        v-if="!replay.loading && replay.loaded && hasNoClipsAtAll"
        class="empty-state"
      >
        <div class="empty-ic">📁</div>
        <p>No clips found</p>
        <p class="empty-sub">Start recording some or import them if you have any</p>
        <button class="empty-import-btn" @click="importFolder">Import Folder</button>
      </div>

      <!-- Empty state B: clips exist but filtered/searched to zero -->
      <div
        v-if="!replay.loading && replay.loaded && isFilteredEmpty"
        class="empty-state"
      >
        <div class="empty-ic">🔍</div>
        <p>No clips found</p>
        <p class="empty-sub">Try again or adjust your filters</p>
      </div>
    </div>

    <!-- ★ Epic 1: Multi-select bulk action bar -->
    <Transition name="slide-up">
      <div v-if="replay.selectMode" class="sel-bar">
        <span>{{ replay.selectedCount }} selected</span>
        <div style="flex:1"></div>

        <!-- Bug 1: Smart toggle — Unfavorite All when all selected are already favorited -->
        <button class="sel-btn" :class="{ 'sel-btn-fav': !allSelectedFavorited, 'sel-btn-unfav': allSelectedFavorited }" @click="bulkFavorite()">
          {{ allSelectedFavorited ? t('clips.bulkActions.unfavoriteAll') : t('clips.bulkActions.favoriteAll') }}
        </button>

        <!-- Bug 2: Change Game drop-up with search filter -->
        <div class="bulk-game-wrap">
          <button class="sel-btn" :class="{ active: bulkGameOpen }" @click="bulkGameOpen = !bulkGameOpen">
            🎮 Change Game
          </button>
          <Transition name="dropup">
            <div v-if="bulkGameOpen" class="bulk-game-drop">
              <div class="bulk-game-search-wrap">
                <input
                  v-model="bulkGameSearch"
                  class="bulk-game-search"
                  placeholder="Filter games…"
                  autofocus
                  @keydown.escape="bulkGameOpen = false"
                />
              </div>
              <div class="bulk-game-list">
                <button
                  v-for="opt in filteredBulkGames"
                  :key="opt.value"
                  class="bulk-game-opt"
                  :class="{ active: bulkGameValue === opt.value }"
                  @click="bulkGameValue = opt.value"
                >
                  {{ opt.label }}
                </button>
                <div v-if="filteredBulkGames.length === 0" class="bulk-game-empty">No matches</div>
              </div>
              <div class="bulk-game-footer">
                <button class="sel-btn sel-btn-p" @click="bulkChangeGame()">Apply</button>
              </div>
            </div>
          </Transition>
        </div>

        <button class="sel-btn" @click="replay.clearSelection()">{{ t('clips.bulkActions.clear') }}</button>
        <button class="sel-btn sel-btn-d" @click="deleteSelected()">🗑 {{ t('clips.bulkActions.delete') }}</button>
      </div>
    </Transition>

    <!-- Rename dialog -->
    <Teleport to="body">
      <div v-if="renameTarget" class="dlg-ov" @click.self="renameTarget=null">
        <div class="dlg">
          <h3>{{ t('clips.renameDialog.title') }}</h3>
          <input v-model="renameValue" class="dlg-in" @keydown.enter="confirmRename" autofocus />
          <div class="dlg-btns">
            <button class="dlg-btn" @click="renameTarget=null">Cancel</button>
            <button class="dlg-btn dlg-pri" @click="confirmRename">{{ t('clips.renameDialog.save') }}</button>
          </div>
        </div>
      </div>
    </Teleport>

    <Transition name="fade"><div v-if="toast" class="toast">{{ toast }}</div></Transition>
    <ClipEditor v-if="editorClip && !advancedClip" :clip="editorClip" :mode="editorMode" @close="editorClip=null" @saved="refreshClips" @toast="showToast" />
    <AdvancedEditor v-if="advancedClip" :clip="advancedClip" @close="advancedClip=null" />

    <!-- Single page-level context menu (grid + list views) -->
    <Teleport to="body">
      <div
        v-if="replay.activeMenuClipId"
        class="ctx-menu"
        :style="{ left: replay.activeMenuPos.x + 'px', top: replay.activeMenuPos.y + 'px' }"
        @click.stop
      >
        <template v-if="contextMenuClip">
          <button class="ctx-item" @click="ctxAction('preview')">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><polygon points="5 3 19 12 5 21 5 3"/></svg>
            {{ t('clips.contextMenu.preview') }}
          </button>
          <button class="ctx-item" @click="ctxAction('editor')">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M6 20h-2a2 2 0 01-2-2v-2m0-4V8m0-4V4a2 2 0 012-2h2m4 0h4m4 0h2a2 2 0 012 2v2m0 4v4m0 4v2a2 2 0 01-2 2h-2m-4 0h-4"/><path d="M9 11l2 2 4-4"/></svg>
            {{ t('clips.contextMenu.edit') }}
          </button>
          <div class="ctx-sep"></div>
          <button class="ctx-item" @click="ctxAction('select')">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><polyline points="9 11 12 14 20 6"/></svg>
            {{ t('clips.contextMenu.select') }}
          </button>
          <button class="ctx-item" @click="ctxAction('favorite')">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
            {{ contextMenuClip.favorite ? t('clips.contextMenu.unfavorite') : t('clips.contextMenu.favorite') }}
          </button>
          <div class="ctx-sep"></div>
          <button class="ctx-item" @click="ctxAction('rename')">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 013 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>
            {{ t('clips.contextMenu.rename') }}
          </button>
          <button class="ctx-item" @click="ctxAction('location')">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>
            {{ t('clips.contextMenu.showInFolder') }}
          </button>
          <div class="ctx-sep"></div>
          <button class="ctx-item ctx-item-d" @click="ctxAction('delete')">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/><path d="M9 6V4a1 1 0 011-1h4a1 1 0 011 1v2"/></svg>
            {{ t('clips.contextMenu.delete') }}
          </button>
        </template>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.page { display:flex; flex-direction:column; gap:14px; height:100%; }
.header { display:flex; align-items:center; justify-content:space-between; flex-shrink:0; }
.title { font-size:22px; font-weight:700; }
.header-r { display:flex; align-items:center; gap:8px; }
.ib { width:30px; height:30px; border:1px solid var(--border); border-radius:6px; background:var(--bg-card); color:var(--text-sec); cursor:pointer; display:flex; align-items:center; justify-content:center; font-size:13px; }
.ib:hover { background:var(--bg-hover); } .ib:disabled { opacity:.4; }

/* ★ Epic 2: Two-group ctrl-bar — left flows, right is pinned */
.ctrl-bar { display:flex; align-items:center; gap:10px; flex-shrink:0; }
.ctrl-left { display:flex; align-items:center; gap:8px; flex:1; flex-wrap:wrap; min-width:0; }
.ctrl-right { display:flex; align-items:center; gap:8px; flex-shrink:0; }

.search-wrap { flex:1; min-width:160px; max-width:340px; position:relative; }
.search-ic { position:absolute; left:10px; top:50%; transform:translateY(-50%); width:15px; height:15px; color:var(--text-muted); pointer-events:none; }
.search { width:100%; padding:7px 12px 7px 32px; background:var(--bg-card); border:1px solid var(--border); border-radius:7px; color:var(--text); font-size:13px; outline:none; color-scheme:dark; }
.search:focus { border-color:var(--accent); } .search::placeholder { color:var(--text-muted); }
.ctrl-sort { width:120px; }
.title-count { font-size:14px; font-weight:400; color:var(--text-muted); margin-left:8px; }

/* ★ Epic 2: Mixer-style grid size slider */
.size-slider-wrap { display:flex; align-items:center; gap:6px; padding:0 10px; height:32px; background:var(--bg-card); border:1px solid var(--border); border-radius:7px; user-select:none; }
.size-ic { width:14px; height:14px; color:var(--text-muted); flex-shrink:0; }
.size-range-wrap { position:relative; display:flex; align-items:center; }

.size-range {
  -webkit-appearance: none; appearance: none;
  width: 72px; height: 6px;
  background: var(--bg-deep); border: 1px solid var(--border); border-radius: 3px;
  outline: none; cursor: pointer; user-select: none;
}
/* Rectangular thumb — same style as Audio Mixer fader */
.size-range::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 12px; height: 24px; border-radius: 4px;
  background: var(--accent); border: none;
  cursor: grab; transition: transform .1s;
}
.size-range::-webkit-slider-thumb:active { cursor: grabbing; transform: scaleY(1.1); }
.size-range::-moz-range-thumb {
  width: 12px; height: 24px; border-radius: 4px;
  background: var(--accent); border: none;
  cursor: grab; transition: transform .1s;
}
.size-range::-moz-range-thumb:active { cursor: grabbing; transform: scaleY(1.1); }
.size-range::-moz-range-track { height: 6px; background: var(--bg-deep); border: 1px solid var(--border); border-radius: 3px; }

/* Drag tooltip */
.size-tip {
  position: absolute; bottom: calc(100% + 10px);
  transform: translateX(-50%);
  padding: 4px 10px; background: var(--bg-card); border: 1px solid var(--border);
  border-radius: 6px; font-size: 11px; font-weight: 600; color: var(--text);
  white-space: nowrap; pointer-events: none;
  box-shadow: 0 4px 12px rgba(0,0,0,.4);
}
.size-tip::after {
  content: ''; position: absolute; top: 100%; left: 50%; transform: translateX(-50%);
  border: 4px solid transparent; border-top-color: var(--border);
}
.tip-fade-enter-active { transition: opacity .1s ease, transform .1s ease; }
.tip-fade-leave-active { transition: opacity .08s ease, transform .08s ease; }
.tip-fade-enter-from, .tip-fade-leave-to { opacity: 0; transform: translateX(-50%) translateY(4px); }
.fav-btn { padding:7px 12px; border:1px solid var(--border); border-radius:var(--radius); background:var(--bg-input); color:var(--text-sec); font-size:13px; font-weight:600; cursor:pointer; white-space:nowrap; }
.fav-btn:hover { background:var(--bg-hover); }
.fav-btn.active { background:var(--accent); border-color:var(--accent); color:#fff; }

.view-toggle { display:flex; border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; }
.vt-btn { width:32px; height:32px; background:var(--bg-input); border:none; color:var(--text-muted); cursor:pointer; display:flex; align-items:center; justify-content:center; transition:background .15s,color .15s; }
.vt-btn svg { width:14px; height:14px; }
.vt-btn:hover { background:var(--bg-hover); color:var(--text); }
.vt-btn.active { background:var(--accent); color:#fff; }

/* Scroll container */
.scroll-area { flex:1; min-height:0; overflow:hidden; display:flex; flex-direction:column; position:relative; user-select:none; }

/* scroll-host: positions the OverlayScrollbar relative to itself */
.scroll-host {
  flex: 1; min-height: 0;
  position: relative;
  display: flex; flex-direction: column;
}

/* Native grid/list host — owns its own scroll, sits inside scroll-host */
.native-grid-host {
  flex: 1; min-height: 0;
  overflow-y: scroll; overflow-x: hidden;
  padding: 8px 22px 60px 10px;
  scrollbar-width: none;
  -webkit-overflow-scrolling: touch;
  will-change: scroll-position;
  user-select: none;
}
.native-grid-host::-webkit-scrollbar { display: none; width: 0; }
.native-grid { user-select: none; }

/* Scan banner */
.scan-banner {
  display: flex; align-items: center; gap: 12px;
  background: linear-gradient(90deg, rgba(var(--accent-rgb, 79,140,255), 0.12), rgba(var(--accent-rgb, 79,140,255), 0.06));
  border: 1px solid rgba(var(--accent-rgb, 79,140,255), 0.25); border-radius: 10px;
  padding: 10px 16px; margin-bottom: 12px; font-size: 13px; color: var(--accent);
}
.scan-spinner {
  width: 16px; height: 16px;
  border: 2px solid rgba(79,140,255,0.3); border-top-color: var(--accent);
  border-radius: 50%; animation: spin 0.8s linear infinite; flex-shrink: 0;
}
@keyframes spin { to { transform: rotate(360deg); } }
.scan-count { margin-left: auto; font-size: 11px; color: var(--text-muted, #8a8a9a); }

/* Card entry animation (from file watcher) */
.clip-enter { animation: cardSlideIn 0.3s ease both; }
@keyframes cardSlideIn {
  from { opacity: 0; transform: translateY(-12px) scale(0.97); }
  to   { opacity: 1; transform: translateY(0) scale(1); }
}

/* Grouped view host */
.grouped-host { padding: 8px 16px 60px 16px; }

/* Grid */
.clip-grid { display:grid; gap:16px; grid-auto-rows:max-content; align-content:start; }

/* View-mode fade when toggling between grid and list */
.view-fade-enter-active { transition: opacity .18s ease, transform .18s ease; }
.view-fade-leave-active { transition: opacity .12s ease, transform .12s ease; }
.view-fade-enter-from   { opacity: 0; transform: translateY(6px); }
.view-fade-leave-to     { opacity: 0; }

/* Skeleton cards */
.skeleton-card { border-radius:8px; overflow:hidden; background:var(--bg-card); border:1px solid var(--border); }
.skeleton-thumb { aspect-ratio:16/9; background:var(--bg-deep); }
.skeleton-info { padding:10px; display:flex; flex-direction:column; gap:6px; }
.skeleton-line { height:10px; background:var(--bg-deep); border-radius:4px; }
.w70 { width:70%; } .w40 { width:40%; }
.animate-pulse { animation:apulse 1.5s ease-in-out infinite; }
@keyframes apulse { 0%,100%{opacity:1} 50%{opacity:.4} }
.watcher-skeleton { position:relative; }
.watcher-label { position:absolute; bottom:0; left:0; right:0; text-align:center; font-size:10px; color:var(--accent); background:rgba(0,0,0,.6); padding:3px 0; font-weight:600; }

/* Stagger fade-in for real cards */
.clip-stagger { animation:fadeSlideIn .3s ease both; }
@keyframes fadeSlideIn { from{opacity:0;transform:translateY(8px)} to{opacity:1;transform:none} }

/* ★ Epic 3: List view — all sizing driven by CSS custom properties from listStyles computed */
.clip-list { display:flex; flex-direction:column; gap:6px; }

.list-row {
  display:flex; align-items:stretch; gap:12px;
  padding: 0 calc(var(--list-pad, 8px) + 4px) 0 0;
  background:var(--bg-card); border:1px solid var(--border); border-radius:8px;
  cursor:pointer; overflow:hidden; user-select:none;
  transition: background .15s, padding .25s ease;
  contain: layout style paint; content-visibility: auto; contain-intrinsic-size: auto 60px;
}
.list-row:hover { background:var(--bg-hover); }
.list-row.selected { border-color:var(--accent); background:color-mix(in srgb, var(--accent) 8%, transparent); }

.list-thumb {
  object-fit:cover; background:var(--bg-deep); user-select:none; -webkit-user-drag:none; pointer-events:none;
}
.list-thumb-empty {
  background:var(--bg-deep); flex-shrink:0;
  display:flex; align-items:center; justify-content:center;
  font-size:18px; color:var(--text-muted);
}
.list-info { flex:1; min-width:0; display:flex; flex-direction:column; gap:3px; padding: var(--list-pad, 8px) 0; justify-content: center; }
.list-name {
  font-size: var(--list-font, 13px); font-weight:600;
  white-space:nowrap; overflow:hidden; text-overflow:ellipsis;
  transition: font-size .25s ease;
}
.list-thumb-wrap {
  position: relative; flex-shrink: 0;
  width: var(--list-thumb-w, 160px); align-self: stretch;
  transition: width .25s ease;
}
.list-thumb-wrap .list-thumb { width: 100%; height: 100%; object-fit: cover; display: block; transition: none; border-radius: 0; }
.list-thumb-wrap .list-thumb-empty { width: 100%; height: 100%; transition: none; border-radius: 0; }
.list-badge {
  position: absolute; bottom: 3px; right: 3px;
  background: rgba(0,0,0,.8); color: #fff;
  font-size: 10px; font-weight: 600;
  padding: 1px 5px; border-radius: 3px;
  pointer-events: none; line-height: 1.4;
}
.list-meta { font-size:11px; color:var(--text-muted); display:flex; align-items:center; flex-wrap:wrap; gap:4px; }
.lm-game {
  font-weight:700; font-size:10px;
  color:var(--accent);
  background:color-mix(in srgb, var(--accent) 14%, transparent);
  padding:2px 8px; border-radius:4px;
  white-space:nowrap;
}
.lm-pill { background:var(--bg-deep); padding:2px 6px; border-radius:3px; }
.lm-date { opacity:.75; }
.list-actions { display:flex; gap:6px; flex-shrink:0; align-items:center; padding: var(--list-pad, 8px) 0; }
.list-act { padding: 4px 10px; border:1px solid var(--border); border-radius:5px; background:var(--bg-surface); color:var(--text-sec); font-size:12px; cursor:pointer; white-space:nowrap; }
.list-act:hover { background:var(--bg-hover); }
.list-act-d { color:var(--danger); }
.list-act-d:hover { background:rgba(220,38,38,.1); }
.list-fav { flex-shrink:0; width:28px; height:28px; border-radius:50%; border:none; background:transparent; color:var(--text-muted); cursor:pointer; display:flex; align-items:center; justify-content:center; transition:color .15s; }
.list-fav:hover { color:var(--text); }
.list-fav.on { color:#E94560; }
.list-fav svg { width:15px; height:15px; }
.list-kebab { flex-shrink:0; width:30px; height:30px; border-radius:6px; border:1px solid var(--border); background:var(--bg-deep); color:var(--text-sec); cursor:pointer; display:flex; align-items:center; justify-content:center; transition:all .15s; }
.list-kebab:hover { background:var(--bg-hover); color:var(--text); border-color:var(--accent); }
.list-kebab svg { width:15px; height:15px; }

/* Thumbnail overlays for list view — heart (top-right) and select checkbox (top-left) */
.lt-heart {
  position: absolute; top: 5px; right: 5px;
  width: 26px; height: 26px; border-radius: 50%;
  border: none; background: rgba(0,0,0,.55);
  color: var(--text-muted); cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  opacity: 0; transition: all .15s;
}
.list-row:hover .lt-heart { opacity: 1; }
.lt-heart.on { opacity: 1; color: #E94560; }
.lt-heart:hover { background: rgba(0,0,0,.8); transform: scale(1.15); }
.lt-heart svg { width: 13px; height: 13px; }

.lt-sel-ov {
  position: absolute; top: 5px; left: 5px;
  opacity: 0; transition: opacity .15s;
}
.lt-sel-ov.vis, .list-row:hover .lt-sel-ov { opacity: 1; }
.lt-sel-box {
  width: 20px; height: 20px; border-radius: 5px;
  border: 2px solid rgba(255,255,255,.55); background: rgba(0,0,0,.4);
  display: flex; align-items: center; justify-content: center;
  font-size: 11px; color: transparent; cursor: pointer;
}
.lt-sel-box.checked { background: var(--accent); border-color: var(--accent); color: #fff; }

/* List view context menu */
.list-ctx { position:fixed; z-index:10000; min-width:200px; background:var(--bg-card); border:1px solid var(--border); border-radius:8px; padding:4px; box-shadow:0 8px 32px rgba(0,0,0,.5); }
.list-ctx-i { display:flex; align-items:center; gap:8px; width:100%; padding:8px 12px; border:none; border-radius:5px; background:transparent; color:var(--text); font-size:13px; cursor:pointer; text-align:left; }
.list-ctx-i:hover { background:var(--bg-hover); }
.list-ctx-ic { width:15px; height:15px; flex-shrink:0; color:var(--text-muted); }
.list-ctx-sep { height:1px; background:var(--border); margin:4px 0; }
.list-ctx-d { color:var(--danger) !important; }

/* Empty state */
.empty-state { position:absolute; inset:0; display:flex; flex-direction:column; align-items:center; justify-content:center; color:var(--text); padding:40px; text-align:center; pointer-events:none; }
.empty-state > * { pointer-events:auto; }
.empty-ic { font-size:48px; margin-bottom:12px; opacity:.6; }
.empty-sub { font-size:13px; color:var(--text); opacity:.65; margin-top:4px; }
.empty-import-btn { margin-top:18px; padding:8px 22px; border:1px solid var(--accent); border-radius:var(--radius); background:transparent; color:var(--accent); font-size:13px; font-weight:600; cursor:pointer; transition:background .15s, color .15s; }
.empty-import-btn:hover { background:var(--accent); color:#fff; }

/* Sentinel */
.sentinel { height:48px; display:flex; align-items:center; justify-content:center; margin-top:8px; }
.sentinel-label { font-size:11px; color:var(--text-muted); opacity:.5; padding:4px 12px; border:1px solid var(--border); border-radius:20px; }

/* ★ Epic 1: Bulk action bar */
.sel-bar { position:sticky; bottom:0; display:flex; align-items:center; gap:8px; padding:10px 16px; background:var(--bg-card); border-top:1px solid var(--border); border-radius:12px 12px 0 0; box-shadow:0 -4px 16px rgba(0,0,0,.3); z-index:50; font-size:13px; font-weight:600; color:var(--text-sec); flex-wrap:wrap; }
.sel-btn { padding:6px 12px; border:1px solid var(--border); border-radius:6px; background:var(--bg-surface); color:var(--text-sec); font-size:12px; cursor:pointer; white-space:nowrap; }
.sel-btn:hover { background:var(--bg-hover); }
.sel-btn.active { border-color:var(--accent); color:var(--accent); }
.sel-btn-d { color:var(--danger); border-color:var(--danger); }
.sel-btn-d:hover { background:rgba(220,38,38,.1); }
.sel-btn-p { background:var(--accent); border-color:var(--accent); color:#fff; }
.sel-btn-p:hover { filter:brightness(1.1); }
.sel-btn-fav { color:#f87171; }
.sel-btn-unfav { color:var(--text-sec); }
.slide-up-enter-active,.slide-up-leave-active { transition:transform .2s,opacity .2s; }
.slide-up-enter-from,.slide-up-leave-to { transform:translateY(100%); opacity:0; }

/* ★ Epic 1 Bug 2: Bulk game drop-up with search */
.bulk-game-wrap { position:relative; }
.bulk-game-drop {
  position:absolute;
  bottom:calc(100% + 8px); /* opens UPWARDS */
  left:0;
  z-index:200;
  width:240px;
  background:var(--bg-card);
  border:1px solid var(--border);
  border-radius:10px;
  box-shadow:0 -8px 24px rgba(0,0,0,.5);
  display:flex; flex-direction:column; overflow:hidden;
}
.bulk-game-search-wrap { padding:8px 8px 6px; border-bottom:1px solid var(--border); }
.bulk-game-search { width:100%; padding:6px 10px; background:var(--bg-input); border:1px solid var(--border); border-radius:6px; color:var(--text); font-size:12px; outline:none; }
.bulk-game-search:focus { border-color:var(--accent); }
.bulk-game-list { max-height:180px; overflow-y:auto; padding:4px; }
.bulk-game-opt { width:100%; padding:7px 10px; background:transparent; border:none; border-radius:5px; color:var(--text-sec); font-size:12px; text-align:left; cursor:pointer; }
.bulk-game-opt:hover { background:var(--bg-hover); color:var(--text); }
.bulk-game-opt.active { color:var(--accent); font-weight:600; background:color-mix(in srgb, var(--accent) 10%, transparent); }
.bulk-game-empty { padding:10px; text-align:center; font-size:11px; color:var(--text-muted); }
.bulk-game-footer { padding:8px; border-top:1px solid var(--border); display:flex; justify-content:flex-end; }
.dropup-enter-active { transition:opacity .12s, transform .12s; }
.dropup-leave-active { transition:opacity .08s, transform .08s; }
.dropup-enter-from,.dropup-leave-to { opacity:0; transform:translateY(6px); }

/* Dialogs */
.dlg-ov { position:fixed; inset:0; z-index:2000; background:rgba(0,0,0,.6); display:flex; align-items:center; justify-content:center; }
.dlg { background:var(--bg-surface); border:1px solid var(--border); border-radius:10px; padding:24px; width:400px; }
.dlg h3 { font-size:16px; font-weight:700; margin-bottom:16px; }
.dlg-in { width:100%; padding:10px 14px; background:var(--bg-input); border:1px solid var(--border); border-radius:6px; color:var(--text); font-size:14px; outline:none; color-scheme:dark; }
.dlg-in:focus { border-color:var(--accent); }
.dlg-btns { display:flex; gap:8px; justify-content:flex-end; margin-top:16px; }
.dlg-btn { padding:8px 18px; border:1px solid var(--border); border-radius:6px; background:var(--bg-card); color:var(--text-sec); font-size:13px; cursor:pointer; }
.dlg-btn:hover { background:var(--bg-hover); }
.dlg-pri { background:var(--accent); border-color:var(--accent); color:#fff; }

/* Date grouping */
.date-groups { display:flex; flex-direction:column; gap:28px; }
.date-group { display:flex; flex-direction:column; gap:12px; }
.date-header { display:flex; align-items:center; gap:10px; padding-bottom:8px; border-bottom:1px solid var(--border); }
.date-label { font-size:14px; font-weight:700; color:var(--text); }
.date-count { font-size:11px; font-weight:600; color:var(--text-muted); background:var(--bg-deep); padding:2px 8px; border-radius:10px; }
.group-sel-box { width:20px; height:20px; border-radius:5px; border:2px solid var(--text-muted); background:transparent; display:flex; align-items:center; justify-content:center; cursor:pointer; font-size:12px; color:transparent; transition:border-color .15s, background .15s; flex-shrink:0; user-select:none; }
.group-sel-box:hover { border-color:var(--accent); }
.group-sel-box.checked { background:var(--accent); border-color:var(--accent); color:#fff; }
.group-sel-box.indeterminate { border-color:var(--accent); color:var(--accent); }

/* Toast */
.toast { position:fixed; bottom:20px; left:50%; transform:translateX(-50%); background:var(--bg-card); border:1px solid var(--accent); color:var(--text); padding:10px 24px; border-radius:8px; font-size:13px; font-weight:600; z-index:9999; box-shadow:0 4px 16px rgba(0,0,0,.3); }
.fade-enter-active,.fade-leave-active { transition:opacity .3s; }
.fade-enter-from,.fade-leave-to { opacity:0; }

/* Phase 2a: Page-level context menu */
.ctx-menu {
  position:fixed; z-index:5000;
  background:var(--bg-card); border:1px solid var(--border);
  border-radius:8px; padding:4px; min-width:180px;
  box-shadow:0 8px 24px rgba(0,0,0,.5);
}
.ctx-item {
  display:flex; align-items:center; gap:10px; width:100%; padding:8px 14px; background:none; border:none;
  border-radius:5px; color:var(--text-sec); font-size:13px; text-align:left;
  cursor:pointer; white-space:nowrap;
}
.ctx-item svg { width:15px; height:15px; flex-shrink:0; opacity:.8; }
.ctx-item:hover { background:var(--bg-hover); color:var(--text); }
.ctx-item-d { color:var(--danger); }
.ctx-item-d:hover { background:rgba(220,38,38,.1); }
.ctx-sep { height:1px; background:var(--border); margin:4px 0; }
</style>

<!-- Unscoped: suppress hover transitions on cards while scrolling -->
<style>
.native-grid-host.scrolling .card {
  transition: none !important;
}
.native-grid-host.scrolling .card:hover {
  transform: none !important;
  box-shadow: none !important;
}
</style>

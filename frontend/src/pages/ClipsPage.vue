<script setup lang="ts">
defineOptions({ name: 'ClipsPage' })
import { ref, computed, onMounted, onBeforeUnmount, inject, watch, onActivated, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { refDebounced } from '@vueuse/core'
import { invoke } from '@tauri-apps/api/core'
import { ask, open as openDialog } from '@tauri-apps/plugin-dialog'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useReplayStore, type Clip, normalizeGameTitle } from '../stores/replay'
import { usePersistenceStore } from '../stores/persistence'
import ClipCard from '../components/ClipCard.vue'
import ClipListRow from '../components/ClipListRow.vue'
import OverlayScrollbar from '../components/OverlayScrollbar.vue'
import ClipEditor from '../components/ClipEditor.vue'
import AdvancedEditor from '../components/AdvancedEditor.vue'
import SelectField from '../components/SelectField.vue'
import GameFilterDropdown from '../components/GameFilterDropdown.vue'
import RecordingDropdown from '../components/RecordingDropdown.vue'
import { mediaUrl } from '../utils/assets'
import { viewMode } from '../composables/useViewMode'
import type { Ref } from 'vue'

const { t } = useI18n()
const replay = useReplayStore()
const persist = usePersistenceStore()
const _mediaPortRef = inject<Ref<number>>('mediaPort', ref(0))
const mediaPortNum  = computed(() => _mediaPortRef.value)

function compareNewestFirst(a: Clip, b: Clip) {
  return b.created.localeCompare(a.created)
    || (b.createdTs - a.createdTs)
    || b.filename.localeCompare(a.filename)
}

function clipNeedsThumbnailWork(clip: Clip) {
  return !clip.isSkeleton && ((!clip.thumbnail && !replay.liveThumbs.get(clip.id)) || clip.duration === 0)
}

function clipNeedsGeneratedThumbnail(clip: Clip) {
  return !clip.isSkeleton && !clip.thumbnail && !replay.liveThumbs.get(clip.id)
}

function refreshThumbnailProgressTotals(extraClips: Clip[] = []) {
  if (prefetchPhase.value === 'idle') return
  for (const clip of [...replay.clips, ...extraClips]) {
    if (clipNeedsGeneratedThumbnail(clip) && !prefetchCompletedThumbIds.has(clip.id)) {
      prefetchPlannedThumbIds.add(clip.id)
    }
  }
  prefetchThumbTotal.value = prefetchPlannedThumbIds.size
  prefetchThumbDone.value = prefetchCompletedThumbIds.size
}

// ── Prefetch progress state ──
type PrefetchPhase = 'idle' | 'probing' | 'thumbnails' | 'done'
const prefetchPhase    = ref<PrefetchPhase>('idle')
const prefetchProbeTotal = ref(0)
const prefetchProbeDone  = ref(0)
const prefetchThumbTotal = ref(0)
const prefetchThumbDone  = ref(0)
let prefetchDoneTimer: ReturnType<typeof setTimeout> | null = null
let prefetchPlannedThumbIds = new Set<string>()
let prefetchCompletedThumbIds = new Set<string>()
let prefetchNeedsRerun = false
const thumbnailPhaseLabel = computed(() => {
  if (prefetchPhase.value === 'probing') return 'Scanning clip metadata'
  if (prefetchPhase.value === 'thumbnails') return replay.scrolling ? 'Generating thumbnails (paused while scrolling)' : 'Generating thumbnails'
  if (prefetchPhase.value === 'done') return 'Thumbnail generation complete'
  return ''
})
const thumbnailProgressText = computed(() => {
  if (prefetchPhase.value === 'probing') return `${prefetchProbeDone.value} / ${prefetchProbeTotal.value}`
  if (prefetchPhase.value === 'thumbnails') return `${prefetchThumbDone.value} / ${prefetchThumbTotal.value}`
  if (prefetchPhase.value === 'done') return `${prefetchThumbTotal.value} / ${prefetchThumbTotal.value}`
  return ''
})
const thumbnailProgressPercent = computed(() => {
  if (prefetchPhase.value === 'probing') {
    return prefetchProbeTotal.value > 0 ? (prefetchProbeDone.value / prefetchProbeTotal.value) * 100 : 0
  }
  if (prefetchPhase.value === 'thumbnails' || prefetchPhase.value === 'done') {
    return prefetchThumbTotal.value > 0 ? (prefetchThumbDone.value / prefetchThumbTotal.value) * 100 : 0
  }
  return 0
})

// ── Phase 4b: Debounce the raw search input by 150ms ──
const searchRaw = ref(replay.search)
const searchDebounced = refDebounced(searchRaw, 150)

// Keep store in sync with debounced value
watch(searchDebounced, (v) => { replay.search = v })
watch(() => replay.search, (v) => { if (v !== searchRaw.value) searchRaw.value = v })

// ── Phase 1d/4a: Use filteredClips directly — no more v-show isMatch() ──
const sortedSkeletons = computed(() => replay.filteredClips.filter((c: Clip) => c.isSkeleton))
const filteredRealClips = computed(() => replay.filteredRealClips)
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
const steamImportBusy = ref(false)

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
      await persist.save()
      await replay.scanFolderRecursive(s)
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
  measuredGridRowHeight.value = 0
  setupRowMeasure()
}

// Native grid host scroll ref for OverlayScrollbar
const gridScrollRef = ref<HTMLElement | null>(null)
const listScrollRef = ref<HTMLElement | null>(null)
const groupedScrollRef = ref<HTMLElement | null>(null)

function getActiveGridHost(): HTMLElement | null {
  return dateGrouped.value ? groupedScrollRef.value : gridScrollRef.value
}

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
  const map: Record<number, {
    thumbW: string
    thumbH: string
    fontSize: string
    metaFontSize: string
    pillPadY: string
    pillPadX: string
    chipFontSize: string
    chipPadY: string
    chipPadX: string
    actionFontSize: string
    actionPadY: string
    actionPadX: string
    padding: string
  }> = {
    1: { thumbW: '400px', thumbH: '225px', fontSize: '18px', metaFontSize: '14px', pillPadY: '3px', pillPadX: '8px', chipFontSize: '12px', chipPadY: '3px', chipPadX: '10px', actionFontSize: '13px', actionPadY: '6px', actionPadX: '12px', padding: '16px' },
    2: { thumbW: '320px', thumbH: '180px', fontSize: '15px', metaFontSize: '12px', pillPadY: '2px', pillPadX: '7px', chipFontSize: '11px', chipPadY: '2px', chipPadX: '9px', actionFontSize: '12px', actionPadY: '5px', actionPadX: '11px', padding: '12px' },
    3: { thumbW: '240px', thumbH: '135px', fontSize: '13px', metaFontSize: '11px', pillPadY: '2px', pillPadX: '6px', chipFontSize: '10px', chipPadY: '2px', chipPadX: '8px', actionFontSize: '12px', actionPadY: '4px', actionPadX: '10px', padding: '8px'  },
    4: { thumbW: '160px', thumbH: '90px',  fontSize: '11px', metaFontSize: '10px', pillPadY: '1px', pillPadX: '5px', chipFontSize: '9px', chipPadY: '1px', chipPadX: '7px', actionFontSize: '11px', actionPadY: '3px', actionPadX: '8px', padding: '6px'  },
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
const pendingScrollSyncTop = ref<number | null>(null)
let scrollTicking = false
function onScroll() {
  if (!scrollTicking) {
    requestAnimationFrame(() => {
      gridScrollTop.value = gridScrollRef.value?.scrollTop ?? listScrollRef.value?.scrollTop ?? groupedScrollRef.value?.scrollTop ?? 0
      scrollTicking = false
    })
    scrollTicking = true
  }

  isScrolling.value = true
  replay.scrolling = true
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

function getActiveScrollHost(): HTMLElement | null {
  if (dateGrouped.value) return groupedScrollRef.value
  return viewMode.value === 'list' ? listScrollRef.value : gridScrollRef.value
}

function clampScrollTop(el: HTMLElement, targetScrollTop: number) {
  return Math.max(0, Math.min(targetScrollTop, Math.max(0, el.scrollHeight - el.clientHeight)))
}

function syncVirtualScrollHost(targetScrollTop = gridScrollTop.value) {
  const el = getActiveScrollHost()
  if (!el) return false
  const nextScrollTop = clampScrollTop(el, targetScrollTop)
  if (Math.abs(el.scrollTop - nextScrollTop) > 1) el.scrollTop = nextScrollTop
  gridScrollTop.value = clampScrollTop(el, el.scrollTop)
  pendingScrollSyncTop.value = null
  return true
}

function scheduleVirtualScrollHostSync(targetScrollTop = gridScrollTop.value) {
  pendingScrollSyncTop.value = targetScrollTop
  let attempts = 0
  const run = () => {
    requestAnimationFrame(() => {
      const desiredScrollTop = pendingScrollSyncTop.value ?? targetScrollTop
      const synced = syncVirtualScrollHost(desiredScrollTop)
      if (viewMode.value === 'grid') setupRowMeasure()
      if (synced) {
        replay.flushClipsNow()
        return
      }
      if (attempts < 4) {
        attempts += 1
        run()
      }
    })
  }
  run()
}

// ── Virtual scroll: only render visible grid rows + buffer ──
// Without this, 420 ClipCards create 420 decoded thumbnail bitmaps (~1.6MB each)
// in WebKitGTK memory. Virtual scroll keeps only ~40-60 cards in the DOM.
const GRID_GAP = 16
const VIRTUAL_BUFFER_ROWS = 10 // Increased buffer to eliminate pop-in at the bottom

// Estimate card row height from container width + column count.
// Card = 16:9 thumbnail + ~80px info section + gap.
const estimatedGridRowHeight = computed(() => {
  const el = getActiveGridHost()
  if (!el) return 300
  const containerW = el.clientWidth - 32 // subtract horizontal padding
  const cols = gridCols.value
  const cardW = (containerW - GRID_GAP * (cols - 1)) / cols
  const thumbH = cardW * 9 / 16
  const infoH = cols <= 2 ? 90 : cols >= 5 ? 65 : 78
  return thumbH + infoH + GRID_GAP
})

// Measured row height (set after first render via ResizeObserver)
const measuredGridRowHeight = ref(0)

const listRowHeight = computed(() => {
  const s = listStyles.value
  const thumbH = parseInt(s.thumbH)
  const pad = parseInt(s.padding)
  // Row height is determined by the thumbnail height or the info height.
  // In list view, thumb is usually the tallest part.
  return thumbH + (pad * 2) + 2 // +2 for border/gap
})

const rowHeight = computed(() => {
  if (viewMode.value === 'list') return listRowHeight.value
  return measuredGridRowHeight.value || estimatedGridRowHeight.value
})

// ── Virtualized Grouped View Logic ──
const GROUP_HEADER_HEIGHT = 48
const GROUP_GAP = 28
const HEADER_GAP = 12

interface VirtualGroupItem {
  type: 'header' | 'row' | 'spacer'
  group?: DateGroup
  clips?: Clip[]
  y: number
  height: number
  key: string
}

const flattenedItems = computed(() => {
  if (!dateGrouped.value) return []
  const items: VirtualGroupItem[] = []
  const isList = viewMode.value === 'list'
  const cols = isList ? 1 : gridCols.value
  const rh = rowHeight.value
  
  let currentY = 0
  for (const group of groupedClips.value) {
    // Add header
    items.push({ 
      type: 'header', 
      group, 
      y: currentY, 
      height: GROUP_HEADER_HEIGHT,
      key: `h-${group.date}`
    })
    currentY += GROUP_HEADER_HEIGHT
    
    // Header gap spacer
    items.push({
      type: 'spacer',
      y: currentY,
      height: HEADER_GAP,
      key: `sp-h-${group.date}`
    })
    currentY += HEADER_GAP
    
    // Add rows
    const groupClips = group.clips
    const totalRows = Math.ceil(groupClips.length / cols)
    for (let r = 0; r < totalRows; r++) {
      const rowClips = groupClips.slice(r * cols, (r + 1) * cols)
      items.push({ 
        type: 'row', 
        clips: rowClips, 
        y: currentY, 
        height: rh,
        key: `r-${group.date}-${r}-${rowClips[0]?.id}`
      })
      currentY += rh
    }
    
    // Group gap spacer
    items.push({
      type: 'spacer',
      y: currentY,
      height: GROUP_GAP,
      key: `sp-g-${group.date}`
    })
    currentY += GROUP_GAP
  }
  return items
})

const virtualRange = computed(() => {
  const total = filteredRealClips.value.length
  if (total === 0) return { start: 0, end: 0, padTop: 0, padBot: 0 }
  
  const isList = viewMode.value === 'list'
  const el = isList ? listScrollRef.value : (dateGrouped.value ? groupedScrollRef.value : gridScrollRef.value)
  const rh = rowHeight.value
  
  if (!el || !rh) return { start: 0, end: total, padTop: 0, padBot: 0 }
  
  const clientH = el.clientHeight
  const scrollTop = gridScrollTop.value
  
  if (dateGrouped.value) {
    const items = flattenedItems.value
    if (items.length === 0) return { start: 0, end: 0, padTop: 0, padBot: 0 }
    
    const bufferHeight = VIRTUAL_BUFFER_ROWS * rh
    const startIdx = Math.max(0, items.findIndex(item => item.y + item.height > scrollTop - bufferHeight))
    
    const endY = scrollTop + clientH + bufferHeight
    let endIdx = items.findIndex((item, i) => i >= startIdx && item.y > endY)
    if (endIdx === -1) endIdx = items.length
    
    const lastVisible = items[endIdx - 1]
    const totalHeight = items[items.length - 1].y + items[items.length - 1].height
    
    return {
      start: startIdx,
      end: endIdx,
      padTop: items[startIdx].y,
      padBot: Math.max(0, totalHeight - (lastVisible.y + lastVisible.height)),
    }
  }
  
  const cols = isList ? 1 : gridCols.value
  const totalRows = Math.ceil(total / cols)
  const startRow = Math.max(0, Math.min(totalRows, Math.floor(scrollTop / rh) - VIRTUAL_BUFFER_ROWS))
  const endRow = Math.max(0, Math.min(totalRows, Math.ceil((scrollTop + clientH) / rh) + VIRTUAL_BUFFER_ROWS))
  
  return {
    start: startRow * cols,
    end: Math.min(endRow * cols, total),
    padTop: startRow * rh,
    padBot: Math.max(0, (totalRows - endRow) * rh),
  }
})

const visibleClips = computed(() => {
  if (dateGrouped.value) return [] // Handled by visibleItems
  const { start, end } = virtualRange.value
  return filteredRealClips.value.slice(start, end)
})

const visibleItems = computed(() => {
  if (!dateGrouped.value) return []
  const { start, end } = virtualRange.value
  return flattenedItems.value.slice(start, end)
})

// Measure actual card height from first rendered card
let rowMeasureRO: ResizeObserver | null = null
function setupRowMeasure() {
  rowMeasureRO?.disconnect()
  rowMeasureRO = new ResizeObserver(() => {
    if (viewMode.value !== 'grid') return
    const host = getActiveGridHost()
    const firstCard = host?.querySelector('.card') as HTMLElement | null
    if (firstCard && firstCard.offsetHeight > 0) {
      const newHeight = firstCard.getBoundingClientRect().height + GRID_GAP
      if (Math.abs(measuredGridRowHeight.value - newHeight) > 0.1) {
        measuredGridRowHeight.value = newHeight
      }
    }
  })
  const host = getActiveGridHost()
  if (host) rowMeasureRO.observe(host)
}

// Ensure row measurement is re-triggered on layout changes
watch([gridCols, viewMode], () => {
  measuredGridRowHeight.value = 0
  if (viewMode.value === 'grid') setupRowMeasure()
})

watch([viewMode, dateGrouped], async () => {
  const previousScrollTop = gridScrollTop.value
  await nextTick()
  scheduleVirtualScrollHostSync(previousScrollTop)
}, { flush: 'post' })

watch(
  () => getActiveScrollHost(),
  (host) => {
    if (!host || pendingScrollSyncTop.value == null) return
    scheduleVirtualScrollHostSync(pendingScrollSyncTop.value)
  },
  { flush: 'post' },
)

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
const steamAccess = computed(() => persist.state.settings.steamLibraryAccess)
const steamGamesLoaded = computed(() => replay.steamGames.length > 0)
function resolveSteamIcon(path: string | null | undefined) {
  if (!path) return ''
  return path.startsWith('/') ? mediaUrl(path, mediaPortNum.value) : path
}
function gameTitlePenalty(title: string) {
  const marks = (title.match(/[©®™]/g) || []).length
  const punct = (title.match(/[()[\]{}:;,.!'"_-]/g) || []).length
  return (marks * 10) + (punct * 2) + (title.length / 1000)
}
function preferredGameTitle(current: string | null, candidate: string) {
  if (!current) return candidate
  const currentPenalty = gameTitlePenalty(current)
  const candidatePenalty = gameTitlePenalty(candidate)
  if (candidatePenalty !== currentPenalty) return candidatePenalty < currentPenalty ? candidate : current
  return candidate.localeCompare(current) < 0 ? candidate : current
}
type CanonicalGameGroup = {
  display: string
  count: number
  icon: string
}
const steamIconByNormalized = computed<Record<string, string>>(() =>
  Object.fromEntries(
    replay.steamGames
      .filter(game => !!game.icon_url)
      .map(game => [
        normalizeGameTitle(game.name) || game.name.toLowerCase(),
        resolveSteamIcon(game.icon_url),
      ])
  )
)
const clipGameGroups = computed(() => {
  const groups = new Map<string, CanonicalGameGroup>()
  for (const c of replay.clips) {
    if (c.isSkeleton || !c.game) continue
    const normalized = normalizeGameTitle(c.game) || c.game.toLowerCase()
    const existing = groups.get(normalized)
    groups.set(normalized, {
      display: preferredGameTitle(existing?.display ?? null, c.game),
      count: (existing?.count ?? 0) + 1,
      icon: existing?.icon ?? '',
    })
  }
  for (const game of replay.steamGames) {
    const normalized = normalizeGameTitle(game.name) || game.name.toLowerCase()
    const existing = groups.get(normalized)
    if (!existing) continue
    groups.set(normalized, {
      ...existing,
      display: preferredGameTitle(existing.display, game.name),
      icon: existing.icon || steamIconByNormalized.value[normalized] || '',
    })
  }
  return groups
})
const steamIcons = computed<Record<string, string>>(() =>
  {
    const icons: Record<string, string> = {}
    for (const group of clipGameGroups.value.values()) {
      if (group.icon) icons[group.display.toLowerCase()] = group.icon
    }
    for (const game of bulkGameNames.value) {
      const normalized = normalizeGameTitle(game) || game.toLowerCase()
      const icon = steamIconByNormalized.value[normalized]
      if (icon) icons[game.toLowerCase()] = icon
    }
    return icons
  }
)
const gameCounts = computed(() =>
  Object.fromEntries(
    Array.from(clipGameGroups.value.values()).map(group => [group.display, group.count])
  ) as Record<string, number>
)
const filterGameNames = computed(() =>
  Array.from(clipGameGroups.value.values())
    .filter(group => group.count > 0)
    .map(group => group.display)
    .sort((a, b) => a.localeCompare(b))
)
const bulkGameNames = computed(() => {
  const names = new Map<string, string>()
  for (const group of clipGameGroups.value.values()) {
    names.set(normalizeGameTitle(group.display), group.display)
  }
  for (const game of replay.games) {
    if (game === 'all') continue
    const normalized = normalizeGameTitle(game) || game.toLowerCase()
    names.set(normalized, preferredGameTitle(names.get(normalized) ?? null, game))
  }
  for (const game of replay.steamGames) {
    const normalized = normalizeGameTitle(game.name) || game.name.toLowerCase()
    names.set(normalized, preferredGameTitle(names.get(normalized) ?? null, game.name))
  }
  return Array.from(names.values()).sort((a, b) => a.localeCompare(b))
})
// kept for legacy SelectField usage (sort) — game SelectField is replaced by GameFilterDropdown
// Uses gameCounts (single-pass O(clips)) instead of re-filtering per game O(clips × games)
const bulkGameOptions = computed(() =>
  ['all', ...bulkGameNames.value].map((g: string) => {
    if (g === 'all') return { value: g, label: t('clips.gamesFilter.allGames') }
    return { value: g, label: `${g} (${gameCounts.value[g] || 0})`, icon: steamIcons.value[g.toLowerCase()] || '' }
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

async function importSteamLibrary(forcePrompt = false) {
  if (steamImportBusy.value) return

  const access = persist.state.settings.steamLibraryAccess
  if (access !== 'granted' || forcePrompt) {
    const confirmed = await ask(t('clips.steamImport.consentMessage'), {
      title: t('clips.steamImport.consentTitle'),
      kind: 'info',
    })
    persist.state.settings.steamLibraryAccess = confirmed ? 'granted' : 'denied'
    if (!confirmed) return
  }

  steamImportBusy.value = true
  const ok = await replay.fetchSteamGames()
  steamImportBusy.value = false

  if (ok) {
    showToast(t('clips.steamImport.imported', { count: replay.steamGames.length }))
  } else {
    showToast(t('clips.steamImport.failed'))
  }
}

const steamBannerTitle = computed(() => {
  if (steamGamesLoaded.value) return t('clips.steamImport.readyTitle', { count: replay.steamGames.length })
  if (steamAccess.value === 'denied') return t('clips.steamImport.deniedTitle')
  return t('clips.steamImport.title')
})

const steamBannerBody = computed(() => {
  if (steamGamesLoaded.value) return t('clips.steamImport.readyBody')
  if (steamAccess.value === 'denied') return t('clips.steamImport.deniedBody')
  return t('clips.steamImport.body')
})

const steamButtonLabel = computed(() => {
  if (steamImportBusy.value) return t('clips.steamImport.loading')
  if (steamGamesLoaded.value) return t('clips.steamImport.refresh')
  if (steamAccess.value === 'granted') return t('clips.steamImport.import')
  return t('clips.steamImport.allowAndImport')
})

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
async function toggleListFav(clip: Clip, e?: Event) {
  e?.stopPropagation()
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
let unlistenImportProgress: UnlistenFn | null = null
let unlistenImportItem: UnlistenFn | null = null

async function prefetchThumbnails() {
  // Re-entry guard. Also guards against the watch() below firing while we're already
  // running — since applyBulkProbe/applyProbeAndThumb mutate clips[], triggerRef can
  // cause the watch to re-fire on every flush.
  if (replay.isPrefetching) {
    prefetchNeedsRerun = true
    refreshThumbnailProgressTotals()
    return
  }
  // Include clips missing thumbnail OR missing duration/resolution — both need processing
  const needsWork = replay.clips
    .filter((c: Clip) => !c.isSkeleton)
    .sort(compareNewestFirst)
    .filter((c: Clip) => clipNeedsThumbnailWork(c))
  if (!needsWork.length) return
  // Set the gate SYNCHRONOUSLY before the first await so ClipCard IO observers,
  // which also check replay.isPrefetching, can never race this function.
  replay.isPrefetching = true
  if (import.meta.env.DEV) console.debug(`[perf] prefetchThumbnails: ${needsWork.length} clips`)

  // Reset progress tracking
  if (prefetchDoneTimer) { clearTimeout(prefetchDoneTimer); prefetchDoneTimer = null }
  prefetchProbeDone.value = 0
  prefetchThumbDone.value = 0
  prefetchNeedsRerun = false
  prefetchPlannedThumbIds = new Set()
  prefetchCompletedThumbIds = new Set()

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
    prefetchPhase.value = 'probing'
    prefetchProbeTotal.value = unprobedFps.length
    prefetchProbeDone.value = 0
    try {
      const probed = await invoke<[string, number, number, number][]>('probe_clips', { filepaths: unprobedFps })
      for (const [fp, duration, width, height] of probed) {
        probeMap.set(fp, { duration, width, height })
      }
      prefetchProbeDone.value = unprobedFps.length
      // Single-shot bulk apply: one triggerRef for liveMeta + one rAF flush.
      // Duration badges + resolution pills appear on every probed card at once.
      replay.applyBulkProbe(probed)
    } catch (e) {
      if (import.meta.env.DEV) console.warn('[perf] bulk probe failed', e)
    }
  }

  // Count clips that need thumbnails generated
  const needsThumb = needsWork.filter(clipNeedsGeneratedThumbnail)
  prefetchPlannedThumbIds = new Set(needsThumb.map(c => c.id))
  prefetchThumbTotal.value = prefetchPlannedThumbIds.size
  prefetchThumbDone.value = 0
  if (needsThumb.length) prefetchPhase.value = 'thumbnails'

  // ── Phase 2: SEQUENTIAL THUMBNAIL LOOP (newest→oldest) ───────────────
  const results: Array<[string, number, number, number, string, string]> = []
  try {
    for (const clip of needsWork) {
      if (replay.scrolling) {
        if (import.meta.env.DEV) console.debug('[perf] prefetch paused (scrolling)')
        await new Promise<void>(resolve => { prefetchWake = resolve })
        if (import.meta.env.DEV) console.debug('[perf] prefetch resumed')
      }
      try {
        const probed = probeMap.get(clip.filepath)
        const duration = probed?.duration ?? clip.duration
        const width = probed?.width ?? clip.width
        const height = probed?.height ?? clip.height

        let thumbPath = clip.thumbnail || replay.liveThumbs.get(clip.id) || ''
        if (!thumbPath) {
          thumbPath = await invoke<string>('generate_thumbnail', {
            filepath: clip.filepath,
            duration: duration > 0 ? duration : undefined,
          })
          prefetchCompletedThumbIds.add(clip.id)
          prefetchThumbDone.value = prefetchCompletedThumbIds.size
        }
        if (thumbPath) {
          // 1. Update Maps IMMEDIATELY so the visible cards show the thumb right away
          replay.liveThumbs.set(clip.id, thumbPath)
          if (duration > 0) replay.liveMeta.set(clip.id, { duration, width, height })

          // 2. Queue for bulk update to the main clips array (O(N) sort/filter avoidance)
          results.push([clip.filepath, duration, width, height, clip.id, thumbPath])
          
          // Apply in chunks of 10 so we don't wait forever but still get O(N) benefits
          if (results.length >= 10) {
            replay.applyBulkProbeAndThumb([...results])
            results.length = 0
          }
        }
      } catch {}
    }
  } finally {
    if (results.length > 0) replay.applyBulkProbeAndThumb(results)
    replay.isPrefetching = false
    refreshThumbnailProgressTotals()
    const remainingThumbs = replay.clips.some(clipNeedsGeneratedThumbnail)
    const remainingWork = replay.clips.some(clipNeedsThumbnailWork)
    if (prefetchNeedsRerun || remainingWork) {
      prefetchPhase.value = remainingThumbs ? 'thumbnails' : 'probing'
      queueMicrotask(() => { prefetchThumbnails() })
    } else {
      prefetchPhase.value = 'done'
      prefetchDoneTimer = setTimeout(() => { prefetchPhase.value = 'idle' }, 3000)
    }
    // Final authoritative trigger so sort-by-duration / "longest" etc. reflect
    // the freshly-probed values on the next natural recompute.
    replay.flushClipsNow()
  }
}

// Reset scroll to top when filters/search change so virtual range recalculates correctly
watch([searchDebounced, () => replay.sortMode, () => replay.filterFav, () => replay.selectedGames], () => {
  pendingScrollSyncTop.value = 0
  const el = getActiveScrollHost()
  if (el) {
    el.scrollTop = 0
    gridScrollTop.value = 0
    replay.flushClipsNow()
    return
  }
  scheduleVirtualScrollHostSync(0)
})

// Re-run prefetchThumbnails every time fetchClips successfully populates the store.
// flush: 'sync' ensures the gate is set before any ClipCard IO observers can fire.
watch(() => replay.clipsLoadedAt, () => { prefetchThumbnails() }, { flush: 'sync' })

onMounted(async () => {
  if (!persist.loaded) await persist.load()
  replay.fetchStatus()
  setupRowMeasure()
  await replay.fetchClips(persist.state?.settings?.clip_directories?.[0] || '')

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
    let clip: Clip | null = null
    for (let i = 0; i < 5; i++) {
      try {
        const res = await invoke<Clip[]>('get_clips_fast', { folder: persist.state.settings.clip_directories[0] })
        clip = res.find(c => c.filepath === fp) || null
        if (clip && clip.filesize > 0) break
      } catch {}
      await new Promise(r => setTimeout(r, 500 * (i + 1)))
    }
    if (clip) {
      replay.replaceSkeleton(tempId, clip)
      prefetchThumbnails()
    }
    else replay.removeClip(tempId)
  })

  unlistenRemoved = await listen<string>('clip_removed', (event) => {
    replay.removeClip(event.payload)
  })

  // ★ Job #3: Import listeners
  unlistenImportProgress = await listen<{current:number, total:number}>('import-progress', (e) => {
    replay.importProgress.current = e.payload.current
    replay.importProgress.total = e.payload.total
  })
  unlistenImportItem = await listen<Clip>('import-item', (e) => {
    replay.addClip(e.payload)
    refreshThumbnailProgressTotals([e.payload])
    prefetchThumbnails()
  })

})

onBeforeUnmount(() => {
  unlistenAdded?.()
  unlistenRemoved?.()
  unlistenImportProgress?.()
  unlistenImportItem?.()
  rowMeasureRO?.disconnect()
})

const allSelectedFavorited = computed(() => {
  const ids = Array.from(replay.selectedIds)
  if (ids.length === 0) return false
  const clips = replay.clips.filter(c => ids.includes(c.id))
  return clips.every(c => c.favorite)
})

async function bulkFavorite() {
  const ids = Array.from(replay.selectedIds)
  const clips = replay.clips.filter(c => ids.includes(c.id))
  const targetState = !allSelectedFavorited.value
  for (const c of clips) {
    replay.updateClipMeta(c.filepath, { favorite: targetState })
    try { await invoke('set_clip_meta', { update: { filepath: c.filepath, custom_name: c.custom_name, favorite: targetState } }) } catch {}
  }
}

// ── Bulk change game logic ──
const bulkGameOpen = ref(false)
const bulkGameSearch = ref('')
const bulkGameValue = ref('Unknown')
const filteredBulkGames = computed(() => {
  const q = bulkGameSearch.value.toLowerCase()
  return bulkGameOptions.value.filter(o => o.label.toLowerCase().includes(q))
})
async function bulkChangeGame() {
  const ids = Array.from(replay.selectedIds)
  const clips = replay.clips.filter(c => ids.includes(c.id))
  for (const c of clips) {
    replay.updateClipMeta(c.filepath, { game: bulkGameValue.value })
    try { await invoke('set_clip_meta', { update: { filepath: c.filepath, custom_name: c.custom_name, favorite: c.favorite, game: bulkGameValue.value } }) } catch {}
  }
  bulkGameOpen.value = false
  replay.clearSelection()
}

// ── Context menu logic ──
const contextMenuClip = computed(() => replay.clips.find(c => c.id === replay.activeMenuClipId))
function ctxAction(act: string) {
  const clip = contextMenuClip.value
  if (!clip) return
  if (act === 'preview') openPreview(clip)
  else if (act === 'editor') openAdvanced(clip)
  else if (act === 'select') replay.toggleSelect(clip.id)
  else if (act === 'favorite') toggleListFav(clip)
  else if (act === 'rename') startRename(clip)

  else if (act === 'location') invoke('open_file_location', { filepath: clip.filepath })
  else if (act === 'delete') deleteClip(clip)
  closeContextMenu()
}
function closeContextMenu(e?: MouseEvent) {
  if (e) {
    if (e.button !== 0) return
    // Don't close if clicking inside the menu (let ctxAction handle it)
    if ((e.target as HTMLElement).closest('.ctx-menu')) return
  }
  replay.activeMenuClipId = ''
}

onActivated(() => {
  // ★ Task 5: Re-sync scroll position on tab switch to prevent invisibility
  syncVirtualScrollHost()
})

// ★ Task 6: Reset scroll to top when toggling grouping to prevent invisibility
watch(dateGrouped, () => {
  const el = gridScrollRef.value || listScrollRef.value || groupedScrollRef.value
  if (el) {
    el.scrollTop = 0
    gridScrollTop.value = 0
  }
})

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
          :games="filterGameNames"
          :clipCounts="gameCounts"
          :steam-icons="steamIcons"
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

    <div v-if="steamAccess !== 'granted'" class="steam-banner" :class="{ ready: steamGamesLoaded }">
      <div class="steam-banner-copy">
        <div class="steam-banner-title">{{ steamBannerTitle }}</div>
        <div class="steam-banner-body">{{ steamBannerBody }}</div>
      </div>
      <button class="steam-banner-btn" :disabled="steamImportBusy" @click="importSteamLibrary(steamAccess === 'denied')">
        {{ steamButtonLabel }}
      </button>
    </div>

    <div class="scroll-area">
      <!-- ★ Job #3: Import Progress Bar -->
      <Transition name="fade">
        <div v-if="replay.importProgress.active" class="import-bar-wrap">
          <div class="import-bar-info">
            <span>Importing clips…</span>
            <span>{{ replay.importProgress.current }} / {{ replay.importProgress.total }}</span>
          </div>
          <div class="import-bar-bg">
            <div class="import-bar-fill" :style="{ width: (replay.importProgress.current / replay.importProgress.total * 100) + '%' }"></div>
          </div>
        </div>
      </Transition>

      <Transition name="fade">
        <div v-if="prefetchPhase !== 'idle'" class="import-bar-wrap thumb-progress-wrap" :class="{ done: prefetchPhase === 'done' }">
          <div class="import-bar-info">
            <span>{{ thumbnailPhaseLabel }}</span>
            <span>{{ thumbnailProgressText }}</span>
          </div>
          <div class="import-bar-bg">
            <div class="import-bar-fill thumb-progress-fill" :style="{ width: thumbnailProgressPercent + '%' }"></div>
          </div>
        </div>
      </Transition>

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

      <!-- View wrapper: fade when switching modes / showing empty states -->
      <Transition name="view-fade" mode="out-in">

      <!-- Empty state A: no clips exist at all -->
      <div
        v-if="!replay.loading && replay.loaded && hasNoClipsAtAll"
        key="empty-all"
        class="empty-state"
      >
        <div class="empty-ic">📁</div>
        <p>No clips found</p>
        <p class="empty-sub">Start recording some or import them if you have any</p>
        <button class="empty-import-btn" @click="importFolder">Import Folder</button>
      </div>

      <!-- Empty state B: clips exist but filtered/searched to zero -->
      <div
        v-else-if="!replay.loading && replay.loaded && isFilteredEmpty"
        key="empty-filtered"
        class="empty-state"
      >
        <div class="empty-ic">🔍</div>
        <p>No clips found</p>
        <p class="empty-sub">Try again or adjust your filters</p>
      </div>

      <!-- ═══ Date-grouped view ═══ -->
      <div v-else-if="dateGrouped" key="grouped" class="scroll-host">
        <div class="native-grid-host grouped-host" ref="groupedScrollRef" :class="{ scrolling: isScrolling }" @scroll.passive="onScroll">
          <div 
            class="date-groups"
            :style="{
              paddingTop: virtualRange.padTop + 'px',
              paddingBottom: virtualRange.padBot + 'px',
            }"
          >
            <template v-for="item in visibleItems" :key="item.key">
              <div v-if="item.type === 'header'" class="date-header" :style="{ height: item.height + 'px' }">
                <div
                  class="group-sel-box"
                  :class="{ checked: groupCheckState(item.group!) === 'all', indeterminate: groupCheckState(item.group!) === 'some' }"
                  @click.stop="toggleGroupSelect(item.group!)"
                >
                  <span v-if="groupCheckState(item.group!) === 'all'">✓</span>
                  <span v-else-if="groupCheckState(item.group!) === 'some'">−</span>
                </div>
                <span class="date-label">{{ item.group!.label }}</span>
                <span class="date-count">{{ item.group!.clips.length }}</span>
              </div>
              <div
                v-else-if="item.type === 'row'"
                class="group-row"
                :class="{ 'group-row-grid': viewMode === 'grid' }"
                :style="viewMode === 'grid' ? { minHeight: item.height + 'px' } : { height: item.height + 'px' }"
              >
                <div
                  v-if="viewMode === 'grid'"
                  class="clip-grid grouped-grid"
                  :style="{
                    gridTemplateColumns: `repeat(${gridCols}, 1fr)`,
                    '--name-size': fontScale.nameSize,
                    '--meta-size': fontScale.metaSize,
                  }"
                >
                  <ClipCard
                    v-for="clip in item.clips"
                    :key="clip.id"
                    v-memo="[clip.id, replay.isSelected(clip.id), clip.duration, clip.thumbnail, clip.custom_name, clip.game, clip.favorite, gridCols]"
                    :clip="clip"
                    :selected="replay.isSelected(clip.id)"
                    class="clip-stagger"
                    @click="onCardClick(clip)"
                    @contextmenu="openListMenu"
                    @preview="openPreview"
                    @editor="openAdvanced"
                    @rename="startRename"
                    @delete="deleteClip"
                  />
                </div>
                <div v-else class="clip-list">
                  <ClipListRow
                    v-for="clip in item.clips"
                    :key="clip.id"
                    v-memo="[clip.id, replay.isSelected(clip.id), clip.duration, clip.thumbnail, clip.custom_name, clip.game, clip.favorite, listStyles.fontSize]"
                    :clip="clip"
                    :selected="replay.isSelected(clip.id)"
                    :font-size="listStyles.fontSize"
                    :meta-font-size="listStyles.metaFontSize"
                    :pill-pad-y="listStyles.pillPadY"
                    :pill-pad-x="listStyles.pillPadX"
                    :chip-font-size="listStyles.chipFontSize"
                    :chip-pad-y="listStyles.chipPadY"
                    :chip-pad-x="listStyles.chipPadX"
                    :action-font-size="listStyles.actionFontSize"
                    :action-pad-y="listStyles.actionPadY"
                    :action-pad-x="listStyles.actionPadX"
                    :padding="listStyles.padding"
                    :thumb-w="listStyles.thumbW"
                    :thumb-h="listStyles.thumbH"
                    :media-url="mediaUrl"
                    :media-port="mediaPortNum"
                    @click="onCardClick"
                    @contextmenu="openListMenu"
                    @preview="openPreview"
                    @editor="openAdvanced"
                    @delete="deleteClip"
                    @favorite="toggleListFav"
                  />
                </div>
              </div>
              <div v-else-if="item.type === 'spacer'" :style="{ height: item.height + 'px' }"></div>
            </template>
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
              v-memo="[clip.id, replay.isSelected(clip.id), clip.duration, clip.thumbnail, clip.custom_name, clip.game, clip.favorite, gridCols]"
              :clip="clip"
              :selected="replay.isSelected(clip.id)"
              :class="{ 'clip-enter': clip._isNew }"
              @click="onCardClick(clip)"
              @contextmenu="openListMenu"
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
        >
          <div
            class="clip-list"
            :style="{
              paddingTop: virtualRange.padTop + 'px',
              paddingBottom: virtualRange.padBot + 'px',
            }"
          >
            <ClipListRow
              v-for="clip in visibleClips"
              :key="clip.id"
              v-memo="[clip.id, replay.isSelected(clip.id), clip.duration, clip.thumbnail, clip.custom_name, clip.game, clip.favorite, listStyles.fontSize]"
              :clip="clip"
              :selected="replay.isSelected(clip.id)"
              :font-size="listStyles.fontSize"
              :meta-font-size="listStyles.metaFontSize"
              :pill-pad-y="listStyles.pillPadY"
              :pill-pad-x="listStyles.pillPadX"
              :chip-font-size="listStyles.chipFontSize"
              :chip-pad-y="listStyles.chipPadY"
              :chip-pad-x="listStyles.chipPadX"
              :action-font-size="listStyles.actionFontSize"
              :action-pad-y="listStyles.actionPadY"
              :action-pad-x="listStyles.actionPadX"
              :padding="listStyles.padding"
              :thumb-w="listStyles.thumbW"
              :thumb-h="listStyles.thumbH"
              :media-url="mediaUrl"
              :media-port="mediaPortNum"
              @click="onCardClick"
              @contextmenu="openListMenu"
              @preview="openPreview"
              @editor="openAdvanced"
              @delete="deleteClip"
              @favorite="toggleListFav"
            />
          </div>
        </div>
        <OverlayScrollbar :scroll-el="listScrollRef" />
      </div>

      </Transition><!-- /view-fade -->
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
                  :placeholder="t('clips.gamesFilter.searchPlaceholder')"
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
                  <img v-if="opt.icon" :src="opt.icon" alt="" class="bulk-game-icon" loading="lazy" />
                  <span>{{ opt.label }}</span>
                </button>
                <div v-if="filteredBulkGames.length === 0" class="bulk-game-empty">{{ t('clips.gamesFilter.noMatches') }}</div>
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
        @contextmenu.prevent
      >
        <template v-if="contextMenuClip">
          <button class="ctx-item" @click="ctxAction('preview')" @contextmenu.prevent>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><polygon points="5 3 19 12 5 21 5 3"/></svg>
            {{ t('clips.contextMenu.preview') }}
          </button>
          <button class="ctx-item" @click="ctxAction('editor')" @contextmenu.prevent>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M6 20h-2a2 2 0 01-2-2v-2m0-4V8m0-4V4a2 2 0 012-2h2m4 0h4m4 0h2a2 2 0 012 2v2m0 4v4m0 4v2a2 2 0 01-2 2h-2m-4 0h-4"/><path d="M9 11l2 2 4-4"/></svg>
            {{ t('clips.contextMenu.edit') }}
          </button>
          <div class="ctx-sep" @contextmenu.prevent></div>
          <button class="ctx-item" @click="ctxAction('select')" @contextmenu.prevent>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><polyline points="9 11 12 14 20 6"/></svg>
            {{ t('clips.contextMenu.select') }}
          </button>
          <button class="ctx-item" @click="ctxAction('favorite')" @contextmenu.prevent>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
            {{ contextMenuClip.favorite ? t('clips.contextMenu.unfavorite') : t('clips.contextMenu.favorite') }}
          </button>
          <div class="ctx-sep" @contextmenu.prevent></div>
          <button class="ctx-item" @click="ctxAction('rename')" @contextmenu.prevent>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 013 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>
            {{ t('clips.contextMenu.rename') }}
          </button>
          <button class="ctx-item" @click="ctxAction('location')" @contextmenu.prevent>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>
            {{ t('clips.contextMenu.showInFolder') }}
          </button>
          <div class="ctx-sep" @contextmenu.prevent></div>
          <button class="ctx-item ctx-item-d" @click="ctxAction('delete')" @contextmenu.prevent>
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

.steam-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  margin: 10px 0 14px;
  padding: 12px 14px;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: linear-gradient(135deg, color-mix(in srgb, var(--accent) 10%, var(--bg-card)), var(--bg-card));
}
.steam-banner.ready {
  border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
}
.steam-banner-copy { min-width: 0; }
.steam-banner-title { font-size: 13px; font-weight: 700; color: var(--text); }
.steam-banner-body { margin-top: 3px; font-size: 12px; color: var(--text-sec); }
.steam-banner-btn {
  padding: 8px 12px;
  border: 1px solid color-mix(in srgb, var(--accent) 60%, var(--border));
  border-radius: 8px;
  background: color-mix(in srgb, var(--accent) 16%, var(--bg-surface));
  color: var(--text);
  font-size: 12px;
  font-weight: 700;
  cursor: pointer;
  white-space: nowrap;
}
.steam-banner-btn:hover:not(:disabled) { filter: brightness(1.06); }
.steam-banner-btn:disabled { opacity: .6; cursor: default; }

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
  overflow-anchor: none;
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

/* ★ Epic 3: List view container */
.clip-list { display:flex; flex-direction:column; gap:6px; }

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
.bulk-game-opt { width:100%; padding:7px 10px; background:transparent; border:none; border-radius:5px; color:var(--text-sec); font-size:12px; text-align:left; cursor:pointer; display:flex; align-items:center; gap:8px; }
.bulk-game-opt:hover { background:var(--bg-hover); color:var(--text); }
.bulk-game-opt.active { color:var(--accent); font-weight:600; background:color-mix(in srgb, var(--accent) 10%, transparent); }
.bulk-game-icon { width:18px; height:18px; border-radius:4px; object-fit:cover; flex-shrink:0; }
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
.date-groups { display:flex; flex-direction:column; }
.date-group { display:flex; flex-direction:column; }
.date-header { display:flex; align-items:center; gap:10px; padding-bottom:8px; border-bottom:1px solid var(--border); }
.group-row { width:100%; }
.group-row-grid { box-sizing:border-box; padding-top: 4px; }
.grouped-grid { width: 100%; }
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

/* Empty State */
.empty-state {
  flex: 1;
  display: flex; flex-direction: column;
  align-items: center; justify-content: center;
  gap: 12px;
  color: var(--text-muted);
  text-align: center;
  padding: 40px;
  user-select: none;
}
.empty-ic { font-size: 48px; opacity: 0.3; margin-bottom: 4px; }
.empty-state p { margin: 0; font-size: 16px; font-weight: 700; color: var(--text-sec); }
.empty-sub { font-size: 13px !important; font-weight: 400 !important; color: var(--text-muted) !important; max-width: 280px; }
.empty-import-btn {
  margin-top: 8px;
  padding: 10px 24px;
  background: var(--accent);
  color: #fff;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  transition: transform 0.2s, filter 0.2s;
}
.empty-import-btn:hover {
  filter: brightness(1.1);
  transform: translateY(-1px);
}
.empty-import-btn:active {
  transform: translateY(0);
}

/* Import Bar */
.import-bar-wrap {
  margin: 8px 22px 16px 10px;
  padding: 12px 16px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  box-shadow: 0 4px 12px rgba(0,0,0,.2);
}
.import-bar-info {
  display: flex;
  justify-content: space-between;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-sec);
}
.import-bar-bg {
  height: 6px;
  background: var(--bg-deep);
  border-radius: 3px;
  overflow: hidden;
}
.import-bar-fill {
  height: 100%;
  background: var(--accent);
  transition: width 0.3s ease;
}
.thumb-progress-wrap { margin-top: 0; }
.thumb-progress-wrap.done { border-color: color-mix(in srgb, var(--accent) 55%, var(--border)); }
.thumb-progress-fill { background: linear-gradient(90deg, var(--accent), color-mix(in srgb, var(--accent) 65%, #ffffff)); }
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

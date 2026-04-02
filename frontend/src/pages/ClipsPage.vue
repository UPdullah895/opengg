<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, inject, watch } from 'vue'
import { refDebounced } from '@vueuse/core'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useReplayStore, type Clip } from '../stores/replay'
import { usePersistenceStore } from '../stores/persistence'
import ClipCard from '../components/ClipCard.vue'
import OverlayScrollbar from '../components/OverlayScrollbar.vue'
import ClipEditor from '../components/ClipEditor.vue'
import AdvancedEditor from '../components/AdvancedEditor.vue'
import SelectField from '../components/SelectField.vue'
import GameFilterDropdown from '../components/GameFilterDropdown.vue'
import { mediaUrl } from '../utils/assets'
import { fmtDur, fmtSize, fmtRes, fmtDate } from '../utils/format'
import type { Ref } from 'vue'

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

// ── Editor / modal state ──
const editorClip   = ref<Clip | null>(null)
const editorMode   = ref<'preview' | 'trim'>('preview')
const advancedClip = ref<Clip | null>(null)
const renameTarget = ref<Clip | null>(null)
const renameValue  = ref('')
const toast        = ref('')

function showToast(msg: string) { toast.value = msg; setTimeout(() => toast.value = '', 3500) }
function refreshClips() { replay.fetchClips(persist.state?.settings?.clip_directories?.[0] || '', true) }

// ── View / sizing / grouping ──
const viewMode = ref<'grid' | 'list'>('grid')
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
    1: { thumbW: '192px', thumbH: '108px', fontSize: '18px', padding: '16px' },
    2: { thumbW: '128px', thumbH: '72px',  fontSize: '15px', padding: '12px' },
    3: { thumbW: '80px',  thumbH: '45px',  fontSize: '13px', padding: '8px'  },
    4: { thumbW: '56px',  thumbH: '32px',  fontSize: '11px', padding: '6px'  },
  }
  return map[gridSlider.value] ?? map[3]
})

// ── Scroll-suppression: disable hover transitions while scrolling ──
const isScrolling = ref(false)
let scrollTimer: ReturnType<typeof setTimeout> | null = null
function onScroll() {
  isScrolling.value = true
  if (scrollTimer) clearTimeout(scrollTimer)
  scrollTimer = setTimeout(() => { isScrolling.value = false }, 150)
}

// ★ Epic 2: Slider drag tooltip
const isDragging = ref(false)
const SLIDER_LABELS: Record<number, string> = { 1: 'Extra Large', 2: 'Large', 3: 'Medium', 4: 'Small' }
const sliderLabel = computed(() => SLIDER_LABELS[gridSlider.value] ?? '')

// ── Filter options ──
const sortOptions = [
  { value: 'newest',   label: 'Newest'   },
  { value: 'oldest',   label: 'Oldest'   },
  { value: 'longest',  label: 'Longest'  },
  { value: 'shortest', label: 'Shortest' },
]
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
const gameOptions = computed(() =>
  replay.games.map((g: string) => {
    if (g === 'all') return { value: g, label: 'All Games' }
    const count = replay.clips.filter((c: Clip) => c.game === g && !c.isSkeleton).length
    return { value: g, label: `${g} (${count})` }
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

function startRename(clip: Clip) { renameTarget.value = clip; renameValue.value = clip.custom_name || clip.filename.replace(/\.[^.]+$/, '') }
async function confirmRename() {
  if (!renameTarget.value) return
  const n = renameValue.value.trim()
  replay.updateClipMeta(renameTarget.value.filepath, { custom_name: n })
  try { await invoke('set_clip_meta', { update: { filepath: renameTarget.value.filepath, custom_name: n, favorite: renameTarget.value.favorite } }) } catch {}
  renameTarget.value = null
}

async function deleteClip(clip: Clip) {
  if (!confirm(`Delete "${clip.custom_name || clip.filename}"?`)) return
  try { await invoke('delete_clip', { filepath: clip.filepath }); replay.removeClip(clip.filepath); showToast('Clip deleted') } catch (e) { showToast(`Error: ${e}`) }
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

onMounted(async () => {
  if (!persist.loaded) await persist.load()
  replay.fetchStatus()
  replay.fetchClips(persist.state?.settings?.clip_directories?.[0] || '')

  unlistenAdded = await listen<string>('clip_added', async (event) => {
    const fp = event.payload
    if (replay.clips.find(c => c.filepath === fp && !c.isSkeleton)) return
    const tempId = `skeleton_${fp}`
    replay.injectSkeleton(tempId, fp)                       // ← grid stays visible
    await new Promise<void>(r => setTimeout(r, 2000))       // wait for muxer to flush
    try {
      const clip = await invoke<Clip | null>('get_clip_by_path', { filepath: fp })
      if (clip) replay.replaceSkeleton(tempId, clip)        // ← smooth swap in-place
      else      replay.removeClip(tempId)
    } catch { replay.removeClip(tempId) }
  })

  unlistenRemoved = await listen<string>('clip_removed', (event) => {
    replay.removeClip(event.payload)
  })
})

onBeforeUnmount(() => {
  unlistenAdded?.()
  unlistenRemoved?.()
})

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
      <div class="header-r">
        <span class="rec-dot" :class="{ active: replay.status !== 'idle' }"></span>
        <span class="rec-txt">{{ replay.status === 'idle' ? 'Idle' : replay.status === 'replay' ? `Replay · ${replay.replayDuration}s` : 'Recording' }}</span>
        <button class="ib" @click="replay.status==='idle'?replay.startReplay():replay.stopRecorder()">{{ replay.status==='idle'?'▶':'■' }}</button>
        <button class="ib" @click="replay.saveReplay()" :disabled="replay.status!=='replay'">💾</button>
        <button class="ib" @click="refreshClips()">↻</button>
      </div>
    </div>

    <!-- Controls — left group + right group so toggle is always far right -->
    <div class="ctrl-bar">
      <div class="ctrl-left">
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
          <div class="size-range-wrap">
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
              >
                <div class="list-thumb-wrap">
                  <img v-if="clip.thumbnail && mediaPortNum" class="list-thumb" :src="mediaUrl(clip.thumbnail, mediaPortNum)" loading="lazy" @error="(e: Event) => ((e.target as HTMLImageElement).style.display='none')" />
                  <div v-else class="list-thumb list-thumb-empty">▶</div>
                  <span v-if="clip.duration" class="list-badge">{{ fmtDur(clip.duration) }}</span>
                </div>
                <div class="list-info">
                  <span class="list-name">{{ clip.custom_name || clip.filename.replace(/\.[^.]+$/, '') }}</span>
                  <span class="list-meta">
                    <span v-if="clip.game && clip.game !== 'Unknown'">{{ clip.game }}</span>
                    <span v-if="clip.duration" class="lm-sep">·</span>
                    <span v-if="clip.duration">{{ fmtDur(clip.duration) }}</span>
                    <span v-if="clip.filesize" class="lm-sep">·</span>
                    <span v-if="clip.filesize">{{ fmtSize(clip.filesize) }}</span>
                    <span v-if="clip.width" class="lm-sep">·</span>
                    <span v-if="clip.width">{{ fmtRes(clip.width, clip.height) }}</span>
                    <span v-if="clip.created" class="lm-sep">·</span>
                    <span v-if="clip.created">{{ fmtDate(clip.created) }}</span>
                  </span>
                </div>
                <div class="list-actions">
                  <button class="list-act" @click.stop="openPreview(clip)">Preview</button>
                  <button class="list-act" @click.stop="openAdvanced(clip)">Edit</button>
                  <button class="list-act list-act-d" @click.stop="deleteClip(clip)">🗑</button>
                </div>
              </div>
            </div>
          </div>
          <div v-if="groupedClips.length === 0" class="empty-state">
            <div class="empty-ic">📅</div>
            <p>No clips match current filters</p>
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
          <!-- Native CSS grid — all cards rendered, browser handles scroll -->
          <div
            class="clip-grid native-grid"
            :style="{
              gridTemplateColumns: `repeat(${gridCols}, 1fr)`,
              '--name-size': fontScale.nameSize,
              '--meta-size': fontScale.metaSize,
            }"
          >
            <ClipCard
              v-for="clip in filteredRealClips"
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
            >
              <div class="list-thumb-wrap">
                <img v-if="clip.thumbnail && mediaPortNum"
                     class="list-thumb"
                     :src="mediaUrl(clip.thumbnail, mediaPortNum)"
                     loading="lazy"
                     @error="(e: Event) => ((e.target as HTMLImageElement).style.display='none')" />
                <div v-else class="list-thumb list-thumb-empty">▶</div>
                <span v-if="clip.duration" class="list-badge">{{ fmtDur(clip.duration) }}</span>
              </div>
              <div class="list-info">
                <span class="list-name">{{ clip.custom_name || clip.filename.replace(/\.[^.]+$/, '') }}</span>
                <span class="list-meta">
                  <span v-if="clip.game && clip.game !== 'Unknown'">{{ clip.game }}</span>
                  <span v-if="clip.duration" class="lm-sep">·</span>
                  <span v-if="clip.duration">{{ fmtDur(clip.duration) }}</span>
                  <span v-if="clip.filesize" class="lm-sep">·</span>
                  <span v-if="clip.filesize">{{ fmtSize(clip.filesize) }}</span>
                  <span v-if="clip.width" class="lm-sep">·</span>
                  <span v-if="clip.width">{{ fmtRes(clip.width, clip.height) }}</span>
                  <span v-if="clip.created" class="lm-sep">·</span>
                  <span v-if="clip.created">{{ fmtDate(clip.created) }}</span>
                </span>
              </div>
              <div class="list-actions">
                <button class="list-act" @click.stop="openPreview(clip)">Preview</button>
                <button class="list-act" @click.stop="openAdvanced(clip)">Edit</button>
                <button class="list-act list-act-d" @click.stop="deleteClip(clip)">🗑</button>
              </div>
            </div>
          </div>
        </div>
        <OverlayScrollbar :scroll-el="listScrollRef" />
      </div>

      </Transition><!-- /view-fade -->

      <!-- Empty state — only shown when no real clips pass the current filters -->
      <div
        v-if="!replay.loading && replay.loaded && cssVisibleCount === 0"
        class="empty-state"
      >
        <div class="empty-ic">{{ replay.filterFav?'❤':replay.search?'🔍':'📁' }}</div>
        <p v-if="replay.search">No clips matching "{{ replay.search }}"</p>
        <p v-else-if="replay.filterFav">No favorited clips</p>
        <template v-else><p>No clips found</p><p class="empty-sub">{{ persist.state?.settings?.clip_directories?.[0] || '~/Videos/OpenGG' }}</p></template>
      </div>
    </div>

    <!-- ★ Epic 1: Multi-select bulk action bar -->
    <Transition name="slide-up">
      <div v-if="replay.selectMode" class="sel-bar">
        <span>{{ replay.selectedCount }} selected</span>
        <div style="flex:1"></div>

        <!-- Bug 1: Smart toggle — Unfavorite All when all selected are already favorited -->
        <button class="sel-btn" :class="{ 'sel-btn-fav': !allSelectedFavorited, 'sel-btn-unfav': allSelectedFavorited }" @click="bulkFavorite()">
          {{ allSelectedFavorited ? '💔 Unfavorite All' : '❤ Favorite All' }}
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

        <button class="sel-btn" @click="replay.clearSelection()">Clear</button>
        <button class="sel-btn sel-btn-d" @click="deleteSelected()">🗑 Delete</button>
      </div>
    </Transition>

    <!-- Rename dialog -->
    <Teleport to="body">
      <div v-if="renameTarget" class="dlg-ov" @click.self="renameTarget=null">
        <div class="dlg">
          <h3>Rename Clip</h3>
          <input v-model="renameValue" class="dlg-in" @keydown.enter="confirmRename" autofocus />
          <div class="dlg-btns">
            <button class="dlg-btn" @click="renameTarget=null">Cancel</button>
            <button class="dlg-btn dlg-pri" @click="confirmRename">Save</button>
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
          <button class="ctx-item" @click="ctxAction('preview')">▶ Preview</button>
          <button class="ctx-item" @click="ctxAction('editor')">✂ Edit</button>
          <div class="ctx-sep"></div>
          <button class="ctx-item" @click="ctxAction('select')">☑ Select</button>
          <button class="ctx-item" @click="ctxAction('favorite')">{{ contextMenuClip.favorite ? '💔 Unfavorite' : '❤ Favorite' }}</button>
          <div class="ctx-sep"></div>
          <button class="ctx-item" @click="ctxAction('rename')">✏ Rename</button>
          <button class="ctx-item" @click="ctxAction('location')">📂 Show in Folder</button>
          <div class="ctx-sep"></div>
          <button class="ctx-item ctx-item-d" @click="ctxAction('delete')">🗑 Delete</button>
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
.rec-dot { width:8px; height:8px; border-radius:50%; background:var(--text-muted); }
.rec-dot.active { background:var(--danger); animation:pulse 1.2s infinite; }
@keyframes pulse { 0%,100%{opacity:1} 50%{opacity:.5} }
.rec-txt { font-size:12px; color:var(--text-sec); }
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
.size-slider-wrap { display:flex; align-items:center; gap:6px; padding:0 10px; height:32px; background:var(--bg-card); border:1px solid var(--border); border-radius:7px; }
.size-ic { width:14px; height:14px; color:var(--text-muted); flex-shrink:0; }
.size-range-wrap { position:relative; display:flex; align-items:center; }

.size-range {
  -webkit-appearance: none; appearance: none;
  width: 72px; height: 6px;
  background: var(--bg-deep); border: 1px solid var(--border); border-radius: 3px;
  outline: none; cursor: pointer;
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
.scroll-area { flex:1; min-height:0; overflow:hidden; display:flex; flex-direction:column; }

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
}
.native-grid-host::-webkit-scrollbar { display: none; width: 0; }
.native-grid { }

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
  display:flex; align-items:center; gap:12px;
  padding: var(--list-pad, 8px) calc(var(--list-pad, 8px) + 4px);
  background:var(--bg-card); border:1px solid var(--border); border-radius:8px;
  cursor:pointer; transition: background .15s, padding .25s ease;
  contain: layout style; content-visibility: auto; contain-intrinsic-size: auto 60px;
}
.list-row:hover { background:var(--bg-hover); }
.list-row.selected { border-color:var(--accent); background:color-mix(in srgb, var(--accent) 8%, transparent); }

.list-thumb {
  width: var(--list-thumb-w, 80px); height: var(--list-thumb-h, 45px);
  object-fit:cover; border-radius:4px; flex-shrink:0; background:var(--bg-deep);
  transition: width .25s ease, height .25s ease;
}
.list-thumb-empty {
  width: var(--list-thumb-w, 80px); height: var(--list-thumb-h, 45px);
  background:var(--bg-deep); border-radius:4px; flex-shrink:0;
  display:flex; align-items:center; justify-content:center;
  font-size:18px; color:var(--text-muted);
  transition: width .25s ease, height .25s ease;
}
.list-info { flex:1; min-width:0; display:flex; flex-direction:column; gap:3px; }
.list-name {
  font-size: var(--list-font, 13px); font-weight:600;
  white-space:nowrap; overflow:hidden; text-overflow:ellipsis;
  transition: font-size .25s ease;
}
.list-thumb-wrap {
  position: relative; flex-shrink: 0;
  width: var(--list-thumb-w, 80px); height: var(--list-thumb-h, 45px);
  transition: width .25s ease, height .25s ease;
}
.list-thumb-wrap .list-thumb { width: 100%; height: 100%; transition: none; }
.list-thumb-wrap .list-thumb-empty { width: 100%; height: 100%; transition: none; }
.list-badge {
  position: absolute; bottom: 3px; right: 3px;
  background: rgba(0,0,0,.8); color: #fff;
  font-size: 10px; font-weight: 600;
  padding: 1px 5px; border-radius: 3px;
  pointer-events: none; line-height: 1.4;
}
.list-meta { font-size:11px; color:var(--text-muted); display:flex; align-items:center; flex-wrap:wrap; gap:3px; }
.lm-sep { opacity: 0.4; }
.list-actions { display:flex; gap:6px; flex-shrink:0; }
.list-act { padding:5px 10px; border:1px solid var(--border); border-radius:5px; background:var(--bg-surface); color:var(--text-sec); font-size:11px; cursor:pointer; }
.list-act:hover { background:var(--bg-hover); }
.list-act-d { color:var(--danger); }
.list-act-d:hover { background:rgba(220,38,38,.1); }
.list-kebab { flex-shrink:0; width:26px; height:26px; border-radius:5px; border:none; background:transparent; color:var(--text-muted); cursor:pointer; display:flex; align-items:center; justify-content:center; transition:all .15s; }
.list-kebab:hover { background:var(--bg-hover); color:var(--text); }
.list-kebab svg { width:14px; height:14px; }

/* List view context menu */
.list-ctx { position:fixed; z-index:10000; min-width:200px; background:var(--bg-card); border:1px solid var(--border); border-radius:8px; padding:4px; box-shadow:0 8px 32px rgba(0,0,0,.5); }
.list-ctx-i { display:flex; align-items:center; gap:8px; width:100%; padding:8px 12px; border:none; border-radius:5px; background:transparent; color:var(--text); font-size:13px; cursor:pointer; text-align:left; }
.list-ctx-i:hover { background:var(--bg-hover); }
.list-ctx-ic { width:15px; height:15px; flex-shrink:0; color:var(--text-muted); }
.list-ctx-sep { height:1px; background:var(--border); margin:4px 0; }
.list-ctx-d { color:var(--danger) !important; }

/* Empty state */
.empty-state { display:flex; flex-direction:column; align-items:center; justify-content:center; color:var(--text-muted); padding:40px; min-height:200px; }
.empty-ic { font-size:36px; margin-bottom:10px; opacity:.4; }
.empty-sub { font-size:12px; opacity:.6; }

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
  display:block; width:100%; padding:8px 12px; background:none; border:none;
  border-radius:5px; color:var(--text-sec); font-size:13px; text-align:left;
  cursor:pointer; white-space:nowrap;
}
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

import { defineStore } from 'pinia'
import { ref, computed, shallowRef, triggerRef, markRaw } from 'vue'
import { invoke } from '@tauri-apps/api/core'
<<<<<<< HEAD
=======
import { perfWrap } from '../utils/perf'
>>>>>>> origin/SH3FAN-Branch
import { usePersistenceStore } from './persistence'

export interface Clip {
  id: string; filename: string; filepath: string; filesize: number
  created: string; duration: number; width: number; height: number
  game: string; custom_name: string; favorite: boolean; thumbnail: string
  isSkeleton?: boolean // ★ Epic 3: live-watcher placeholder — never persisted
  _isNew?: boolean     // transient UI flag — cleared after card-entry animation
  _search?: string     // pre-lowercased searchable haystack (derived, not persisted)
}

export type SortMode = 'newest' | 'oldest' | 'longest' | 'shortest'
export type DateFormat = 'YMD' | 'YDM' // YYYY/MM/DD or YYYY/DD/MM

// ── Month lookup: full names + 3-letter abbreviations → 2-digit month ──
const MONTH_MAP: Record<string, string> = {
  january: '01', jan: '01',
  february: '02', feb: '02',
  march: '03', mar: '03',
  april: '04', apr: '04',
  may: '05',
  june: '06', jun: '06',
  july: '07', jul: '07',
  august: '08', aug: '08',
  september: '09', sep: '09', sept: '09',
  october: '10', oct: '10',
  november: '11', nov: '11',
  december: '12', dec: '12',
}
const MONTH_NAMES_FULL = ['', 'january','february','march','april','may','june','july','august','september','october','november','december']

// Build pre-lowercased searchable haystack for a clip.
// Includes: custom_name, filename, game, year, month name + abbrev, and a __mMM sentinel
// so month-name queries can be AND'd with free-text tokens.
function buildSearch(c: { custom_name: string; filename: string; game: string; created: string }): string {
  const parts: string[] = []
  if (c.custom_name) parts.push(c.custom_name.toLowerCase())
  if (c.filename)    parts.push(c.filename.toLowerCase())
  if (c.game)        parts.push(c.game.toLowerCase())
  if (c.created && c.created.length >= 10) {
    // created is "YYYY-MM-DD ..." — extract parts
    const y = c.created.slice(0, 4)
    const m = c.created.slice(5, 7)
    const d = c.created.slice(8, 10)
    const mn = parseInt(m, 10)
    parts.push(y)
    parts.push(`__m${m}`) // sentinel for month filter
    if (mn >= 1 && mn <= 12) parts.push(MONTH_NAMES_FULL[mn])
    // Date forms the user might type — both year-first slash orderings so
    // either YYYY/MM/DD or YYYY/DD/MM typed by the user will match.
    parts.push(`${y}/${m}/${d}`)
    parts.push(`${y}/${d}/${m}`)
    // Also match bare year-month like "2026/04" or "2026-04"
    parts.push(`${y}/${m}`)
    parts.push(`${y}-${m}`)
  }
  return parts.join(' ')
}

// ── Query parser: rewrites month-name tokens into __mMM sentinels and
// normalizes slash-dates to match the order-agnostic forms in the haystack.
// Returns an array of lowercased tokens to AND together.
function parseSearchQuery(q: string, dateFormat: DateFormat): string[] {
  const raw = q.trim().toLowerCase()
  if (!raw) return []
  const out: string[] = []
  for (const tok of raw.split(/\s+/)) {
    if (!tok) continue
    // Month name → sentinel
    if (MONTH_MAP[tok]) { out.push(`__m${MONTH_MAP[tok]}`); continue }
    // Full date: YYYY/MM/DD or YYYY/DD/MM (dateFormat decides interpretation)
    const full = tok.match(/^(\d{4})[/-](\d{1,2})[/-](\d{1,2})$/)
    if (full) {
      const y = full[1]
      const a = full[2].padStart(2, '0')
      const b = full[3].padStart(2, '0')
      // In YMD mode a=month, b=day. In YDM mode a=day, b=month.
      const mo = dateFormat === 'YMD' ? a : b
      const da = dateFormat === 'YMD' ? b : a
      out.push(`${y}/${mo}/${da}`)
      continue
    }
    // Year-month only: YYYY/MM — works the same in both modes
    const ym = tok.match(/^(\d{4})[/-](\d{1,2})$/)
    if (ym) {
      out.push(`${ym[1]}/${ym[2].padStart(2, '0')}`)
      continue
    }
    // Bare year
    if (/^\d{4}$/.test(tok)) { out.push(tok); continue }
    out.push(tok)
  }
  return out
}

export const useReplayStore = defineStore('replay', () => {
  const status = ref<'idle' | 'replay' | 'recording'>('idle')
  const replayDuration = ref(0)
  // Phase 2d: shallowRef avoids deep reactivity on every clip object
  const clips = shallowRef<Clip[]>([])
  // Reactive thumbnail map — updated by setThumbnail so list view can react
  // without triggering the expensive filteredClips recompute.
  const liveThumbs = shallowRef(new Map<string, string>())
  // Reactive probe map — mirrors liveThumbs pattern for duration/width/height.
  // ClipCard watches this directly so duration badge and resolution pill update
  // without relying on the filteredClips computed prop chain.
  const liveMeta = shallowRef(new Map<string, { duration: number; width: number; height: number }>())
  const loading = ref(false)
  const loaded = ref(false)
  const clipsProbed = ref(false)   // true after Phase 2 (probe_clips) completes or is skipped
  const isPrefetching = ref(false) // true while prefetchThumbnails() sequential loop is running
  /** Monotonic counter — incremented every time fetchClips() successfully populates clips.
   * ClipsPage.vue watches this so prefetchThumbnails runs on the initial mount AND every
   * subsequent refresh (e.g., after the user changes clip_directories in Settings).
   * Without this, the prefetch only ran on the first mount — if that mount had 0 clips
   * (reset sentinel path), the 419-clip import was served entirely by the IO fallback
   * path in ClipCard.vue, firing 218 single-clip probe_clips calls instead of 1 batch. */
  const clipsLoadedAt = ref(0)
  const scrolling = ref(false)     // set by ClipsPage while user is actively scrolling
  const pageActive = ref(true)     // false when ClipsPage is deactivated by KeepAlive — ClipCard clears thumbUrl
  const lastFolder = ref('')

  // Search/Sort/Filter
  const search = ref('')
  const sortMode = ref<SortMode>('newest')
  const filterGame = ref('all')       // legacy single-select (kept for compat)
  const selectedGames = ref<string[]>([]) // ★ Epic 3: multi-select game filter
  const filterFav = ref(false)

  // Multi-select
  const selectedIds = ref<Set<string>>(new Set())
  const selectMode = ref(false)

  // ── Context menu singleton ──
  const activeMenuClipId = ref('')
  const activeMenuPos = ref({ x: 0, y: 0 })

  // ── Cross-page preview trigger (set before navigating to /clips) ──
  const previewTargetClipId = ref<string | null>(null)

  // ── Scan state (for scan banner in grid view) ──
  const scanActive = ref(false)
  const scanCount = ref(0)

  // ── Computed ──
  const games = computed(() => {
    const s = new Set<string>()
    for (const c of clips.value) if (c.game) s.add(c.game)
    return ['all', ...Array.from(s).sort()]
  })

  // Dev-only recomputation counter — helps detect unexpected triggers during thumbnail bursts
  let _fcRunCount = 0

  const filteredClips = computed(() => {
    if (import.meta.env.DEV) {
      _fcRunCount++
      console.debug(`[perf] filteredClips run #${_fcRunCount}`)
    }
    const t0 = performance.now()
    // Skeletons always float to the top regardless of any filter/sort
    const skeletons: Clip[] = []
    const real: Clip[] = []
    for (const c of clips.value) {
      if (c.isSkeleton) skeletons.push(c)
      else real.push(c)
    }
    // Pull the user's configured date format (persistence is lazy — default to YMD if not loaded)
    let dateFormat: DateFormat = 'YMD'
    try {
      const persist = usePersistenceStore()
      const df = persist.state?.settings?.dateFormat as DateFormat | undefined
      if (df === 'YMD' || df === 'YDM') dateFormat = df
    } catch { /* store not yet initialized during SSR-like contexts */ }

    // Tokenize query once per recompute
    const tokens = parseSearchQuery(search.value, dateFormat)
    let r = real
    if (tokens.length > 0) {
      r = r.filter(c => {
        const h = c._search || ''
        for (const t of tokens) { if (!h.includes(t)) return false }
        return true
      })
    }
    if (filterFav.value) r = r.filter(c => c.favorite)
    // ★ Epic 3: multi-select game filter takes priority over legacy single filterGame
    if (selectedGames.value.length > 0) r = r.filter(c => selectedGames.value.includes(c.game))
    else if (filterGame.value !== 'all') r = r.filter(c => c.game === filterGame.value)
    // Sort last — allocate a fresh array so we don't mutate the filtered slice
    const sorted = [...r]
    switch (sortMode.value) {
      case 'oldest':   sorted.sort((a,b) => a.created.localeCompare(b.created)); break
      case 'longest':  sorted.sort((a,b) => b.duration - a.duration); break
      case 'shortest': sorted.sort((a,b) => a.duration - b.duration); break
      default:         sorted.sort((a,b) => b.created.localeCompare(a.created))
    }
    const result = [...skeletons, ...sorted]
    if (import.meta.env.DEV) {
      const dt = performance.now() - t0
      if (dt > 1) console.debug(`[perf] filteredClips: ${dt.toFixed(1)}ms (${result.length} clips)`)
    }
    return result
  })

  const selectedCount = computed(() => selectedIds.value.size)
  const favCount = computed(() => clips.value.filter(c => c.favorite).length)

  // ── Actions ──
  async function fetchStatus() {
    try { const r = await invoke<string>('get_recorder_status'); if (r.startsWith('replay:')) { status.value='replay'; replayDuration.value=parseInt(r.split(':')[1])||30 } else if (r==='recording') status.value='recording'; else status.value='idle' }
    catch (e) { console.error('fetchStatus:', e) }
  }
  async function startReplay(d=30) { await invoke('start_replay',{duration:d}); status.value='replay'; replayDuration.value=d }
  async function stopRecorder() { await invoke('stop_recorder'); status.value='idle' }
  async function saveReplay() {
    const persist = usePersistenceStore()
    if (persist.state.settings.gsrEnabled) {
      await invoke('save_gsr_replay', { restartOnSave: persist.state.settings.gsrRestartOnSave })
      return
    }
    await invoke('save_replay')
  }

  async function fetchClips(folder='', force=false) {
    if (loaded.value && !force && folder === lastFolder.value) return
    loading.value = true
    clipsProbed.value = false
    _fcRunCount = 0 // reset counter for fresh load
    try {
      // Phase 1: fast load — skips ffprobe for uncached clips so grid appears immediately.
      // Uncached clips arrive with duration=0, width=0, height=0.
      const raw = await perfWrap('ipc:get_clips_fast', () => invoke<Clip[]>('get_clips_fast', { folder }))
      // ★ Epic 2 P5: Auto-fill game from filename prefix if empty
      for (const c of raw) {
        if (!c.game || c.game === 'Unknown') {
          const prefix = c.filename.split(/[_\-.]/)[0]
          if (prefix && prefix.length > 1) c.game = prefix.replace(/-/g, ' ')
        }
      }
      // Phase 2d: markRaw prevents deep reactive wrapping of clip objects
      // Also precompute the _search haystack so filteredClips() can do one
      // .includes() per clip per token instead of re-lowercasing every keystroke.
      if (import.meta.env.DEV) {
        const t0 = performance.now()
        clips.value = raw.map(c => { c._search = buildSearch(c); return markRaw(c) })
        console.debug(`[perf] markRaw map: ${(performance.now() - t0).toFixed(1)}ms (${raw.length} clips)`)
      } else {
        clips.value = raw.map(c => { c._search = buildSearch(c); return markRaw(c) })
      }
      lastFolder.value = folder
      loaded.value = true
      loading.value = false // show clips NOW — probe+thumbnail handled per-clip in prefetchThumbnails
      clipsLoadedAt.value++ // signal to ClipsPage that a fresh batch is ready for prefetch
    }
    catch (e) { console.error('fetchClips:', e); clips.value = [] }
    finally { loading.value = false; clipsProbed.value = true }
  }

  // rAF-coalesced clips trigger — collapses N backfill completions into ONE
  // filteredClips recompute per frame instead of N per frame. This is the key
  // fix for the scroll-to-bottom spike: thumbnail completions no longer cause
  // a recompute storm on the grid's main render path.
  let _clipsFlushScheduled = false
  function scheduleClipsFlush() {
    if (_clipsFlushScheduled) return
    _clipsFlushScheduled = true
    requestAnimationFrame(() => {
      _clipsFlushScheduled = false
      triggerRef(clips)
    })
  }
  /** Force an immediate filteredClips recompute. Used by prefetch loops at burst end. */
  function flushClipsNow() { triggerRef(clips) }

  // Update probe data (duration/width/height) AND thumbnail for a single clip atomically.
  // Both changes go into ONE new markRaw object so when filteredClips eventually
  // recomputes, the latest values are visible. By default we do NOT triggerRef(clips)
  // directly — ClipCard watches liveMeta + liveThumbs so the card updates immediately,
  // and we coalesce the (optional) clips trigger into one rAF flush per frame.
  function applyProbeAndThumb(fp: string, dur: number, w: number, h: number, thumbId: string, thumbPath: string, opts?: { triggerClips?: boolean }) {
    const i = clips.value.findIndex(c => c.filepath === fp)
    if (i >= 0) {
      const prev = clips.value[i]
      const next = { ...prev, duration: dur, width: w, height: h, thumbnail: thumbPath }
      next._search = buildSearch(next)
      clips.value[i] = markRaw(next)
      // Default: defer to next rAF — coalesces bursts. Pass {triggerClips:true}
      // if the caller needs an immediate recompute (rare).
      if (opts?.triggerClips) triggerRef(clips)
      else scheduleClipsFlush()
    }
    // Push duration/resolution via liveMeta (same pattern as liveThumbs for thumbnail).
    // ClipCard watches this directly — no dependence on the filteredClips prop chain.
    if (dur > 0) {
      liveMeta.value.set(thumbId, { duration: dur, width: w, height: h })
      triggerRef(liveMeta)
    }
    liveThumbs.value.set(thumbId, thumbPath)
    triggerRef(liveThumbs)
  }

  /**
   * Bulk-apply probe results (duration/width/height) for many clips at once.
   * Called by ClipsPage.prefetchThumbnails after a single batched probe_clips() call.
   * Much cheaper than N × applyProbeAndThumb — one liveMeta trigger + one rAF-coalesced
   * clips flush for the entire batch, instead of N triggers.
   */
  function applyBulkProbe(results: Array<[string, number, number, number]>) {
    if (!results.length) return
    // Build a quick lookup: filepath → index in clips.value (one pass)
    const idxByFp = new Map<string, number>()
    for (let i = 0; i < clips.value.length; i++) idxByFp.set(clips.value[i].filepath, i)
    for (const [fp, dur, w, h] of results) {
      const i = idxByFp.get(fp)
      if (i === undefined) continue
      const prev = clips.value[i]
      const next = { ...prev, duration: dur, width: w, height: h }
      next._search = buildSearch(next)
      clips.value[i] = markRaw(next)
      if (dur > 0) liveMeta.value.set(prev.id, { duration: dur, width: w, height: h })
    }
    // Single trigger for the whole batch — badges on every probed card appear in one frame
    triggerRef(liveMeta)
    scheduleClipsFlush()
  }

  // Phase 2d: updateClipMeta replaces the clip object with a new markRaw copy so Vue's
  // shallowRef prop-diffing detects the change and re-renders affected ClipCard components.
  // Also recomputes _search when custom_name/game/filename/created changed.
  function updateClipMeta(fp: string, u: Partial<Clip>) {
    const i = clips.value.findIndex(c => c.filepath === fp)
    if (i >= 0) {
      const next = { ...clips.value[i], ...u }
      if ('custom_name' in u || 'game' in u || 'filename' in u || 'created' in u) {
        next._search = buildSearch(next)
      }
      clips.value[i] = markRaw(next)
      triggerRef(clips)
    }
  }
  function removeClip(fp: string) { clips.value = clips.value.filter(c => c.filepath !== fp && c.id !== fp); selectedIds.value.delete(fp) }
  function setThumbnail(id: string, p: string) {
    const i = clips.value.findIndex(c => c.id === id)
    // Mutate clip's thumbnail in-place but do NOT triggerRef(clips).
    // ClipCard.vue already has a local `thumbUrl` ref that updates immediately.
    // Skipping triggerRef prevents ~100 full recomputes of filteredClips/games
    // during the thumbnail loading burst. The updated thumbnail will be visible in the
    // clip object the next time filteredClips naturally recomputes (e.g., on filter change).
    if (i >= 0) { clips.value[i] = markRaw({ ...clips.value[i], thumbnail: p }) }
    // Update liveThumbs so list view re-renders without touching the clips computeds.
    liveThumbs.value.set(id, p)
    triggerRef(liveThumbs)
  }
  /** Prepend a new clip (from file-watcher) without a full rescan. */
  function addClip(clip: Clip) {
    if (!clips.value.find(c => c.filepath === clip.filepath)) {
      clip._search = buildSearch(clip)
      clips.value = [markRaw(clip), ...clips.value]
    }
  }

  // ★ Epic 3: Inject a loading skeleton at the top of the list, then swap it for real data.
  // This avoids wiping the entire clips array while a new file is being parsed.
  function injectSkeleton(tempId: string, filepath: string) {
    if (clips.value.find(c => c.id === tempId)) return
    clips.value = [markRaw({
      id: tempId, filename: '', filepath, filesize: 0,
      created: '', duration: 0, width: 0, height: 0,
      game: '', custom_name: '', favorite: false, thumbnail: '',
      isSkeleton: true,
    }), ...clips.value]
  }
  function replaceSkeleton(tempId: string, clip: Clip) {
    clip._isNew = true
    clip._search = buildSearch(clip)
    const rawClip = markRaw(clip)
    const idx = clips.value.findIndex(c => c.id === tempId)
    if (idx >= 0) { const arr = [...clips.value]; arr.splice(idx, 1, rawClip); clips.value = arr }
    else clips.value = [rawClip, ...clips.value]
    setTimeout(() => { rawClip._isNew = false }, 400)
  }

  // Multi-select
  function toggleSelect(id: string) { if (selectedIds.value.has(id)) selectedIds.value.delete(id); else selectedIds.value.add(id); selectMode.value = selectedIds.value.size > 0 }
  function clearSelection() { selectedIds.value.clear(); selectMode.value = false }
  function isSelected(id: string) { return selectedIds.value.has(id) }

  return {
    status, replayDuration, clips, loading, loaded, clipsProbed, isPrefetching, scrolling, pageActive, clipsLoadedAt,
    search, sortMode, filterGame, selectedGames, filterFav,
    games, filteredClips, favCount,
    selectedIds, selectMode, selectedCount,
    fetchStatus, startReplay, stopRecorder, saveReplay,
    liveThumbs,
    fetchClips, updateClipMeta, applyProbeAndThumb, applyBulkProbe, flushClipsNow, removeClip, setThumbnail, addClip, injectSkeleton, replaceSkeleton,
    liveMeta,
    toggleSelect, clearSelection, isSelected,
    activeMenuClipId, activeMenuPos,
    previewTargetClipId,
    scanActive, scanCount,
  }
})

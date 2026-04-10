import { defineStore } from 'pinia'
import { ref, computed, shallowRef, triggerRef, markRaw } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { perfWrap } from '../utils/perf'

export interface Clip {
  id: string; filename: string; filepath: string; filesize: number
  created: string; duration: number; width: number; height: number
  game: string; custom_name: string; favorite: boolean; thumbnail: string
  isSkeleton?: boolean // ★ Epic 3: live-watcher placeholder — never persisted
  _isNew?: boolean     // transient UI flag — cleared after card-entry animation
}

export type SortMode = 'newest' | 'oldest' | 'longest' | 'shortest'

export const useReplayStore = defineStore('replay', () => {
  const status = ref<'idle' | 'replay' | 'recording'>('idle')
  const replayDuration = ref(0)
  // Phase 2d: shallowRef avoids deep reactivity on every clip object
  const clips = shallowRef<Clip[]>([])
  // Reactive thumbnail map — updated by setThumbnail so list view can react
  // without triggering the expensive filteredClips/sortedClips recompute.
  const liveThumbs = shallowRef(new Map<string, string>())
  // Reactive probe map — mirrors liveThumbs pattern for duration/width/height.
  // ClipCard watches this directly so duration badge and resolution pill update
  // without relying on the filteredClips computed prop chain.
  const liveMeta = shallowRef(new Map<string, { duration: number; width: number; height: number }>())
  const loading = ref(false)
  const loaded = ref(false)
  const clipsProbed = ref(false)   // true after Phase 2 (probe_clips) completes or is skipped
  const isPrefetching = ref(false) // true while prefetchThumbnails() sequential loop is running
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
      const dt = performance.now()
      const stack = new Error().stack?.split('\n')[2]?.trim() ?? ''
      console.debug(`[perf] filteredClips run #${_fcRunCount} ${stack}`)
      void dt // referenced below after compute
    }
    const t0 = performance.now()
    // Skeletons always float to the top regardless of any filter/sort
    const skeletons = clips.value.filter(c => c.isSkeleton)
    let r = clips.value.filter(c => !c.isSkeleton)
    if (search.value) { const q = search.value.toLowerCase(); r = r.filter(c => (c.custom_name || c.filename).toLowerCase().includes(q) || c.game.toLowerCase().includes(q)) }
    if (filterFav.value) r = r.filter(c => c.favorite)
    // ★ Epic 3: multi-select game filter takes priority over legacy single filterGame
    if (selectedGames.value.length > 0) r = r.filter(c => selectedGames.value.includes(c.game))
    else if (filterGame.value !== 'all') r = r.filter(c => c.game === filterGame.value)
    switch (sortMode.value) {
      case 'oldest': r = [...r].sort((a,b) => a.created.localeCompare(b.created)); break
      case 'longest': r = [...r].sort((a,b) => b.duration - a.duration); break
      case 'shortest': r = [...r].sort((a,b) => a.duration - b.duration); break
      default: r = [...r].sort((a,b) => b.created.localeCompare(a.created))
    }
    const result = [...skeletons, ...r]
    if (import.meta.env.DEV) {
      const dt = performance.now() - t0
      if (dt > 1) console.debug(`[perf] filteredClips: ${dt.toFixed(1)}ms (${result.length} clips)`)
    }
    return result
  })

  // ★ CSS-filter arch: sort ONLY — no filter applied. ClipsPage uses v-show="isMatch()" instead.
  const sortedClips = computed(() => {
    const t0 = performance.now()
    const skeletons = clips.value.filter(c => c.isSkeleton)
    let r = clips.value.filter(c => !c.isSkeleton)
    switch (sortMode.value) {
      case 'oldest':   r = [...r].sort((a,b) => a.created.localeCompare(b.created)); break
      case 'longest':  r = [...r].sort((a,b) => b.duration - a.duration); break
      case 'shortest': r = [...r].sort((a,b) => a.duration - b.duration); break
      default:         r = [...r].sort((a,b) => b.created.localeCompare(a.created))
    }
    const result = [...skeletons, ...r]
    if (import.meta.env.DEV) {
      const dt = performance.now() - t0
      if (dt > 1) console.debug(`[perf] sortedClips: ${dt.toFixed(1)}ms (${result.length} clips)`)
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
  async function saveReplay() { await invoke('save_replay') }

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
      if (import.meta.env.DEV) {
        const t0 = performance.now()
        clips.value = raw.map(c => markRaw(c))
        console.debug(`[perf] markRaw map: ${(performance.now() - t0).toFixed(1)}ms (${raw.length} clips)`)
      } else {
        clips.value = raw.map(c => markRaw(c))
      }
      lastFolder.value = folder
      loaded.value = true
      loading.value = false // show clips NOW — probe+thumbnail handled per-clip in prefetchThumbnails
    }
    catch (e) { console.error('fetchClips:', e); clips.value = [] }
    finally { loading.value = false; clipsProbed.value = true }
  }

  // Update probe data (duration/width/height) AND thumbnail for a single clip atomically.
  // Both changes go into ONE new markRaw object → ONE triggerRef(clips) → ONE render cycle.
  // This guarantees the duration badge, resolution pill, and thumbnail all appear at the same time.
  function applyProbeAndThumb(fp: string, dur: number, w: number, h: number, thumbId: string, thumbPath: string) {
    const i = clips.value.findIndex(c => c.filepath === fp)
    if (i >= 0) {
      clips.value[i] = markRaw({ ...clips.value[i], duration: dur, width: w, height: h, thumbnail: thumbPath })
      triggerRef(clips)
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

  // Phase 2d: updateClipMeta replaces the clip object with a new markRaw copy so Vue's
  // shallowRef prop-diffing detects the change and re-renders affected ClipCard components.
  function updateClipMeta(fp: string, u: Partial<Clip>) {
    const i = clips.value.findIndex(c => c.filepath === fp)
    if (i >= 0) {
      clips.value[i] = markRaw({ ...clips.value[i], ...u })
      triggerRef(clips)
    }
  }
  function removeClip(fp: string) { clips.value = clips.value.filter(c => c.filepath !== fp && c.id !== fp); selectedIds.value.delete(fp) }
  function setThumbnail(id: string, p: string) {
    const i = clips.value.findIndex(c => c.id === id)
    // Mutate clip's thumbnail in-place but do NOT triggerRef(clips).
    // ClipCard.vue already has a local `thumbUrl` ref that updates immediately.
    // Skipping triggerRef prevents ~100 full recomputes of filteredClips/sortedClips/games
    // during the thumbnail loading burst. The updated thumbnail will be visible in the
    // clip object the next time filteredClips naturally recomputes (e.g., on filter change).
    if (i >= 0) { clips.value[i] = markRaw({ ...clips.value[i], thumbnail: p }) }
    // Update liveThumbs so list view re-renders without touching the clips computeds.
    liveThumbs.value.set(id, p)
    triggerRef(liveThumbs)
  }
  /** Prepend a new clip (from file-watcher) without a full rescan. */
  function addClip(clip: Clip) { if (!clips.value.find(c => c.filepath === clip.filepath)) clips.value = [markRaw(clip), ...clips.value] }

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
    status, replayDuration, clips, loading, loaded, clipsProbed, isPrefetching,
    search, sortMode, filterGame, selectedGames, filterFav,
    games, filteredClips, sortedClips, favCount,
    selectedIds, selectMode, selectedCount,
    fetchStatus, startReplay, stopRecorder, saveReplay,
    liveThumbs,
    fetchClips, updateClipMeta, applyProbeAndThumb, removeClip, setThumbnail, addClip, injectSkeleton, replaceSkeleton,
    liveMeta,
    toggleSelect, clearSelection, isSelected,
    activeMenuClipId, activeMenuPos,
    scanActive, scanCount,
  }
})

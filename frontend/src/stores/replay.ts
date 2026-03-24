import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface Clip {
  id: string; filename: string; filepath: string; filesize: number
  created: string; duration: number; width: number; height: number
  game: string; custom_name: string; favorite: boolean; thumbnail: string
}

export type SortMode = 'newest' | 'oldest' | 'longest' | 'shortest'

export const useReplayStore = defineStore('replay', () => {
  const status = ref<'idle' | 'replay' | 'recording'>('idle')
  const replayDuration = ref(0)
  const clips = ref<Clip[]>([])
  const loading = ref(false)
  const loaded = ref(false)
  const lastFolder = ref('')

  // Search/Sort/Filter
  const search = ref('')
  const sortMode = ref<SortMode>('newest')
  const filterGame = ref('all')
  const filterFav = ref(false)

  // Multi-select
  const selectedIds = ref<Set<string>>(new Set())
  const selectMode = ref(false)

  // ── Computed ──
  const games = computed(() => {
    const s = new Set<string>()
    for (const c of clips.value) if (c.game) s.add(c.game)
    return ['all', ...Array.from(s).sort()]
  })

  const filteredClips = computed(() => {
    let r = clips.value
    if (search.value) { const q = search.value.toLowerCase(); r = r.filter(c => (c.custom_name || c.filename).toLowerCase().includes(q) || c.game.toLowerCase().includes(q)) }
    if (filterFav.value) r = r.filter(c => c.favorite)
    if (filterGame.value !== 'all') r = r.filter(c => c.game === filterGame.value)
    switch (sortMode.value) {
      case 'oldest': r = [...r].sort((a,b) => a.created.localeCompare(b.created)); break
      case 'longest': r = [...r].sort((a,b) => b.duration - a.duration); break
      case 'shortest': r = [...r].sort((a,b) => a.duration - b.duration); break
      default: r = [...r].sort((a,b) => b.created.localeCompare(a.created))
    }
    return r
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
    try {
      const raw = await invoke<Clip[]>('get_clips', { folder })
      // ★ Epic 2 P5: Auto-fill game from filename prefix if empty
      for (const c of raw) {
        if (!c.game || c.game === 'Unknown') {
          const prefix = c.filename.split(/[_\-.]/)[0]
          if (prefix && prefix.length > 1) c.game = prefix.replace(/-/g, ' ')
        }
      }
      clips.value = raw
      lastFolder.value = folder; loaded.value = true
    }
    catch (e) { console.error('fetchClips:', e); clips.value = [] }
    finally { loading.value = false }
  }

  // ★ Epic 2 P6: Reactive update — changes games dropdown instantly
  function updateClipMeta(fp: string, u: Partial<Clip>) {
    const i = clips.value.findIndex(c => c.filepath === fp)
    if (i >= 0) {
      Object.assign(clips.value[i], u)
      // Force Vue reactivity by replacing the array item
      clips.value = [...clips.value]
    }
  }
  function removeClip(fp: string) { clips.value = clips.value.filter(c => c.filepath !== fp); selectedIds.value.delete(fp) }
  function setThumbnail(id: string, p: string) { const c = clips.value.find(c => c.id===id); if (c) c.thumbnail = p }

  // Multi-select
  function toggleSelect(id: string) { if (selectedIds.value.has(id)) selectedIds.value.delete(id); else selectedIds.value.add(id); selectMode.value = selectedIds.value.size > 0 }
  function clearSelection() { selectedIds.value.clear(); selectMode.value = false }
  function isSelected(id: string) { return selectedIds.value.has(id) }

  return {
    status, replayDuration, clips, loading, loaded,
    search, sortMode, filterGame, filterFav,
    games, filteredClips, favCount,
    selectedIds, selectMode, selectedCount,
    fetchStatus, startReplay, stopRecorder, saveReplay,
    fetchClips, updateClipMeta, removeClip, setThumbnail,
    toggleSelect, clearSelection, isSelected,
  }
})

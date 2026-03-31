<script setup lang="ts">
import { ref, computed, onMounted, inject } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { mediaUrl } from '../utils/assets'
import type { Clip } from '../stores/replay'
import { useReplayStore } from '../stores/replay'
import type { Ref } from 'vue'
import CustomVideoPlayer from './CustomVideoPlayer.vue'

const props = defineProps<{ clip: Clip; mode: 'preview' | 'trim' }>()
const emit = defineEmits<{ 'close': []; 'saved': [string]; 'toast': [string] }>()

const replay = useReplayStore()
const playerComp = ref<InstanceType<typeof CustomVideoPlayer> | null>(null)

const mediaPort = inject<Ref<number>>('mediaPort', ref(0))
const videoSrc = computed(() => mediaUrl(props.clip.filepath, mediaPort.value))

const duration = ref(props.clip.duration || 0)
const currentTime = ref(0)
const playing = ref(false)
const editTitle = ref(props.clip.custom_name || props.clip.filename.replace(/\.[^.]+$/, ''))
const titleDirty = ref(false)
const trimStart = ref(0)
const trimEnd = ref(props.clip.duration || 0)
const trimDuration = computed(() => Math.max(0, trimEnd.value - trimStart.value))
const exporting = ref(false)
const exportMenuOpen = ref(false)

function onMeta(dur: number) { duration.value = dur; if (trimEnd.value <= 0) trimEnd.value = dur }
function onTimeUpdate(ct: number) { currentTime.value = ct }
function seekTo(s: number) { playerComp.value?.seekTo(s); currentTime.value = s }

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

const GAMES = ['Counter-Strike 2','League of Legends','Valorant','Overwatch 2','Apex Legends','Fortnite','Minecraft','Dota 2','Rocket League','Elden Ring',"Baldur's Gate 3",'Cyberpunk 2077','Honkai: Star Rail','Genshin Impact','Helldivers 2','Path of Exile 2']
const gameTag = ref('')
const gameOpen = ref(false)
const gameFiltered = computed(() => { const q = gameTag.value.toLowerCase(); return q ? GAMES.filter(g => g.toLowerCase().includes(q)) : GAMES })

async function saveTrimState() { try { await invoke('save_trim_state', { filepath: props.clip.filepath, trimStart: trimStart.value, trimEnd: trimEnd.value }) } catch {} }
onMounted(async () => {
  try { const s = await invoke<{ trim_start: number; trim_end: number } | null>('get_trim_state', { filepath: props.clip.filepath }); if (s && s.trim_end > 0) { trimStart.value = s.trim_start; trimEnd.value = s.trim_end } } catch {}
  try { const m = await invoke<string>('get_clip_meta', { filepath: props.clip.filepath }); if (m && m !== 'null') { const d = JSON.parse(m); if (d.game_tag) gameTag.value = d.game_tag } } catch {}
})

async function saveTitle() {
  const name = editTitle.value.trim()
  replay.updateClipMeta(props.clip.filepath, { custom_name: name, game: gameTag.value })
  try { await invoke('set_clip_meta', { update: { filepath: props.clip.filepath, custom_name: name, favorite: props.clip.favorite, game_tag: gameTag.value } }) } catch {}
  titleDirty.value = false
}

async function toggleFavorite() {
  const newFav = !props.clip.favorite
  replay.updateClipMeta(props.clip.filepath, { favorite: newFav })
  try { await invoke('set_clip_meta', { update: { filepath: props.clip.filepath, custom_name: editTitle.value.trim() || props.clip.filename, favorite: newFav, game_tag: gameTag.value } }) } catch {}
}

async function deleteClip() {
  if (!confirm(`Delete "${editTitle.value || props.clip.filename}"?`)) return
  try { await invoke('delete_clip', { filepath: props.clip.filepath }); emit('close') }
  catch (e) { emit('toast', `Delete failed: ${e}`) }
}
function selectGame(g: string) { gameTag.value = g; gameOpen.value = false; saveTitle() }
function closeGameDropDelayed() { setTimeout(() => { gameOpen.value = false }, 150) }

async function doExport(targetMb: number) {
  exportMenuOpen.value = false; exporting.value = true
  try {
    const out = targetMb <= 0
      ? await invoke<string>('trim_clip', { inputPath: props.clip.filepath, startSec: trimStart.value, endSec: trimEnd.value, outputPath: '' })
      : await invoke<string>('export_clip_sized', { inputPath: props.clip.filepath, startSec: trimStart.value, endSec: trimEnd.value, targetMb, outputPath: '' })
    emit('saved', out); emit('toast', `Export complete: ${out.split('/').pop()}`)
  } catch (e) { emit('toast', `Export failed: ${e}`) }
  finally { exporting.value = false }
}

function fmt(s: number) { return `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}.${Math.floor((s % 1) * 10)}` }
</script>

<template>
  <div class="overlay" @click.self="emit('close')">
    <div class="modal" :class="{ wide: mode === 'trim' }">
      <div class="modal-hdr">
        <input v-model="editTitle" class="title-input" @input="titleDirty = true" @blur="saveTitle" @keydown.enter="($event.target as HTMLInputElement).blur()" spellcheck="false" />
        <!-- ★ E2-P6: Game tag in Quick Preview -->
        <div class="game-wrap">
          <input v-model="gameTag" class="game-in" placeholder="Game..." @focus="gameOpen = true" @blur="closeGameDropDelayed()" />
          <div v-if="gameOpen && gameFiltered.length" class="game-drop">
            <button v-for="g in gameFiltered.slice(0, 8)" :key="g" @mousedown.prevent="selectGame(g)">{{ g }}</button>
          </div>
        </div>
        <button class="fav-btn" :class="{ on: clip.favorite }" @click="toggleFavorite" title="Toggle favorite">
          <svg viewBox="0 0 24 24" :fill="clip.favorite ? 'currentColor' : 'none'" stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
        </button>
        <button class="del-btn" @click="deleteClip" title="Delete clip">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6"/><path d="M10 11v6M14 11v6"/></svg>
        </button>
        <button class="close-btn" @click="emit('close')"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg></button>
      </div>

      <!-- Custom video player -->
      <CustomVideoPlayer
        ref="playerComp"
        :src="videoSrc"
        :capture-keyboard="true"
        @loadedmetadata="onMeta"
        @timeupdate="onTimeUpdate"
        @play="playing = true"
        @pause="playing = false"
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
        <div class="transport">
          <button class="tb" @click="togglePlay"><svg v-if="playing" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg><svg v-else viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg></button>
          <button class="tb" @click="seekTo(trimStart)">⏮</button>
          <button class="tb" @click="seekTo(trimEnd)">⏭</button>
          <span class="trim-info">{{ fmt(trimDuration) }}</span>
          <div class="exp-wrap">
            <button class="exp-btn" :disabled="exporting" @click="exportMenuOpen=!exportMenuOpen">{{ exporting?'Exporting...':'Export' }} <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:12px;height:12px"><path d="M6 9l6 6 6-6"/></svg></button>
            <div v-if="exportMenuOpen" class="exp-menu"><button @click="doExport(0)">Original</button><button @click="doExport(10)">10 MB</button><button @click="doExport(50)">50 MB</button><button @click="doExport(100)">100 MB</button></div>
          </div>
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
.game-wrap { position:relative; }
.game-in { width:120px; padding:5px 8px; background:var(--bg-input); border:1px solid var(--border); border-radius:6px; color:var(--text-sec); font-size:11px; outline:none; } .game-in:focus { border-color:var(--accent); }
.game-drop { position:absolute; top:100%; left:0; right:0; margin-top:2px; background:var(--bg-card); border:1px solid var(--border); border-radius:6px; padding:2px; z-index:30; max-height:160px; overflow-y:auto; box-shadow:0 4px 12px rgba(0,0,0,.3); }
.game-drop button { width:100%; padding:4px 8px; border:none; background:transparent; color:var(--text-sec); font-size:11px; text-align:left; cursor:pointer; border-radius:4px; } .game-drop button:hover { background:var(--bg-hover); color:var(--text); }
.title-static { font-size:15px; font-weight:700; flex:1; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.fav-btn, .del-btn { width:32px; height:32px; border:none; background:transparent; color:var(--text-sec); cursor:pointer; border-radius:6px; display:flex; align-items:center; justify-content:center; flex-shrink:0; }
.fav-btn svg, .del-btn svg { width:16px; height:16px; }
.fav-btn:hover { background:var(--bg-hover); color:var(--accent); }
.fav-btn.on { color:#E94560; }
.del-btn:hover { background:color-mix(in srgb,var(--danger) 15%,transparent); color:var(--danger); }
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

/* In Quick Preview (non-trim modal), always show the built-in controls */
.modal:not(.wide) :deep(.cvp-ctrl) {
  opacity: 1;
  pointer-events: auto;
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
</style>

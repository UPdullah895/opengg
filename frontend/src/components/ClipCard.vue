<script setup lang="ts">
import { ref, inject, onMounted, onBeforeUnmount, reactive } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { mediaUrl } from '../utils/assets'
import type { Clip } from '../stores/replay'
import { useReplayStore } from '../stores/replay'
import type { Ref } from 'vue'

const props = defineProps<{ clip: Clip; selected?: boolean }>()
const emit = defineEmits<{ 'preview': [Clip]; 'editor': [Clip]; 'rename': [Clip]; 'delete': [Clip] }>()

const replay = useReplayStore()
const cardRef = ref<HTMLElement | null>(null)
const thumbUrl = ref('')
const thumbLoading = ref(false)

// ★ Get media server port from App.vue's provide()
const mediaPort = inject<Ref<number>>('mediaPort', ref(0))

// Singleton context menu
const _menu = reactive<{ clipId: string; x: number; y: number }>({ clipId: '', x: 0, y: 0 })
function isMenuOpen() { return _menu.clipId === props.clip.id }
function openMenu(e: MouseEvent) { e.preventDefault(); e.stopPropagation(); _menu.clipId = props.clip.id; _menu.x = e.clientX; _menu.y = e.clientY }
function closeMenus() { _menu.clipId = '' }
function menuAction(a: string) {
  closeMenus()
  switch (a) {
    case 'preview': emit('preview', props.clip); break
    case 'editor': emit('editor', props.clip); break
    case 'select': replay.toggleSelect(props.clip.id); break
    case 'location': invoke('open_file_location', { filepath: props.clip.filepath }); break
    case 'rename': emit('rename', props.clip); break
    case 'delete': emit('delete', props.clip); break
  }
}

let _registered = false
onMounted(() => {
  if (!_registered) { document.addEventListener('click', closeMenus, true); _registered = true }
})

// ★ Lazy thumbnail — uses media server HTTP URL
let obs: IntersectionObserver | null = null
onMounted(() => {
  if (props.clip.thumbnail && mediaPort.value) {
    thumbUrl.value = mediaUrl(props.clip.thumbnail, mediaPort.value)
    return
  }
  obs = new IntersectionObserver(async ([entry]) => {
    if (!entry.isIntersecting || thumbLoading.value || thumbUrl.value) return
    thumbLoading.value = true
    try {
      const path = await invoke<string>('generate_thumbnail', { filepath: props.clip.filepath })
      if (mediaPort.value) {
        thumbUrl.value = mediaUrl(path, mediaPort.value)
      }
      replay.setThumbnail(props.clip.id, path)
    } catch (e) { console.warn('thumb:', e) }
    finally { thumbLoading.value = false }
  }, { threshold: 0.1 })
  if (cardRef.value) obs.observe(cardRef.value)
})
onBeforeUnmount(() => { obs?.disconnect() })

async function toggleFav(e: Event) {
  e.stopPropagation()
  const v = !props.clip.favorite
  replay.updateClipMeta(props.clip.filepath, { favorite: v })
  try { await invoke('set_clip_meta', { update: { filepath: props.clip.filepath, custom_name: props.clip.custom_name, favorite: v } }) } catch {}
}

function onDrag(e: DragEvent) {
  if (!e.dataTransfer) return
  e.dataTransfer.setData('text/uri-list', `file://${props.clip.filepath}`)
  e.dataTransfer.setData('text/plain', props.clip.filepath)
  e.dataTransfer.effectAllowed = 'copy'
}

function fmtDur(s: number) { if (!s) return '0:00'; return `${Math.floor(s/60)}:${String(Math.floor(s%60)).padStart(2,'0')}` }
function fmtSize(b: number) { if (!b) return '0 B'; const u=['B','KB','MB','GB']; let i=0,v=b; while(v>=1024&&i<3){v/=1024;i++} return `${v.toFixed(i?1:0)} ${u[i]}` }
function fmtRes(w: number, h: number) { if (!w) return ''; if (h>=2160) return '4K'; if (h>=1440) return '1440p'; if (h>=1080) return '1080p'; if (h>=720) return '720p'; return `${w}×${h}` }
</script>

<template>
  <div ref="cardRef" class="card" :class="{ selected }" @contextmenu="openMenu" draggable="true" @dragstart="onDrag">
    <div class="thumb">
      <img v-if="thumbUrl" :src="thumbUrl" class="thumb-img" loading="lazy" alt="" />
      <div v-else-if="thumbLoading" class="thumb-ph">⏳</div>
      <div v-else class="thumb-ph">🎬</div>
      <span v-if="clip.duration" class="badge">{{ fmtDur(clip.duration) }}</span>
      <button class="heart" :class="{ on: clip.favorite }" @click="toggleFav"><svg viewBox="0 0 24 24" :fill="clip.favorite?'currentColor':'none'" stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg></button>
      <div class="sel-ov" :class="{ vis: selected || replay.selectMode }" @click.stop="replay.toggleSelect(clip.id)">
        <div class="sel-box" :class="{ checked: selected }">✓</div>
      </div>
    </div>
    <div class="info">
      <div class="clip-name">{{ clip.custom_name || clip.filename }}</div>
      <div class="clip-meta">
        <span class="pill">{{ fmtSize(clip.filesize) }}</span>
        <span v-if="clip.width" class="pill">{{ fmtRes(clip.width, clip.height) }}</span>
        <span v-if="clip.game && clip.game !== 'Unknown'" class="game">{{ clip.game }}</span>
      </div>
    </div>
    <Teleport to="body">
      <div v-if="isMenuOpen()" class="ctx" :style="{ left:_menu.x+'px', top:_menu.y+'px' }" @click.stop>
        <button class="ctx-i" @click="menuAction('preview')">▶ Quick Preview</button>
        <button class="ctx-i" @click="menuAction('editor')">✂ Open in Editor</button>
        <button class="ctx-i" @click="menuAction('select')">☑ {{ selected ? 'Deselect' : 'Select' }}</button>
        <button class="ctx-i" @click="menuAction('location')">📂 Open File Location</button>
        <div class="ctx-sep"></div>
        <button class="ctx-i" @click="menuAction('rename')">✏ Rename</button>
        <button class="ctx-i ctx-d" @click="menuAction('delete')">🗑 Delete</button>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.card { background:var(--bg-card); border:1px solid var(--border); border-radius:10px; overflow:hidden; cursor:pointer; transition:all .15s; }
.card:hover { border-color:var(--accent); transform:translateY(-2px); box-shadow:0 6px 20px rgba(0,0,0,.25); }
.card.selected { border-color:var(--accent); box-shadow:0 0 0 2px var(--accent); }
.thumb { width:100%; aspect-ratio:16/9; background:var(--bg-deep); position:relative; display:flex; align-items:center; justify-content:center; overflow:hidden; }
.thumb-img { width:100%; height:100%; object-fit:cover; display:block; }
.thumb-ph { font-size:28px; opacity:.3; }
.badge { position:absolute; bottom:6px; right:6px; background:rgba(0,0,0,.8); color:#fff; font-size:11px; font-weight:600; padding:2px 7px; border-radius:4px; pointer-events:none; }
.heart { position:absolute; top:6px; right:6px; width:28px; height:28px; border-radius:50%; border:none; background:rgba(0,0,0,.5); color:var(--text-muted); cursor:pointer; display:flex; align-items:center; justify-content:center; opacity:0; transition:all .15s; }
.card:hover .heart { opacity:1; } .heart.on { opacity:1; color:#E94560; } .heart:hover { background:rgba(0,0,0,.8); transform:scale(1.15); } .heart svg { width:14px; height:14px; }
.sel-ov { position:absolute; top:6px; left:6px; opacity:0; transition:opacity .15s; } .sel-ov.vis,.card:hover .sel-ov { opacity:1; }
.sel-box { width:22px; height:22px; border-radius:5px; border:2px solid rgba(255,255,255,.5); background:rgba(0,0,0,.4); display:flex; align-items:center; justify-content:center; font-size:12px; color:transparent; cursor:pointer; }
.sel-box.checked { background:var(--accent); border-color:var(--accent); color:#fff; }
.info { padding:10px 12px 12px; }
.clip-name { font-size:13px; font-weight:600; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; margin-bottom:6px; line-height:1.3; }
.clip-meta { display:flex; align-items:center; gap:6px; font-size:11px; color:var(--text-muted); flex-wrap:wrap; }
.pill { background:var(--bg-deep); padding:2px 6px; border-radius:3px; }
.game { margin-left:auto; color:var(--text-sec); font-weight:500; }
.ctx { position:fixed; z-index:9999; background:var(--bg-card); border:1px solid var(--border); border-radius:8px; padding:4px; width:192px; box-shadow:0 8px 24px rgba(0,0,0,.5); }
.ctx-i { width:100%; padding:7px 12px; border:none; background:transparent; color:var(--text-sec); font-size:12px; text-align:left; cursor:pointer; border-radius:5px; }
.ctx-i:hover { background:var(--bg-hover); color:var(--text); }
.ctx-d:hover { background:rgba(220,38,38,.12); color:var(--danger); }
.ctx-sep { height:1px; background:var(--border); margin:3px 6px; }
</style>

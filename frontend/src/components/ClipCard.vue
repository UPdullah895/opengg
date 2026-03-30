<script setup lang="ts">
import { ref, inject, onMounted, onBeforeUnmount, nextTick } from 'vue'
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

// Singleton context menu — backed by global store so only one menu is open at a time
function isMenuOpen() { return replay.activeMenuClipId === props.clip.id }
function openMenu(e: MouseEvent) {
  e.preventDefault(); e.stopPropagation()
  const menuW = 200, menuH = 270
  const x = e.clientX + menuW > window.innerWidth  ? e.clientX - menuW : e.clientX
  const y = e.clientY + menuH > window.innerHeight ? e.clientY - menuH : e.clientY
  replay.activeMenuClipId = props.clip.id
  replay.activeMenuPos = { x, y }
}
function closeMenus() { replay.activeMenuClipId = '' }
function menuAction(a: string) {
  closeMenus()
  switch (a) {
    case 'preview': emit('preview', props.clip); break
    case 'editor': emit('editor', props.clip); break
    case 'select': replay.toggleSelect(props.clip.id); break
    case 'favorite': toggleFav(new MouseEvent('click')); break
    case 'location': invoke('open_file_location', { filepath: props.clip.filepath }); break
    case 'rename': emit('rename', props.clip); break
    case 'delete': emit('delete', props.clip); break
  }
}

onMounted(() => { document.addEventListener('click', closeMenus, true) })
onBeforeUnmount(() => { document.removeEventListener('click', closeMenus, true) })

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
function fmtDate(created: string) {
  if (!created) return ''
  const d = new Date(created.replace(' ', 'T'))
  if (isNaN(d.getTime())) return ''
  return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })
}

// Inline rename
const isEditing = ref(false)
const editValue = ref('')
const editInput = ref<HTMLInputElement | null>(null)

function startEdit(e: MouseEvent) {
  e.stopPropagation()
  isEditing.value = true
  editValue.value = props.clip.custom_name || props.clip.filename.replace(/\.[^.]+$/, '')
  nextTick(() => editInput.value?.select())
}
async function confirmEdit() {
  if (!isEditing.value) return
  isEditing.value = false
  const n = editValue.value.trim()
  const orig = props.clip.custom_name || props.clip.filename.replace(/\.[^.]+$/, '')
  if (!n || n === orig) return
  replay.updateClipMeta(props.clip.filepath, { custom_name: n })
  try { await invoke('set_clip_meta', { update: { filepath: props.clip.filepath, custom_name: n, favorite: props.clip.favorite } }) } catch {}
}
function cancelEdit() { isEditing.value = false }
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
      <div class="clip-name-row">
        <div class="clip-name" @click.stop="startEdit">
          <input
            v-if="isEditing"
            ref="editInput"
            v-model="editValue"
            class="inline-edit"
            @blur="confirmEdit"
            @keydown.enter.prevent="confirmEdit"
            @keydown.escape.prevent="cancelEdit"
            @click.stop
          />
          <span v-else>{{ clip.custom_name || clip.filename }}</span>
        </div>
        <button class="kebab" @click.stop="openMenu" title="More options">
          <svg viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="5" r="1.5"/><circle cx="12" cy="12" r="1.5"/><circle cx="12" cy="19" r="1.5"/></svg>
        </button>
      </div>
      <div class="clip-meta">
        <span class="pill">{{ fmtSize(clip.filesize) }}</span>
        <span v-if="clip.width" class="pill">{{ fmtRes(clip.width, clip.height) }}</span>
        <span v-if="clip.created" class="pill date-pill">{{ fmtDate(clip.created) }}</span>
        <span v-if="clip.game && clip.game !== 'Unknown'" class="game">{{ clip.game }}</span>
      </div>
    </div>
    <Teleport to="body">
      <div v-if="isMenuOpen()" class="ctx" :style="{ left:replay.activeMenuPos.x+'px', top:replay.activeMenuPos.y+'px' }" @click.stop>
        <button class="ctx-i" @click="menuAction('preview')"><svg class="ctx-ic" viewBox="0 0 24 24" fill="currentColor"><polygon points="5,3 19,12 5,21"/></svg>Quick Preview</button>
        <button class="ctx-i" @click="menuAction('editor')"><svg class="ctx-ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>Open in Editor</button>
        <button class="ctx-i" @click="menuAction('select')"><svg class="ctx-ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"/></svg>{{ selected ? 'Deselect' : 'Select' }}</button>
        <button class="ctx-i" @click="menuAction('location')"><svg class="ctx-ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>Open File Location</button>
        <div class="ctx-sep"></div>
        <button class="ctx-i" @click="menuAction('favorite')"><svg class="ctx-ic" viewBox="0 0 24 24" :fill="clip.favorite?'currentColor':'none'" stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>{{ clip.favorite ? 'Remove from Favorites' : 'Add to Favorites' }}</button>
        <button class="ctx-i" @click="menuAction('rename')"><svg class="ctx-ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>Rename</button>
        <button class="ctx-i ctx-d" @click="menuAction('delete')"><svg class="ctx-ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6"/><path d="M10 11v6M14 11v6"/></svg>Delete</button>
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
.clip-name-row { display:flex; align-items:center; gap:4px; margin-bottom:6px; }
.clip-name { font-size:13px; font-weight:600; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; flex:1; line-height:1.3; }
.clip-meta { display:flex; align-items:center; gap:6px; font-size:11px; color:var(--text-muted); flex-wrap:wrap; }
.pill { background:var(--bg-deep); padding:2px 6px; border-radius:3px; }
.date-pill { opacity:.75; }
.game { margin-left:auto; color:var(--text-sec); font-weight:500; }
.kebab {
  flex-shrink:0; width:22px; height:22px; border-radius:4px; border:none;
  background:transparent; color:var(--text-muted); cursor:pointer;
  display:flex; align-items:center; justify-content:center;
  transition:all .15s;
}
.kebab:hover { background:var(--bg-hover); color:var(--text); }
.kebab svg { width:16px; height:16px; }

.inline-edit {
  width:100%; padding:2px 4px; margin:-2px -4px;
  background:var(--bg-input); border:1px solid var(--accent); border-radius:4px;
  color:var(--text); font-size:13px; font-weight:600; outline:none; line-height:1.3;
}


.ctx { position:fixed; z-index:9999; background:var(--bg-card); border:1px solid var(--border); border-radius:8px; padding:4px; width:200px; box-shadow:0 8px 24px rgba(0,0,0,.5); }
.ctx-i {
  width:100%; padding:7px 10px; border:none; background:transparent;
  color:var(--text-sec); font-size:12px; text-align:left; cursor:pointer;
  border-radius:5px; display:flex; align-items:center; gap:8px;
}
.ctx-i:hover { background:var(--bg-hover); color:var(--text); }
.ctx-d:hover { background:rgba(220,38,38,.12); color:var(--danger); }
.ctx-sep { height:1px; background:var(--border); margin:3px 6px; }
.ctx-ic { width:14px; height:14px; flex-shrink:0; opacity:.7; color:currentColor; }
.ctx-i:hover .ctx-ic { opacity:1; }
</style>

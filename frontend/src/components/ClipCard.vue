<script setup lang="ts">
import { ref, inject, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { mediaUrl } from '../utils/assets'
import type { Clip } from '../stores/replay'
import { useReplayStore } from '../stores/replay'
import { useSharedIntersectionObserver } from '../composables/useSharedIntersectionObserver'
import { useThumbnailQueue } from '../composables/useThumbnailQueue'
import { fmtDur, fmtSize, fmtRes, fmtDate } from '../utils/format'
import type { Ref } from 'vue'

const props = defineProps<{ clip: Clip; selected?: boolean }>()
const emit = defineEmits<{ 'preview': [Clip]; 'editor': [Clip]; 'rename': [Clip]; 'delete': [Clip]; 'contextmenu': [{ clip: Clip; x: number; y: number }] }>()

const replay = useReplayStore()
const cardRef = ref<HTMLElement | null>(null)
const thumbUrl = ref('')
const thumbLoading = ref(false)
const thumbLoaded = ref(false)

// ★ Get media server port from App.vue's provide()
const mediaPort = inject<Ref<number>>('mediaPort', ref(0))

// Context menu — emit event to parent (ClipsPage) instead of managing own menu
function openMenu(e: MouseEvent) {
  e.preventDefault(); e.stopPropagation()
  const menuW = 200, menuH = 270
  const x = e.clientX + menuW > window.innerWidth  ? e.clientX - menuW : e.clientX
  const y = e.clientY + menuH > window.innerHeight ? e.clientY - menuH : e.clientY
  replay.activeMenuClipId = props.clip.id
  replay.activeMenuPos = { x, y }
  emit('contextmenu', { clip: props.clip, x, y })
}

// ★ Lazy thumbnail — uses shared IntersectionObserver + concurrency-limited queue
const { observe, unobserve } = useSharedIntersectionObserver()
const { enqueue } = useThumbnailQueue()

onMounted(() => {
  if (props.clip.thumbnail && mediaPort.value) {
    thumbUrl.value = mediaUrl(props.clip.thumbnail, mediaPort.value)
    return  // already have thumb — never register observer
  }
  if (cardRef.value) {
    observe(cardRef.value, async (entry) => {
      if (!entry.isIntersecting || thumbLoading.value || thumbUrl.value) return
      thumbLoading.value = true
      // observeOnce: stop firing callbacks now that we're loading
      if (cardRef.value) unobserve(cardRef.value)
      try {
        const path = await enqueue(() => invoke<string>('generate_thumbnail', { filepath: props.clip.filepath }))
        if (mediaPort.value) {
          thumbUrl.value = mediaUrl(path, mediaPort.value)
        }
        replay.setThumbnail(props.clip.id, path)
      } catch (e) { console.warn('thumb:', e) }
      finally { thumbLoading.value = false }
    })
  }
})
onBeforeUnmount(() => { if (cardRef.value) unobserve(cardRef.value) })


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
  <div ref="cardRef" class="card" :class="{ selected }" @contextmenu.prevent="openMenu" draggable="true" @dragstart="onDrag">
    <div class="thumb">
      <img v-if="thumbUrl" :src="thumbUrl" class="thumb-img" :class="{ loaded: thumbLoaded }" loading="lazy" alt="" @load="thumbLoaded = true" />
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
  </div>
</template>

<style scoped>
.card { background:var(--bg-card); border:1px solid var(--border); border-radius:10px; overflow:hidden; cursor:pointer; transition:border-color .15s, transform .15s, box-shadow .15s; contain:layout style paint; content-visibility:auto; contain-intrinsic-size:auto 260px; }
.card:hover { border-color:var(--accent); transform:translateY(-2px); box-shadow:0 6px 20px rgba(0,0,0,.25); }
.card.selected { border-color:var(--accent); box-shadow:0 0 0 2px var(--accent); }
.thumb { width:100%; aspect-ratio:16/9; background:var(--bg-deep); position:relative; display:flex; align-items:center; justify-content:center; overflow:hidden; }
.thumb-img { width:100%; height:100%; object-fit:cover; display:block; opacity:0; transition:opacity 0.25s; }
.thumb-img.loaded { opacity:1; }
.thumb-ph { font-size:28px; opacity:.3; }
.badge { position:absolute; bottom:6px; right:6px; background:rgba(0,0,0,.8); color:#fff; font-size:11px; font-weight:600; padding:2px 7px; border-radius:4px; pointer-events:none; }
.heart { position:absolute; top:6px; right:6px; width:28px; height:28px; border-radius:50%; border:none; background:rgba(0,0,0,.5); color:var(--text-muted); cursor:pointer; display:flex; align-items:center; justify-content:center; opacity:0; transition:all .15s; }
.card:hover .heart { opacity:1; } .heart.on { opacity:1; color:#E94560; } .heart:hover { background:rgba(0,0,0,.8); transform:scale(1.15); } .heart svg { width:14px; height:14px; }
.sel-ov { position:absolute; top:6px; left:6px; opacity:0; transition:opacity .15s; } .sel-ov.vis,.card:hover .sel-ov { opacity:1; }
.sel-box { width:22px; height:22px; border-radius:5px; border:2px solid rgba(255,255,255,.5); background:rgba(0,0,0,.4); display:flex; align-items:center; justify-content:center; font-size:12px; color:transparent; cursor:pointer; }
.sel-box.checked { background:var(--accent); border-color:var(--accent); color:#fff; }
.info { padding:10px 12px 12px; }
.clip-name-row { display:flex; align-items:center; gap:4px; margin-bottom:6px; }
.clip-name { font-size:var(--name-size, 13px); font-weight:600; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; flex:1; line-height:1.3; }
.clip-meta { display:flex; align-items:center; gap:6px; font-size:var(--meta-size, 11px); color:var(--text-muted); flex-wrap:wrap; }
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

</style>

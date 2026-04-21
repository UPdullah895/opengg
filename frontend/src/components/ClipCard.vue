<script setup lang="ts">
import { ref, inject, watch, onMounted, onBeforeUnmount, nextTick } from 'vue'
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
const thumbLoaded = ref(false)

// Local duration/resolution — updated via liveMeta watch (same pattern as liveThumbs/thumbUrl).
// Bypasses the filteredClips prop chain so they appear at the same time as the thumbnail.
const liveDuration = ref(props.clip.duration)
const liveWidth = ref(props.clip.width)
const liveHeight = ref(props.clip.height)

// ★ Get media server port from App.vue's provide()
const mediaPort = inject<Ref<number>>('mediaPort', ref(0))

// ── RAM optimization: resolved thumbnail path (non-reactive) ──
// Stores the resolved filesystem path so we can clear/restore thumbUrl without
// re-fetching. When card scrolls off-screen, thumbUrl is set to '' which removes
// the <img> from the DOM and releases the decoded bitmap (~410KB–1.6MB per image).
// When card scrolls back into view, we restore from this cached path instantly.
let resolvedThumbPath = ''

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

// React to prefetchThumbnails() generating this card's thumbnail.
// liveThumbs is updated sequentially newest→oldest, so thumbnails appear in order
// without needing the IO observer to fire first.
watch(() => replay.liveThumbs.get(props.clip.id), (path) => {
  if (path && mediaPort.value) {
    resolvedThumbPath = path
    // Only set thumbUrl if card is visible (IO will restore it otherwise)
    if (!thumbUrl.value) thumbUrl.value = mediaUrl(path, mediaPort.value)
  }
})

// React to per-clip probe results — updates duration badge and resolution pill
// at the same time as the thumbnail, bypassing the filteredClips prop chain.
watch(() => replay.liveMeta.get(props.clip.id), (meta) => {
  if (meta) {
    liveDuration.value = meta.duration
    liveWidth.value = meta.width
    liveHeight.value = meta.height
  }
})

/** Resolve the initial thumbnail path from clip data or liveThumbs. */
function resolveInitialThumb(): string {
  if (props.clip.thumbnail) return props.clip.thumbnail
  return replay.liveThumbs.get(props.clip.id) || ''
}

onMounted(() => {
  const initial = resolveInitialThumb()
  if (initial) resolvedThumbPath = initial

  // Track whether this card has already kicked off its own thumbnail load so
  // repeated observer callbacks (e.g., scroll jitter) don't spawn duplicates.
  let loadStarted = false

  if (cardRef.value) {
    const mountTime = import.meta.env.DEV ? performance.now() : 0
    observe(cardRef.value, async (entry) => {
      // ── RAM: unload decoded bitmap when card scrolls far off-screen ──
      if (!entry.isIntersecting) {
        if (thumbUrl.value) {
          thumbUrl.value = ''
          thumbLoaded.value = false
        }
        return
      }

      // Card is intersecting — restore thumbnail if we already have a path
      if (resolvedThumbPath && !thumbUrl.value && mediaPort.value) {
        thumbUrl.value = mediaUrl(resolvedThumbPath, mediaPort.value)
        return
      }

      // ── First-time thumbnail generation (IO fallback path) ──
      if (loadStarted || thumbUrl.value) return
      if (!replay.clipsProbed && props.clip.duration === 0) return
      if (replay.isPrefetching) return
      loadStarted = true
      const priority = entry.intersectionRatio > 0.3 ? 'high' : 'normal'
      try {
        let duration = props.clip.duration
        let width = props.clip.width
        let height = props.clip.height
        if (duration === 0) {
          const probed = await invoke<[string, number, number, number][]>('probe_clips', { filepaths: [props.clip.filepath] })
          if (probed.length > 0) [, duration, width, height] = probed[0]
        }
        const path = await enqueue(
          () => invoke<string>('generate_thumbnail', { filepath: props.clip.filepath, duration: duration > 0 ? duration : undefined }),
          priority
        )
        resolvedThumbPath = path
        if (mediaPort.value) {
          thumbUrl.value = mediaUrl(path, mediaPort.value)
        }
        if (import.meta.env.DEV) {
          console.debug(`[perf] card thumb loaded: ${(performance.now() - mountTime).toFixed(0)}ms (${priority}) ${props.clip.filename}`)
        }
        replay.applyProbeAndThumb(props.clip.filepath, duration, width, height, props.clip.id, path)
      } catch (e) { console.warn('thumb:', e); loadStarted = false }
    })
  }
})
// ── RAM: clear/restore thumb when ClipsPage is deactivated/activated by KeepAlive ──
watch(() => replay.pageActive, (active) => {
  if (!active) {
    thumbUrl.value = ''
    thumbLoaded.value = false
  } else if (resolvedThumbPath && mediaPort.value) {
    thumbUrl.value = mediaUrl(resolvedThumbPath, mediaPort.value)
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
  editValue.value = props.clip.custom_name || (props.clip.game !== 'Unknown' ? props.clip.game : props.clip.filename.replace(/\.[^.]+$/, ''))
  nextTick(() => editInput.value?.select())
}
async function confirmEdit() {
  if (!isEditing.value) return
  isEditing.value = false
  const n = editValue.value.trim()
  const orig = props.clip.custom_name || (props.clip.game !== 'Unknown' ? props.clip.game : props.clip.filename.replace(/\.[^.]+$/, ''))
  if (!n || n === orig) return
  replay.updateClipMeta(props.clip.filepath, { custom_name: n })
  try { await invoke('set_clip_meta', { update: { filepath: props.clip.filepath, custom_name: n, favorite: props.clip.favorite } }) } catch {}
}
function cancelEdit() { isEditing.value = false }
</script>

<template>
  <div ref="cardRef" class="card" :class="{ selected }" @contextmenu.prevent="openMenu" draggable="true" @dragstart="onDrag">
    <div class="thumb">
      <img v-if="thumbUrl" :src="thumbUrl" class="thumb-img" :class="{ loaded: thumbLoaded }" alt="" decoding="async" loading="lazy" @load="thumbLoaded = true" />
      <div v-else class="thumb-ph">🎬</div>
      <span v-if="liveDuration" class="badge">{{ fmtDur(liveDuration) }}</span>
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
          <span v-else>{{ clip.custom_name || (clip.game !== 'Unknown' ? clip.game : clip.filename) }}</span>
        </div>
        <button class="kebab" @click.stop="openMenu" title="More options">
          <svg viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="5" r="1.5"/><circle cx="12" cy="12" r="1.5"/><circle cx="12" cy="19" r="1.5"/></svg>
        </button>
      </div>
      <div class="clip-meta">
        <span class="pill">{{ fmtSize(clip.filesize) }}</span>
        <span v-if="liveWidth" class="pill">{{ fmtRes(liveWidth, liveHeight) }}</span>
        <span v-if="clip.created" class="pill date-pill">{{ fmtDate(clip.created) }}</span>
        <span v-if="clip.game && clip.game !== 'Unknown'" class="game">{{ clip.game }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.card { background:var(--bg-card); border:1px solid var(--border); border-radius:10px; overflow:hidden; cursor:pointer; transition:border-color .15s, transform .15s, box-shadow .15s; contain:layout style paint; content-visibility:auto; contain-intrinsic-size:auto 260px; contain-intrinsic-block-size:auto 280px; user-select:none; }
.card:hover { border-color:var(--accent); transform:translateY(-2px); box-shadow:0 6px 20px rgba(0,0,0,.25); }
.card.selected { border-color:var(--accent); box-shadow:0 0 0 2px var(--accent); }
.thumb { width:100%; aspect-ratio:16/9; background:var(--bg-deep); position:relative; display:flex; align-items:center; justify-content:center; overflow:hidden; }
.thumb-img { width:100%; height:100%; object-fit:cover; display:block; opacity:0; transition:opacity 0.25s; user-select:none; -webkit-user-drag:none; pointer-events:none; }
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
.game {
  margin-left:auto; font-weight:700; font-size:10px;
  color:var(--accent);
  background:color-mix(in srgb, var(--accent) 14%, transparent);
  padding:2px 8px; border-radius:4px;
  white-space:nowrap; overflow:hidden; text-overflow:ellipsis; max-width:120px;
}
.kebab {
  flex-shrink:0; width:22px; height:22px; border-radius:4px; border:none;
  background:transparent; color:var(--text-muted); cursor:pointer;
  display:flex; align-items:center; justify-content:center;
  transition:all .15s;
}
.kebab:hover { background:color-mix(in srgb, var(--accent) 10%, transparent); color:var(--accent); }
.kebab svg { width:16px; height:16px; }

.inline-edit {
  width:100%; padding:2px 4px; margin:-2px -4px;
  background:var(--bg-input); border:1px solid var(--accent); border-radius:4px;
  color:var(--text); font-size:13px; font-weight:600; outline:none; line-height:1.3;
}

</style>

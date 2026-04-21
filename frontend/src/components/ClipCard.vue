<script setup lang="ts">
import { ref, inject, watch, onMounted, onBeforeUnmount, nextTick, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { mediaUrl } from '../utils/assets'
import type { Clip } from '../stores/replay'
import { useReplayStore } from '../stores/replay'
import { useSharedIntersectionObserver } from '../composables/useSharedIntersectionObserver'
import { useThumbnailQueue } from '../composables/useThumbnailQueue'
import { fmtDur, fmtSize, fmtRes, fmtDate, fmtTime, clipDisplayTitle } from '../utils/format'
import type { Ref } from 'vue'

const props = defineProps<{ clip: Clip; selected?: boolean }>()
const emit = defineEmits<{ 'preview': [Clip]; 'editor': [Clip]; 'rename': [Clip]; 'delete': [Clip]; 'contextmenu': [Clip, MouseEvent] }>()

const replay = useReplayStore()
const cardRef = ref<HTMLElement | null>(null)
const thumbUrl = ref('')
const thumbLoaded = ref(false)
const trimmedDuration = ref<number | null>(null)

// Local duration/resolution — updated via liveMeta watch (same pattern as liveThumbs/thumbUrl).
// Bypasses the filteredClips prop chain so they appear at the same time as the thumbnail.
const liveDuration = ref(props.clip.duration)
const liveWidth = ref(props.clip.width)
const liveHeight = ref(props.clip.height)
const displayDuration = computed(() => trimmedDuration.value ?? liveDuration.value)
const isTrimmed = computed(() => trimmedDuration.value != null)

// ★ Get media server port from App.vue's provide()
const mediaPort = inject<Ref<number>>('mediaPort', ref(0))

// ── RAM optimization: resolved thumbnail path (non-reactive) ──
// Stores the resolved filesystem path so we can clear/restore thumbUrl without
// re-fetching. When card scrolls off-screen, thumbUrl is set to '' which removes
// the <img> from the DOM and releases the decoded bitmap (~410KB–1.6MB per image).
// When card scrolls back into view, we restore from this cached path instantly.
let resolvedThumbPath = ''
let removeTrimListener: (() => void) | null = null

// Context menu — emit event to parent (ClipsPage) instead of managing own menu
function openMenu(e: MouseEvent) {
  emit('contextmenu', props.clip, e)
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
  void loadTrimState()
  const trimListener = (event: Event) => {
    const detail = (event as CustomEvent<{ filepath?: string; trimStart?: number; trimEnd?: number }>).detail
    if (!detail || detail.filepath !== props.clip.filepath) return
    if (typeof detail.trimStart === 'number' && typeof detail.trimEnd === 'number' && detail.trimEnd > detail.trimStart) {
      const nextDuration = Math.max(0, detail.trimEnd - detail.trimStart)
      trimmedDuration.value = nextDuration > 0 && Math.abs(nextDuration - liveDuration.value) > 0.05 ? nextDuration : null
      return
    }
    void loadTrimState()
  }
  window.addEventListener('clip-trim-updated', trimListener as EventListener)
  removeTrimListener = () => window.removeEventListener('clip-trim-updated', trimListener as EventListener)

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

onBeforeUnmount(() => {
  if (cardRef.value) unobserve(cardRef.value)
  removeTrimListener?.()
  removeTrimListener = null
})

async function loadTrimState() {
  try {
    const state = await invoke<{ trim_start: number; trim_end: number } | null>('get_trim_state', { filepath: props.clip.filepath })
    if (state && state.trim_end > state.trim_start) {
      const nextDuration = Math.max(0, state.trim_end - state.trim_start)
      trimmedDuration.value = nextDuration > 0 && Math.abs(nextDuration - liveDuration.value) > 0.05 ? nextDuration : null
      return
    }
  } catch {}
  trimmedDuration.value = null
}


async function toggleFav(e: Event) {
  e.stopPropagation()
  const v = !props.clip.favorite
  replay.updateClipMeta(props.clip.filepath, { favorite: v })
  try { await invoke('set_clip_meta', { update: { filepath: props.clip.filepath, custom_name: props.clip.custom_name, favorite: v } }) } catch {}
}


// Inline rename
const isEditing = ref(false)
const editValue = ref('')
const editInput = ref<HTMLInputElement | null>(null)

function startEdit(e: MouseEvent) {
  e.stopPropagation()
  isEditing.value = true
  editValue.value = clipDisplayTitle(props.clip.custom_name || '', props.clip.game || '', props.clip.filename)
  nextTick(() => editInput.value?.select())
}
async function confirmEdit() {
  if (!isEditing.value) return
  isEditing.value = false
  const n = editValue.value.trim()
  const orig = clipDisplayTitle(props.clip.custom_name || '', props.clip.game || '', props.clip.filename)
  if (!n || n === orig) return
  replay.updateClipMeta(props.clip.filepath, { custom_name: n })
  try { await invoke('set_clip_meta', { update: { filepath: props.clip.filepath, custom_name: n, favorite: props.clip.favorite } }) } catch {}
}
function cancelEdit() { isEditing.value = false }
</script>

<template>
  <div ref="cardRef" class="card" :class="{ selected }" @contextmenu.prevent="openMenu">
    <div class="thumb">
      <img v-if="thumbUrl" :src="thumbUrl" class="thumb-img" :class="{ loaded: thumbLoaded }" alt="" decoding="async" loading="lazy" @load="thumbLoaded = true" />
      <div v-else class="thumb-ph">🎬</div>
      <span v-if="displayDuration" class="badge" :class="{ trimmed: isTrimmed }">
        <svg v-if="isTrimmed" viewBox="0 0 24 24" aria-hidden="true">
          <path fill="currentColor" d="M9.64 7.64a2.5 2.5 0 1 1-3.54-3.54 2.5 2.5 0 0 1 3.54 3.54Zm0 8.72a2.5 2.5 0 1 1-3.54 3.54 2.5 2.5 0 0 1 3.54-3.54ZM14.59 12l6.2 6.2-1.41 1.41L12 12.41l-7.38 7.2-1.4-1.42L9.41 12 3.22 5.8l1.4-1.41L12 11.59l7.38-7.2 1.41 1.42z"/>
        </svg>
        {{ fmtDur(displayDuration) }}
      </span>
      <span v-if="clip.created" class="time-badge">{{ fmtTime(clip.created) }}</span>
      <button class="heart" :class="{ on: clip.favorite }" @click="toggleFav"><svg viewBox="0 0 24 24" :fill="clip.favorite?'currentColor':'none'" stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg></button>
      <div class="sel-ov" :class="{ vis: selected || replay.selectMode }" @click.stop="replay.toggleSelect(clip.id)">
        <div class="sel-box" :class="{ checked: selected }">✓</div>
      </div>
      <Transition name="fade">
        <div v-if="clip.probing" class="probing-ov">
          <div class="probing-spinner"></div>
          <span>Fetching video data…</span>
        </div>
      </Transition>
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
          <span v-else>{{ clipDisplayTitle(clip.custom_name || '', clip.game || '', clip.filename) }}</span>
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
.card { background:var(--bg-card); border:1px solid var(--border); border-radius:10px; overflow:hidden; cursor:pointer; transition:border-color .15s, transform .15s, box-shadow .15s; contain:layout style paint; height: 100%; user-select:none; }
.card:hover { border-color:var(--accent); transform:translateY(-2px); box-shadow:0 6px 20px rgba(0,0,0,.25); }
.card.selected { border-color:var(--accent); box-shadow:0 0 0 2px var(--accent); }
.thumb { width:100%; aspect-ratio:16/9; background:var(--bg-deep); position:relative; display:flex; align-items:center; justify-content:center; overflow:hidden; }
.thumb-img { width:100%; height:100%; object-fit:cover; display:block; opacity:0; transition:opacity 0.25s; user-select:none; -webkit-user-drag:none; pointer-events:none; }
.thumb-img.loaded { opacity:1; }
.thumb-ph { font-size:28px; opacity:.3; }
.badge { position:absolute; bottom:6px; right:6px; background:rgba(0,0,0,.8); color:#fff; font-size:11px; font-weight:600; padding:2px 7px; border-radius:4px; pointer-events:none; display:flex; align-items:center; gap:4px; }
.badge svg { width:12px; height:12px; }
.badge.trimmed { color:#ffd27a; }
.time-badge { position:absolute; bottom:6px; left:6px; background:rgba(0,0,0,.8); color:#fff; font-size:11px; font-weight:600; padding:2px 7px; border-radius:4px; pointer-events:none; }
.heart { position:absolute; top:6px; right:6px; width:28px; height:28px; border-radius:50%; border:none; background:rgba(0,0,0,.5); color:var(--text-muted); cursor:pointer; display:flex; align-items:center; justify-content:center; opacity:0; transition:all .15s; }
.card:hover .heart { opacity:1; } .heart.on { opacity:1; color:#E94560; } .heart:hover { background:rgba(0,0,0,.8); transform:scale(1.15); } .heart svg { width:14px; height:14px; }
.sel-ov { position:absolute; top:6px; left:6px; opacity:0; transition:opacity .15s; } .sel-ov.vis,.card:hover .sel-ov { opacity:1; }
.sel-box { width:22px; height:22px; border-radius:5px; border:2px solid rgba(255,255,255,.5); background:rgba(0,0,0,.4); display:flex; align-items:center; justify-content:center; font-size:12px; color:transparent; cursor:pointer; }
.sel-box.checked { background:var(--accent); border-color:var(--accent); color:#fff; }
.info { padding:10px 12px 12px; }
.clip-name-row { display:flex; align-items:center; gap:4px; margin-bottom:6px; }
.clip-name { font-size:var(--name-size, 13px); font-weight:600; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; flex:1; line-height:1.3; }
.clip-meta { display:flex; align-items:center; gap:6px; font-size:var(--meta-size, 11px); color:var(--text-muted); white-space:nowrap; overflow:hidden; }
.clip-meta > * { min-width:0; }
.pill { background:var(--bg-deep); padding:2px 6px; border-radius:3px; flex-shrink:0; max-width: 80px; overflow: hidden; text-overflow: ellipsis; }
.date-pill { opacity:.75; max-width: 180px; }
.game {
  margin-left:auto; min-width:0; max-width:100%; flex-shrink:1; font-weight:700; font-size:calc(var(--meta-size, 11px) - 1px);
  color:var(--accent);
  background:color-mix(in srgb, var(--accent) 14%, transparent);
  padding:2px 8px; border-radius:4px;
  white-space:nowrap; overflow:hidden; text-overflow:ellipsis;
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

  .probing-ov {
  position:absolute; inset:0;
  background:rgba(0,0,0,.7);
  display:flex; flex-direction:column; align-items:center; justify-content:center; gap:8px;
  color:var(--text); font-size:11px; font-weight:600;
  z-index:5;
  }
  .probing-spinner {
  width:20px; height:20px;
  border:2px solid rgba(255,255,255,.2); border-top-color:#fff;
  border-radius:50%; animation:spin .8s linear infinite;
  }
  @keyframes spin { to { transform:rotate(360deg); } }

  .fade-enter-active, .fade-leave-active { transition:opacity .2s; }
  .fade-enter-from, .fade-leave-to { opacity:0; }
  </style>

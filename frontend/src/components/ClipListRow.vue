<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { Clip } from '../stores/replay'
import { useReplayStore } from '../stores/replay'
import { useSharedIntersectionObserver } from '../composables/useSharedIntersectionObserver'
import { fmtDur, fmtSize, fmtRes, fmtDate, fmtTime, clipDisplayTitle } from '../utils/format'

const props = defineProps<{
  clip: Clip
  selected: boolean
  fontSize: string
  metaFontSize: string
  pillPadY: string
  pillPadX: string
  chipFontSize: string
  chipPadY: string
  chipPadX: string
  actionFontSize: string
  actionPadY: string
  actionPadX: string
  padding: string
  thumbW: string
  thumbH: string
  mediaUrl: (path: string, port: number) => string
  mediaPort: number
}>()

const emit = defineEmits<{
  'click': [Clip]
  'contextmenu': [Clip, MouseEvent]
  'preview': [Clip]
  'editor': [Clip]
  'delete': [Clip]
  'favorite': [Clip, Event]
}>()

const replay = useReplayStore()
const rowRef = ref<HTMLElement | null>(null)
const thumbUrl = ref('')
const thumbLoaded = ref(false)
const isVisible = ref(false)
const trimmedDuration = ref<number | null>(null)

// Local cache for the resolved path to avoid re-calculating or re-fetching
let resolvedThumbPath = ''
let removeTrimListener: (() => void) | null = null

// Surgical reactivity: watch specific keys in the reactive Maps
const liveMeta = computed(() => replay.liveMeta.get(props.clip.id))

const duration = computed(() => liveMeta.value?.duration ?? props.clip.duration)
const width = computed(() => liveMeta.value?.width ?? props.clip.width)
const height = computed(() => liveMeta.value?.height ?? props.clip.height)
const displayDuration = computed(() => trimmedDuration.value ?? duration.value)
const isTrimmed = computed(() => trimmedDuration.value != null)

const { observe, unobserve } = useSharedIntersectionObserver()

function syncThumbUrl(path?: string) {
  const nextPath = path || replay.liveThumbs.get(props.clip.id) || props.clip.thumbnail || ''
  if (nextPath) resolvedThumbPath = nextPath
  if (!resolvedThumbPath || !props.mediaPort) return
  if (isVisible.value || thumbUrl.value || !rowRef.value) {
    thumbUrl.value = props.mediaUrl(resolvedThumbPath, props.mediaPort)
  }
}

watch(
  [() => replay.liveThumbs.get(props.clip.id), () => props.clip.thumbnail, () => props.mediaPort],
  ([path]) => { syncThumbUrl(path) },
  { immediate: true },
)

onMounted(() => {
  if (!resolvedThumbPath) resolvedThumbPath = props.clip.thumbnail || ''
  void loadTrimState()
  const trimListener = (event: Event) => {
    const detail = (event as CustomEvent<{ filepath?: string; trimStart?: number; trimEnd?: number }>).detail
    if (!detail || detail.filepath !== props.clip.filepath) return
    if (typeof detail.trimStart === 'number' && typeof detail.trimEnd === 'number' && detail.trimEnd > detail.trimStart) {
      const nextDuration = Math.max(0, detail.trimEnd - detail.trimStart)
      trimmedDuration.value = nextDuration > 0 && Math.abs(nextDuration - duration.value) > 0.05 ? nextDuration : null
      return
    }
    void loadTrimState()
  }
  window.addEventListener('clip-trim-updated', trimListener as EventListener)
  removeTrimListener = () => window.removeEventListener('clip-trim-updated', trimListener as EventListener)

  if (rowRef.value) {
    observe(rowRef.value, (entry) => {
      isVisible.value = entry.isIntersecting
      if (!entry.isIntersecting) {
        if (thumbUrl.value) {
          thumbUrl.value = ''
          thumbLoaded.value = false
        }
        return
      }

      // Is intersecting
      syncThumbUrl()
    })
  }
})

onBeforeUnmount(() => {
  if (rowRef.value) unobserve(rowRef.value)
  removeTrimListener?.()
  removeTrimListener = null
})

async function loadTrimState() {
  try {
    const state = await invoke<{ trim_start: number; trim_end: number } | null>('get_trim_state', { filepath: props.clip.filepath })
    if (state && state.trim_end > state.trim_start) {
      const nextDuration = Math.max(0, state.trim_end - state.trim_start)
      trimmedDuration.value = nextDuration > 0 && Math.abs(nextDuration - duration.value) > 0.05 ? nextDuration : null
      return
    }
  } catch {}
  trimmedDuration.value = null
}
</script>

<template>
  <div
    ref="rowRef"
    class="list-row"
    :class="{ selected }"
    :style="{
      '--list-pad': padding,
      '--list-font': fontSize,
      '--list-meta-font': metaFontSize,
      '--list-pill-pad-y': pillPadY,
      '--list-pill-pad-x': pillPadX,
      '--list-chip-font': chipFontSize,
      '--list-chip-pad-y': chipPadY,
      '--list-chip-pad-x': chipPadX,
      '--list-act-font': actionFontSize,
      '--list-act-pad-y': actionPadY,
      '--list-act-pad-x': actionPadX,
      '--list-thumb-w': thumbW,
      '--list-thumb-h': thumbH,
    }"
    @click="emit('click', clip)"
    @contextmenu.prevent="e => emit('contextmenu', clip, e)"
  >
    <div class="list-thumb-wrap">
      <div
        class="list-select"
        :class="{ vis: selected || replay.selectMode }"
        @click.stop="replay.toggleSelect(clip.id)"
      >
        <div class="list-sel-box" :class="{ checked: selected }">✓</div>
      </div>
      <img
        v-if="thumbUrl"
        :src="thumbUrl"
        class="list-thumb"
        :class="{ loaded: thumbLoaded }"
        loading="lazy"
        decoding="async"
        alt=""
        @load="thumbLoaded = true"
      />
      <div v-else class="list-thumb-empty">🎬</div>
      <span v-if="displayDuration" class="list-badge" :class="{ trimmed: isTrimmed }">
        <svg v-if="isTrimmed" viewBox="0 0 24 24" aria-hidden="true">
          <path fill="currentColor" d="M9.64 7.64a2.5 2.5 0 1 1-3.54-3.54 2.5 2.5 0 0 1 3.54 3.54Zm0 8.72a2.5 2.5 0 1 1-3.54 3.54 2.5 2.5 0 0 1 3.54-3.54ZM14.59 12l6.2 6.2-1.41 1.41L12 12.41l-7.38 7.2-1.4-1.42L9.41 12 3.22 5.8l1.4-1.41L12 11.59l7.38-7.2 1.41 1.42z"/>
        </svg>
        {{ fmtDur(displayDuration) }}
      </span>
      <span v-if="clip.created" class="list-time-badge">{{ fmtTime(clip.created) }}</span>
      <button class="list-fav" :class="{ on: clip.favorite }" @click.stop="e => emit('favorite', clip, e)" title="Favorite">
        <svg viewBox="0 0 24 24" :fill="clip.favorite?'currentColor':'none'" stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
      </button>
    </div>
    <div class="list-info">
      <div class="list-name">{{ clipDisplayTitle(clip.custom_name || '', clip.game || '', clip.filename) }}</div>
      <div class="list-meta">
        <span class="lm-pill">{{ fmtSize(clip.filesize) }}</span>
        <span v-if="width" class="lm-pill">{{ fmtRes(width, height) }}</span>
        <span v-if="clip.created" class="lm-pill lm-date">{{ fmtDate(clip.created) }}</span>
        <span v-if="clip.game && clip.game !== 'Unknown'" class="lm-game">{{ clip.game }}</span>
      </div>
    </div>
    <div class="list-actions">
      <button class="list-act" @click.stop="emit('preview', clip)">Preview</button>
      <button class="list-act" @click.stop="emit('editor', clip)">Edit</button>
      <button class="list-act list-act-d" @click.stop="emit('delete', clip)">Delete</button>
    </div>
  </div>
</template>

<style scoped>
.list-row {
  display:flex; align-items:stretch; gap:12px;
  padding: 0 calc(var(--list-pad, 8px) + 4px) 0 0;
  background:var(--bg-card); border:1px solid var(--border); border-radius:8px;
  cursor:pointer; overflow:hidden; user-select:none;
  transition: background .15s, padding .25s ease;
  contain: layout style paint;
}
.list-row:hover { background:var(--bg-hover); }
.list-row.selected { border-color:var(--accent); background:color-mix(in srgb, var(--accent) 8%, transparent); }
.list-select {
  position:absolute; top:6px; left:6px;
  opacity:0; transition:opacity .15s;
  z-index:2;
}
.list-select.vis,
.list-row:hover .list-select { opacity:1; }
.list-sel-box {
  width:22px; height:22px; border-radius:5px;
  border:2px solid rgba(255,255,255,.5);
  background:rgba(0,0,0,.4);
  display:flex; align-items:center; justify-content:center;
  color:transparent; font-size:12px; cursor:pointer;
  transition:border-color .15s, background .15s, color .15s;
}
.list-sel-box.checked { background:var(--accent); border-color:var(--accent); color:#fff; }

.list-thumb {
  object-fit:cover; background:var(--bg-deep); user-select:none; -webkit-user-drag:none; pointer-events:none;
}
.list-thumb-empty {
  background:var(--bg-deep); flex-shrink:0;
  display:flex; align-items:center; justify-content:center;
  font-size:18px; color:var(--text-muted);
}
.list-info { flex:1; min-width:0; display:flex; flex-direction:column; gap:3px; padding: var(--list-pad, 8px) 0; justify-content: center; }
.list-name {
  font-size: var(--list-font, 13px); font-weight:600;
  white-space:nowrap; overflow:hidden; text-overflow:ellipsis;
  transition: font-size .25s ease;
}
.list-thumb-wrap {
  position: relative; flex-shrink: 0;
  width: var(--list-thumb-w, 160px);
  height: var(--list-thumb-h, 90px);
  align-self: center;
  transition: width .25s ease, height .25s ease;
}
.list-thumb-wrap .list-thumb { width: 100%; height: 100%; object-fit: cover; display: block; transition: none; border-radius: 0; }
.list-thumb-wrap .list-thumb-empty { width: 100%; height: 100%; transition: none; border-radius: 0; }
.list-badge {
  position: absolute; bottom: 3px; right: 3px;
  background: rgba(0,0,0,.8); color: #fff;
  font-size: 10px; font-weight: 600;
  padding: 1px 5px; border-radius: 3px;
  pointer-events: none; line-height: 1.4;
  display:flex; align-items:center; gap:4px;
}
.list-badge svg { width:11px; height:11px; }
.list-badge.trimmed { color:#ffd27a; }
.list-time-badge {
  position:absolute; bottom:3px; left:3px;
  background: rgba(0,0,0,.8); color: #fff;
  font-size: 10px; font-weight: 600;
  padding: 1px 5px; border-radius: 3px;
  pointer-events: none; line-height: 1.4;
}
.list-fav {
  position:absolute; top:6px; right:6px;
  width:28px; height:28px; border-radius:50%; border:none;
  background:rgba(0,0,0,.5); color:var(--text-muted);
  cursor:pointer; display:flex; align-items:center; justify-content:center;
  opacity:0; transition:all .15s; z-index:2;
}
.list-fav svg { width:14px; height:14px; }
.list-thumb-wrap:hover .list-fav,
.list-row:hover .list-fav,
.list-fav.on { opacity:1; }
.list-fav:hover { background:rgba(0,0,0,.8); color:var(--text); transform:scale(1.1); }
.list-fav.on { color:#E94560; }
.list-meta { font-size:var(--list-meta-font, 11px); color:var(--text-muted); display:flex; align-items:center; flex-wrap:nowrap; gap:4px; overflow:hidden; }
.list-meta > * { min-width:0; }
.lm-game {
  margin-left:auto; min-width:0; max-width:100%; flex-shrink:1; font-weight:700; font-size:var(--list-chip-font, 10px);
  color:var(--accent);
  background:color-mix(in srgb, var(--accent) 14%, transparent);
  padding:var(--list-chip-pad-y, 2px) var(--list-chip-pad-x, 8px); border-radius:4px;
  white-space:nowrap; overflow:hidden; text-overflow:ellipsis;
}
.lm-pill { background:var(--bg-deep); padding:var(--list-pill-pad-y, 2px) var(--list-pill-pad-x, 6px); border-radius:3px; flex-shrink:0; }
.lm-date { opacity:.75; }
.list-actions { display:flex; gap:6px; flex-shrink:0; align-items:center; padding: var(--list-pad, 8px) 0; }
.list-act { padding: var(--list-act-pad-y, 4px) var(--list-act-pad-x, 10px); border:1px solid var(--border); border-radius:5px; background:var(--bg-surface); color:var(--text-sec); font-size:var(--list-act-font, 12px); cursor:pointer; white-space:nowrap; }
.list-act:hover { background:var(--bg-hover); color: var(--text); border-color: var(--text-muted); }
.list-act-d { color:var(--danger); }
.list-act-d:hover { background:rgba(220,38,38,.1); border-color: var(--danger); }
</style>

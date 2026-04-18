<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, watch } from 'vue'
import type { Clip } from '../stores/replay'
import { useReplayStore } from '../stores/replay'
import { useSharedIntersectionObserver } from '../composables/useSharedIntersectionObserver'
import { fmtDur, fmtSize, fmtRes, fmtDate } from '../utils/format'

const props = defineProps<{
  clip: Clip
  selected: boolean
  fontSize: string
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

// Local cache for the resolved path to avoid re-calculating or re-fetching
let resolvedThumbPath = ''

// Surgical reactivity: watch specific keys in the reactive Maps
const liveMeta = computed(() => replay.liveMeta.get(props.clip.id))

const duration = computed(() => liveMeta.value?.duration ?? props.clip.duration)
const width = computed(() => liveMeta.value?.width ?? props.clip.width)
const height = computed(() => liveMeta.value?.height ?? props.clip.height)

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
})
</script>

<template>
  <div
    ref="rowRef"
    class="list-row"
    :class="{ selected }"
    :style="{ '--list-pad': padding, '--list-font': fontSize, '--list-thumb-w': thumbW, '--list-thumb-h': thumbH }"
    @click="emit('click', clip)"
    @contextmenu.prevent="e => emit('contextmenu', clip, e)"
  >
    <div class="list-thumb-wrap">
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
      <span v-if="duration" class="list-badge">{{ fmtDur(duration) }}</span>
    </div>
    <div class="list-info">
      <div class="list-name">{{ clip.custom_name || (clip.game !== 'Unknown' ? clip.game : clip.filename) }}</div>
      <div class="list-meta">
        <span class="lm-pill">{{ fmtSize(clip.filesize) }}</span>
        <span v-if="width" class="lm-pill">{{ fmtRes(width, height) }}</span>
        <span v-if="clip.created" class="lm-pill lm-date">{{ fmtDate(clip.created) }}</span>
        <span v-if="clip.game && clip.game !== 'Unknown'" class="lm-game">{{ clip.game }}</span>
      </div>
    </div>
    <div class="list-actions">
      <button class="list-fav" :class="{ on: clip.favorite }" @click.stop="e => emit('favorite', clip, e)">
        <svg viewBox="0 0 24 24" :fill="clip.favorite?'currentColor':'none'" stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
      </button>
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
}
.list-meta { font-size:11px; color:var(--text-muted); display:flex; align-items:center; flex-wrap:nowrap; gap:4px; overflow:hidden; }
.lm-game {
  font-weight:700; font-size:10px;
  color:var(--accent);
  background:color-mix(in srgb, var(--accent) 14%, transparent);
  padding:2px 8px; border-radius:4px;
  white-space:nowrap; overflow:hidden; text-overflow:ellipsis; max-width:120px;
}
.lm-pill { background:var(--bg-deep); padding:2px 6px; border-radius:3px; flex-shrink:0; }
.lm-date { opacity:.75; }
.list-actions { display:flex; gap:6px; flex-shrink:0; align-items:center; padding: var(--list-pad, 8px) 0; }
.list-act { padding: 4px 10px; border:1px solid var(--border); border-radius:5px; background:var(--bg-surface); color:var(--text-sec); font-size:12px; cursor:pointer; white-space:nowrap; }
.list-act:hover { background:var(--bg-hover); }
.list-act-d { color:var(--danger); }
.list-act-d:hover { background:rgba(220,38,38,.1); }
.list-fav { flex-shrink:0; width:28px; height:28px; border-radius:50%; border:none; background:transparent; color:var(--text-muted); cursor:pointer; display:flex; align-items:center; justify-content:center; transition:color .15s; }
.list-fav:hover { color:var(--text); }
.list-fav.on { color:#E94560; }
</style>

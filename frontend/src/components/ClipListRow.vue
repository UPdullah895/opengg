<script setup lang="ts">
import { ref, computed, onMounted, nextTick } from 'vue'
import type { Clip } from '../stores/replay'
import { useReplayStore } from '../stores/replay'
import { useClipThumbnail } from '../composables/useClipThumbnail'
import { fmtDur, fmtSize, fmtRes, fmtDate, fmtTime, clipDisplayTitle } from '../utils/format'
import { invoke } from '@tauri-apps/api/core'
// ── Inline icon SVGs ──
const ICON_PLAY = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polygon points="10 8 16 12 10 16 10 8"/></svg>`
const ICON_EDIT = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>`
const ICON_HARD_DRIVE = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="22" y1="12" x2="2" y2="12"/><path d="M5.45 5.11L2 12v6a2 2 0 002 2h16a2 2 0 002-2v-6l-3.45-6.89A2 2 0 0016.76 4H7.24a2 2 0 00-1.79 1.11z"/><line x1="6" y1="16" x2="6.01" y2="16"/><line x1="10" y1="16" x2="10.01" y2="16"/></svg>`
const ICON_MONITOR = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>`
const ICON_CLOCK = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>`
const ICON_GAMEPAD_SM = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="6" y1="12" x2="10" y2="12"/><line x1="8" y1="10" x2="8" y2="14"/><line x1="15" y1="13" x2="15.01" y2="13"/><line x1="18" y1="11" x2="18.01" y2="11"/><rect x="2" y="6" width="20" height="12" rx="2"/></svg>`

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

// ── Inline rename ──
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

const {
  thumbUrl, thumbLoaded, displayDuration, isTrimmed,
  liveWidth, liveHeight, bind,
} = useClipThumbnail(
  props.clip.id,
  props.clip.filepath,
  props.clip.duration,
  props.clip.thumbnail,
  props.mediaPort,
  props.mediaUrl,
)

const width = computed(() => liveWidth.value)
const height = computed(() => liveHeight.value)

onMounted(() => {
  if (rowRef.value) bind(rowRef.value)
})
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
      <div v-else class="list-thumb-empty"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" style="width:18px;height:18px"><rect x="2" y="2" width="20" height="20" rx="2.18" ry="2.18"/><line x1="7" y1="2" x2="7" y2="22"/><line x1="17" y1="2" x2="17" y2="22"/><line x1="2" y1="12" x2="22" y2="12"/><line x1="2" y1="7" x2="7" y2="7"/><line x1="2" y1="17" x2="7" y2="17"/><line x1="17" y1="17" x2="22" y2="17"/><line x1="17" y1="7" x2="22" y2="7"/></svg></div>
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
      <div class="list-name" @click.stop="startEdit">
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
      <div class="list-meta">
        <span class="lm-pill"><span class="lm-ic" v-html="ICON_HARD_DRIVE"></span>{{ fmtSize(clip.filesize) }}</span>
        <span v-if="width" class="lm-pill"><span class="lm-ic" v-html="ICON_MONITOR"></span>{{ fmtRes(width, height) }}</span>
        <span v-if="clip.created" class="lm-pill lm-date"><span class="lm-ic" v-html="ICON_CLOCK"></span>{{ fmtDate(clip.created) }}</span>
        <span v-if="clip.game && clip.game !== 'Unknown'" class="lm-game"><span class="lm-ic" v-html="ICON_GAMEPAD_SM"></span>{{ clip.game }}</span>
      </div>
    </div>
    <div class="list-actions">
      <button class="list-act-icon" @click.stop="emit('preview', clip)" title="Preview">
        <span v-html="ICON_PLAY"></span>
      </button>
      <button class="list-act-icon" @click.stop="emit('editor', clip)" title="Edit">
        <span v-html="ICON_EDIT"></span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.list-row {
  display:flex; align-items:stretch; gap:12px;
  padding: 0 calc(var(--list-pad, 8px) + 4px) 0 0;
  background:var(--bg-card); border:1px solid var(--border); border-radius:8px;
  cursor:pointer; overflow:hidden;
  transition: background .15s, padding .25s ease;
  contain: layout style paint;
}
.list-row:hover { background:var(--bg-hover); border-left:3px solid var(--accent); padding-left:calc(var(--list-pad, 8px) + 4px); }
.list-row.selected { background:color-mix(in srgb, var(--accent) 8%, transparent); border-left:3px solid var(--accent); padding-left:calc(var(--list-pad, 8px) + 4px); }
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
  object-fit:cover; background:var(--bg-deep); pointer-events:none;
}
.list-thumb-empty {
  background:var(--bg-deep); flex-shrink:0;
  display:flex; align-items:center; justify-content:center;
  font-size:18px; color:var(--text-muted);
}
.list-info { flex:1; min-width:0; display:flex; flex-direction:column; gap:5px; padding: var(--list-pad, 8px) 0; justify-content: center; align-self:center; }
.list-name {
  font-size: var(--list-font, 13px); font-weight:600;
  white-space:nowrap; overflow:hidden; text-overflow:ellipsis;
  transition: font-size .25s ease;
  cursor:pointer;
}
.list-name:hover { color:var(--accent); }
.inline-edit {
  width:100%;
  padding:2px 4px;
  margin:-2px -4px;
  background:var(--bg-input);
  border:1px solid var(--accent);
  border-radius:4px;
  color:var(--text);
  font-size:var(--list-font, 13px);
  font-weight:600;
  outline:none;
  line-height:1.3;
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
  margin-left:auto; min-width:0; max-width:100%; flex-shrink:1; font-weight:700; font-size:var(--list-meta-font, 11px);
  color:var(--accent);
  background:color-mix(in srgb, var(--accent) 14%, transparent);
  padding:var(--list-act-pad-y, 4px) var(--list-chip-pad-x, 8px); border-radius:4px;
  white-space:nowrap; overflow:hidden; text-overflow:ellipsis;
  display:flex; align-items:center; gap:4px;
}
.lm-pill { background:var(--bg-deep); padding:var(--list-act-pad-y, 4px) var(--list-pill-pad-x, 6px); border-radius:3px; flex-shrink:0; display:flex; align-items:center; gap:4px; }
.lm-ic { width:calc(var(--list-meta-font, 11px) * 0.95); height:calc(var(--list-meta-font, 11px) * 0.95); display:inline-flex; color:var(--text-muted); }
.lm-ic :deep(svg) { width:calc(var(--list-meta-font, 11px) * 0.95); height:calc(var(--list-meta-font, 11px) * 0.95); }
.lm-date { opacity:.75; }
.list-actions { display:flex; gap:8px; flex-shrink:0; align-items:center; padding: var(--list-pad, 8px) 0; }
.list-act-icon { width:calc(var(--list-act-font, 12px) * 2.5); height:calc(var(--list-act-font, 12px) * 2.5); border:1px solid var(--border); border-radius:6px; background:var(--bg-surface); color:var(--text-sec); cursor:pointer; display:flex; align-items:center; justify-content:center; transition:all .15s; }
.list-act-icon:hover { background:var(--bg-hover); color:var(--text); border-color:var(--text-muted); }
.list-act-icon span { width:calc(var(--list-act-font, 12px) * 1.2); height:calc(var(--list-act-font, 12px) * 1.2); display:inline-flex; }
.list-act-icon span :deep(svg) { width:calc(var(--list-act-font, 12px) * 1.2); height:calc(var(--list-act-font, 12px) * 1.2); }
</style>

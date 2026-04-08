<script setup lang="ts">
import { ref, computed } from 'vue'
import { useAudioStore } from '../stores/audio'

const props = defineProps<{
  channel: string
  color: string
  apps: { id: number; name: string; binary: string }[]
}>()

const audio = useAudioStore()
const hovering = ref(false)
const isDragging = computed(() => audio.draggedApp !== null)

function onDragOver(e: DragEvent) {
  e.preventDefault()
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'move'
  hovering.value = true
}

function onDragLeave(e: DragEvent) {
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
  if (e.clientX < rect.left || e.clientX > rect.right || e.clientY < rect.top || e.clientY > rect.bottom) {
    hovering.value = false
  }
}

function onDrop(e: DragEvent) {
  e.preventDefault()
  hovering.value = false
  // Dual-path: custom MIME works on desktop; text/plain is the WebKitGTK fallback
  // because WebKitGTK silently returns '' for non-text MIME types on getData().
  const raw = e.dataTransfer?.getData('application/opengg-app-id')
           || e.dataTransfer?.getData('text/plain')
           || ''
  const parsedId = parseInt(raw, 10)
  if (!isNaN(parsedId)) {
    audio.dropOnChannelById(parsedId, props.channel)
  } else if (audio.draggedApp) {
    // Last-resort: use the store's draggedApp (same-tab drag where store is reliable)
    audio.dropOnChannel(props.channel)
  }
}

function startDragApp(e: DragEvent, app: { id: number; name: string; binary: string }) {
  if (!e.dataTransfer) return
  e.dataTransfer.setData('application/opengg-app-id', String(app.id))
  // text/plain carries the numeric ID as a reliable WebKitGTK fallback
  e.dataTransfer.setData('text/plain', String(app.id))
  e.dataTransfer.effectAllowed = 'move'
  audio.startDrag({ id: app.id, name: app.name, binary: app.binary, channel: props.channel, icon: '' })
}
</script>

<template>
  <div class="dropzone" :class="{ hovering, 'show-target': isDragging }" :style="{ '--dz': color }"
    @dragover="onDragOver" @dragleave="onDragLeave" @drop="onDrop">
    <div v-if="apps.length > 0" class="dz-apps">
      <div v-for="app in apps" :key="app.id" class="dz-chip" draggable="true"
        @dragstart="startDragApp($event, app)" @dragend="audio.endDrag()">
        <span class="dz-dot" :style="{ background: color }"></span>
        <span class="dz-name">{{ app.name }}</span>
      </div>
    </div>
    <div v-else class="dz-empty" :class="{ active: isDragging }">
      <span v-if="isDragging">Drop here</span>
      <span v-else>No apps</span>
    </div>
  </div>
</template>

<style scoped>
.dropzone { width: 100%; min-height: 40px; max-height: 140px; overflow-y: auto; border: 1px dashed var(--border); border-radius: 6px; padding: 4px; display: flex; flex-direction: column; gap: 2px; transition: all .15s; scrollbar-width: thin; scrollbar-color: var(--border) transparent; }
.show-target { border-color: color-mix(in srgb, var(--dz) 40%, var(--border)); background: color-mix(in srgb, var(--dz) 3%, transparent); }
.hovering { border-color: var(--dz) !important; background: color-mix(in srgb, var(--dz) 10%, transparent) !important; border-style: solid !important; box-shadow: 0 0 12px color-mix(in srgb, var(--dz) 20%, transparent); }
.dz-apps { display: flex; flex-direction: column; gap: 2px; }
.dz-chip { display: flex; align-items: center; gap: 5px; padding: 6px 8px; background: var(--bg-deep); border-radius: 4px; font-size: 11px; color: var(--text-sec); cursor: grab; overflow: hidden; transition: background .1s; }
.dz-chip:hover { background: var(--bg-hover); }
.dz-chip:active { cursor: grabbing; opacity: .6; }
.dz-dot { width: 5px; height: 5px; border-radius: 50%; flex-shrink: 0; }
.dz-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.dz-empty { text-align: center; font-size: 9px; color: var(--text-muted); padding: 8px 0; opacity: .6; }
.dz-empty.active { color: var(--dz); opacity: 1; font-weight: 600; }
</style>

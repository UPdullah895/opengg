<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue'

const props = defineProps<{ modelValue: string }>()
const emit = defineEmits<{ (e: 'update:modelValue', v: string): void }>()

const open = ref(false)
const btnRef = ref<HTMLButtonElement | null>(null)

const ICONS = [
  { id: 'video',   label: 'Video',
    path: 'M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M3 6h10a2 2 0 012 2v8a2 2 0 01-2 2H3a2 2 0 01-2-2V8a2 2 0 012-2z' },
  { id: 'game',    label: 'Game',
    path: 'M6 11h4m-2-2v4m7-1h.01M18 11h.01M2 6a2 2 0 012-2h16a2 2 0 012 2v10a4 4 0 01-4 4H6a4 4 0 01-4-4V6z' },
  { id: 'chat',    label: 'Chat',
    path: 'M3 18v-6a9 9 0 0118 0v6M21 19a2 2 0 01-2 2h-1a2 2 0 01-2-2v-3a2 2 0 012-2h3zM3 19a2 2 0 002 2h1a2 2 0 002-2v-3a2 2 0 00-2-2H3z' },
  { id: 'mic',     label: 'Mic',
    path: 'M12 1a3 3 0 00-3 3v8a3 3 0 006 0V4a3 3 0 00-3-3zM19 10v2a7 7 0 01-14 0v-2M12 19v3M8 23h8' },
  { id: 'media',   label: 'Media',
    path: 'M9 18V5l12-2v13M9 19c0 1.1-1.34 2-3 2s-3-.9-3-2 1.34-2 3-2 3 .9 3 2zm12-3c0 1.1-1.34 2-3 2s-3-.9-3-2 1.34-2 3-2 3 .9 3 2z' },
  { id: 'overlay', label: 'Overlay',
    path: 'M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5' },
]

function select(id: string) {
  emit('update:modelValue', id)
  open.value = false
}

function handleOutside(e: MouseEvent) {
  if (btnRef.value && !btnRef.value.closest('.icon-picker-wrap')?.contains(e.target as Node)) {
    open.value = false
  }
}

onMounted(() => document.addEventListener('mousedown', handleOutside))
onBeforeUnmount(() => document.removeEventListener('mousedown', handleOutside))

const current = () => ICONS.find(i => i.id === props.modelValue) ?? ICONS[0]
</script>

<template>
  <div class="icon-picker-wrap">
    <button ref="btnRef" class="icon-btn" @click.stop="open = !open" :title="current().label">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path :d="current().path" />
      </svg>
    </button>
    <div v-if="open" class="icon-popover">
      <button
        v-for="ic in ICONS"
        :key="ic.id"
        class="icon-opt"
        :class="{ active: ic.id === modelValue }"
        :title="ic.label"
        @click.stop="select(ic.id)"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path :d="ic.path" />
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.icon-picker-wrap { position: relative; display: inline-flex; }

.icon-btn {
  width: 32px; height: 32px; border-radius: 6px; border: 1px solid var(--border);
  background: var(--bg-deep); color: var(--text-sec); cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  transition: border-color .15s, color .15s;
}
.icon-btn:hover { border-color: var(--accent); color: var(--text); }
.icon-btn svg { width: 15px; height: 15px; }

.icon-popover {
  position: absolute; bottom: calc(100% + 6px); right: 0; left: auto; z-index: 500;
  background: var(--bg-card); border: 1px solid var(--border); border-radius: 8px;
  padding: 6px; display: grid; grid-template-columns: repeat(3, 1fr); gap: 4px;
  box-shadow: 0 8px 24px rgba(0,0,0,.5);
}

.icon-opt {
  width: 34px; height: 34px; border-radius: 6px; border: 1px solid transparent;
  background: var(--bg-deep); color: var(--text-muted); cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  transition: all .15s;
}
.icon-opt:hover { border-color: var(--accent); color: var(--text); }
.icon-opt.active { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 15%, transparent); color: var(--accent); }
.icon-opt svg { width: 16px; height: 16px; }
</style>

<script setup lang="ts">
defineOptions({ name: 'ClipsContextMenu' })
import { useI18n } from 'vue-i18n'
import type { Clip } from '../../stores/replay'

interface Props {
  clip: Clip | null
  position: { x: number; y: number }
  activeIndex: number
}

interface Emits {
  action: [id: string]
  keydown: [e: KeyboardEvent]
}

defineProps<Props>()
const emit = defineEmits<Emits>()

useI18n()
</script>

<template>
  <div
    class="ctx-menu"
    :style="{ left: position.x + 'px', top: position.y + 'px' }"
    @click.stop
    @contextmenu.prevent
    @keydown="emit('keydown', $event)"
    tabindex="0"
  >
    <slot />
  </div>
</template>

<style scoped>
.ctx-menu {
  position: fixed; z-index: 5000;
  background: var(--bg-card); border: 1px solid var(--border);
  border-radius: 8px; padding: 4px; min-width: 180px;
  box-shadow: 0 8px 24px rgba(0,0,0,.5);
}

.ctx-item {
  display: flex; align-items: center; gap: 10px; width: 100%; padding: 8px 14px; background: none; border: none;
  border-radius: 5px; color: var(--text-sec); font-size: 13px; text-align: left;
  cursor: pointer; white-space: nowrap;
}

.ctx-icon { width: 15px; height: 15px; flex-shrink: 0; opacity: .8; display: flex; align-items: center; justify-content: center; }
.ctx-icon :deep(svg) { width: 15px; height: 15px; }

.ctx-item:hover, .ctx-item.active { background: var(--bg-hover); color: var(--text); }
.ctx-item-d { color: var(--danger); }
.ctx-item-d:hover, .ctx-item-d.active { background: rgba(220,38,38,.1); }

.ctx-sep { height: 1px; background: var(--border); margin: 4px 0; }
</style>

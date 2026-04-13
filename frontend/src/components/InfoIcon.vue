<script setup lang="ts">
import { ref, onMounted } from 'vue'

defineProps<{ title: string }>()

const iconEl = ref<HTMLElement | null>(null)
const pos = ref<'center' | 'left' | 'right'>('center')

onMounted(() => {
  if (!iconEl.value) return
  const rect = iconEl.value.getBoundingClientRect()
  const half = 125 // half of max-width 250px
  if (rect.left < half) {
    pos.value = 'right'
  } else if (window.innerWidth - rect.right < half) {
    pos.value = 'left'
  }
})
</script>

<template>
  <span ref="iconEl" class="info-icon" :data-pos="pos" tabindex="0" role="tooltip" :aria-label="title">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="12" r="10"/>
      <line x1="12" y1="8" x2="12" y2="12"/>
      <line x1="12" y1="16" x2="12.01" y2="16"/>
    </svg>
    <span class="info-tooltip">{{ title }}</span>
  </span>
</template>

<style scoped>
.info-icon {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  height: 14px;
  color: var(--text-muted);
  cursor: help;
  flex-shrink: 0;
  vertical-align: middle;
  margin-inline-start: 5px;
  transition: color .12s;
}
.info-icon:hover,
.info-icon:focus-visible { color: var(--accent); outline: none; }
.info-icon svg { width: 14px; height: 14px; }

.info-tooltip {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 50%;
  transform: translateX(-50%);
  white-space: normal;
  max-width: 250px;
  width: max-content;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 8px 10px;
  font-size: 11px;
  color: var(--text-sec);
  line-height: 1.5;
  box-shadow: 0 6px 20px rgba(0,0,0,.45);
  pointer-events: none;
  opacity: 0;
  transition: opacity .15s;
  z-index: 9999;
  font-weight: 400;
  text-transform: none;
  letter-spacing: 0;
}
/* Flip tooltip to the right when near left edge */
.info-icon[data-pos="right"] .info-tooltip {
  left: 0;
  transform: none;
}
/* Flip tooltip to the left when near right edge */
.info-icon[data-pos="left"] .info-tooltip {
  left: auto;
  right: 0;
  transform: none;
}
.info-icon:hover .info-tooltip,
.info-icon:focus-visible .info-tooltip { opacity: 1; }
</style>

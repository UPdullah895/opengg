<script setup lang="ts">
defineProps<{ title: string }>()
</script>

<template>
  <span class="info-icon" tabindex="0" role="tooltip" :aria-label="title">
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
  margin-left: 5px;
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
  width: 220px;
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
.info-icon:hover .info-tooltip,
.info-icon:focus-visible .info-tooltip { opacity: 1; }
</style>

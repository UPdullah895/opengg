<script setup lang="ts">
import SmartTooltip from './SmartTooltip.vue'

defineProps<{ title: string }>()
</script>

<template>
  <SmartTooltip :title="title">
    <span class="info-icon" tabindex="-1" role="img" :aria-label="title">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="10"/>
        <line x1="12" y1="8" x2="12" y2="12"/>
        <line x1="12" y1="16" x2="12.01" y2="16"/>
      </svg>
    </span>
  </SmartTooltip>
</template>

<style scoped>
.info-icon {
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
  transition: color 0.12s;
}
.info-icon:hover {
  color: var(--accent);
}
.info-icon svg {
  width: 14px;
  height: 14px;
}
</style>

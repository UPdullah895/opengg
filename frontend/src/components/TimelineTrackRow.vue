<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(defineProps<{
  part: 'header' | 'body'
  color: string
  label?: string
  kindLabel?: string
  icon?: string
  showIcon?: boolean
  preview?: boolean
}>(), {
  showIcon: true,
  preview: false,
})

// Icon SVG paths map
const iconPaths: Record<string, string> = {
  video: 'M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M3 6h10a2 2 0 012 2v8a2 2 0 01-2 2H3a2 2 0 01-2-2V8a2 2 0 012-2z',
  game: 'M6 11h4m-2-2v4m7-1h.01M18 11h.01M2 6a2 2 0 012-2h16a2 2 0 012 2v10a4 4 0 01-4 4H6a4 4 0 01-4-4V6z',
  mic: 'M12 1a3 3 0 00-3 3v8a3 3 0 006 0V4a3 3 0 00-3-3zM19 10v2a7 7 0 01-14 0v-2M12 19v3M8 23h8',
  chat: 'M3 18v-6a9 9 0 0118 0v6M21 19a2 2 0 01-2 2h-1a2 2 0 01-2-2v-3a2 2 0 012-2h3zM3 19a2 2 0 002 2h1a2 2 0 002-2v-3a2 2 0 00-2-2H3z',
  media: 'M9 18V5l12-2v13M9 19c0 1.1-1.34 2-3 2s-3-.9-3-2 1.34-2 3-2 3 .9 3 2zm12-3c0 1.1-1.34 2-3 2s-3-.9-3-2 1.34-2 3-2 3 .9 3 2z',
  overlay: 'M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5',
}

const svgPath = computed(() => {
  if (!props.icon || !props.showIcon) return ''
  return iconPaths[props.icon] || iconPaths.video
})
</script>

<template>
  <!-- Header part: 36px height, border-left, icon, label -->
  <div v-if="part === 'header'" class="tl-header" :style="{ '--tc': color }">
    <svg v-if="showIcon && icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="hdr-icon-svg">
      <path :d="svgPath" />
    </svg>
    <span v-if="label" class="hdr-name">{{ label }}</span>
    <span v-if="kindLabel" class="hdr-kind">{{ kindLabel }}</span>
    <slot name="actions"></slot>
  </div>

  <!-- Body part: 36px height, background color-mix, slot + optional preview bar -->
  <div v-else class="tl-row" :style="{ '--tc': color }">
    <slot>
      <!-- Default slot for waveform canvas or other content -->
    </slot>
    <!-- Preview mode: show a fake clip bar -->
    <div v-if="preview" class="tl-preview-bar"></div>
  </div>
</template>

<style scoped>
.tl-header {
  height: 36px;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 0 8px;
  border-bottom: 1px solid var(--border);
  border-left: 3px solid var(--tc);
  background: color-mix(in srgb, var(--tc) 8%, var(--bg-deep));
  position: relative;
}

.hdr-icon-svg {
  width: 11px;
  height: 11px;
  color: var(--tc);
  opacity: 0.75;
  flex-shrink: 0;
}

.hdr-name {
  font-size: 9px;
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Role badge: distinguishes input (mic) vs output (desktop) vs video tracks. */
.hdr-kind {
  flex-shrink: 0;
  font-size: 7.5px;
  font-weight: 700;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  line-height: 1;
  padding: 2px 4px;
  border-radius: 4px;
  color: var(--tc);
  background: color-mix(in srgb, var(--tc) 16%, transparent);
  border: 1px solid color-mix(in srgb, var(--tc) 30%, transparent);
}

.tl-row {
  height: 36px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--border);
  background: color-mix(in srgb, var(--tc) 6%, var(--bg-deep));
  position: relative;
  overflow: hidden;
}

/* Preview bar: simple fake clip visualization */
.tl-preview-bar {
  position: absolute;
  left: 8px;
  right: 8px;
  top: 50%;
  transform: translateY(-50%);
  height: 16px;
  background: linear-gradient(to right, color-mix(in srgb, var(--tc) 50%, transparent), color-mix(in srgb, var(--tc) 30%, transparent));
  border-radius: 2px;
}
</style>

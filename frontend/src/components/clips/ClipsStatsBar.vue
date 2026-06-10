<script setup lang="ts">
defineOptions({ name: 'ClipsStatsBar' })
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

interface ClipStats {
  count: number
  totalDuration: number
  totalSize: number
  avgDuration: number
}

interface Props {
  clipStats: ClipStats
  totalClipCount: number
  show: boolean
}

defineProps<Props>()

function fmtStatDuration(s: number) {
  const m = Math.floor(s / 60)
  const h = Math.floor(m / 60)
  if (h > 0) return `${h}h ${m % 60}m`
  return `${m}m ${Math.floor(s % 60)}s`
}

function fmtStatSize(bytes: number) {
  const gb = bytes / (1024 ** 3)
  if (gb >= 1) return `${gb.toFixed(1)} GB`
  const mb = bytes / (1024 ** 2)
  if (mb >= 1) return `${mb.toFixed(0)} MB`
  return `${bytes} B`
}
</script>

<template>
  <div v-if="show && totalClipCount > 0" class="stats-bar">
    <div class="stat-item">
      <span class="stat-val">{{ clipStats.count }}</span>
      <span class="stat-label">{{ t('clips.stats.clips') }}</span>
    </div>
    <div class="stat-sep"></div>
    <div class="stat-item">
      <span class="stat-val">{{ fmtStatDuration(clipStats.totalDuration) }}</span>
      <span class="stat-label">{{ t('clips.stats.totalDuration') }}</span>
    </div>
    <div class="stat-sep"></div>
    <div class="stat-item">
      <span class="stat-val">{{ fmtStatSize(clipStats.totalSize) }}</span>
      <span class="stat-label">{{ t('clips.stats.totalSize') }}</span>
    </div>
    <div class="stat-sep"></div>
    <div class="stat-item">
      <span class="stat-val">{{ fmtStatDuration(clipStats.avgDuration) }}</span>
      <span class="stat-label">{{ t('clips.stats.avgDuration') }}</span>
    </div>
  </div>
</template>

<style scoped>
.stats-bar {
  display: flex; align-items: center; gap: 12px; flex-shrink: 0;
  padding: 6px 14px; background: var(--bg-card); border: 1px solid var(--border);
  border-radius: 8px; font-size: 12px;
}
.stat-item { display: flex; align-items: center; gap: 4px; }
.stat-val { font-weight: 700; color: var(--text); }
.stat-label { color: var(--text-muted); font-size: 11px; }
.stat-sep { width: 1px; height: 14px; background: var(--border); }
</style>

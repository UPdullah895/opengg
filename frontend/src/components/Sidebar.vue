<script setup lang="ts">
import { useI18n } from 'vue-i18n'
defineProps<{ active: string }>()
const emit = defineEmits<{ navigate: [page: string] }>()
const { t } = useI18n()

const items = [
  { id: 'home',     icon: 'M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-4 0a1 1 0 01-1-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 01-1 1h-2z' },
  { id: 'mixer',    icon: 'M4 21V14m0-4V3m8 18V12m0-4V3m8 18V16m0-4V3M1 14h6M9 8h6M17 16h6' },
  { id: 'clips',    icon: 'M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M3 6h10a2 2 0 012 2v8a2 2 0 01-2 2H3a2 2 0 01-2-2V8a2 2 0 012-2z' },
  { id: 'devices',  icon: 'M3 18v-6a9 9 0 0 1 18 0v6M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z' },
  { id: 'settings', icon: 'M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4' },
]
</script>

<template>
  <nav class="sidebar">
    <div class="nav-items">
      <button
        v-for="item in items"
        :key="item.id"
        class="nav-item"
        :class="{ active: active === item.id }"
        :data-tour="`nav-${item.id}`"
        @click="emit('navigate', item.id)"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path :d="item.icon" />
        </svg>
        <span>{{ t(`nav.${item.id}`) }}</span>
      </button>
    </div>
    <div class="sidebar-footer">
      <div class="sidebar-tip">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="tip-icon">
          <circle cx="12" cy="12" r="10"/>
          <line x1="12" y1="16" x2="12" y2="12"/>
          <line x1="12" y1="8" x2="12.01" y2="8"/>
        </svg>
        <span class="tip-text"><span class="tip-text-inner">{{ t('sidebar.tip') }}</span></span>
      </div>
    </div>
  </nav>
</template>

<style scoped>
.sidebar {
  width: var(--sidebar-w);
  min-width: var(--sidebar-w);
  background: var(--bg-surface);
  border-inline-end: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  padding: 8px 0;
}
.nav-items {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px;
}
.nav-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 14px;
  border-radius: var(--radius);
  cursor: pointer;
  color: var(--text-sec);
  border: 1px solid transparent;
  font-size: 13.5px;
  font-weight: 500;
  background: transparent;
  transition: all .15s;
  text-align: start;
}
.nav-item svg { width: 18px; height: 18px; flex-shrink: 0; }
.nav-item:hover { background: color-mix(in srgb, var(--accent) 10%, transparent); color: var(--accent); }
.nav-item.active {
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  color: var(--text);
  border-color: var(--accent);
}
.nav-item.active svg { color: var(--accent); }
.sidebar-footer {
  padding: 12px 14px;
  border-top: 1px solid var(--border);
}
.daemon-status {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-sec);
}
.dot {
  width: 8px; height: 8px;
  border-radius: 50%;
  background: var(--text-muted);
}
.dot.connected {
  background: var(--success);
  box-shadow: 0 0 6px var(--success);
}
.sidebar-tip {
  display: flex;
  align-items: flex-start;
  gap: 7px;
  font-size: 11px;
  color: var(--text-muted);
  line-height: 1.5;
  user-select: none !important;
}
.tip-icon {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
  margin-top: 1px;
  color: var(--accent);
  opacity: 0.7;
}
.tip-text {
  flex: 1;
  overflow: hidden;
  white-space: nowrap;
  display: flex;
  align-items: center;
  min-width: 0;
}
.tip-text-inner {
  display: inline-block;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  will-change: transform;
  transition: transform 0.3s ease, opacity 0.3s ease;
  transform: translateX(0);
  opacity: 1;
}
.tip-text:hover .tip-text-inner {
  overflow: visible;
  text-overflow: initial;
  animation: tip-marquee 8s linear infinite;
  opacity: 0;
}
@keyframes tip-marquee {
  0%   { transform: translateX(0);     opacity: 0; }
  5%   { opacity: 1; }
  45%  { transform: translateX(-100%); opacity: 1; }
  50%  { transform: translateX(-100%); opacity: 0; }
  55%  { transform: translateX(calc(-100% - 4px)); opacity: 0; }
  95%  { opacity: 1; }
  100% { transform: translateX(0);     opacity: 0; }
}
</style>

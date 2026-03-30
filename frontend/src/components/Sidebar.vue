<script setup lang="ts">
defineProps<{ active: string }>()
const emit = defineEmits<{ navigate: [page: string] }>()

const items = [
  { id: 'home', label: 'Home', icon: 'M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-4 0a1 1 0 01-1-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 01-1 1h-2z' },
  { id: 'mixer', label: 'Mixer', icon: 'M4 21V14m0-4V3m8 18V12m0-4V3m8 18V16m0-4V3M1 14h6M9 8h6M17 16h6' },
  { id: 'clips', label: 'Clips', icon: 'M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M3 6h10a2 2 0 012 2v8a2 2 0 01-2 2H3a2 2 0 01-2-2V8a2 2 0 012-2z' },
  { id: 'devices', label: 'Devices', icon: 'M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z' },
  { id: 'settings', label: 'Settings', icon: 'M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4' },
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
        @click="emit('navigate', item.id)"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path :d="item.icon" />
        </svg>
        <span>{{ item.label }}</span>
      </button>
    </div>
    <div class="sidebar-footer">
      <div class="daemon-status">
        <span class="dot connected"></span>
        <span>Core Active</span>
      </div>
    </div>
  </nav>
</template>

<style scoped>
.sidebar {
  width: var(--sidebar-w);
  min-width: var(--sidebar-w);
  background: var(--bg-surface);
  border-right: 1px solid var(--border);
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
  text-align: left;
}
.nav-item svg { width: 18px; height: 18px; flex-shrink: 0; }
.nav-item:hover { background: var(--bg-hover); color: var(--text); }
.nav-item.active {
  background: rgba(233,69,96,.08);
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
</style>

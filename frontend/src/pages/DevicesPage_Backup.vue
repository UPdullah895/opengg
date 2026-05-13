<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '../stores/devices'
import type { DeviceInfo } from '../stores/devices'
import DeviceCard from '../components/DeviceCard.vue'
import MouseSettings from '../components/MouseSettings.vue'
import HeadsetSettings from '../components/HeadsetSettings.vue'
import PageHeader from '../components/PageHeader.vue'
import { ICON_GRID, ICON_LIST } from '../assets/deviceAssets'

const { t } = useI18n()
const store = useDeviceStore()

type View = 'grid' | 'mouse' | 'headset'
const activeView = ref<View>('grid')
const selectedDevice = ref<DeviceInfo | null>(null)
const listMode = ref(false)

onMounted(async () => {
  await store.fetchDevices()
})

function openDevice(device: DeviceInfo) {
  selectedDevice.value = device
  activeView.value = device.deviceType === 'mouse' ? 'mouse' : 'headset'
}

function goBack() {
  activeView.value = 'grid'
  selectedDevice.value = null
}

const currentDevice = computed(() =>
  selectedDevice.value
    ? store.devices.find(d => d.id === selectedDevice.value!.id) ?? selectedDevice.value
    : null
)

const headerBattery = computed(() => {
  const d = currentDevice.value
  if (!d || activeView.value !== 'headset') return null
  if (d.batteryLevel === undefined || d.batteryLevel < 0) return null
  return d.batteryLevel
})

const headerBatteryColor = computed(() => {
  const lvl = headerBattery.value ?? 50
  if (lvl > 60) return 'var(--success)'
  if (lvl > 20) return 'var(--accent)'
  return '#ef4444'
})
</script>

<template>
  <div class="devices-page">
    <!-- Grid view -->
    <template v-if="activeView === 'grid'">
      <PageHeader :title="t('devices.title')">
        <!-- Segmented view-toggle pill -->
        <div class="view-toggle">
          <button class="vt-btn" :class="{ active: !listMode }" @click="listMode = false" :title="t('devices.viewGrid')">
            <span v-html="ICON_GRID" />
          </button>
          <button class="vt-btn" :class="{ active: listMode }" @click="listMode = true" :title="t('devices.viewList')">
            <span v-html="ICON_LIST" />
          </button>
        </div>
        <div class="sep" />
        <button class="action-btn" @click="store.fetchDevices()" :disabled="store.loading">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" :class="{ spinning: store.loading }">
            <path d="M21 2v6h-6M3 12a9 9 0 0 1 15-6.7L21 8M3 22v-6h6M21 12a9 9 0 0 1-15 6.7L3 16" />
          </svg>
          {{ t('devices.refresh') }}
        </button>
      </PageHeader>

      <div v-if="store.error" class="error-banner">{{ store.error }}</div>

      <!-- Loading skeleton -->
      <div v-if="store.loading && store.devices.length === 0" :class="listMode ? 'devices-list' : 'device-grid'">
        <div v-for="i in 4" :key="i" class="skeleton-card">
          <div class="sk-header" />
          <div v-if="!listMode" class="sk-image" />
          <div class="sk-footer" />
        </div>
      </div>

      <!-- Empty state -->
      <div v-else-if="!store.loading && store.devices.length === 0" class="empty-state">
        <div class="empty-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M3 18v-6a9 9 0 0 1 18 0v6M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"/>
          </svg>
        </div>
        <p class="empty-title">{{ t('devices.noDevices') }}</p>
        <p class="empty-desc">{{ t('devices.noDevicesDesc') }}</p>
      </div>

      <!-- Device grid/list -->
      <div v-else :class="listMode ? 'devices-list' : 'device-grid'">
        <DeviceCard
          v-for="device in store.devices"
          :key="device.id"
          :device="device"
          :mode="listMode ? 'list' : 'grid'"
          @open-settings="openDevice(device)"
        />
      </div>
    </template>

    <!-- Settings view -->
    <template v-else>
      <PageHeader :title="currentDevice?.name ?? ''">
        <!-- Battery badge in header -->
        <span v-if="headerBattery !== null" class="header-battery" :style="{ color: headerBatteryColor }">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="14" height="14">
            <rect x="2" y="7" width="18" height="11" rx="2"/><path d="M22 11v3"/>
          </svg>
          {{ headerBattery }}%
        </span>
        <button class="action-btn" @click="goBack()">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M19 12H5M12 5l-7 7 7 7" />
          </svg>
          {{ t('devices.back') }}
        </button>
      </PageHeader>

      <div v-if="currentDevice" class="settings-wrap">
        <MouseSettings v-if="activeView === 'mouse'" :device="currentDevice" />
        <HeadsetSettings v-else-if="activeView === 'headset'" :device="currentDevice" />
      </div>
    </template>
  </div>
</template>

<style scoped>
.devices-page {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 20px;
  overflow-y: auto;
}
.sep {
  width: 1px;
  height: 20px;
  background: var(--border);
  margin: 0 2px;
}
.action-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 7px 14px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--bg-surface);
  color: var(--text-sec);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: color .15s, border-color .15s;
}
.action-btn:hover:not(:disabled) { color: var(--accent); border-color: var(--accent); }
.action-btn:disabled { opacity: .5; cursor: not-allowed; }
.action-btn svg { width: 14px; height: 14px; }
.spinning { animation: spin .7s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }

.header-battery {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 13px;
  font-weight: 700;
}

.error-banner {
  background: color-mix(in srgb, #ef4444 12%, transparent);
  border: 1px solid color-mix(in srgb, #ef4444 30%, transparent);
  color: #ef4444;
  padding: 10px 14px;
  border-radius: 8px;
  font-size: 13px;
}

.device-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 18px;
}

.devices-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

/* Skeleton */
.skeleton-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 14px;
  aspect-ratio: 1 / 1;
  padding: 18px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.sk-header { height: 18px; border-radius: 6px; background: var(--bg-surface); animation: pulse 1.4s ease-in-out infinite; }
.sk-image { height: 160px; border-radius: 8px; background: var(--bg-surface); animation: pulse 1.4s ease-in-out infinite .15s; }
.sk-footer { height: 28px; border-radius: 6px; background: var(--bg-surface); animation: pulse 1.4s ease-in-out infinite .3s; }
@keyframes pulse { 0%, 100% { opacity: .5; } 50% { opacity: 1; } }

/* Empty state */
.empty-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 64px 32px;
  text-align: center;
}
.empty-icon {
  width: 64px; height: 64px;
  border-radius: 16px;
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
  display: flex; align-items: center; justify-content: center;
  color: var(--accent);
  margin-bottom: 8px;
}
.empty-icon svg { width: 30px; height: 30px; }
.empty-title { font-size: 16px; font-weight: 700; color: var(--text); margin: 0; }
.empty-desc { font-size: 13px; color: var(--text-muted); margin: 0; max-width: 340px; }

.settings-wrap {
  flex: 1;
  overflow-y: auto;
}

/* ── Segmented view-toggle (Clips-style) ── */
.view-toggle {
  display: flex;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  overflow: hidden;
}
.vt-btn {
  width: 34px;
  height: 34px;
  background: var(--bg-input);
  border: 1px solid transparent;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 200ms ease-in-out;
}
/* Explicitly center the v-html SVG spans */
.vt-btn > span {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  height: 14px;
}
.vt-btn :deep(svg) { width: 14px; height: 14px; display: block; }
.vt-btn:hover {
  background: var(--color-accent-alpha-10);
  border: 1px solid var(--color-accent-alpha-50);
  color: var(--accent);
}
.vt-btn.active {
  background: color-mix(in srgb, var(--accent) 20%, transparent);
  border: 1px solid var(--color-accent-alpha-50);
  color: var(--accent);
}
</style>

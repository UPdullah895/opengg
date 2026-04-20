<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { DeviceInfo } from '../stores/devices'
import { getDeviceImage, trimDeviceName, ICON_GEAR, ICON_BATTERY, ICON_BOLT } from '../assets/deviceAssets'

const { t } = useI18n()
const props = defineProps<{
  device: DeviceInfo
  mode?: 'grid' | 'list'
}>()
const emit = defineEmits<{ 'open-settings': [] }>()

const imgSrc = computed(() => getDeviceImage(props.device.vid, props.device.pid, props.device.deviceType))

const batteryColor = computed(() => {
  const lvl = props.device.batteryLevel ?? -1
  if (lvl < 0) return 'var(--text-muted)'
  if (lvl > 60) return 'var(--success)'
  if (lvl > 20) return 'var(--accent)'
  return '#ef4444'
})
</script>

<template>
  <div class="device-card" :class="mode ?? 'grid'" @click="emit('open-settings')">
    <!-- List layout -->
    <template v-if="mode === 'list'">
      <div class="list-image">
        <img v-if="imgSrc" :src="imgSrc" :alt="device.name" draggable="false" />
        <div v-else class="list-icon">
          <svg v-if="device.deviceType === 'headset'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M3 18v-6a9 9 0 0 1 18 0v6M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"/>
          </svg>
          <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <rect x="5" y="2" width="14" height="20" rx="7"/>
            <line x1="12" y1="2" x2="12" y2="10"/>
          </svg>
        </div>
      </div>
      <div class="list-body">
        <span class="device-name">{{ trimDeviceName(device.name) }}</span>
        <span class="type-badge">{{ t(`devices.${device.deviceType}`) }}</span>
      </div>
      <div class="list-meta">
        <div v-if="device.deviceType === 'headset' && device.batteryLevel !== undefined && device.batteryLevel >= 0" class="battery" :style="{ color: batteryColor }">
          <span v-html="ICON_BATTERY" class="bat-icon" />
          <span class="bat-level">{{ device.batteryLevel }}%</span>
          <span v-if="device.batteryCharging" v-html="ICON_BOLT" class="bat-bolt" />
        </div>
        <div v-else-if="device.deviceType === 'mouse' && device.dpi !== undefined" class="dpi-info">
          {{ device.dpi }} DPI
        </div>
      </div>
      <button class="gear-btn" @click.stop="emit('open-settings')" :title="t('devices.settings')">
        <span v-html="ICON_GEAR" />
      </button>
    </template>

    <!-- Grid layout -->
    <template v-else>
      <div class="card-header">
        <span class="device-name">{{ trimDeviceName(device.name) }}</span>
        <span class="type-badge">{{ t(`devices.${device.deviceType}`) }}</span>
      </div>

      <div class="card-image">
        <img v-if="imgSrc" :src="imgSrc" :alt="device.name" draggable="false" />
        <div v-else class="device-icon">
          <svg v-if="device.deviceType === 'headset'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M3 18v-6a9 9 0 0 1 18 0v6M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"/>
          </svg>
          <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <rect x="5" y="2" width="14" height="20" rx="7"/>
            <line x1="12" y1="2" x2="12" y2="10"/>
          </svg>
        </div>
      </div>

      <div class="card-footer">
        <div v-if="device.deviceType === 'headset' && device.batteryLevel !== undefined && device.batteryLevel >= 0" class="battery" :style="{ color: batteryColor }">
          <span v-html="ICON_BATTERY" class="bat-icon" />
          <span class="bat-level">{{ device.batteryLevel }}%</span>
          <span v-if="device.batteryCharging" v-html="ICON_BOLT" class="bat-bolt" />
        </div>
        <div v-else-if="device.deviceType === 'mouse' && device.dpi !== undefined" class="dpi-info">
          {{ device.dpi }} DPI
        </div>
        <div class="spacer" />
        <button class="gear-btn" @click.stop="emit('open-settings')" :title="t('devices.settings')">
          <span v-html="ICON_GEAR" />
        </button>
      </div>
    </template>
  </div>
</template>

<style scoped>
/* ── Base ── */
.device-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 14px;
  cursor: pointer;
  transition: border-color .15s, transform .15s, box-shadow .15s;
  user-select: none;
}
.device-card:hover {
  border-color: var(--accent);
  box-shadow: 0 4px 20px color-mix(in srgb, var(--accent) 15%, transparent);
}

/* ── Grid card ── */
.device-card.grid {
  padding: 18px;
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-height: 280px;
}
.device-card.grid:hover { transform: translateY(-2px); }

.card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.device-name {
  font-size: 15px;
  font-weight: 700;
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.type-badge {
  font-size: 10px;
  font-weight: 600;
  letter-spacing: .5px;
  text-transform: uppercase;
  color: var(--accent);
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
  padding: 2px 8px;
  border-radius: 20px;
  flex-shrink: 0;
  width: fit-content;
}
.card-image {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 160px;
  border-radius: 10px;
  background: var(--bg-surface);
  overflow: hidden;
}
.card-image img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
}
.device-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  color: var(--accent);
}
.device-icon svg { width: 56px; height: 56px; opacity: 0.7; }

.card-footer {
  display: flex;
  align-items: center;
  gap: 8px;
}

/* ── List card ── */
.device-card.list {
  padding: 16px 18px;
  min-height: 120px;
  display: flex;
  flex-direction: row;
  align-items: center;
  gap: 16px;
}

.list-image {
  width: 92px;
  height: 92px;
  border-radius: 12px;
  background: var(--bg-surface);
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}
.list-image img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}
.list-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  color: var(--accent);
}
.list-icon svg { width: 36px; height: 36px; opacity: 0.7; }

.list-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 6px;
  overflow: hidden;
}

.list-meta {
  display: flex;
  align-items: center;
  flex-shrink: 0;
}

/* ── Shared ── */
.battery {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 14px;
  font-weight: 600;
}
.bat-icon { width: 16px; height: 16px; display: flex; align-items: center; }
.bat-icon :deep(svg) { width: 16px; height: 16px; }
.bat-level { font-size: 14px; }
.bat-bolt { width: 14px; height: 14px; display: flex; align-items: center; }
.bat-bolt :deep(svg) { width: 14px; height: 14px; }
.dpi-info {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-sec);
}
.spacer { flex: 1; }
.gear-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--bg-surface);
  cursor: pointer;
  color: var(--text-sec);
  transition: color .15s, background .15s;
  flex-shrink: 0;
}
.gear-btn:hover {
  color: var(--accent);
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  border-color: color-mix(in srgb, var(--accent) 30%, transparent);
}
.gear-btn :deep(svg) { width: 16px; height: 16px; }
</style>

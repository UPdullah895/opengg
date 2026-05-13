<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '../stores/devices'
import type { DeviceInfo } from '../stores/devices'
import { getDeviceImage, trimDeviceName } from '../assets/deviceAssets'
import MouseMacros from './MouseMacros.vue'

const { t } = useI18n()
const props = defineProps<{ device: DeviceInfo }>()
const store = useDeviceStore()

const dpiInput = ref(props.device.dpi ?? 800)
const pollingRate = ref(props.device.pollingRate ?? 1000)

watch(() => props.device.dpi, v => { if (v !== undefined) dpiInput.value = v })
watch(() => props.device.pollingRate, v => { if (v !== undefined) pollingRate.value = v })

const pollingOptions = [125, 250, 500, 1000]

function onDpiChange() {
  store.setMouseDpi(props.device.id, dpiInput.value)
}

function onPollingChange(rate: number) {
  pollingRate.value = rate
  store.setMousePollingRate(props.device.id, rate)
}

const dpiMin = 100
const dpiMax = 25600

const imgSrc = computed(() => getDeviceImage(props.device.vid, props.device.pid, 'mouse'))
const hasRealImage = computed(() => !imgSrc.value.startsWith('data:'))
</script>

<template>
  <div class="mouse-settings">
    <div class="settings-panel">
      <section class="setting-section">
        <h3 class="section-title">{{ t('devices.dpi') }}</h3>
        <div class="dpi-row">
          <input
            type="range"
            :min="dpiMin"
            :max="dpiMax"
            step="50"
            v-model.number="dpiInput"
            @change="onDpiChange"
            class="dpi-slider"
          />
          <input
            type="number"
            :min="dpiMin"
            :max="dpiMax"
            step="50"
            v-model.number="dpiInput"
            @change="onDpiChange"
            class="dpi-number"
          />
        </div>
        <div v-if="device.dpiOptions && device.dpiOptions.length" class="dpi-presets">
          <button
            v-for="opt in device.dpiOptions"
            :key="opt"
            class="preset-btn"
            :class="{ active: dpiInput === opt }"
            @click="() => { dpiInput = opt; onDpiChange() }"
          >{{ opt }}</button>
        </div>
      </section>

      <section class="setting-section">
        <h3 class="section-title">{{ t('devices.pollingRate') }}</h3>
        <div class="polling-row">
          <button
            v-for="rate in pollingOptions"
            :key="rate"
            class="poll-btn"
            :class="{ active: pollingRate === rate }"
            @click="onPollingChange(rate)"
          >{{ rate }} Hz</button>
        </div>
      </section>

      <section class="setting-section">
        <h3 class="section-title">{{ t('devices.macros') }}</h3>
        <MouseMacros :device="device" />
      </section>
    </div>

    <div class="hero-panel">
      <div class="hero-image">
        <img v-if="hasRealImage" :src="imgSrc" :alt="device.name" draggable="false" />
        <div v-else class="hero-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <rect x="5" y="2" width="14" height="20" rx="7"/>
            <line x1="12" y1="2" x2="12" y2="10"/>
          </svg>
        </div>
      </div>
      <div class="hero-footer">
        <span class="hero-name">{{ trimDeviceName(device.name) }}</span>
        <span class="hero-type">{{ t('devices.mouse') }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.mouse-settings {
  display: grid;
  grid-template-columns: 1fr 300px;
  gap: 24px;
  align-items: start;
}
.settings-panel {
  display: flex;
  flex-direction: column;
  gap: 24px;
}
.setting-section {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.section-title {
  font-size: 13px;
  font-weight: 700;
  color: var(--text-sec);
  text-transform: uppercase;
  letter-spacing: .5px;
  margin: 0;
}
.dpi-row {
  display: flex;
  align-items: center;
  gap: 12px;
}
.dpi-slider {
  flex: 1;
  accent-color: var(--accent);
  height: 4px;
  cursor: pointer;
}
.dpi-number {
  width: 80px;
  background: var(--bg-surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  color: var(--text);
  padding: 6px 10px;
  font-size: 13px;
  text-align: center;
}
.dpi-number:focus { outline: none; border-color: var(--accent); }
.dpi-presets {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.preset-btn, .poll-btn {
  padding: 5px 12px;
  border-radius: 20px;
  border: 1px solid var(--border);
  background: var(--bg-surface);
  color: var(--text-sec);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: all .15s;
}
.preset-btn:hover, .poll-btn:hover { color: var(--text); border-color: var(--accent); }
.preset-btn.active, .poll-btn.active {
  background: color-mix(in srgb, var(--accent) 15%, transparent);
  color: var(--accent);
  border-color: var(--accent);
}
.polling-row {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

/* ── Hero panel ── */
.hero-panel {
  position: sticky;
  top: 0;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 16px;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.hero-image {
  flex: 1;
  min-height: 200px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-surface);
  padding: 24px;
}
.hero-image img {
  max-width: 100%;
  max-height: 180px;
  object-fit: contain;
}
.hero-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
}
.hero-icon svg {
  width: 64px;
  height: 64px;
  opacity: .5;
  color: var(--accent);
}
.hero-footer {
  padding: 16px;
  border-top: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.hero-name {
  font-size: 14px;
  font-weight: 800;
  color: var(--text);
}
.hero-type {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: .5px;
  color: var(--accent);
}
</style>

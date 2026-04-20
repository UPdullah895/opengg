<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '../stores/devices'
import type { DeviceInfo } from '../stores/devices'
import { getDeviceImage, ICON_AUDIO, ICON_POWER, ICON_VOLUME, ICON_BLUETOOTH } from '../assets/deviceAssets'

const { t } = useI18n()
const props = defineProps<{ device: DeviceInfo }>()
const store = useDeviceStore()

const sidetone = ref(props.device.sidetone ?? 0)
watch(() => props.device.sidetone, v => { if (v !== undefined) sidetone.value = v })

const caps = computed(() => new Set(props.device.capabilities ?? []))
const local = computed(() => store.getHeadsetLocal(props.device.id))
const isSyncing = computed(() => store.syncing.has(props.device.id))

const AUTO_OFF_OPTIONS = [0, 5, 10, 15, 30, 60, 90]

function autoOffLabel(val: number) {
  return val === 0 ? t('devices.autoOffNever') : t('devices.autoOffMinutes', { n: val })
}

function onSidetoneChange() {
  store.setHeadsetSidetone(props.device.id, sidetone.value)
}

// Custom dropdown state
const autoOffOpen = ref(false)
const autoOffSelected = computed(() => local.value.inactiveTime)
function setAutoOff(val: number) {
  store.setHeadsetInactiveTime(props.device.id, val)
  autoOffOpen.value = false
}
function toggleAutoOff() { autoOffOpen.value = !autoOffOpen.value }
function onAutoOffClickOutside() {
  if (autoOffOpen.value) autoOffOpen.value = false
}

// Close on outside click
import { onMounted, onBeforeUnmount } from 'vue'
onMounted(() => document.addEventListener('click', onAutoOffClickOutside))
onBeforeUnmount(() => document.removeEventListener('click', onAutoOffClickOutside))

const deviceImage = computed(() => getDeviceImage(props.device.vid, props.device.pid, props.device.deviceType))
</script>

<template>
  <div class="headset-settings" :style="isSyncing ? 'opacity: 0.6; pointer-events: none' : ''">
    <div class="settings-layout">

      <!-- ── Left: Controls ───────────────────────────────────────────── -->
      <div class="settings-panel">
        <div class="settings-grid">

          <!-- Sidetone -->
          <section v-if="caps.has('sidetone')" class="setting-section">
            <div class="setting-title">
              <span class="title-icon" v-html="ICON_AUDIO" />
              <h3 class="section-title">{{ t('devices.sidetone') }}</h3>
            </div>
            <div class="slider-row">
              <span class="slider-label">0</span>
              <input
                type="range" min="0" max="128" step="1"
                v-model.number="sidetone"
                @change="onSidetoneChange"
                class="setting-slider"
              />
              <span class="slider-label">128</span>
              <span class="slider-value">{{ sidetone }}</span>
            </div>
          </section>

          <!-- Auto-Off — custom dropdown -->
          <section v-if="caps.has('inactive_time')" class="setting-section">
            <div class="setting-title">
              <span class="title-icon" v-html="ICON_POWER" />
              <h3 class="section-title">{{ t('devices.autoOff') }}</h3>
            </div>
            <div class="custom-select-wrap" @click.stop>
              <button class="custom-select" @click="toggleAutoOff" :class="{ open: autoOffOpen }">
                <span>{{ autoOffLabel(autoOffSelected) }}</span>
                <svg class="chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <polyline points="6 9 12 15 18 9"/>
                </svg>
              </button>
              <Transition name="dropdown">
                <div v-if="autoOffOpen" class="custom-dropdown">
                  <button
                    v-for="v in AUTO_OFF_OPTIONS" :key="v"
                    class="dropdown-option"
                    :class="{ selected: v === autoOffSelected }"
                    @click="setAutoOff(v)"
                  >{{ autoOffLabel(v) }}</button>
                </div>
              </Transition>
            </div>
          </section>

          <!-- Volume Limiter -->
          <section v-if="caps.has('volume_limiter')" class="setting-section">
            <div class="setting-title">
              <span class="title-icon" v-html="ICON_VOLUME" />
              <h3 class="section-title" style="text-transform:none;letter-spacing:0;font-size:14px">{{ t('devices.volumeLimiter') }}</h3>
            </div>
            <div class="toggle-row">
              <button
                class="toggle-btn"
                :class="{ on: local.volumeLimiter }"
                @click="store.setHeadsetVolumeLimiter(device.id, !local.volumeLimiter)"
              />
            </div>
          </section>

          <!-- BT When Powered On -->
          <section v-if="caps.has('bt_when_powered_on')" class="setting-section">
            <div class="setting-title">
              <span class="title-icon" v-html="ICON_BLUETOOTH" />
              <h3 class="section-title" style="text-transform:none;letter-spacing:0;font-size:14px">{{ t('devices.btPoweredOn') }}</h3>
            </div>
            <div class="toggle-row">
              <button
                class="toggle-btn"
                :class="{ on: local.btPoweredOn }"
                @click="store.setHeadsetBtPoweredOn(device.id, !local.btPoweredOn)"
              />
            </div>
          </section>

          <!-- BT Call Volume -->
          <section v-if="caps.has('bt_call_volume')" class="setting-section">
            <div class="setting-title">
              <span class="title-icon" v-html="ICON_VOLUME" />
              <h3 class="section-title">{{ t('devices.btCallVolume') }}</h3>
            </div>
            <div class="slider-row">
              <span class="slider-label">0</span>
              <input
                type="range" min="0" max="100" step="1"
                :value="local.btCallVolume"
                @change="store.setHeadsetBtCallVolume(device.id, +($event.target as HTMLInputElement).value)"
                class="setting-slider"
              />
              <span class="slider-label">100</span>
              <span class="slider-value">{{ local.btCallVolume }}</span>
            </div>
          </section>

        </div><!-- /.settings-grid -->
      </div><!-- /.settings-panel -->

      <!-- ── Right: Device image hero ─────────────────────────────────── -->
      <div class="device-hero">
        <img :src="deviceImage" :alt="device.name" class="hero-img" />
        <div class="hero-overlay">
          <span class="hero-name">{{ device.name }}</span>
          <span class="hero-type">{{ t(`devices.${device.deviceType}`) }}</span>
        </div>
      </div>

    </div><!-- /.settings-layout -->
  </div>
</template>

<style scoped>
.headset-settings {
  transition: opacity .2s;
}

/* ── Split-screen layout ── */
.settings-layout {
  display: grid;
  grid-template-columns: 1fr 380px;
  gap: 24px;
  align-items: start;
}

/* ── Settings grid ── */
.settings-panel { min-width: 0; }

.settings-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  gap: 16px;
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

/* ── Setting title with icon ── */
.setting-title {
  display: flex;
  align-items: center;
  gap: 10px;
}
.title-icon {
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  color: var(--accent);
  flex-shrink: 0;
}
.title-icon :deep(svg) { width: 20px; height: 20px; }

.section-title {
  font-size: 13px;
  font-weight: 700;
  color: var(--text-sec);
  text-transform: uppercase;
  letter-spacing: .5px;
  margin: 0;
}

/* ── Slider ── */
.slider-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.setting-slider {
  flex: 1;
  accent-color: var(--accent);
  cursor: pointer;
}
.slider-label {
  font-size: 11px;
  color: var(--text-muted);
  min-width: 20px;
  text-align: center;
}
.slider-value {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
  min-width: 48px;
  text-align: right;
}

/* ── Toggle ── */
.toggle-row {
  display: flex;
  align-items: center;
}
.toggle-btn {
  width: 42px;
  height: 24px;
  border-radius: 12px;
  border: none;
  cursor: pointer;
  background: var(--bg-surface);
  transition: background .2s;
  position: relative;
  flex-shrink: 0;
}
.toggle-btn.on { background: var(--accent); }
.toggle-btn::after {
  content: '';
  position: absolute;
  top: 3px;
  left: 3px;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: white;
  transition: transform .2s;
}
.toggle-btn.on::after { transform: translateX(18px); }

/* ── Custom dropdown ── */
.custom-select-wrap {
  position: relative;
  width: 100%;
}
.custom-select {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 10px 14px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--bg-surface);
  color: var(--text);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: border-color .15s, background .15s;
  text-align: left;
}
.custom-select:hover,
.custom-select.open {
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 6%, var(--bg-surface));
}
.chevron {
  width: 14px;
  height: 14px;
  color: var(--text-muted);
  transition: transform .2s;
  flex-shrink: 0;
}
.custom-select.open .chevron { transform: rotate(180deg); }

.custom-dropdown {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: hidden;
  z-index: 200;
  box-shadow: 0 8px 24px rgba(0,0,0,.4);
}
.dropdown-option {
  width: 100%;
  padding: 10px 14px;
  background: none;
  border: none;
  color: var(--text-sec);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  text-align: left;
  transition: background .1s, color .1s;
}
.dropdown-option:hover {
  background: var(--bg-hover);
  color: var(--text);
}
.dropdown-option.selected {
  color: var(--accent);
  font-weight: 700;
}

/* Dropdown transition */
.dropdown-enter-active, .dropdown-leave-active { transition: opacity .15s, transform .15s; }
.dropdown-enter-from, .dropdown-leave-to { opacity: 0; transform: translateY(-4px); }

/* ── Device hero (right panel) ── */
.device-hero {
  position: sticky;
  top: 0;
  border-radius: 16px;
  overflow: hidden;
  background: var(--bg-card);
  border: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  min-height: 320px;
}
.hero-img {
  flex: 1;
  width: 100%;
  object-fit: contain;
  background: var(--bg-surface);
  min-height: 260px;
}
.hero-overlay {
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  border-top: 1px solid var(--border);
}
.hero-name {
  font-size: 15px;
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

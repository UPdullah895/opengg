<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { loadDeviceAccessStatus, deviceAccess } from '../composables/useDependencyStatus'

const { t } = useI18n()
const bannerDismissed = ref(false)

onMounted(async () => {
  await loadDeviceAccessStatus()
})

// The Devices feature is not shipped yet, so the device-access setup banner (which tells
// the user to run `./dev.sh setup`) is irrelevant noise — suppress it for now. The
// detection logic below stays in place, dormant; re-enable by restoring the real check
// when the Devices feature goes live.
const DEVICES_FEATURE_ENABLED = false
const showBanner = () => {
  if (!DEVICES_FEATURE_ENABLED || bannerDismissed.value) return false
  return !deviceAccess.value.ratbagd_available ||
         !deviceAccess.value.in_input_group ||
         !deviceAccess.value.in_audio_group ||
         !deviceAccess.value.in_video_group ||
         !deviceAccess.value.udev_rules_present
}

const missingItems = () => {
  const items: string[] = []
  if (!deviceAccess.value.ratbagd_available) items.push(t('devices.access.ratbagd'))
  if (!deviceAccess.value.in_input_group) items.push(t('devices.access.inputGroup'))
  if (!deviceAccess.value.in_audio_group) items.push(t('devices.access.audioGroup'))
  if (!deviceAccess.value.in_video_group) items.push(t('devices.access.videoGroup'))
  if (!deviceAccess.value.udev_rules_present) items.push(t('devices.access.udevRules'))
  return items
}
</script>

<template>
  <div class="devices-page">
    <!-- Device Access Guidance Banner -->
    <div v-if="showBanner()" class="access-banner">
      <div class="banner-content">
        <div class="banner-icon">⚠️</div>
        <div class="banner-text">
          <h3 class="banner-title">{{ t('devices.access.title') }}</h3>
          <p class="banner-desc">{{ t('devices.access.description') }}</p>
          <div class="missing-list">
            <div v-for="item in missingItems()" :key="item" class="missing-item">{{ item }}</div>
          </div>
          <div class="banner-command">
            <p class="command-label">{{ t('devices.access.runSetup') }}</p>
            <code class="command-code">./dev.sh setup</code>
          </div>
        </div>
        <button class="banner-close" @click="bannerDismissed = true" aria-label="Dismiss">×</button>
      </div>
    </div>

    <div class="devices-placeholder">
      <svg
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.2"
        class="devices-icon"
        aria-hidden="true"
      >
        <path d="M3 18v-6a9 9 0 0118 0v6" />
        <path d="M21 19a2 2 0 01-2 2h-1a2 2 0 01-2-2v-3a2 2 0 012-2h3z" />
        <path d="M3 19a2 2 0 002 2h1a2 2 0 002-2v-3a2 2 0 00-2-2H3z" />
      </svg>
      <h2 class="placeholder-title">{{ t('devices.comingSoon') }}</h2>
      <p class="placeholder-desc">{{ t('devices.comingSoonDesc') }}</p>
    </div>
  </div>
</template>

<style scoped>
.devices-page {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.access-banner {
  background: color-mix(in srgb, var(--warning, #f59e0b) 8%, var(--bg-card));
  border-bottom: 1px solid color-mix(in srgb, var(--warning, #f59e0b) 30%, var(--border));
  padding: 16px;
  flex-shrink: 0;
}

.banner-content {
  display: flex;
  gap: 16px;
  max-width: 1200px;
  margin: 0 auto;
  width: 100%;
  position: relative;
}

.banner-icon {
  font-size: 20px;
  flex-shrink: 0;
  margin-top: 2px;
}

.banner-text {
  flex: 1;
}

.banner-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
  margin: 0 0 6px 0;
}

.banner-desc {
  font-size: 13px;
  color: var(--text-secondary);
  margin: 0 0 10px 0;
  line-height: 1.5;
}

.missing-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 0 0 10px 0;
}

.missing-item {
  font-size: 12px;
  color: var(--text-secondary);
  padding-left: 20px;
  position: relative;
}

.missing-item::before {
  content: '•';
  position: absolute;
  left: 8px;
}

.banner-command {
  margin: 0;
  padding: 10px;
  background: var(--bg-secondary, rgba(0, 0, 0, 0.2));
  border-radius: 4px;
  border-left: 2px solid var(--warning, #f59e0b);
}

.command-label {
  font-size: 12px;
  color: var(--text-secondary);
  margin: 0 0 6px 0;
  font-weight: 500;
}

.command-code {
  display: block;
  font-family: monospace;
  font-size: 12px;
  color: var(--text);
  word-break: break-all;
  line-height: 1.4;
}

.banner-close {
  position: absolute;
  top: 0;
  right: 0;
  background: none;
  border: none;
  color: var(--text-secondary);
  font-size: 24px;
  cursor: pointer;
  padding: 4px 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0.6;
  transition: opacity 0.2s;
}

.banner-close:hover {
  opacity: 1;
}

.devices-placeholder-container {
  display: flex;
  align-items: center;
  justify-content: center;
  flex: 1;
}

.devices-placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 20px;
  text-align: center;
  padding: 48px;
  max-width: 400px;
  margin: auto;
  height: 100%;
}

.devices-icon {
  width: 80px;
  height: 80px;
  color: var(--accent);
  opacity: 0.25;
}

.placeholder-title {
  font-size: 24px;
  font-weight: 700;
  color: var(--text);
  margin: 0;
  letter-spacing: -0.3px;
}

.placeholder-desc {
  font-size: 14px;
  color: var(--text-muted);
  margin: 0;
  line-height: 1.7;
}
</style>

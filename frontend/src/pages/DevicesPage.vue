<script setup lang="ts">
import { useI18n } from 'vue-i18n'
const { t } = useI18n()
</script>

<template>
  <div class="devices-page">
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
  align-items: center;
  justify-content: center;
  height: 100%;
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

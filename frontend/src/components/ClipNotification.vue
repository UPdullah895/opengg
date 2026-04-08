<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

// Read metadata from URL search params (set by show_clip_notification Rust command)
const params = new URLSearchParams(window.location.search)
const game       = ref(params.get('game')     ?? 'Unknown')
const filename   = ref(params.get('filename') ?? '')
const filesizeMb = ref(parseFloat(params.get('filesize') ?? '0'))
const success    = ref(params.get('success') !== '0')

// Fade-out animation: starts at opacity 1, begins fading 500 ms before close (3.5 s in)
const visible = ref(true)
const fading  = ref(false)
onMounted(() => {
  setTimeout(() => { fading.value = true }, 3500)
  setTimeout(() => { visible.value = false }, 4100)
})

const statusLabel = computed(() => success.value ? t('notification.clipSaved') : t('notification.clipFailed'))
const fileSizeStr = computed(() => {
  if (filesizeMb.value <= 0) return ''
  return filesizeMb.value >= 1000
    ? `${(filesizeMb.value / 1000).toFixed(1)} GB`
    : `${filesizeMb.value.toFixed(1)} MB`
})
</script>

<template>
  <div v-if="visible" class="notif-root" :class="{ fading }">
    <div class="notif-card" :class="{ success, failure: !success }">
      <!-- Status icon + label -->
      <div class="notif-header">
        <span class="notif-icon">{{ success ? '✓' : '✕' }}</span>
        <span class="notif-status">{{ statusLabel }}</span>
      </div>
      <!-- Metadata rows -->
      <div class="notif-row">
        <span class="notif-key">{{ t('notification.game') }}</span>
        <span class="notif-val">{{ game }}</span>
      </div>
      <div v-if="filename" class="notif-row">
        <span class="notif-key">{{ t('notification.filename') }}</span>
        <span class="notif-val notif-filename">{{ filename }}</span>
      </div>
      <div v-if="fileSizeStr" class="notif-row">
        <span class="notif-key">{{ t('notification.filesize') }}</span>
        <span class="notif-val">{{ fileSizeStr }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* Overlay window: transparent root, card fills bottom-right aligned content */
.notif-root {
  width: 100vw; height: 100vh;
  display: flex; align-items: flex-end; justify-content: flex-end;
  padding: 0; margin: 0;
  background: transparent;
  transition: opacity 0.6s ease;
  pointer-events: none;
}
.notif-root.fading { opacity: 0; }

.notif-card {
  width: 360px;
  background: var(--bg-card, #1a1a2e);
  border-radius: 8px;
  border-left: 4px solid var(--accent, #E94560);
  padding: 12px 14px 10px;
  box-shadow: 0 8px 30px rgba(0,0,0,.55);
  display: flex; flex-direction: column; gap: 5px;
  pointer-events: none;
}
.notif-card.failure { border-left-color: #ef4444; }

.notif-header {
  display: flex; align-items: center; gap: 8px;
  margin-bottom: 4px;
}
.notif-icon {
  width: 20px; height: 20px; border-radius: 50%;
  background: var(--accent, #E94560);
  display: flex; align-items: center; justify-content: center;
  font-size: 11px; font-weight: 800; color: #fff; flex-shrink: 0;
  line-height: 1;
}
.notif-card.failure .notif-icon { background: #ef4444; }
.notif-status { font-size: 13px; font-weight: 700; color: var(--text, #e0e0e0); }

.notif-row {
  display: flex; align-items: baseline; gap: 6px;
  font-size: 11px; line-height: 1.4;
  overflow: hidden;
}
.notif-key {
  color: var(--text-muted, #6b7280); flex-shrink: 0;
  min-width: 56px; font-weight: 500;
}
.notif-val {
  color: var(--text-sec, #a0aec0);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.notif-filename { font-family: monospace; font-size: 10px; }
</style>

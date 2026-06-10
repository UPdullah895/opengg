<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { usePersistenceStore } from '../../stores/persistence'
import SelectField from '../SelectField.vue'
import InfoIcon from '../InfoIcon.vue'

const { t } = useI18n()
const persist = usePersistenceStore()

const settings = computed(() => persist.state.settings)

const notificationPositionOptions = [
  { value: 'top-right', label: t('settings.notificationsPage.positionTopRight') },
  { value: 'top-left', label: t('settings.notificationsPage.positionTopLeft') },
  { value: 'bottom-right', label: t('settings.notificationsPage.positionBottomRight') },
  { value: 'bottom-left', label: t('settings.notificationsPage.positionBottomLeft') },
]

defineEmits<{ navigate: [page: string] }>()
</script>

<template>
  <section>
    <h2 class="sec-title">{{ t('settings.notificationsPage.title') }}</h2>

    <!-- Notification Style card -->
    <div class="card">
      <div class="card-head">{{ t('settings.notificationsPage.style') }} <InfoIcon :title="t('settings.notificationsPage.description')" /></div>

      <!-- Style selection grid -->
      <div class="notif-style-grid">
        <div class="notif-option" :class="{ active: settings.notificationStyle === 'auto' }" @click="settings.notificationStyle = 'auto'">
          <div class="notif-option-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 014 10 15.3 15.3 0 01-4 10 15.3 15.3 0 01-4-10 15.3 15.3 0 014-10z"/></svg>
          </div>
          <span class="notif-option-label">{{ t('settings.notificationsPage.styleAuto') }}</span>
        </div>
        <div class="notif-option" :class="{ active: settings.notificationStyle === 'gsr-notify' }" @click="settings.notificationStyle = 'gsr-notify'">
          <div class="notif-option-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>
          </div>
          <span class="notif-option-label">{{ t('settings.notificationsPage.styleGsrNotify') }}</span>
        </div>
        <div class="notif-option" :class="{ active: settings.notificationStyle === 'x11-overlay' }" @click="settings.notificationStyle = 'x11-overlay'">
          <div class="notif-option-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="3" width="20" height="14" rx="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>
          </div>
          <span class="notif-option-label">{{ t('settings.notificationsPage.styleX11Overlay') }}</span>
        </div>
        <div class="notif-option" :class="{ active: settings.notificationStyle === 'system' }" @click="settings.notificationStyle = 'system'">
          <div class="notif-option-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9"/><path d="M13.73 21a2 2 0 01-3.46 0"/></svg>
          </div>
          <span class="notif-option-label">{{ t('settings.notificationsPage.styleSystem') }}</span>
        </div>
        <div class="notif-option" :class="{ active: settings.notificationStyle === 'disabled' }" @click="settings.notificationStyle = 'disabled'">
          <div class="notif-option-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="4.93" y1="4.93" x2="19.07" y2="19.07"/></svg>
          </div>
          <span class="notif-option-label">{{ t('settings.notificationsPage.styleDisabled') }}</span>
        </div>
      </div>
    </div>

    <!-- Position + Duration card — hidden when disabled -->
    <div class="card" v-if="settings.notificationStyle !== 'disabled'">
      <!-- Position -->
      <div class="field">
        <label>{{ t('settings.notificationsPage.position') }} <InfoIcon :title="t('settings.notificationsPage.positionDesc')" /></label>
        <SelectField v-model="settings.notificationPosition" :options="notificationPositionOptions" />
      </div>

      <!-- Duration — only for X11 Overlay -->
      <div class="field notif-duration-field" v-if="settings.notificationStyle === 'x11-overlay'">
        <label>{{ t('settings.notificationsPage.duration') }}: {{ settings.notificationDuration }}s <InfoIcon :title="t('settings.notificationsPage.durationDesc')" /></label>
        <input
          type="range"
          class="notif-duration-slider"
          :value="settings.notificationDuration"
          min="1"
          max="10"
          step="1"
          @input="settings.notificationDuration = Number(($event.target as HTMLInputElement).value)"
        />
      </div>
    </div>
  </section>
</template>

<style scoped>
/* Inherited from parent */
</style>

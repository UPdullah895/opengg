<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { useAudioStore } from '../../stores/audio'
import { usePersistenceStore } from '../../stores/persistence'
import { useModalStore } from '../../stores/modal'
import { useToast } from '../../composables/useToast'
import InfoIcon from '../InfoIcon.vue'
import ToggleSwitch from '../ToggleSwitch.vue'
import VolumeSlider from '../VolumeSlider.vue'
import './settings-shared.css'

const { t } = useI18n()
const audio = useAudioStore()
const persist = usePersistenceStore()
const modal = useModalStore()
const toast = useToast()

// ─── Epic 3: Danger Zone ───
const vaLoading = ref(false)
const dangerMsg = ref('')

async function removeVirtualAudio() {
  modal.showConfirm({
    kind: 'danger',
    title: t('settings.dangerZone.title'),
    message: t('settings.dangerZone.confirmMsg'),
    confirmLabel: t('common.confirmDelete'),
    onConfirm: async () => {
      vaLoading.value = true; dangerMsg.value = ''
      try {
        await invoke('remove_virtual_audio')
        await audio.refreshVirtualAudioStatus()
        if (!audio.virtualAudioReady) {
          toast.success(t('settings.dangerZone.removeVirtualAudio'))
          setTimeout(() => {
            window.dispatchEvent(new CustomEvent('openOnboarding', { detail: { step: 1 } }))
          }, 500)
        }
      } catch (e: any) {
        await audio.refreshVirtualAudioStatus()
        toast.error(String(e))
      } finally {
        vaLoading.value = false
      }
    }
  })
}

async function resetVirtualAudio() {
  modal.showConfirm({
    kind: 'danger',
    title: t('settings.dangerZone.resetVirtualAudio'),
    message: t('settings.dangerZone.resetVirtualAudioDesc'),
    confirmLabel: t('common.confirmDelete'),
    onConfirm: async () => {
      vaLoading.value = true; dangerMsg.value = ''
      try {
        await invoke('remove_virtual_audio')
        await audio.refreshVirtualAudioStatus()
        if (!audio.virtualAudioReady) {
          toast.success(t('settings.dangerZone.removeVirtualAudio'))
          setTimeout(() => {
            window.dispatchEvent(new CustomEvent('openOnboarding', { detail: { step: 1 } }))
          }, 500)
        }
      } catch (e: any) {
        await audio.refreshVirtualAudioStatus()
        toast.error(String(e))
      } finally {
        vaLoading.value = false
      }
    }
  })
}

function createVirtualAudio() {
  modal.showConfirm({
    kind: 'info',
    title: t('settings.dangerZone.createConfirmTitle'),
    message: t('settings.dangerZone.createConfirmMsg'),
    confirmLabel: t('common.confirm'),
    onConfirm: () => {
      window.dispatchEvent(new CustomEvent('openOnboarding', { detail: { step: 1 } }))
    }
  })
}

defineEmits<{ navigate: [page: string] }>()
</script>

<template>
  <section class="settings-section">
    <h2 class="sec-title">{{ t('settings.sections.mixerRouting') }}</h2>

    <!-- ★ Epic 3: Danger Zone -->
    <div class="card danger-zone-card">
      <div class="card-head danger-head">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:15px;height:15px;flex-shrink:0"><path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
        {{ t('settings.dangerZone.title') }}
      </div>
      <!-- Reset Virtual Audio -->
      <div class="danger-action-row">
        <div class="danger-info">
          <span class="danger-label">
            {{ t('settings.dangerZone.resetVirtualAudio') }}
            <InfoIcon :title="t('settings.dangerZone.resetVirtualAudioDesc')" />
          </span>
        </div>
        <button class="danger-icon-btn" :disabled="vaLoading" @click="resetVirtualAudio">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 11-2.12-9.36L23 10"/></svg>
        </button>
      </div>

      <!-- Create Virtual Audio (when not ready) -->
      <div v-if="!audio.virtualAudioReady" class="danger-action-row">
        <div class="danger-info">
          <span class="danger-label">
            {{ t('settings.dangerZone.createVirtualAudio') }}
            <InfoIcon :title="t('settings.dangerZone.createVirtualAudioDesc')" />
          </span>
        </div>
        <button class="btn btn-accent" :disabled="vaLoading" @click="createVirtualAudio">
          <svg v-if="!vaLoading" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="9 11 12 14 22 4"/><path d="M21 12v7a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2h11"/></svg>
          <span>{{ vaLoading ? t('settings.dangerZone.creating') : 'Create' }}</span>
        </button>
      </div>

      <!-- Remove Virtual Audio (when ready) -->
      <div v-else class="danger-action-row">
        <div class="danger-info">
          <span class="danger-label">
            {{ t('settings.dangerZone.removeVirtualAudio') }}
            <InfoIcon :title="t('settings.dangerZone.removeVirtualAudioDesc')" />
          </span>
        </div>
        <button class="danger-icon-btn danger-icon-btn--delete" :disabled="vaLoading" @click="removeVirtualAudio">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6m3 0V4a1 1 0 011-1h4a1 1 0 011 1v2"/></svg>
        </button>
      </div>

      <div v-if="dangerMsg" class="danger-msg" :class="{ 'danger-ok': dangerMsg.startsWith('✓') }">{{ dangerMsg }}</div>
    </div>

    <!-- ★ Ear Blast Protection settings -->
    <div class="card eb-card">
      <div class="card-head">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="width:15px;height:15px;flex-shrink:0"><path d="M3 18v-6a9 9 0 0 1 18 0v6"/><path d="M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3z"/><path d="M3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"/></svg>
        <span>{{ t('settings.earBlast.title') }}</span>
        <InfoIcon :title="t('settings.earBlast.desc')" />
        <div class="card-head-actions">
          <ToggleSwitch :model-value="persist.state.mixer.earBlast.enabled" @update:model-value="audio.toggleEarBlast()" />
        </div>
      </div>

      <div class="field eb-channels">
        <label>{{ t('settings.earBlast.channels') }}</label>
        <div class="channel-checks">
          <button
            v-for="ch in ['Master', 'Game', 'Chat', 'Media', 'Aux', 'Mic']" :key="ch"
            class="channel-pill"
            :class="{ 'channel-pill--active': persist.state.mixer.earBlast.channels.includes(ch) }"
            @click="() => {
              const arr = persist.state.mixer.earBlast.channels.includes(ch)
                ? persist.state.mixer.earBlast.channels.filter((c: string) => c !== ch)
                : [...persist.state.mixer.earBlast.channels, ch]
              audio.setEarBlastChannels(arr)
            }"
          >
            {{ ch }}
          </button>
        </div>
      </div>

      <div class="form-grid">
        <div class="field">
          <label>{{ t('settings.earBlast.threshold') }}</label>
          <VolumeSlider
            :model-value="persist.state.mixer.earBlast.threshold"
            color="var(--accent)"
            :min="1"
            :max="100"
            @update:model-value="v => audio.setEarBlastThreshold(v)"
          />
        </div>
        <div class="field">
          <label>{{ t('settings.earBlast.target') }}</label>
          <VolumeSlider
            :model-value="persist.state.mixer.earBlast.target"
            color="var(--accent)"
            :min="0"
            :max="100"
            @update:model-value="v => audio.setEarBlastTarget(v)"
          />
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* Ear Blast card: even, consistent internal spacing (the shared .card-head has no gap,
   which left the icon/title/InfoIcon cramped). */
.eb-card :deep(.card-head) { gap: 8px; }
.eb-card .eb-channels { margin-top: 4px; }
.eb-card .eb-channels :deep(.channel-checks) { gap: 8px; margin-bottom: 0; }
.eb-card :deep(.channel-pill) { padding: 5px 12px; }
/* Threshold/Target sliders: comfortable column + row gap, separated from the channels row. */
.eb-card :deep(.form-grid) { gap: 16px 18px; margin-top: 16px; }
.eb-card :deep(.field) label { margin-bottom: 8px; }
</style>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { usePersistenceStore } from '../../stores/persistence'
import IconPicker from '../IconPicker.vue'
import InfoIcon from '../InfoIcon.vue'
import './settings-shared.css'

const { t } = useI18n()
const persist = usePersistenceStore()

const settings = computed(() => persist.state.settings)
const colorInputRefs = ref<HTMLInputElement[]>([])

function setColorRef(el: any, idx: number) { if (el) colorInputRefs.value[idx] = el }
function openColorPicker(idx: number) { colorInputRefs.value[idx]?.click() }

function addTrackDef() {
  const idx = settings.value.trackDefs.length
  settings.value.trackDefs.push({ id: `A${idx}`, name: `Audio ${idx}`, color: '#64748b', icon: 'game', visible: true })
}

function removeTrackDef(i: number) {
  if (settings.value.trackDefs.length <= 1) return
  settings.value.trackDefs.splice(i, 1)
}

defineEmits<{ navigate: [page: string] }>()
</script>

<template>
  <section class="settings-section">
    <h2 class="sec-title">{{ t('settings.timelineTracks.title') }}</h2>

    <div class="card">
      <div class="card-head">{{ t('settings.timelineTracks.trackList') }} <InfoIcon :title="t('settings.timelineTracks.trackListHint')" /></div>
      <div class="tdef-list">
        <div v-for="(def, idx) in settings.trackDefs" :key="def.id" class="tdef-row">
          <button
            class="track-vis-btn"
            :class="{ active: def.visible }"
            :title="t('settings.timelineTracks.visibilityTooltip')"
            @click="def.visible = !def.visible"
          >
            <svg v-if="def.visible" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
              <circle cx="12" cy="12" r="3"/>
            </svg>
            <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M17.94 17.94A10.07 10.07 0 0112 20c-7 0-11-8-11-8a18.45 18.45 0 015.06-5.94M9.9 4.24A9.12 9.12 0 0112 4c7 0 11 8 11 8a18.5 18.5 0 01-2.16 3.19m-6.72-1.07a3 3 0 11-4.24-4.24"/>
              <line x1="1" y1="1" x2="23" y2="23"/>
            </svg>
          </button>
          <div class="tdef-swatch" :style="{ background: def.color }" @click="openColorPicker(idx)" title="Pick color"></div>
          <input type="color" :ref="(el) => setColorRef(el, idx)" :value="def.color" class="tdef-color-input tdef-color-hidden"
            @input="def.color = ($event.target as HTMLInputElement).value" />
          <input type="text" v-model="def.name" class="tdef-name-input" :placeholder="def.id" maxlength="20" />
          <IconPicker v-model="def.icon" />
          <button
            class="btn-icon btn-remove"
            :disabled="def.id === 'V1' || def.id === 'O1'"
            :title="def.id === 'V1' ? 'The primary Video track cannot be deleted'
                  : def.id === 'O1' ? 'The Overlays track cannot be deleted'
                  : 'Remove'"
            @click="def.id !== 'V1' && def.id !== 'O1' && removeTrackDef(idx)"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 6L6 18M6 6l12 12"/></svg>
          </button>
        </div>
      </div>
      <button class="btn btn-ghost add-row" @click="addTrackDef">{{ t('settings.timelineTracks.addAudioTrack') }}</button>
    </div>

    <!-- Live Preview -->
    <div class="card">
      <div class="card-head">{{ t('settings.timelineTracks.livePreview') }} <InfoIcon :title="t('settings.timelineTracks.livePreviewHint')" /></div>
      <div class="tl-preview">
        <div v-for="def in settings.trackDefs" :key="def.id" class="tl-preview-row" :style="{ '--pv': def.color }">
          <div class="tl-pv-accent"></div>
          <span class="tl-pv-id">{{ def.id }}</span>
          <svg v-if="def.visible" class="tl-pv-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path v-if="def.icon==='video'"   d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M3 6h10a2 2 0 012 2v8a2 2 0 01-2 2H3a2 2 0 01-2-2V8a2 2 0 012-2z"/>
            <path v-else-if="def.icon==='game'"    d="M6 11h4m-2-2v4m7-1h.01M18 11h.01M2 6a2 2 0 012-2h16a2 2 0 012 2v10a4 4 0 01-4 4H6a4 4 0 01-4-4V6z"/>
            <path v-else-if="def.icon==='mic'"     d="M12 1a3 3 0 00-3 3v8a3 3 0 006 0V4a3 3 0 00-3-3zM19 10v2a7 7 0 01-14 0v-2M12 19v3M8 23h8"/>
            <path v-else-if="def.icon==='chat'"    d="M3 18v-6a9 9 0 0118 0v6M21 19a2 2 0 01-2 2h-1a2 2 0 01-2-2v-3a2 2 0 012-2h3zM3 19a2 2 0 002 2h1a2 2 0 002-2v-3a2 2 0 00-2-2H3z"/>
            <path v-else-if="def.icon==='media'"   d="M9 18V5l12-2v13M9 19c0 1.1-1.34 2-3 2s-3-.9-3-2 1.34-2 3-2 3 .9 3 2zm12-3c0 1.1-1.34 2-3 2s-3-.9-3-2 1.34-2 3-2 3 .9 3 2z"/>
            <path v-else-if="def.icon==='overlay'" d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/>
          </svg>
          <span class="tl-pv-name" :style="{ color: def.color }">{{ def.name || def.id }}</span>
          <div class="tl-pv-track-body">
            <div class="tl-pv-bar" :style="{ background: def.color }"></div>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* Inherited from parent */
</style>

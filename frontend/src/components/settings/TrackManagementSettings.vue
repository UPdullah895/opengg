<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { usePersistenceStore } from '../../stores/persistence'
import IconPicker from '../IconPicker.vue'
import InfoIcon from '../InfoIcon.vue'
import TimelineTrackRow from '../TimelineTrackRow.vue'
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
        <div v-for="def in settings.trackDefs" :key="def.id" class="tl-preview-row">
          <!-- Header: constrained to 110px width like the editor -->
          <div class="tl-preview-hdr">
            <TimelineTrackRow
              part="header"
              :color="def.color"
              :label="def.name || def.id"
              :icon="def.icon"
              :show-icon="def.visible"
            />
          </div>
          <!-- Body: flex to fill, with preview bar -->
          <TimelineTrackRow
            part="body"
            :color="def.color"
            preview
          />
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* Live preview header wrapper */
.tl-preview-hdr {
  width: 110px;
  flex-shrink: 0;
  display: flex;
}

.tl-preview-hdr > * {
  flex: 1;
  min-width: 0;
}

/* Body row stretches to fill remaining space */
.tl-preview-row > :last-child {
  flex: 1;
}
</style>

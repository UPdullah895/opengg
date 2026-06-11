<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { usePersistenceStore, DEFAULTS } from '../../stores/persistence'
import InfoIcon from '../InfoIcon.vue'
import './settings-shared.css'

const { t } = useI18n()
const persist = usePersistenceStore()

const settings = computed(() => persist.state.settings)

// ─── Shortcuts ───
const recordingKey = ref<string | null>(null)

function startRecord(key: string) { recordingKey.value = key }
function cancelRecord() { recordingKey.value = null }

function onShortcutKeydown(e: KeyboardEvent) {
  if (!recordingKey.value) return
  e.preventDefault()
  if (e.key === 'Escape') { cancelRecord(); return }
  const parts: string[] = []
  if (e.ctrlKey)  parts.push('Ctrl')
  if (e.shiftKey) parts.push('Shift')
  if (e.altKey)   parts.push('Alt')
  if (e.metaKey)  parts.push('Meta')
  const bare = e.key
  if (!['Control','Shift','Alt','Meta'].includes(bare)) {
    parts.push(bare.length === 1 ? bare.toUpperCase() : bare)
  }
  if (parts.length > 0 && !['Control','Shift','Alt','Meta'].includes(e.key)) {
    ;(settings.value.shortcuts as Record<string, string>)[recordingKey.value] = parts.join('+')
    recordingKey.value = null
  }
}

const shortcutActions = computed<Array<{ key: string; label: string; hint: string }>>(() => [
  { key: 'saveReplay',      label: t('settings.shortcuts.actions.saveReplay'),      hint: t('settings.shortcuts.hints.saveReplay') },
  { key: 'toggleRecording', label: t('settings.shortcuts.actions.toggleRecording'), hint: t('settings.shortcuts.hints.toggleRecording') },
  { key: 'screenshot',      label: t('settings.shortcuts.actions.screenshot'),      hint: t('settings.shortcuts.hints.screenshot') },
  { key: 'toggleEarBlast',  label: t('settings.shortcuts.actions.toggleEarBlast'),  hint: t('settings.shortcuts.hints.toggleEarBlast') },
  { key: 'splitClip',       label: t('settings.shortcuts.actions.splitClip'),       hint: t('settings.shortcuts.hints.splitClip') },
  { key: 'exportClip',      label: t('settings.shortcuts.actions.exportClip'),      hint: t('settings.shortcuts.hints.exportClip') },
  { key: 'toggleMic',       label: t('settings.shortcuts.actions.toggleMic'),       hint: t('settings.shortcuts.hints.toggleMic') },
  { key: 'undo',            label: t('settings.shortcuts.actions.undo'),            hint: t('settings.shortcuts.hints.undo') },
  { key: 'redo',            label: t('settings.shortcuts.actions.redo'),            hint: t('settings.shortcuts.hints.redo') },
])

const isDefaultShortcuts = computed(() =>
  JSON.stringify(settings.value.shortcuts) === JSON.stringify(DEFAULTS.settings.shortcuts)
)

function resetShortcuts() { persist.resetShortcuts() }

defineEmits<{ navigate: [page: string] }>()

// Export keydown handler for parent to attach
defineExpose({ onShortcutKeydown, recordingKey })
</script>

<template>
  <section class="settings-section" @keydown="onShortcutKeydown" tabindex="-1">
    <h2 class="sec-title">{{ t('settings.shortcuts.title') }}</h2>
    <div class="card">
      <div class="shortcut-hdr">
        <span class="shortcut-hdr-label">{{ t('settings.shortcuts.title') }} <InfoIcon :title="t('settings.shortcuts.hint')" /></span>
        <button class="btn-reset-sc" :disabled="isDefaultShortcuts" @click="resetShortcuts">{{ t('settings.shortcuts.resetToDefaults') }}</button>
      </div>
      <div class="shortcut-list">
        <div
          v-for="action in shortcutActions" :key="action.key"
          class="shortcut-row"
        >
          <span class="shortcut-action">
            {{ action.label }}
            <InfoIcon :title="action.hint" />
          </span>
          <button
            class="shortcut-key"
            :class="{ recording: recordingKey === action.key }"
            @click="recordingKey === action.key ? cancelRecord() : startRecord(action.key)"
          >
            <span v-if="recordingKey === action.key" class="rec-dot"></span>
            {{ recordingKey === action.key
                ? t('settings.shortcuts.recording')
                : (settings.shortcuts as Record<string,string>)[action.key] || '—' }}
          </button>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* Inherited from parent */
</style>

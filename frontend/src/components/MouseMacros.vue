<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { DeviceInfo } from '../stores/devices'
import { usePersistenceStore } from '../stores/persistence'
import type { MouseMacro, MacroAction, PlaybackMode } from '../stores/persistence'

const { t } = useI18n()
const props = defineProps<{ device: DeviceInfo }>()
const persistence = usePersistenceStore()

const ZONES = [
  { id: 'lmb',   label: 'LMB' },
  { id: 'rmb',   label: 'RMB' },
  { id: 'mmb',   label: 'Scroll' },
  { id: 'side1', label: 'Side 1' },
  { id: 'side2', label: 'Side 2' },
]

const selectedZone = ref<string | null>(null)
const recording = ref(false)
const currentKey = ref('')
const macroName = ref('')
const playbackMode = ref<PlaybackMode>('run_once')
const pendingActions = ref<MacroAction[]>([])

const deviceMacros = computed<MouseMacro[]>(() => {
  return persistence.state.macros[props.device.id] ?? []
})

function macroForZone(zoneId: string): MouseMacro | undefined {
  return deviceMacros.value.find(m => m.button === zoneId)
}

function selectZone(zoneId: string) {
  if (selectedZone.value === zoneId) return
  selectedZone.value = zoneId
  recording.value = false
  currentKey.value = ''
  const existing = macroForZone(zoneId)
  macroName.value = existing?.name ?? ''
  playbackMode.value = existing?.playback ?? 'run_once'
  pendingActions.value = existing ? [...existing.actions] : []
}

function startRecording() {
  recording.value = true
  currentKey.value = ''
}

function recordKey(e: KeyboardEvent) {
  if (!recording.value) return
  if (e.key === 'Escape') { recording.value = false; return }
  const parts: string[] = []
  if (e.ctrlKey) parts.push('Ctrl')
  if (e.altKey) parts.push('Alt')
  if (e.shiftKey) parts.push('Shift')
  if (e.metaKey) parts.push('Meta')
  const main = e.key.length === 1 ? e.key.toUpperCase() : e.key
  if (!['Control', 'Alt', 'Shift', 'Meta'].includes(e.key)) parts.push(main)
  if (parts.length === 0) return
  currentKey.value = parts.join('+')
  recording.value = false
}

function addAction() {
  if (!currentKey.value) return
  pendingActions.value.push({ type: 'key_down', key: currentKey.value })
  currentKey.value = ''
}

function removeAction(idx: number) {
  pendingActions.value.splice(idx, 1)
}

function saveMacro() {
  if (!selectedZone.value) return
  const macros = [...deviceMacros.value.filter(m => m.button !== selectedZone.value)]
  if (pendingActions.value.length > 0) {
    macros.push({
      button: selectedZone.value,
      name: macroName.value || selectedZone.value.toUpperCase(),
      actions: [...pendingActions.value],
      playback: playbackMode.value,
    })
  }
  persistence.state.macros = {
    ...persistence.state.macros,
    [props.device.id]: macros,
  }
}

function clearMacro() {
  if (!selectedZone.value) return
  macroName.value = ''
  playbackMode.value = 'run_once'
  pendingActions.value = []
  const macros = deviceMacros.value.filter(m => m.button !== selectedZone.value)
  persistence.state.macros = {
    ...persistence.state.macros,
    [props.device.id]: macros,
  }
}
</script>

<template>
  <div class="macros-root">
    <!-- SVG Mouse Diagram -->
    <div class="diagram-panel">
      <svg class="mouse-svg" viewBox="0 0 120 200" xmlns="http://www.w3.org/2000/svg">
        <!-- Body -->
        <path d="M20 70 Q20 15 60 15 Q100 15 100 70 L100 165 Q100 185 60 185 Q20 185 20 165 Z"
              fill="var(--bg-surface)" stroke="var(--border)" stroke-width="2"/>
        <!-- Center split line -->
        <line x1="60" y1="15" x2="60" y2="90" stroke="var(--border)" stroke-width="1.5"/>
        <!-- LMB -->
        <path
          class="zone"
          :class="{ active: selectedZone === 'lmb', assigned: !!macroForZone('lmb') }"
          d="M21 70 Q21 15 60 15 L60 90 L21 90 Z"
          @click="selectZone('lmb')"
        />
        <!-- RMB -->
        <path
          class="zone"
          :class="{ active: selectedZone === 'rmb', assigned: !!macroForZone('rmb') }"
          d="M60 15 Q99 15 99 70 L99 90 L60 90 Z"
          @click="selectZone('rmb')"
        />
        <!-- Scroll wheel (MMB) -->
        <rect
          class="zone"
          :class="{ active: selectedZone === 'mmb', assigned: !!macroForZone('mmb') }"
          x="50" y="32" width="20" height="36" rx="6"
          @click="selectZone('mmb')"
        />
        <!-- Side button 1 -->
        <rect
          class="zone"
          :class="{ active: selectedZone === 'side1', assigned: !!macroForZone('side1') }"
          x="6" y="100" width="16" height="22" rx="5"
          @click="selectZone('side1')"
        />
        <!-- Side button 2 -->
        <rect
          class="zone"
          :class="{ active: selectedZone === 'side2', assigned: !!macroForZone('side2') }"
          x="6" y="128" width="16" height="22" rx="5"
          @click="selectZone('side2')"
        />
        <!-- Zone labels -->
        <text x="35" y="58" class="zone-label">LMB</text>
        <text x="75" y="58" class="zone-label">RMB</text>
      </svg>

      <!-- Zone legend -->
      <div class="legend">
        <button
          v-for="z in ZONES"
          :key="z.id"
          class="legend-btn"
          :class="{ active: selectedZone === z.id, assigned: !!macroForZone(z.id) }"
          @click="selectZone(z.id)"
        >{{ z.label }}</button>
      </div>
    </div>

    <!-- Assignment Panel -->
    <div class="assign-panel">
      <div v-if="!selectedZone" class="empty-hint">
        {{ t('devices.macroNoButton') }}
      </div>

      <template v-else>
        <div class="assign-header">
          <span class="zone-title">{{ ZONES.find(z => z.id === selectedZone)?.label }}</span>
          <span v-if="macroForZone(selectedZone)" class="assigned-badge">{{ t('devices.macros') }}</span>
        </div>

        <!-- Name -->
        <div class="field">
          <label class="field-label">{{ t('devices.macroName') }}</label>
          <input v-model="macroName" class="text-input" :placeholder="selectedZone?.toUpperCase()" />
        </div>

        <!-- Playback -->
        <div class="field">
          <label class="field-label">{{ t('devices.macroPlayback') }}</label>
          <div class="radio-group">
            <label class="radio-option" :class="{ active: playbackMode === 'run_once' }">
              <input type="radio" value="run_once" v-model="playbackMode" />
              {{ t('devices.playbackRunOnce') }}
            </label>
            <label class="radio-option" :class="{ active: playbackMode === 'hold_repeat' }">
              <input type="radio" value="hold_repeat" v-model="playbackMode" />
              {{ t('devices.playbackHoldRepeat') }}
            </label>
            <label class="radio-option" :class="{ active: playbackMode === 'toggle' }">
              <input type="radio" value="toggle" v-model="playbackMode" />
              {{ t('devices.playbackToggle') }}
            </label>
          </div>
        </div>

        <!-- Actions -->
        <div class="field">
          <label class="field-label">{{ t('devices.macroActions') }}</label>
          <div class="actions-list" v-if="pendingActions.length">
            <div v-for="(act, i) in pendingActions" :key="i" class="action-row">
              <span class="key-chip">{{ act.key }}</span>
              <button class="remove-btn" @click="removeAction(i)">✕</button>
            </div>
          </div>
          <div class="empty-actions" v-else>—</div>

          <!-- Key recorder -->
          <div class="recorder-row">
            <div
              class="key-recorder"
              :class="{ active: recording }"
              tabindex="0"
              @click="startRecording"
              @keydown.prevent="recordKey"
            >
              <span v-if="recording" class="recording-hint">{{ t('devices.macroRecord') }}…</span>
              <span v-else-if="currentKey" class="key-chip">{{ currentKey }}</span>
              <span v-else class="recording-placeholder">{{ t('devices.macroRecord') }}</span>
            </div>
            <button class="add-btn" :disabled="!currentKey" @click="addAction">+</button>
          </div>
        </div>

        <!-- Save / Clear -->
        <div class="action-btns">
          <button class="btn-save" @click="saveMacro">{{ t('devices.macroSave') }}</button>
          <button class="btn-clear" @click="clearMacro">{{ t('devices.macroClear') }}</button>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.macros-root {
  display: grid;
  grid-template-columns: 160px 1fr;
  gap: 20px;
  align-items: start;
}

/* Diagram */
.diagram-panel {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
}
.mouse-svg {
  width: 120px;
  height: 200px;
}
.zone {
  fill: transparent;
  cursor: pointer;
  transition: fill .15s;
}
.zone:hover {
  fill: color-mix(in srgb, var(--accent) 18%, transparent);
}
.zone.active {
  fill: color-mix(in srgb, var(--accent) 35%, transparent);
  stroke: var(--accent);
  stroke-width: 1.5;
}
.zone.assigned {
  fill: color-mix(in srgb, var(--accent) 12%, transparent);
}
.zone.assigned.active {
  fill: color-mix(in srgb, var(--accent) 40%, transparent);
}
.zone-label {
  font-size: 9px;
  fill: var(--text-muted, #888);
  text-anchor: middle;
  pointer-events: none;
  user-select: none;
}
.legend {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 100%;
}
.legend-btn {
  width: 100%;
  padding: 5px 8px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--bg-surface);
  color: var(--text-sec);
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
  text-align: left;
  transition: all .15s;
}
.legend-btn:hover { color: var(--text); border-color: var(--accent); }
.legend-btn.active {
  color: var(--accent);
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 12%, transparent);
}
.legend-btn.assigned::after {
  content: '●';
  float: right;
  color: var(--accent);
  font-size: 8px;
}

/* Assignment panel */
.assign-panel {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.empty-hint {
  font-size: 13px;
  color: var(--text-muted, #888);
  padding: 12px 0;
}
.assign-header {
  display: flex;
  align-items: center;
  gap: 10px;
}
.zone-title {
  font-size: 15px;
  font-weight: 700;
  color: var(--text);
}
.assigned-badge {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: .5px;
  color: var(--accent);
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
  padding: 2px 8px;
  border-radius: 20px;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.field-label {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: .5px;
  color: var(--text-sec);
}
.text-input {
  background: var(--bg-surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  color: var(--text);
  padding: 7px 12px;
  font-size: 13px;
}
.text-input:focus { outline: none; border-color: var(--accent); }

/* Playback radio */
.radio-group {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.radio-option {
  display: flex;
  align-items: center;
  gap: 6px;
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
.radio-option input[type="radio"] { display: none; }
.radio-option.active {
  color: var(--accent);
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 12%, transparent);
}
.radio-option:hover { color: var(--text); border-color: var(--accent); }

/* Actions */
.actions-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 120px;
  overflow-y: auto;
}
.action-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.key-chip {
  padding: 3px 10px;
  border-radius: 6px;
  background: var(--bg-surface);
  border: 1px solid var(--border);
  font-size: 12px;
  font-weight: 600;
  color: var(--text);
  font-family: monospace;
}
.remove-btn {
  width: 20px;
  height: 20px;
  border-radius: 4px;
  border: none;
  background: transparent;
  color: var(--text-muted, #888);
  cursor: pointer;
  font-size: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
}
.remove-btn:hover { color: #ef4444; }
.empty-actions {
  font-size: 13px;
  color: var(--text-muted, #888);
}
.recorder-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.key-recorder {
  flex: 1;
  min-height: 36px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--bg-surface);
  display: flex;
  align-items: center;
  padding: 0 12px;
  cursor: pointer;
  transition: border-color .15s;
  outline: none;
}
.key-recorder:focus, .key-recorder.active {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 20%, transparent);
}
.recording-hint {
  font-size: 12px;
  color: var(--accent);
  animation: blink 1s step-end infinite;
}
.recording-placeholder {
  font-size: 12px;
  color: var(--text-muted, #888);
}
@keyframes blink { 50% { opacity: 0 } }
.add-btn {
  width: 36px;
  height: 36px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--bg-surface);
  color: var(--text-sec);
  font-size: 18px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all .15s;
  flex-shrink: 0;
}
.add-btn:not(:disabled):hover {
  color: var(--accent);
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 10%, transparent);
}
.add-btn:disabled { opacity: .4; cursor: not-allowed; }

/* Save/Clear */
.action-btns {
  display: flex;
  gap: 8px;
}
.btn-save {
  padding: 7px 20px;
  border-radius: 8px;
  border: none;
  background: var(--accent);
  color: #fff;
  font-size: 13px;
  font-weight: 700;
  cursor: pointer;
  transition: opacity .15s;
}
.btn-save:hover { opacity: .85; }
.btn-clear {
  padding: 7px 20px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--bg-surface);
  color: var(--text-sec);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: all .15s;
}
.btn-clear:hover { color: #ef4444; border-color: #ef4444; }
</style>

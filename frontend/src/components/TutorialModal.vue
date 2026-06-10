<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ (e: 'update:modelValue', v: boolean): void; (e: 'complete'): void }>()

const { t } = useI18n()
const currentStep = ref(0)

const stepDefs = [
  { key: 'welcome',  icon: 'sparkles' },
  { key: 'dashboard', icon: 'layout' },
  { key: 'mixer',    icon: 'music' },
  { key: 'clips',    icon: 'video' },
  { key: 'scanning', icon: 'search' },
  { key: 'settings', icon: 'settings' },
  { key: 'finish',   icon: 'check' },
]

const totalSteps = stepDefs.length
const isFirst = computed(() => currentStep.value === 0)
const isLast  = computed(() => currentStep.value === totalSteps - 1)

function next() {
  if (isLast.value) { finish() }
  else { currentStep.value++ }
}
function prev() { if (!isFirst.value) currentStep.value-- }
function skip() { finish() }
function finish() {
  emit('complete')
  close()
}
function close() {
  emit('update:modelValue', false)
  // Reset step after animation frame so it doesn't flash back to step 1 while closing
  requestAnimationFrame(() => { currentStep.value = 0 })
}

watch(() => props.modelValue, (v) => { if (v) currentStep.value = 0 })
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="modelValue" class="tutorial-overlay" @click.self="skip">
        <div class="tutorial-box">
          <!-- Step dots -->
          <div class="tutorial-steps">
            <div
              v-for="(s, idx) in stepDefs" :key="s.key"
              class="tutorial-dot"
              :class="{ active: idx === currentStep, done: idx < currentStep }"
            />
          </div>

          <!-- Icon -->
          <div class="tutorial-icon" :class="{ 'tutorial-icon--ok': stepDefs[currentStep].key === 'finish' }">
            <!-- sparkles -->
            <svg v-if="stepDefs[currentStep].icon === 'sparkles'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <path d="M12 3l1.5 4.5L18 9l-4.5 1.5L12 15l-1.5-4.5L6 9l4.5-1.5z"/>
              <path d="M5 3l.5 1.5L7 5l-1.5.5L5 7l-.5-1.5L3 5l1.5-.5z"/>
              <path d="M19 15l.5 1.5L21 17l-1.5.5L19 19l-.5-1.5L17 17l1.5-.5z"/>
            </svg>
            <!-- layout -->
            <svg v-else-if="stepDefs[currentStep].icon === 'layout'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <rect x="3" y="3" width="7" height="7" rx="1"/>
              <rect x="14" y="3" width="7" height="7" rx="1"/>
              <rect x="14" y="14" width="7" height="7" rx="1"/>
              <rect x="3" y="14" width="7" height="7" rx="1"/>
            </svg>
            <!-- music -->
            <svg v-else-if="stepDefs[currentStep].icon === 'music'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <path d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2z"/>
            </svg>
            <!-- video -->
            <svg v-else-if="stepDefs[currentStep].icon === 'video'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <rect x="2" y="2" width="20" height="20" rx="2.18"/>
              <path d="M7 2v20M17 2v20M2 12h20M2 7h5M2 17h5M17 17h5M17 7h5"/>
            </svg>
            <!-- search -->
            <svg v-else-if="stepDefs[currentStep].icon === 'search'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <circle cx="11" cy="11" r="8"/>
              <line x1="21" y1="21" x2="16.65" y2="16.65"/>
            </svg>
            <!-- settings -->
            <svg v-else-if="stepDefs[currentStep].icon === 'settings'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <circle cx="12" cy="12" r="3"/>
              <path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-2 2 2 2 0 01-2-2v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83 0 2 2 0 010-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 01-2-2 2 2 0 012-2h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 010-2.83 2 2 0 012.83 0l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 012-2 2 2 0 012 2v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 0 2 2 0 010 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 012 2 2 2 0 01-2 2h-.09a1.65 1.65 0 00-1.51 1z"/>
            </svg>
            <!-- check -->
            <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="20 6 9 17 4 12"/>
            </svg>
          </div>

          <!-- Text -->
          <h2 class="tutorial-title">{{ t(`tutorial.steps.${stepDefs[currentStep].key}.title`) }}</h2>
          <p class="tutorial-desc">{{ t(`tutorial.steps.${stepDefs[currentStep].key}.desc`) }}</p>

          <!-- Actions -->
          <div class="tutorial-actions">
            <button class="tutorial-skip" @click="skip">{{ t('tutorial.skip') }}</button>
            <div class="tutorial-nav">
              <button v-if="!isFirst" class="tutorial-secondary" @click="prev">{{ t('tutorial.previous') }}</button>
              <button class="tutorial-primary" @click="next">
                {{ isLast ? t('tutorial.finish') : t('tutorial.next') }}
              </button>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.tutorial-overlay {
  position: fixed; inset: 0; z-index: 9998;
  background: rgba(0,0,0,.78);
  backdrop-filter: blur(6px);
  display: flex; align-items: center; justify-content: center;
  padding: 20px;
}
.tutorial-box {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 16px;
  padding: 32px;
  width: 100%; max-width: 480px;
  box-shadow: 0 32px 80px rgba(0,0,0,.65);
  display: flex; flex-direction: column; gap: 18px;
}

/* Step dots */
.tutorial-steps { display: flex; align-items: center; justify-content: center; gap: 8px; }
.tutorial-dot {
  width: 8px; height: 8px; border-radius: 50%;
  background: var(--border);
  transition: background .2s, transform .2s;
}
.tutorial-dot.active { background: var(--accent); transform: scale(1.3); }
.tutorial-dot.done   { background: color-mix(in srgb, var(--accent) 60%, transparent); }

/* Icon */
.tutorial-icon {
  width: 58px; height: 58px; border-radius: 14px;
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
  display: flex; align-items: center; justify-content: center; color: var(--accent);
  align-self: center;
}
.tutorial-icon svg { width: 28px; height: 28px; }
.tutorial-icon--ok { background: color-mix(in srgb, var(--success) 12%, transparent); border-color: color-mix(in srgb, var(--success) 30%, transparent); color: var(--success); }

.tutorial-title { font-size: 20px; font-weight: 800; letter-spacing: -.3px; text-align: center; }
.tutorial-desc { font-size: 13px; color: var(--text-sec); line-height: 1.6; text-align: center; margin: 0; }

/* Actions */
.tutorial-actions {
  display: flex; align-items: center; justify-content: space-between;
  padding-top: 8px;
}
.tutorial-nav { display: flex; align-items: center; gap: 10px; margin-left: auto; }
.tutorial-skip { background: transparent; border: none; color: var(--text-muted); font-size: 13px; cursor: pointer; padding: 8px 4px; }
.tutorial-skip:hover { color: var(--text-sec); }
.tutorial-secondary {
  display: inline-flex; align-items: center; gap: 6px;
  padding: 9px 18px; border-radius: 8px;
  font-size: 13px; font-weight: 600; cursor: pointer;
  background: transparent;
  border: 1px solid color-mix(in srgb, var(--text-sec) 50%, transparent);
  color: var(--text-sec);
  transition: background .15s, border-color .15s, color .15s;
}
.tutorial-secondary:hover {
  border-color: var(--text);
  color: var(--text);
}
.tutorial-primary {
  display: inline-flex; align-items: center; gap: 8px;
  padding: 10px 20px; border-radius: 8px;
  border: none; background: var(--accent);
  color: #fff; font-size: 13px; font-weight: 700;
  cursor: pointer; transition: opacity .15s;
}
.tutorial-primary:hover { opacity: .88; }
.tutorial-primary:disabled { opacity: .45; cursor: not-allowed; }

/* Fade transition */
.fade-enter-active, .fade-leave-active { transition: opacity .25s ease; }
.fade-enter-from, .fade-leave-to { opacity: 0; }
</style>

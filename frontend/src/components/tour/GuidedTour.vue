<script setup lang="ts">
import { ref, computed, watch, nextTick, onBeforeUnmount } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  useFloating, autoUpdate, offset, flip, shift, arrow,
  type Placement,
} from '@floating-ui/vue'
import { useElementBounding } from '@vueuse/core'
import { useTour } from '../../composables/useTour'
import { usePersistenceStore } from '../../stores/persistence'
import { useAudioStore } from '../../stores/audio'
import ToggleSwitch from '../ToggleSwitch.vue'
import RecorderInstallHelper from '../settings/RecorderInstallHelper.vue'

const { t, te } = useI18n()
const tour = useTour()
const persist = usePersistenceStore()
const audio = useAudioStore()

// "Don't show again" — checked by default so the first-run tour won't nag on next launch.
const dontShowAgain = ref(true)

// Paused = tour hidden while the audio-setup wizard is open over it (see audio CTA).
const paused = ref(false)

// Highlighted target element (null = centered, full-dim step e.g. welcome).
const referenceEl = ref<HTMLElement | null>(null)
const floatingEl = ref<HTMLElement | null>(null)
const arrowEl = ref<HTMLElement | null>(null)

// RTL mirrors physical left/right placements (floating-ui keeps physical sides as-is).
const isRtl = computed(() => document.documentElement.dir === 'rtl')
const placement = computed<Placement>(() => {
  const p = (tour.current.value?.placement ?? 'bottom') as Placement
  if (!isRtl.value) return p
  if (p.startsWith('left')) return p.replace('left', 'right') as Placement
  if (p.startsWith('right')) return p.replace('right', 'left') as Placement
  return p
})

const { floatingStyles, middlewareData, placement: actualPlacement } = useFloating(
  referenceEl,
  floatingEl,
  {
    placement,
    middleware: [offset(14), flip({ padding: 8 }), shift({ padding: 10 }), arrow({ element: arrowEl })],
    whileElementsMounted: autoUpdate,
  },
)

// Live target rect for the spotlight cutout (follows scroll/resize).
const { left, top, width, height, update } = useElementBounding(referenceEl)
const PAD = 6
const hasTarget = computed(() => !!referenceEl.value && width.value > 0)
const ring = computed(() => ({
  left: `${left.value - PAD}px`, top: `${top.value - PAD}px`,
  width: `${width.value + PAD * 2}px`, height: `${height.value + PAD * 2}px`,
}))
// Four dim panels around the target leave an interactive hole over it.
const panels = computed(() => {
  if (!hasTarget.value) return null
  const x = left.value - PAD, y = top.value - PAD
  const w = width.value + PAD * 2, h = height.value + PAD * 2
  return {
    top: { left: '0', top: '0', width: '100%', height: `${Math.max(0, y)}px` },
    bottom: { left: '0', top: `${y + h}px`, width: '100%', bottom: '0' },
    left: { left: '0', top: `${y}px`, width: `${Math.max(0, x)}px`, height: `${h}px` },
    right: { left: `${x + w}px`, top: `${y}px`, right: '0', height: `${h}px` },
  }
})

// Arrow placement: opposite of the resolved side.
const arrowStyle = computed(() => {
  const d = middlewareData.value.arrow
  if (!d) return {}
  const side = actualPlacement.value.split('-')[0]
  const opp: Record<string, string> = { top: 'bottom', bottom: 'top', left: 'right', right: 'left' }
  return {
    left: d.x != null ? `${d.x}px` : '',
    top: d.y != null ? `${d.y}px` : '',
    [opp[side]]: '-5px',
  }
})

// i18n with graceful fallback if a key is missing.
const tt = (key: string) => (te(key) ? t(key) : '')
const stepId = computed(() => tour.current.value?.id)
const title = computed(() => tt(`tour.steps.${stepId.value}.title`))
const body = computed(() => tt(`tour.steps.${stepId.value}.body`))
const deepText = computed(() =>
  tour.current.value?.deepKey ? tt(`tour.steps.${stepId.value}.deep`) : '',
)
const actionText = computed(() =>
  tour.current.value?.action ? tt(`tour.steps.${stepId.value}.action`) : '',
)

// ── Two-tier: collapsed "basic" by default; "More" reveals the deep-dive ──
const showDeep = ref(false)

// ── State-aware CTAs ──
const audioReady = computed(() => audio.virtualAudioReady)
// Mixer action: only suggest the audio split once the engine exists; otherwise show the setup CTA.
const showAudioSetupCta = computed(() => tour.current.value?.cta === 'audioSetup' && !audioReady.value)
const showSplitSuggestion = computed(() =>
  !!tour.current.value?.action && (tour.current.value?.cta !== 'audioSetup' || audioReady.value),
)
const showRecorderHelper = computed(() => tour.current.value?.cta === 'recorderInstall')

function startAudioSetup() {
  // Hide the tour while the wizard is open over it; resume when the wizard closes.
  paused.value = true
  window.dispatchEvent(new CustomEvent('openOnboarding', { detail: { step: 1 } }))
}
function onOnboardingClosed() { paused.value = false }
window.addEventListener('onboardingClosed', onOnboardingClosed)

// Action satisfied indicator — shows a ✓ but DOES NOT auto-advance (the user clicks Next).
const satisfied = ref(false)

// ── Resolve the target element when the step changes (after page navigation) ──
function waitForElement(sel: string, timeout = 1600): Promise<HTMLElement | null> {
  return new Promise(resolve => {
    const startT = performance.now()
    const tick = () => {
      const el = document.querySelector(sel) as HTMLElement | null
      if (el) return resolve(el)
      if (performance.now() - startT > timeout) return resolve(null)
      requestAnimationFrame(tick)
    }
    tick()
  })
}

let actionTimer: number | null = null
function cleanupAction() {
  if (actionTimer) { clearInterval(actionTimer); actionTimer = null }
}
function setupAction(detect: () => boolean) {
  cleanupAction()
  // Poll the predicate only to flip the ✓ indicator — never advance on our own.
  actionTimer = window.setInterval(() => {
    try { satisfied.value = detect() } catch { /* store not ready */ }
  }, 300)
}

watch(() => tour.current.value, async (step) => {
  cleanupAction()
  satisfied.value = false
  showDeep.value = false
  referenceEl.value = null
  if (!step) return
  await nextTick()
  if (step.target) {
    const el = await waitForElement(step.target)
    referenceEl.value = el
    if (el) {
      el.scrollIntoView({ block: 'center', inline: 'center', behavior: 'smooth' })
      requestAnimationFrame(() => update())
    }
  }
  if (step.action) setupAction(step.action.detect)
}, { immediate: true })

// Persist "don't show again" on any exit (skip / finish / last-next).
watch(() => tour.active.value, (a, prev) => {
  if (prev && !a) {
    cleanupAction()
    paused.value = false
    if (dontShowAgain.value) persist.state.settings.tutorialSeen = true
  }
})

function onKeydown(e: KeyboardEvent) {
  if (!tour.active.value || paused.value) return
  if (e.key === 'Escape') tour.skip()
  else if (e.key === 'ArrowRight') tour.next()
  else if (e.key === 'ArrowLeft') tour.back()
}
watch(() => tour.active.value, (a) => {
  if (a) window.addEventListener('keydown', onKeydown)
  else window.removeEventListener('keydown', onKeydown)
})
onBeforeUnmount(() => {
  cleanupAction()
  window.removeEventListener('keydown', onKeydown)
  window.removeEventListener('onboardingClosed', onOnboardingClosed)
})
</script>

<template>
  <Teleport to="body">
    <div v-if="tour.active.value && !paused" class="tour-root">
      <!-- Spotlight: four dim panels around the target (interactive hole), or full dim -->
      <template v-if="panels">
        <div class="tour-dim" :style="panels.top"></div>
        <div class="tour-dim" :style="panels.bottom"></div>
        <div class="tour-dim" :style="panels.left"></div>
        <div class="tour-dim" :style="panels.right"></div>
        <div class="tour-ring" :class="{ 'tour-ring--done': satisfied }" :style="ring"></div>
      </template>
      <div v-else class="tour-dim tour-dim--full"></div>

      <!-- Explanation card -->
      <div
        ref="floatingEl"
        class="tour-card"
        :class="{ 'tour-card--centered': !hasTarget }"
        :style="hasTarget ? floatingStyles : undefined"
      >
        <div v-if="hasTarget" ref="arrowEl" class="tour-arrow" :style="arrowStyle"></div>

        <div class="tour-dots">
          <span
            v-for="(s, i) in tour.steps" :key="s.id"
            class="tour-dot" :class="{ active: i === tour.stepIndex.value, done: i < tour.stepIndex.value }"
          ></span>
        </div>

        <h3 class="tour-title">{{ title }}</h3>
        <p class="tour-body">{{ body }}</p>

        <!-- Two-tier: optional deep-dive, collapsed by default -->
        <template v-if="deepText">
          <button class="tour-more" @click="showDeep = !showDeep">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" :class="{ open: showDeep }"><polyline points="6 9 12 15 18 9"/></svg>
            {{ showDeep ? t('tour.controls.less') : t('tour.controls.more') }}
          </button>
          <p v-if="showDeep" class="tour-deep">{{ deepText }}</p>
        </template>

        <!-- State-aware: set up audio engine first -->
        <button v-if="showAudioSetupCta" class="tour-cta" @click="startAudioSetup">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 5v14M5 12h14"/></svg>
          {{ t('tour.controls.setupAudio') }}
        </button>

        <!-- State-aware: recorder install helper (distro-aware command + copy + recheck) -->
        <RecorderInstallHelper v-if="showRecorderHelper" compact />

        <!-- Audio-split suggestion (only once the engine is ready); ✓ when satisfied, no auto-advance -->
        <div v-if="showSplitSuggestion && actionText" class="tour-action" :class="{ 'tour-action--done': satisfied }">
          <svg v-if="satisfied" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
          <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z"/></svg>
          <span>{{ satisfied ? t('tour.controls.done') : actionText }}</span>
        </div>

        <label class="tour-dsa">
          <ToggleSwitch v-model="dontShowAgain" compact />
          <span>{{ t('tour.controls.dontShowAgain') }}</span>
        </label>

        <div class="tour-controls">
          <button class="tour-skip" @click="tour.skip()">{{ t('tour.controls.skip') }}</button>
          <div class="tour-nav">
            <button v-if="!tour.isFirst.value" class="tour-btn tour-btn--ghost" @click="tour.back()">
              {{ t('tour.controls.back') }}
            </button>
            <button class="tour-btn tour-btn--primary" @click="tour.next()">
              {{ tour.isLast.value ? t('tour.controls.finish') : t('tour.controls.next') }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.tour-root { position: fixed; inset: 0; z-index: 10000; }
.tour-dim { position: fixed; background: rgba(0,0,0,.62); }
.tour-dim--full { inset: 0; }
/* Bright highlight ring around the interactive target (clicks pass through it). */
.tour-ring {
  position: fixed; pointer-events: none; border-radius: 10px;
  box-shadow: 0 0 0 2px var(--accent), 0 0 18px 2px color-mix(in srgb, var(--accent) 45%, transparent);
  transition: box-shadow .2s;
}
.tour-ring--done { box-shadow: 0 0 0 2px var(--success), 0 0 22px 3px color-mix(in srgb, var(--success) 55%, transparent); }

.tour-card {
  position: absolute; z-index: 10001; width: 340px; max-width: calc(100vw - 24px);
  background: var(--bg-card); border: 1px solid var(--border); border-radius: 14px;
  padding: 18px; box-shadow: 0 24px 60px rgba(0,0,0,.6);
  display: flex; flex-direction: column; gap: 12px;
}
.tour-card--centered {
  position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%);
}
.tour-arrow {
  position: absolute; width: 10px; height: 10px;
  background: var(--bg-card); border: 1px solid var(--border);
  transform: rotate(45deg); border-radius: 2px;
}

.tour-dots { display: flex; gap: 6px; justify-content: center; flex-wrap: wrap; }
.tour-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--border); transition: all .2s; }
.tour-dot.active { background: var(--accent); transform: scale(1.35); }
.tour-dot.done { background: color-mix(in srgb, var(--accent) 55%, transparent); }

.tour-title { font-size: 17px; font-weight: 800; letter-spacing: -.2px; }
.tour-body { font-size: 13px; color: var(--text-sec); line-height: 1.6; margin: 0; }

/* Two-tier "More" expander */
.tour-more {
  display: inline-flex; align-items: center; gap: 5px; align-self: flex-start;
  background: transparent; border: none; padding: 0; cursor: pointer;
  color: var(--accent); font-size: 12px; font-weight: 700;
}
.tour-more svg { width: 13px; height: 13px; transition: transform .18s; }
.tour-more svg.open { transform: rotate(180deg); }
.tour-deep {
  font-size: 12.5px; color: var(--text-sec); line-height: 1.6; margin: 0;
  padding: 10px 12px; border-radius: 8px; background: var(--bg-deep); border: 1px solid var(--border);
}

/* State-aware setup CTA */
.tour-cta {
  display: flex; align-items: center; justify-content: center; gap: 8px;
  padding: 10px 14px; border-radius: 8px; border: none; cursor: pointer;
  background: var(--accent); color: #fff; font-size: 13px; font-weight: 700;
}
.tour-cta:hover { opacity: .9; }
.tour-cta svg { width: 15px; height: 15px; }

.tour-action {
  display: flex; align-items: center; gap: 8px; font-size: 12px; font-weight: 600;
  padding: 8px 10px; border-radius: 8px; color: var(--accent);
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 28%, transparent);
}
.tour-action svg { width: 14px; height: 14px; flex-shrink: 0; }
.tour-action--done { color: var(--success); background: color-mix(in srgb, var(--success) 12%, transparent); border-color: color-mix(in srgb, var(--success) 35%, transparent); }

.tour-dsa { display: flex; align-items: center; gap: 8px; font-size: 11px; color: var(--text-muted); cursor: pointer; user-select: none; }

.tour-controls { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding-top: 2px; }
.tour-nav { display: flex; gap: 8px; margin-inline-start: auto; }
.tour-skip { background: transparent; border: none; color: var(--text-muted); font-size: 12px; cursor: pointer; padding: 6px 2px; }
.tour-skip:hover { color: var(--text-sec); }
.tour-btn { padding: 8px 16px; border-radius: 8px; font-size: 12px; font-weight: 700; cursor: pointer; transition: opacity .15s, background .15s, color .15s; }
.tour-btn--ghost { background: transparent; border: 1px solid color-mix(in srgb, var(--text-sec) 45%, transparent); color: var(--text-sec); }
.tour-btn--ghost:hover { border-color: var(--text); color: var(--text); }
.tour-btn--primary { background: var(--accent); border: none; color: #fff; }
.tour-btn--primary:hover { opacity: .9; }
</style>

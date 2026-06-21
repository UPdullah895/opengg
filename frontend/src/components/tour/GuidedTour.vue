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

const { t, te } = useI18n()
const tour = useTour()
const persist = usePersistenceStore()

// "Don't show again" — checked by default so the first-run tour won't nag on next launch.
const dontShowAgain = ref(true)

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
const title = computed(() => tt(`tour.steps.${tour.current.value?.id}.title`))
const body = computed(() => tt(`tour.steps.${tour.current.value?.id}.body`))
const actionText = computed(() =>
  tour.current.value?.action ? tt(`tour.steps.${tour.current.value.id}.action`) : '',
)
const doneFlash = ref(false)

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
  // Already satisfied? Don't auto-skip immediately — let the user read the step.
  actionTimer = window.setInterval(() => {
    let done = false
    try { done = detect() } catch { /* store not ready */ }
    if (done) {
      cleanupAction()
      doneFlash.value = true
      setTimeout(() => { doneFlash.value = false; tour.next() }, 800)
    }
  }, 250)
}

watch(() => tour.current.value, async (step) => {
  cleanupAction()
  doneFlash.value = false
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
    if (dontShowAgain.value) persist.state.settings.tutorialSeen = true
  }
})

function onKeydown(e: KeyboardEvent) {
  if (!tour.active.value) return
  if (e.key === 'Escape') tour.skip()
  else if (e.key === 'ArrowRight') tour.next()
  else if (e.key === 'ArrowLeft') tour.back()
}
watch(() => tour.active.value, (a) => {
  if (a) window.addEventListener('keydown', onKeydown)
  else window.removeEventListener('keydown', onKeydown)
})
onBeforeUnmount(() => { cleanupAction(); window.removeEventListener('keydown', onKeydown) })
</script>

<template>
  <Teleport to="body">
    <div v-if="tour.active.value" class="tour-root">
      <!-- Spotlight: four dim panels around the target (interactive hole), or full dim -->
      <template v-if="panels">
        <div class="tour-dim" :style="panels.top"></div>
        <div class="tour-dim" :style="panels.bottom"></div>
        <div class="tour-dim" :style="panels.left"></div>
        <div class="tour-dim" :style="panels.right"></div>
        <div class="tour-ring" :class="{ 'tour-ring--done': doneFlash }" :style="ring"></div>
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

        <div v-if="actionText" class="tour-action" :class="{ 'tour-action--done': doneFlash }">
          <svg v-if="doneFlash" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
          <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z"/></svg>
          <span>{{ doneFlash ? t('tour.controls.done') : actionText }}</span>
        </div>

        <label class="tour-dsa">
          <input type="checkbox" v-model="dontShowAgain" />
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
  position: absolute; z-index: 10001; width: 320px; max-width: calc(100vw - 24px);
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

.tour-dots { display: flex; gap: 6px; justify-content: center; }
.tour-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--border); transition: all .2s; }
.tour-dot.active { background: var(--accent); transform: scale(1.35); }
.tour-dot.done { background: color-mix(in srgb, var(--accent) 55%, transparent); }

.tour-title { font-size: 17px; font-weight: 800; letter-spacing: -.2px; }
.tour-body { font-size: 13px; color: var(--text-sec); line-height: 1.6; margin: 0; }

.tour-action {
  display: flex; align-items: center; gap: 8px; font-size: 12px; font-weight: 600;
  padding: 8px 10px; border-radius: 8px; color: var(--accent);
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 28%, transparent);
}
.tour-action svg { width: 14px; height: 14px; flex-shrink: 0; }
.tour-action--done { color: var(--success); background: color-mix(in srgb, var(--success) 12%, transparent); border-color: color-mix(in srgb, var(--success) 35%, transparent); }

.tour-dsa { display: flex; align-items: center; gap: 7px; font-size: 11px; color: var(--text-muted); cursor: pointer; user-select: none; }
.tour-dsa input { accent-color: var(--accent); cursor: pointer; }

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

import { ref, computed } from 'vue'
import { TOUR_STEPS, type TourStep, type TourPage } from '../tour/steps'

// ── Singleton tour state (module-scoped so any component shares one tour) ──
const active = ref(false)
const stepIndex = ref(0)
let navigateFn: ((page: TourPage) => void) | null = null

/**
 * Guided-tour controller. `GuidedTour.vue` renders the spotlight/card from this state;
 * `App.vue` injects `setNavigate` so the tour can drive the app's `currentPage`.
 */
export function useTour() {
  const steps = TOUR_STEPS
  const current = computed<TourStep | null>(() =>
    active.value ? steps[stepIndex.value] ?? null : null,
  )
  const isFirst = computed(() => stepIndex.value === 0)
  const isLast = computed(() => stepIndex.value === steps.length - 1)

  /** App.vue passes its `navigate(page)` so cross-page steps can route automatically. */
  function setNavigate(fn: (page: TourPage) => void) { navigateFn = fn }

  function go(i: number) {
    if (i < 0 || i >= steps.length) return
    stepIndex.value = i
    navigateFn?.(steps[i].page) // route first; GuidedTour waits for the target to mount
  }

  function start() {
    stepIndex.value = 0
    active.value = true
    navigateFn?.(steps[0].page)
  }
  function next() { isLast.value ? finish() : go(stepIndex.value + 1) }
  function back() { if (!isFirst.value) go(stepIndex.value - 1) }
  function skip() { finish() }
  function finish() { active.value = false }

  return {
    active, stepIndex, steps, current, isFirst, isLast,
    setNavigate, start, next, back, skip, finish, go,
  }
}

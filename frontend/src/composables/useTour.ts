import { ref, computed } from 'vue'
import { TOUR_STEPS, type TourStep, type TourPage } from '../tour/steps'

// ── Singleton tour state (module-scoped so any component shares one tour) ──
const active = ref(false)
const stepIndex = ref(0)
let navigateFn: ((page: TourPage) => void) | null = null

/** A step is shown only if it has no condition, or its condition() is currently true. */
function isVisible(step: TourStep | undefined): boolean {
  if (!step) return false
  return !step.condition || step.condition()
}

/**
 * Guided-tour controller. `GuidedTour.vue` renders the spotlight/card from this state;
 * `App.vue` injects `setNavigate` so the tour can drive the app's `currentPage`.
 * Conditional steps (e.g. the editor steps, shown only when a real clip exists) are
 * skipped automatically during forward/back navigation.
 */
export function useTour() {
  const steps = TOUR_STEPS
  const current = computed<TourStep | null>(() =>
    active.value ? steps[stepIndex.value] ?? null : null,
  )
  // First/last among *visible* steps so the Back/Finish buttons reflect what the user sees.
  const isFirst = computed(() => prevVisible(stepIndex.value) === -1)
  const isLast = computed(() => nextVisible(stepIndex.value) === -1)

  /** App.vue passes its `navigate(page)` so cross-page steps can route automatically. */
  function setNavigate(fn: (page: TourPage) => void) { navigateFn = fn }

  function nextVisible(from: number): number {
    for (let i = from + 1; i < steps.length; i++) if (isVisible(steps[i])) return i
    return -1
  }
  function prevVisible(from: number): number {
    for (let i = from - 1; i >= 0; i--) if (isVisible(steps[i])) return i
    return -1
  }

  function go(i: number) {
    if (i < 0 || i >= steps.length) return
    stepIndex.value = i
    navigateFn?.(steps[i].page) // route first; GuidedTour waits for the target to mount
  }

  function start() {
    // Begin at the first visible step (welcome has no condition, so normally index 0).
    const first = isVisible(steps[0]) ? 0 : nextVisible(0)
    if (first === -1) return
    active.value = true
    go(first)
  }
  function next() {
    const i = nextVisible(stepIndex.value)
    i === -1 ? finish() : go(i)
  }
  function back() {
    const i = prevVisible(stepIndex.value)
    if (i !== -1) go(i)
  }
  function skip() { finish() }
  function finish() { active.value = false }

  return {
    active, stepIndex, steps, current, isFirst, isLast,
    setNavigate, start, next, back, skip, finish, go,
  }
}

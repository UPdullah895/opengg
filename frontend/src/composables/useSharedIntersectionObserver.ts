/**
 * Shared IntersectionObserver composable.
 * A single IO instance is reused across all callers — far cheaper than
 * creating one observer per card.
 *
 * Microtask batching: all entries from a single browser IO callback are
 * collected and dispatched together in the next microtask. This coalesces
 * the initial mount burst (30-50 entries) into one batch instead of
 * flooding the thumbnail queue with individual callbacks.
 *
 * Threshold [0, 0.1, 0.5] lets ClipCard know if a card is mostly visible
 * (ratio > 0.3) so it can request high-priority thumbnail generation.
 */

type Callback = (entry: IntersectionObserverEntry) => void

let sharedObserver: IntersectionObserver | null = null
const callbackMap = new WeakMap<Element, Callback>()

// Microtask batch state
let pendingEntries: IntersectionObserverEntry[] = []
let scheduled = false

function processBatch() {
  scheduled = false
  const batch = pendingEntries
  pendingEntries = []
  if (import.meta.env.DEV && batch.length > 0) {
    const intersecting = batch.filter(e => e.isIntersecting).length
    console.debug(`[perf] IO callback: entries=${batch.length} intersecting=${intersecting}`)
  }
  for (const entry of batch) {
    const cb = callbackMap.get(entry.target)
    if (cb) cb(entry)
  }
}

function getObserver(): IntersectionObserver {
  if (!sharedObserver) {
    sharedObserver = new IntersectionObserver(
      (entries) => {
        pendingEntries.push(...entries)
        if (!scheduled) {
          scheduled = true
          queueMicrotask(processBatch)
        }
      },
      { threshold: [0, 0.1, 0.5], rootMargin: '300px' }
    )
  }
  return sharedObserver
}

export function useSharedIntersectionObserver() {
  function observe(el: Element, callback: Callback) {
    callbackMap.set(el, callback)
    getObserver().observe(el)
  }

  function unobserve(el: Element) {
    callbackMap.delete(el)
    getObserver().unobserve(el)
  }

  return { observe, unobserve }
}

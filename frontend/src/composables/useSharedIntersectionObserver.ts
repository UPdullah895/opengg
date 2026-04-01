/**
 * Shared IntersectionObserver composable.
 * A single IO instance is reused across all callers — far cheaper than
 * creating one observer per card.
 */

type Callback = (entry: IntersectionObserverEntry) => void

let sharedObserver: IntersectionObserver | null = null
const callbackMap = new WeakMap<Element, Callback>()

function getObserver(): IntersectionObserver {
  if (!sharedObserver) {
    sharedObserver = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const cb = callbackMap.get(entry.target)
          if (cb) cb(entry)
        }
      },
      { threshold: 0.1, rootMargin: '300px' }
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

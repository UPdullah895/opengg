/**
 * Limits concurrent generate_thumbnail IPC calls to MAX_CONCURRENT.
 * Uses a two-tier priority queue: 'high' for viewport-visible cards,
 * 'normal' for cards within the 300px rootMargin pre-load zone.
 * ClipCard calls queue.enqueue() instead of invoke() directly to avoid
 * saturating FFmpeg/CPU when many cards enter the viewport simultaneously.
 */

const MAX_CONCURRENT = 5

let running = 0
const highPri: Array<() => void> = []
const normalPri: Array<() => void> = []

// Perf counters (dev only)
let _totalEnqueued = 0
let _totalCompleted = 0
let _batchStart = 0
let _batchDurations: number[] = []

function drain() {
  while (running < MAX_CONCURRENT) {
    const next = highPri.shift() ?? normalPri.shift()
    if (!next) break
    running++
    next()
  }
}

export function useThumbnailQueue() {
  function enqueue<T>(task: () => Promise<T>, priority: 'high' | 'normal' = 'normal'): Promise<T> {
    if (import.meta.env.DEV) {
      _totalEnqueued++
      if (_totalEnqueued === 1 || running + highPri.length + normalPri.length === 0) {
        _batchStart = performance.now()
        _batchDurations = []
      }
    }

    return new Promise<T>((resolve, reject) => {
      const bucket = priority === 'high' ? highPri : normalPri
      bucket.push(() => {
        const t0 = import.meta.env.DEV ? performance.now() : 0
        task().then(resolve, reject).finally(() => {
          running--
          if (import.meta.env.DEV) {
            _totalCompleted++
            _batchDurations.push(performance.now() - t0)
            const pending = highPri.length + normalPri.length
            if (pending === 0 && running === 0) {
              const elapsed = performance.now() - _batchStart
              const avg = _batchDurations.length
                ? (_batchDurations.reduce((a, b) => a + b, 0) / _batchDurations.length).toFixed(0)
                : 0
              console.debug(
                `[perf] thumbQueue DRAINED: total=${_totalCompleted} elapsed=${elapsed.toFixed(0)}ms avg=${avg}ms/thumb`
              )
              _totalEnqueued = 0
              _totalCompleted = 0
            } else if (_totalCompleted % 5 === 0) {
              console.debug(
                `[perf] thumbQueue: running=${running} high=${highPri.length} normal=${normalPri.length} completed=${_totalCompleted}`
              )
            }
          }
          drain()
        })
      })
      drain()
    })
  }

  return { enqueue }
}

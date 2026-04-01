/**
 * Limits concurrent generate_thumbnail IPC calls to MAX_CONCURRENT.
 * ClipCard calls queue.enqueue() instead of invoke() directly to avoid
 * saturating FFmpeg/CPU when many cards enter the viewport simultaneously.
 */

const MAX_CONCURRENT = 3

let running = 0
const pending: Array<() => void> = []

function drain() {
  while (running < MAX_CONCURRENT && pending.length > 0) {
    const next = pending.shift()!
    running++
    next()
  }
}

export function useThumbnailQueue() {
  function enqueue<T>(task: () => Promise<T>): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      pending.push(() => {
        task().then(resolve, reject).finally(() => {
          running--
          drain()
        })
      })
      drain()
    })
  }

  return { enqueue }
}

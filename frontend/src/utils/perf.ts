/**
 * Lightweight performance instrumentation utilities.
 * All functions are no-ops in production builds (import.meta.env.DEV === false).
 * Usage:
 *   const result = await perfWrap('ipc:get_clips', () => invoke('get_clips', { folder }))
 *   perfStart('my-op'); doWork(); perfEnd('my-op')
 *   perfReport()  // console.table of all measures
 */

const PERF_ENABLED = import.meta.env.DEV

export function perfStart(label: string): void {
  if (!PERF_ENABLED) return
  performance.mark(`${label}:start`)
}

export function perfEnd(label: string): void {
  if (!PERF_ENABLED) return
  try {
    performance.mark(`${label}:end`)
    const m = performance.measure(label, `${label}:start`, `${label}:end`)
    console.debug(`[perf] ${label}: ${m.duration.toFixed(1)}ms`)
  } catch {}
}

export async function perfWrap<T>(label: string, fn: () => T | Promise<T>): Promise<Awaited<T>> {
  if (!PERF_ENABLED) return fn() as Awaited<T>
  perfStart(label)
  try {
    return await (fn() as Promise<Awaited<T>>)
  } finally {
    perfEnd(label)
  }
}

export function perfReport(): void {
  if (!PERF_ENABLED) return
  const entries = performance.getEntriesByType('measure')
  console.table(entries.map(e => ({ name: e.name, duration: `${e.duration.toFixed(1)}ms` })))
}

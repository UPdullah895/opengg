import { ref, computed, watch, onMounted, onBeforeUnmount, type Ref } from 'vue'
import { useReplayStore } from '../stores/replay'
import { useSharedIntersectionObserver } from './useSharedIntersectionObserver'

/**
 * Shared composable for clip thumbnail loading, visibility-based unloading,
 * trim-state tracking, and live metadata reactivity.
 *
 * Used by ClipCard and ClipListRow to avoid duplicating ~80 LOC of
 * IntersectionObserver + thumbnail lifecycle logic.
 */
export function useClipThumbnail(
  clipId: string,
  filepath: string,
  initialDuration: number,
  initialThumb: string | undefined,
  mediaPort: Ref<number> | number,
  mediaUrlFn: (path: string, port: number) => string,
) {
  const replay = useReplayStore()
  const thumbUrl = ref('')
  const thumbLoaded = ref(false)
  const liveDuration = ref(initialDuration)
  const liveWidth = ref(0)
  const liveHeight = ref(0)
  const trimmedDuration = ref<number | null>(null)

  let resolvedThumbPath = ''
  let removeTrimListener: (() => void) | null = null
  let observedEl: HTMLElement | null = null

  const displayDuration = computed(() => trimmedDuration.value ?? liveDuration.value)
  const isTrimmed = computed(() => trimmedDuration.value != null)

  const { observe, unobserve } = useSharedIntersectionObserver()

  function resolveThumbUrl(path: string) {
    const port = typeof mediaPort === 'number' ? mediaPort : mediaPort.value
    if (!port) return ''
    return mediaUrlFn(path, port)
  }

  function restoreThumb() {
    if (resolvedThumbPath && !thumbUrl.value) {
      thumbUrl.value = resolveThumbUrl(resolvedThumbPath)
    }
  }

  function unloadThumb() {
    if (thumbUrl.value) {
      thumbUrl.value = ''
      thumbLoaded.value = false
    }
  }

  // React to prefetchThumbnails() generating this clip's thumbnail.
  const unwatchLiveThumbs = watch(
    () => replay.liveThumbs.get(clipId),
    (path) => {
      if (path) {
        resolvedThumbPath = path
        restoreThumb()
      }
    },
  )

  // React to per-clip probe results.
  const unwatchLiveMeta = watch(
    () => replay.liveMeta.get(clipId),
    (meta) => {
      if (meta) {
        liveDuration.value = meta.duration
        liveWidth.value = meta.width
        liveHeight.value = meta.height
      }
    },
  )

  // RAM: clear/restore thumb when page is deactivated/activated by KeepAlive.
  const unwatchPageActive = watch(
    () => replay.pageActive,
    (active) => {
      if (!active) unloadThumb()
      else restoreThumb()
    },
  )

  function bind(el: HTMLElement) {
    observedEl = el
    let loadStarted = false
    observe(el, (entry) => {
      if (!entry.isIntersecting) {
        unloadThumb()
        return
      }
      restoreThumb()
      if (!loadStarted && !thumbUrl.value) {
        loadStarted = true
        const path = initialThumb || replay.liveThumbs.get(clipId) || ''
        if (path) {
          resolvedThumbPath = path
          restoreThumb()
        }
      }
    })
  }

  function setupTrimListener() {
    const trimListener = (event: Event) => {
      const detail = (event as CustomEvent<{ filepath?: string; trimStart?: number; trimEnd?: number }>).detail
      if (!detail || detail.filepath !== filepath) return
      if (typeof detail.trimStart === 'number' && typeof detail.trimEnd === 'number' && detail.trimEnd > detail.trimStart) {
        const nextDuration = Math.max(0, detail.trimEnd - detail.trimStart)
        trimmedDuration.value = nextDuration > 0 && Math.abs(nextDuration - liveDuration.value) > 0.05 ? nextDuration : null
        return
      }
      void loadTrimState()
    }
    window.addEventListener('clip-trim-updated', trimListener as EventListener)
    removeTrimListener = () => window.removeEventListener('clip-trim-updated', trimListener as EventListener)
  }

  async function loadTrimState() {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      const state = await invoke<{ trim_start: number; trim_end: number } | null>('get_trim_state', { filepath })
      if (state && state.trim_end > state.trim_start) {
        const nextDuration = Math.max(0, state.trim_end - state.trim_start)
        trimmedDuration.value = nextDuration > 0 && Math.abs(nextDuration - liveDuration.value) > 0.05 ? nextDuration : null
        return
      }
    } catch {}
    trimmedDuration.value = null
  }

  onMounted(() => {
    const initial = initialThumb || replay.liveThumbs.get(clipId) || ''
    if (initial) resolvedThumbPath = initial
    void loadTrimState()
    setupTrimListener()
  })

  onBeforeUnmount(() => {
    unwatchLiveThumbs()
    unwatchLiveMeta()
    unwatchPageActive()
    removeTrimListener?.()
    if (observedEl) unobserve(observedEl)
  })

  return {
    thumbUrl,
    thumbLoaded,
    liveDuration,
    liveWidth,
    liveHeight,
    displayDuration,
    isTrimmed,
    bind,
  }
}

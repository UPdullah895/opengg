import { ref } from 'vue'

/**
 * Shared "only one context menu open at a time" coordinator.
 *
 * Each menu owner (e.g. a DropZone instance) gets a unique id and calls `openMenu(id)`
 * when its menu opens. All other owners watch `activeMenuId` and close themselves when
 * it no longer matches their id. This enforces a single open mixer context menu across
 * the independent DropZone component instances (one per channel).
 */
const activeMenuId = ref<string | null>(null)
let _seq = 0

/** Mint a process-unique menu owner id. */
export function nextMenuId(): string {
  _seq += 1
  return `menu-${_seq}`
}

/** Mark `id` as the sole open menu (closes every other owner via the shared ref). */
export function openMenu(id: string) {
  activeMenuId.value = id
}

/** Clear the active menu if `id` currently owns it (no-op otherwise). */
export function closeMenu(id: string) {
  if (activeMenuId.value === id) activeMenuId.value = null
}

export { activeMenuId }

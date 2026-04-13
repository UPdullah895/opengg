/**
 * Extension runtime store — loads IIFE bundles from the media server,
 * merges extension locales into vue-i18n, and exposes settings components.
 *
 * Extensions are discovered on disk via the `scan_extensions` Tauri command.
 * Enabled extensions that declare a `main` bundle are fetched at app boot
 * from `http://localhost:<port>/ext/<id>/<main>` and evaluated as IIFEs.
 *
 * Each IIFE registers itself on the window global `window.__ext_<id>`:
 *   window.__ext_overlays_system = { settingsComponent: <VueComponent> }
 *
 * Locale files are fetched from `/ext/<id>/locales/<lang>.json` and merged
 * into the running vue-i18n instance under the `ext.<id>.*` namespace.
 */
import { defineStore } from 'pinia'
import { ref, shallowRef } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { i18n } from '../i18n'
import type { Component } from 'vue'

export interface ExtManifest {
  id: string
  name: string
  description: string
  version: string
  path: string
  has_settings?: boolean
  icon?: string | null
  main?: string | null
  ui?: string | null
}

interface ExtIIFEExport {
  settingsComponent?: Component | null
  [key: string]: unknown
}

export interface ExtRuntime {
  manifest: ExtManifest
  /** Vue component for the settings panel, or null if not provided */
  settingsComponent: Component | null
}

/** Locale codes that the loader attempts to merge from each extension. */
const LOCALE_CODES = ['en', 'ar']

export const useExtensionStore = defineStore('extensions', () => {
  /** Fully loaded extension runtimes, keyed by extension id */
  const runtimes = shallowRef<Record<string, ExtRuntime>>({})
  /** Whether the boot-time load sweep is in progress */
  const initializing = ref(false)
  /** Per-extension diagnostic error messages (empty string = no error) */
  const loadErrors = ref<Record<string, string>>({})

  /**
   * Fetch, evaluate, and register a single extension's IIFE bundle.
   * Locale files are merged as a side effect. Idempotent — safe to call
   * multiple times for the same extension (overwrites previous runtime).
   */
  async function loadExtension(manifest: ExtManifest, port: number): Promise<void> {
    if (!manifest.main || !port) return

    const extId = manifest.id
    const baseUrl = `http://localhost:${port}/ext/${encodeURIComponent(extId)}`

    // ── 1. Fetch the IIFE bundle ──────────────────────────────────────────────
    let src: string
    try {
      const res = await fetch(`${baseUrl}/${encodeURIComponent(manifest.main)}`)
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      src = await res.text()
    } catch (e) {
      loadErrors.value = { ...loadErrors.value, [extId]: `fetch bundle: ${e}` }
      return
    }

    // ── 2. Evaluate the IIFE ──────────────────────────────────────────────────
    // The bundle runs synchronously and is expected to set window.__ext_<id>.
    // Dashes in the extension id are mapped to underscores for the global key:
    //   "overlays-system"  →  window.__ext_overlays_system
    const globalKey = `__ext_${extId.replace(/-/g, '_')}`
    try {
      // eslint-disable-next-line no-new-func
      new Function(src)()
    } catch (e) {
      loadErrors.value = { ...loadErrors.value, [extId]: `eval: ${e}` }
      return
    }

    // ── 3. Merge extension locales ────────────────────────────────────────────
    for (const lang of LOCALE_CODES) {
      try {
        const lr = await fetch(`${baseUrl}/locales/${lang}.json`)
        if (!lr.ok) continue
        const locData = await lr.json() as Record<string, unknown>
        const existing = i18n.global.getLocaleMessage(lang) as Record<string, unknown>
        const extNs   = (existing.ext ?? {}) as Record<string, unknown>
        i18n.global.setLocaleMessage(lang, {
          ...existing,
          ext: { ...extNs, [extId]: locData },
        })
      } catch { /* locale file absent — acceptable */ }
    }

    // ── 4. Capture the exported component ────────────────────────────────────
    const exported = (window as unknown as Record<string, unknown>)[globalKey] as ExtIIFEExport | undefined

    runtimes.value = {
      ...runtimes.value,
      [extId]: {
        manifest,
        settingsComponent: exported?.settingsComponent ?? null,
      },
    }

    // Clear any previous error for this extension
    const errs = { ...loadErrors.value }
    delete errs[extId]
    loadErrors.value = errs
  }

  /**
   * Scan the extensions directory and load every enabled extension that
   * declares a `main` IIFE bundle. Called once at app boot after the media
   * server port is known.
   */
  async function loadAllEnabled(port: number): Promise<void> {
    initializing.value = true
    try {
      // Lazy import to avoid circular module dependency at load time
      const { usePersistenceStore } = await import('./persistence')
      const persist = usePersistenceStore()
      const manifests = await invoke<ExtManifest[]>('scan_extensions')
      for (const m of manifests) {
        const enabled = persist.state.extensions[m.id] ?? true
        if (enabled && m.main) {
          await loadExtension(m, port)
        }
      }
    } catch (e) {
      console.warn('[extensions] loadAllEnabled failed:', e)
    } finally {
      initializing.value = false
    }
  }

  /** Remove a runtime from the loaded map and clean its window global. */
  function unload(id: string): void {
    const next = { ...runtimes.value }
    delete next[id]
    runtimes.value = next
    const globalKey = `__ext_${id.replace(/-/g, '_')}`
    delete (window as unknown as Record<string, unknown>)[globalKey]
  }

  function getRuntime(id: string): ExtRuntime | null {
    return runtimes.value[id] ?? null
  }

  return { runtimes, initializing, loadErrors, loadExtension, loadAllEnabled, unload, getRuntime }
})

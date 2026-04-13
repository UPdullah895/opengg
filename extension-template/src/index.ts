/**
 * Extension entry point — builds to dist/index.iife.js via Vite.
 *
 * This file runs as an IIFE inside the OpenGG WebView. It has access to:
 *   - window.Vue       — full Vue 3 API (ref, computed, defineComponent, h, …)
 *   - window.opengg    — restricted Tauri bridge (see opengg.invoke docs)
 *
 * Register your extension by setting window.__ext_<id> where <id> is your
 * manifest `id` with dashes replaced by underscores.
 * e.g. id = "my-extension" → window.__ext_my_extension
 */
import { defineAsyncComponent } from 'vue'

// Dynamically import the settings component so it's only evaluated when needed.
// Vite will inline it into the IIFE bundle since there's no HTTP import here.
const SettingsPanel = defineAsyncComponent(() => import('./Settings.vue'))

window.__ext_my_extension = {
  /**
   * settingsComponent — shown in Settings → Extensions when the user clicks
   * the gear icon on this extension's card.
   * Must be a Vue 3 component (options object or setup function).
   */
  settingsComponent: SettingsPanel,
}

// Augment window type so TypeScript doesn't complain
declare global {
  interface Window {
    __ext_my_extension: {
      settingsComponent: typeof SettingsPanel | null
    }
  }
}

import { ref } from 'vue'

/** Cross-page navigation signal — set before emit('navigate', 'settings') to auto-select a tab. */
export const settingsTargetTab = ref<string | null>(null)

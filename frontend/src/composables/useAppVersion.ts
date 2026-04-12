import { ref } from 'vue'
import { getVersion } from '@tauri-apps/api/app'

// Module-level singleton — fetched once, shared across all consumers.
const appVersion = ref('')
let _loaded = false

export async function loadAppVersion() {
  if (_loaded) return
  try { appVersion.value = await getVersion() }
  catch { appVersion.value = '0.1.1' }
  _loaded = true
}

export { appVersion }

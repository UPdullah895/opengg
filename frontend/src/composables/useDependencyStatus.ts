import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface DependencyStatus {
  binary: string
  available: boolean
  feature: string
}

// Module-level singleton — fetched once, shared across all consumers.
const deps = ref<DependencyStatus[]>([])
let _loaded = false

export async function loadDependencyStatus() {
  if (_loaded) return
  try {
    deps.value = await invoke<DependencyStatus[]>('get_dependency_status')
  } catch (e) {
    console.error('Failed to load dependency status:', e)
    deps.value = []
  }
  _loaded = true
}

export function missing(feature: string): boolean {
  return deps.value.some(d => d.feature === feature && !d.available)
}

export function isAvailable(feature: string): boolean {
  return deps.value.some(d => d.feature === feature && d.available)
}

export function missingBinary(binary: string): boolean {
  const dep = deps.value.find(d => d.binary === binary)
  return dep ? !dep.available : true
}

export { deps }

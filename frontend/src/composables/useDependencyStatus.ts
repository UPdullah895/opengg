import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface DependencyStatus {
  binary: string
  available: boolean
  feature: string
}

export interface DeviceAccessStatus {
  ratbagd_available: boolean
  in_input_group: boolean
  in_audio_group: boolean
  in_video_group: boolean
  udev_rules_present: boolean
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

// Device access status singleton
const deviceAccess = ref<DeviceAccessStatus>({
  ratbagd_available: false,
  in_input_group: false,
  in_audio_group: false,
  in_video_group: false,
  udev_rules_present: false,
})
let _deviceLoaded = false

export async function loadDeviceAccessStatus() {
  if (_deviceLoaded) return
  try {
    deviceAccess.value = await invoke<DeviceAccessStatus>('get_device_access_status')
  } catch (e) {
    console.error('Failed to load device access status:', e)
    deviceAccess.value = {
      ratbagd_available: false,
      in_input_group: false,
      in_audio_group: false,
      in_video_group: false,
      udev_rules_present: false,
    }
  }
  _deviceLoaded = true
}

export { deps, deviceAccess }

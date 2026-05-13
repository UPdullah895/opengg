import { ref } from 'vue'
import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'

export type DeviceType = 'mouse' | 'headset'

export interface DeviceInfo {
  id: string
  name: string
  model: string
  deviceType: DeviceType
  vid: number
  pid: number
  // Mouse
  dpi?: number
  pollingRate?: number
  dpiOptions?: number[]
  // Headset
  batteryLevel?: number
  batteryCharging?: boolean
  sidetone?: number
  chatmix?: number
  capabilities?: string[]
  eqPresets?: Record<string, number[]>
  eqMeta?: { bands: number; min: number; max: number; step: number }
}

interface HeadsetLocal {
  inactiveTime: number
  micVolume: number
  micMuteLed: number
  volumeLimiter: boolean
  btPoweredOn: boolean
  btCallVolume: number
  eqPreset: number
}

function defaultHeadsetLocal(): HeadsetLocal {
  return { inactiveTime: 0, micVolume: 64, micMuteLed: 1,
    volumeLimiter: false, btPoweredOn: false, btCallVolume: 50, eqPreset: 0 }
}

export const useDeviceStore = defineStore('devices', () => {
  const devices = ref<DeviceInfo[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  const syncing = ref(new Set<string>())
  const headsetLocal = ref<Record<string, HeadsetLocal>>({})

  function getHeadsetLocal(id: string): HeadsetLocal {
    if (!headsetLocal.value[id]) {
      headsetLocal.value[id] = defaultHeadsetLocal()
    }
    return headsetLocal.value[id]
  }

  async function withSync<T>(deviceId: string, fn: () => Promise<T>): Promise<T> {
    syncing.value = new Set(syncing.value).add(deviceId)
    try {
      return await fn()
    } finally {
      const next = new Set(syncing.value)
      next.delete(deviceId)
      syncing.value = next
    }
  }

  async function fetchDevices() {
    loading.value = true
    error.value = null
    try {
      const json = await invoke<string>('get_devices')
      devices.value = JSON.parse(json) as DeviceInfo[]
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function setMouseDpi(deviceId: string, dpi: number) {
    devices.value = devices.value.map(d =>
      d.id === deviceId ? { ...d, dpi } : d
    )
    try {
      await invoke('set_mouse_dpi', { deviceId, dpi })
    } catch (e) {
      error.value = String(e)
      await fetchDevices()
    }
  }

  async function setMousePollingRate(deviceId: string, rate: number) {
    devices.value = devices.value.map(d =>
      d.id === deviceId ? { ...d, pollingRate: rate } : d
    )
    try {
      await invoke('set_mouse_polling_rate', { deviceId, rate })
    } catch (e) {
      error.value = String(e)
      await fetchDevices()
    }
  }

  async function setHeadsetSidetone(deviceId: string, level: number) {
    devices.value = devices.value.map(d =>
      d.id === deviceId ? { ...d, sidetone: level } : d
    )
    try {
      await invoke('set_headset_sidetone', { deviceId, level })
    } catch (e) {
      error.value = String(e)
      await fetchDevices()
    }
  }

  async function setHeadsetChatmix(deviceId: string, level: number) {
    devices.value = devices.value.map(d =>
      d.id === deviceId ? { ...d, chatmix: level } : d
    )
    try {
      await invoke('set_headset_chatmix', { deviceId, level })
    } catch (e) {
      error.value = String(e)
      await fetchDevices()
    }
  }

  async function setHeadsetInactiveTime(deviceId: string, minutes: number) {
    return withSync(deviceId, async () => {
      try {
        await invoke('set_headset_inactive_time', { deviceId, minutes })
        getHeadsetLocal(deviceId).inactiveTime = minutes
      } catch (e) { error.value = String(e) }
    })
  }

  async function setHeadsetMicVolume(deviceId: string, level: number) {
    return withSync(deviceId, async () => {
      try {
        await invoke('set_headset_mic_volume', { deviceId, level })
        getHeadsetLocal(deviceId).micVolume = level
      } catch (e) { error.value = String(e) }
    })
  }

  async function setHeadsetMicMuteLed(deviceId: string, brightness: number) {
    return withSync(deviceId, async () => {
      try {
        await invoke('set_headset_mic_mute_led', { deviceId, brightness })
        getHeadsetLocal(deviceId).micMuteLed = brightness
      } catch (e) { error.value = String(e) }
    })
  }

  async function setHeadsetVolumeLimiter(deviceId: string, enabled: boolean) {
    return withSync(deviceId, async () => {
      try {
        await invoke('set_headset_volume_limiter', { deviceId, enabled })
        getHeadsetLocal(deviceId).volumeLimiter = enabled
      } catch (e) { error.value = String(e) }
    })
  }

  async function setHeadsetBtPoweredOn(deviceId: string, enabled: boolean) {
    return withSync(deviceId, async () => {
      try {
        await invoke('set_headset_bt_powered_on', { deviceId, enabled })
        getHeadsetLocal(deviceId).btPoweredOn = enabled
      } catch (e) { error.value = String(e) }
    })
  }

  async function setHeadsetBtCallVolume(deviceId: string, level: number) {
    return withSync(deviceId, async () => {
      try {
        await invoke('set_headset_bt_call_volume', { deviceId, level })
        getHeadsetLocal(deviceId).btCallVolume = level
      } catch (e) { error.value = String(e) }
    })
  }

  async function setHeadsetEqPreset(deviceId: string, presetIdx: number) {
    return withSync(deviceId, async () => {
      try {
        await invoke('set_headset_eq_preset', { deviceId, presetIdx })
        getHeadsetLocal(deviceId).eqPreset = presetIdx
      } catch (e) { error.value = String(e) }
    })
  }

  async function setHeadsetEqCurve(deviceId: string, bandsJson: string) {
    return withSync(deviceId, async () => {
      try {
        await invoke('set_headset_eq_curve', { deviceId, bandsJson })
      } catch (e) { error.value = String(e) }
    })
  }

  return {
    devices,
    loading,
    error,
    syncing,
    headsetLocal,
    getHeadsetLocal,
    fetchDevices,
    setMouseDpi,
    setMousePollingRate,
    setHeadsetSidetone,
    setHeadsetChatmix,
    setHeadsetInactiveTime,
    setHeadsetMicVolume,
    setHeadsetMicMuteLed,
    setHeadsetVolumeLimiter,
    setHeadsetBtPoweredOn,
    setHeadsetBtCallVolume,
    setHeadsetEqPreset,
    setHeadsetEqCurve,
  }
})

import { defineStore } from 'pinia'
import { ref, computed, reactive } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { usePersistenceStore } from './persistence'

export interface AppInfo { id: number; name: string; binary: string; channel: string; icon: string; volume?: number; auto_channel?: string }
export interface Channel { name: string; volume: number; muted: boolean; node_id: number }
export interface AudioDevice { name: string; description: string; device_type: 'sink' | 'source'; is_default: boolean }

const CHANNEL_NAMES = ['Game', 'Chat', 'Media', 'Aux']

export const useAudioStore = defineStore('audio', () => {
  const allApps = ref<AppInfo[]>([])
  const devices = ref<AudioDevice[]>([])
  const vuLevels = ref<Record<string, number>>({})
  const loading = ref(false)
  const error = ref<string | null>(null)
  const channelDevices = reactive<Record<string, string>>({})
  const draggedApp = ref<AppInfo | null>(null)
  const routingInProgress = ref(false)
  let vuUnlisten: UnlistenFn | null = null

  // ★ P7: Reactive channel state with anti-snap debounce
  const channelVolumes = reactive<Record<string, number>>({ Master: 100, Mic: 100 })
  const channelMutes = reactive<Record<string, boolean>>({ Master: false, Mic: false })
  for (const n of CHANNEL_NAMES) { channelVolumes[n] = 100; channelMutes[n] = false }

  // ★ P7 FIX: Track which channels were manually changed recently.
  // Polling won't overwrite these for 3 seconds, preventing slider snap-back.
  const recentlyChanged: Record<string, number> = {}
  function markChanged(ch: string) { recentlyChanged[ch] = Date.now() }
  function wasRecentlyChanged(ch: string) { return (Date.now() - (recentlyChanged[ch] || 0)) < 3000 }

  // Channel map derived from allApps + reactive volumes
  const channelMap = computed(() => {
    const m: Record<string, { name: string; volume: number; muted: boolean; node_id: number; apps: AppInfo[] }> = {}
    for (const name of CHANNEL_NAMES) {
      m[name] = { name, volume: channelVolumes[name] ?? 100, muted: channelMutes[name] ?? false, node_id: 0, apps: [] }
    }
    for (const app of allApps.value) {
      if (app.channel && m[app.channel]) m[app.channel].apps.push(app)
    }
    return m
  })

  const outputDevices = computed(() => devices.value.filter(d => d.device_type === 'sink'))
  const inputDevices = computed(() => devices.value.filter(d => d.device_type === 'source'))
  const unassignedApps = computed(() =>
    allApps.value.filter(a => !a.channel && !a.name.toLowerCase().includes('opengg') && a.name.toLowerCase() !== 'wireplumber' && a.name.toLowerCase() !== 'pipewire')
  )
  const masterChannel = computed(() => ({
    name: 'Master',
    volume: channelVolumes.Master ?? 100,
    muted: channelMutes.Master ?? false,
    node_id: 0,
    apps: unassignedApps.value,
  }))

  // Mic is an input channel — apps are source-outputs captured from the mic (scanned via pactl).
  const micChannel = computed(() => ({
    name: 'Mic',
    volume: channelVolumes.Mic ?? 100,
    muted: channelMutes.Mic ?? false,
    node_id: 0,
    apps: allApps.value.filter(a => a.channel === 'Mic'),
  }))

  // ★ P8: fetchChannels reads real state from D-Bus/pactl, respecting debounce
  async function fetchChannels() {
    try {
      loading.value = true
      const j = await invoke<string>('get_channels')
      const chs = JSON.parse(j) as Channel[]
      for (const ch of chs) {
        // ★ P7: Don't overwrite channels the user just dragged
        if (!wasRecentlyChanged(ch.name)) {
          channelVolumes[ch.name] = ch.volume
          channelMutes[ch.name] = ch.muted
        }
      }
      error.value = null
    } catch {
      // D-Bus daemon not running — try reading Master from pactl directly
      try {
        await invoke<string>('get_apps') // force a pactl scan to populate sink state
      } catch {}
    } finally { loading.value = false }
  }

  // ★ P8: fetchApps reads real PipeWire routing.
  // Uses full array replacement (never property mutation) so Vue's reactivity
  // system tracks the change even for nested object properties.
  //
  // ★ FIX: The D-Bus path (daemon running) serializes `id` as a string inside
  // HashMap<String,String> — coerce to number so allApps.find(a => a.id === appId)
  // doesn't silently fail with "42" === 42 → false.
  async function fetchApps() {
    try {
      const j = await invoke<string>('get_apps')
      const fetched = JSON.parse(j) as AppInfo[]
      const rules = usePersistenceStore().state.mixer.appRules
      allApps.value = fetched.map(app => {
        const normalized: AppInfo = { ...app, id: Number(app.id) }
        const key = normalized.binary || normalized.name
        const rule = key ? rules[key] : undefined
        if (rule && rule !== 'default' && rule !== 'Master') {
          return { ...normalized, channel: rule }
        }
        return normalized
      })

      // Smart auto-routing: for apps with no saved rule and no current channel,
      // use the backend's classification suggestion. Only fires on first appearance
      // (once routed, the rule is saved and this branch is never reached again).
      for (const app of allApps.value) {
        if (app.channel || !app.auto_channel) continue
        const key = app.binary || app.name
        const rule = key ? rules[key] : undefined
        if (rule && rule !== 'default' && rule !== 'Master') continue
        // Route without waiting — optimistic update happens inside routeApp
        routeApp(app.id, app.auto_channel).catch(() => {})
      }
    } catch (e) { console.error('[opengg] fetchApps:', e) }
  }
  async function fetchDevices() {
    try { devices.value = await invoke<AudioDevice[]>('get_audio_devices') } catch {}
  }

  // ★ P7: setVolume marks the channel as recently changed
  async function setVolume(ch: string, vol: number) {
    channelVolumes[ch] = vol
    markChanged(ch) // prevents polling from snapping back
    usePersistenceStore().setChannelVolume(ch, vol)
    try { await invoke('set_volume', { channel: ch, volume: vol }) } catch (e) { console.error('[opengg] set_volume failed:', e) }
  }
  async function setMute(ch: string, muted: boolean) {
    channelMutes[ch] = muted
    markChanged(ch)
    usePersistenceStore().setChannelMute(ch, muted)
    try { await invoke('set_mute', { channel: ch, muted }) } catch (e) { console.error('[opengg] set_mute failed:', e) }
  }
  async function setAppVolume(appIndex: number, vol: number) {
    const app = allApps.value.find(a => a.id === appIndex); if (app) app.volume = vol
    try { await invoke('set_app_volume', { appIndex, volume: vol }) } catch {}
  }

  async function routeApp(appId: number, channel: string) {
    await invoke('route_app', { appId, channel })
    const app = allApps.value.find(a => a.id === appId)
    const key = app?.binary || app?.name
    if (key) usePersistenceStore().setAppRule(key, channel)
  }
  async function unrouteApp(id: number) { await routeApp(id, 'default') }

  async function setChannelDevice(ch: string, dev: string) {
    channelDevices[ch] = dev; usePersistenceStore().setChannelDevice(ch, dev)
    try { await invoke('set_channel_device', { channel: ch, deviceName: dev }) } catch {}
  }
  function restoreFromPersistence() {
    Object.assign(channelDevices, usePersistenceStore().state.mixer.devices)
    const saved = usePersistenceStore().state.mixer.volumes || {}
    for (const [k, v] of Object.entries(saved)) { if (typeof v === 'number') channelVolumes[k] = v }
  }

  function startDrag(app: AppInfo) { draggedApp.value = app }
  function endDrag() { draggedApp.value = null; routingInProgress.value = false }

  async function dropOnChannel(channel: string) {
    if (!draggedApp.value || routingInProgress.value) return
    await dropOnChannelById(draggedApp.value.id, channel)
  }

  // Explicit-ID variant used by DropZone when dataTransfer carries the app ID directly.
  // This is the canonical implementation; dropOnChannel delegates to it.
  async function dropOnChannelById(appId: number, channel: string) {
    if (routingInProgress.value) return
    routingInProgress.value = true
    const app = allApps.value.find(a => a.id === appId)
    if (!app) { routingInProgress.value = false; return }
    const prev = app.channel || ''
    draggedApp.value = null
    // Optimistic update: replace array so Vue reactivity is guaranteed
    const targetChannel = (channel === 'Master') ? '' : channel
    allApps.value = allApps.value.map(a => a.id === appId ? { ...a, channel: targetChannel } : a)
    try {
      if (channel === 'Master') await unrouteApp(appId)
      else await routeApp(appId, channel)
      // Delay refresh — let PipeWire settle before querying; optimistic update is already applied
      setTimeout(() => fetchApps(), 600)
    } catch (e) {
      console.error('[opengg] routeApp failed, reverting:', e)
      // Revert on failure
      allApps.value = allApps.value.map(a => a.id === appId ? { ...a, channel: prev } : a)
      await fetchApps()
    } finally { routingInProgress.value = false }
  }

  async function startVuStream() { try { await invoke('start_vu_stream'); vuUnlisten = await listen<{ channels: Record<string, number> }>('vu-levels', e => { vuLevels.value = e.payload.channels }) } catch {} }
  async function stopVuStream() { try { await invoke('stop_vu_stream'); vuUnlisten?.(); vuUnlisten = null } catch {} }

  let interval: ReturnType<typeof setInterval> | null = null
  function startPolling(ms = 2000) {
    stopPolling(); restoreFromPersistence()
    // ★ P8: First fetch reads real PipeWire state — apps appear in correct columns
    fetchChannels(); fetchApps(); fetchDevices(); startVuStream()
    interval = setInterval(() => { fetchChannels(); fetchApps() }, ms)
  }
  function stopPolling() { if (interval) { clearInterval(interval); interval = null }; stopVuStream() }

  return {
    allApps, devices, vuLevels, channelDevices, channelVolumes, channelMutes,
    unassignedApps, channelMap, masterChannel, micChannel, outputDevices, inputDevices,
    loading, error, draggedApp, routingInProgress,
    fetchChannels, fetchApps, fetchDevices,
    setVolume, setMute, setAppVolume, routeApp, unrouteApp,
    setChannelDevice, restoreFromPersistence,
    startDrag, endDrag, dropOnChannel, dropOnChannelById,
    startPolling, stopPolling,
  }
})

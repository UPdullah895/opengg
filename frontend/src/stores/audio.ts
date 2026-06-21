import { defineStore } from 'pinia'
import { ref, computed, reactive, shallowRef } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { usePersistenceStore } from './persistence'

export interface AppInfo { id: number; name: string; binary: string; channel: string; icon: string; volume?: number; auto_channel?: string; locked?: boolean }
export interface Channel { name: string; volume: number; muted: boolean; node_id: number }
export interface AudioDevice { name: string; description: string; device_type: 'sink' | 'source'; is_default: boolean }

const CHANNEL_NAMES = ['Game', 'Chat', 'Media', 'Aux']

export const useAudioStore = defineStore('audio', () => {
  const allApps = ref<AppInfo[]>([])
  const devices = ref<AudioDevice[]>([])
  const virtualAudioReady = ref(false)
  const checkingVirtualAudio = ref(true)
  const vuLevels = shallowRef<Record<string, number>>({})
  const loading = ref(false)
  const error = ref<string | null>(null)
  const channelDevices = reactive<Record<string, string>>({})
  const draggedApp = ref<AppInfo | null>(null)
  const selectedAppForClickRoute = ref<AppInfo | null>(null)
  const routingInProgress = ref(false)
  let vuUnlisten: UnlistenFn | null = null

  // ★ P7: Reactive channel state with anti-snap debounce
  const channelVolumes = reactive<Record<string, number>>({ Master: 100, Mic: 100 })
  const channelMutes = reactive<Record<string, boolean>>({ Master: false, Mic: false })
  for (const n of CHANNEL_NAMES) { channelVolumes[n] = 100; channelMutes[n] = false }

  // ★ Ear Blast Protection state
  const earBlastEnabled = ref(false)
  const earBlastChannels = ref<string[]>(['Game'])
  const earBlastThreshold = ref(85)
  const earBlastTarget = ref(60)
  const earBlastActiveChannels = ref<Set<string>>(new Set())
  let earBlastUnlisten: UnlistenFn | null = null

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
      if (app.channel && m[app.channel] && !isInternalApp(app.name)) m[app.channel].apps.push(app)
    }
    return m
  })

  const outputDevices = computed(() => devices.value.filter(d => d.device_type === 'sink'))
  const inputDevices = computed(() => devices.value.filter(d => d.device_type === 'source'))
  // Defensive filter (backend already excludes these): never show OpenGG-internal helpers
  // as user apps — VU readers (pw-cat), recorder captures (gsr-/gpu-screen-recorder),
  // mic loopback streams, or the audio server itself.
  const isInternalApp = (name: string) => {
    const n = (name || '').toLowerCase()
    return n.includes('opengg') || n === 'wireplumber' || n === 'pipewire'
      || n.includes('pw-cat') || n.includes('gsr-') || n.includes('gpu-screen-recorder')
      || n.includes('loopback')
  }
  // Master shows ONLY genuinely unrouted apps: not internal, not physically on a named
  // channel, AND not assigned to one by a saved rule (covers the brief window after a
  // reboot/restart before the rule is re-applied to the live stream).
  const unassignedApps = computed(() => {
    const rules = usePersistenceStore().state.mixer.appRules
    return allApps.value.filter(a => {
      if (isInternalApp(a.name)) return false
      if (a.channel) return false
      const key = a.binary || a.name
      const rule = key ? rules[key] : undefined
      return !(rule && rule !== 'default' && rule !== 'Master')
    })
  })
  const masterChannel = computed(() => ({
    name: 'Master',
    volume: channelVolumes.Master ?? 100,
    muted: channelMutes.Master ?? false,
    node_id: 0,
    // Master shows only real app streams NOT claimed by a named channel (Game/Chat/
    // Media/Aux/Mic) — i.e. streams going straight to the master/output. Apps routed
    // to a channel appear under that channel only, never duplicated here.
    apps: unassignedApps.value,
  }))

  // Mic is an input channel — apps are source-outputs captured from the mic (scanned via pactl).
  const micChannel = computed(() => ({
    name: 'Mic',
    volume: channelVolumes.Mic ?? 100,
    muted: channelMutes.Mic ?? false,
    node_id: 0,
    apps: allApps.value.filter(a => a.channel === 'Mic' && !isInternalApp(a.name)),
  }))

  // ★ P8: fetchChannels reads real state from D-Bus/pactl, respecting debounce
  async function fetchChannels() {
    try {
      loading.value = true
      const j = await invoke<string>('get_channels')
      if (import.meta.env.DEV) console.debug('[opengg][audio] fetchChannels raw:', j)
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

      // Re-apply saved routing rules to streams whose actual PipeWire channel doesn't
      // match the saved rule — happens after reboot, wake-from-sleep, or daemon restart.
      // Uses `fetched` (raw backend state) not `allApps.value` (already rule-overridden).
      // Cooldown guard prevents infinite loop when routing appears to fail (pw-metadata
      // reports success but stream move is not actually applied by PipeWire).
      for (const fetchedApp of fetched) {
        const key = fetchedApp.binary || fetchedApp.name
        const rule = key ? rules[key] : undefined
        if (!rule || rule === 'default' || rule === 'Master') continue
        if ((fetchedApp.channel || '') !== rule && !isRoutingCooldown(Number(fetchedApp.id))) {
          routeApp(Number(fetchedApp.id), rule, fetchedApp.binary).catch(() => {})
        }
      }

      // Smart auto-routing: for apps with no saved rule and no current channel,
      // use the backend's classification suggestion. Only fires on first appearance
      // (once routed, the rule is saved and this branch is never reached again).
      for (const app of allApps.value) {
        if (app.channel || !app.auto_channel) continue
        const key = app.binary || app.name
        const rule = key ? rules[key] : undefined
        if (rule && rule !== 'default') continue
        // ★ FIX: apply cooldown guard to auto-routing branch too
        if (isRoutingCooldown(app.id)) continue
        routeApp(app.id, app.auto_channel, app.binary).catch(() => {})
      }
    } catch (e) { console.error('[opengg] fetchApps:', e) }
  }
  async function fetchDevices() {
    try { devices.value = await invoke<AudioDevice[]>('get_audio_devices') } catch {}
  }

  async function refreshVirtualAudioStatus() {
    checkingVirtualAudio.value = true
    try {
      virtualAudioReady.value = await invoke<boolean>('check_virtual_audio_status')
    } catch {
      virtualAudioReady.value = false
    } finally {
      checkingVirtualAudio.value = false
    }
  }

  function setVirtualAudioReady(ready: boolean) {
    virtualAudioReady.value = ready
    checkingVirtualAudio.value = false
    if (!ready) stopPolling()
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

  // Routing cooldown: prevents the same app from being re-routed within ROUTING_COOLDOWN_MS
  // during polling. Fixes the infinite re-routing loop where pw-metadata reports success
  // but the stream isn't actually moved, causing fetchApps() to trigger routeApp again
  // every 2 seconds → thousands of command executions per minute.
  const pendingRoutes = new Map<number, number>()
  const ROUTING_COOLDOWN_MS = 8000

  function isRoutingCooldown(appId: number): boolean {
    const lastRoute = pendingRoutes.get(appId)
    if (lastRoute && Date.now() - lastRoute < ROUTING_COOLDOWN_MS) return true
    return false
  }
  function setRoutingDone(appId: number) { pendingRoutes.set(appId, Date.now()) }

  async function routeApp(appId: number, channel: string, binary?: string) {
    if (isRoutingCooldown(appId)) return
    pendingRoutes.set(appId, Date.now())
    try {
      await invoke('route_app', { appId, channel, binary: binary || '' })
      const app = allApps.value.find(a => a.id === appId)
      const key = app?.binary || app?.name
      if (key) usePersistenceStore().setAppRule(key, channel)
      setRoutingDone(appId) // success: start cooldown
    } catch (e) {
      // On failure, clear the cooldown so the next user attempt isn't swallowed.
      // The backend's circuit breaker will rate-limit if there are too many retries.
      pendingRoutes.delete(appId)
      throw e
    }
  }
  async function unrouteApp(id: number) { await routeApp(id, 'default') }

  async function setChannelDevice(ch: string, dev: string) {
    channelDevices[ch] = dev; usePersistenceStore().setChannelDevice(ch, dev)
    try { await invoke('set_channel_device', { channel: ch, deviceName: dev }) } catch {}
  }
  function restoreFromPersistence() {
    Object.assign(channelDevices, usePersistenceStore().state.mixer.devices)
    const saved = usePersistenceStore().state.mixer.volumes || {}
    if (import.meta.env.DEV) console.debug('[opengg][audio] restoreFromPersistence saved volumes:', JSON.stringify(saved))
    for (const [k, v] of Object.entries(saved)) { if (typeof v === 'number') channelVolumes[k] = v }
  }

  // Self-heal after the virtual audio engine is (re)created. The Mixer stops polling while
  // the engine isn't ready, and App.vue only (re)starts polling on a page change — so right
  // after creation the store keeps its default 100% volumes until a manual leave/return.
  // rehydrate() runs the same one-shot refresh page re-entry does (restore saved → fetch live
  // state) so correct volumes appear immediately with no navigation. Continuous polling is
  // resumed by MixerPage when the engine becomes ready (only relevant while on that page).
  function rehydrate() {
    if (import.meta.env.DEV) console.debug('[opengg][audio] rehydrate() after engine-created')
    restoreFromPersistence()
    fetchChannels()
    fetchApps()
    fetchDevices()
  }

  // Drag watchdog: if pointer events are lost (drag never completes), auto-clear the
  // drag state so the channel drop-target highlight doesn't stick until page navigation.
  let dragWatchdog: ReturnType<typeof setTimeout> | null = null
  function startDrag(app: AppInfo) {
    draggedApp.value = app
    if (dragWatchdog) clearTimeout(dragWatchdog)
    dragWatchdog = setTimeout(() => { draggedApp.value = null; routingInProgress.value = false }, 6000)
  }
  function endDrag() {
    if (dragWatchdog) { clearTimeout(dragWatchdog); dragWatchdog = null }
    draggedApp.value = null; routingInProgress.value = false
  }
  // Clear ALL transient interaction highlight (drag + click-to-route selection) — used by
  // the Mixer page on mouse-leave / focus-change so the "active" border reliably resets.
  function clearInteractionState() { endDrag(); deselectApp() }

  // Click-to-route selection is transient: it must not stay highlighted indefinitely.
  // Auto-clear after a short idle so the channel "drop here" highlight reverts on its own.
  let selectClearTimer: ReturnType<typeof setTimeout> | null = null
  function clearSelectTimer() {
    if (selectClearTimer) { clearTimeout(selectClearTimer); selectClearTimer = null }
  }
  function selectAppForRoute(app: AppInfo | null) {
    selectedAppForClickRoute.value = app
    clearSelectTimer()
    if (app) selectClearTimer = setTimeout(() => { selectedAppForClickRoute.value = null }, 4000)
  }
  function deselectApp() { clearSelectTimer(); selectedAppForClickRoute.value = null }

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
    clearSelectTimer()
    selectedAppForClickRoute.value = null
    // Optimistic update: replace array so Vue reactivity is guaranteed
    const targetChannel = (channel === 'Master') ? '' : channel
    allApps.value = allApps.value.map(a => a.id === appId ? { ...a, channel: targetChannel } : a)

    // Store the app's persistence key for potential rollback
    const appKey = app.binary || app.name
    const prevRule = appKey ? usePersistenceStore().state.mixer.appRules[appKey] : undefined

    try {
      if (channel === 'Master') await unrouteApp(appId)
      else await routeApp(appId, channel, app?.binary)
      // Delay refresh — let PipeWire settle before querying; optimistic update is already applied
      setTimeout(() => fetchApps(), 600)
    } catch (e) {
      console.error('[opengg] routeApp failed, reverting:', e)
      // Revert on failure: restore UI state AND persisted routing rule
      allApps.value = allApps.value.map(a => a.id === appId ? { ...a, channel: prev } : a)
      if (appKey && prevRule) {
        usePersistenceStore().setAppRule(appKey, prevRule)
      }
      await fetchApps()
    } finally { routingInProgress.value = false }
  }

  async function startVuStream() {
    try {
      await invoke('start_vu_stream')
      vuUnlisten = await listen<{ channels: Array<[string, number]> }>('vu-levels', e => {
        const map: Record<string, number> = {}
        for (const [ch, db] of e.payload.channels) { map[ch] = db }
        vuLevels.value = map
      })
      earBlastUnlisten = await listen<{ channel: string; active: boolean }>('ear-blast-state', e => {
        const s = new Set(earBlastActiveChannels.value)
        if (e.payload.active) s.add(e.payload.channel)
        else s.delete(e.payload.channel)
        earBlastActiveChannels.value = s
      })
    } catch {}
  }
  async function stopVuStream() {
    try {
      await invoke('stop_vu_stream')
      vuUnlisten?.(); vuUnlisten = null
      earBlastUnlisten?.(); earBlastUnlisten = null
    } catch {}
  }

  async function syncEarBlastState() {
    const persist = usePersistenceStore()
    const eb = persist.state.mixer.earBlast
    earBlastEnabled.value = eb.enabled
    earBlastChannels.value = [...eb.channels]
    earBlastThreshold.value = eb.threshold
    earBlastTarget.value = eb.target
    try {
      await invoke('set_ear_blast_enabled', { enabled: eb.enabled })
      await invoke('set_ear_blast_channels', { channels: eb.channels })
      await invoke('set_ear_blast_threshold', { percent: eb.threshold })
      await invoke('set_ear_blast_target', { percent: eb.target })
    } catch (e) { console.warn('[opengg] syncEarBlastState:', e) }
  }

  async function toggleEarBlast() {
    const persist = usePersistenceStore()
    const newVal = !persist.state.mixer.earBlast.enabled
    persist.setEarBlastEnabled(newVal)
    earBlastEnabled.value = newVal
    try { await invoke('set_ear_blast_enabled', { enabled: newVal }) } catch {}
  }

  async function setEarBlastChannels(chs: string[]) {
    const persist = usePersistenceStore()
    persist.setEarBlastChannels(chs)
    earBlastChannels.value = [...chs]
    try { await invoke('set_ear_blast_channels', { channels: chs }) } catch {}
  }

  async function setEarBlastThreshold(v: number) {
    const persist = usePersistenceStore()
    persist.setEarBlastThreshold(v)
    earBlastThreshold.value = v
    try { await invoke('set_ear_blast_threshold', { percent: v }) } catch {}
  }

  async function setEarBlastTarget(v: number) {
    const persist = usePersistenceStore()
    persist.setEarBlastTarget(v)
    earBlastTarget.value = v
    try { await invoke('set_ear_blast_target', { percent: v }) } catch {}
  }

  let interval: ReturnType<typeof setInterval> | null = null
  function startPolling(ms = 2000) {
    stopPolling(); restoreFromPersistence()
    // ★ P8: First fetch reads real PipeWire state — apps appear in correct columns
    fetchChannels(); fetchApps(); fetchDevices(); startVuStream()
    void syncEarBlastState()
    interval = setInterval(() => { fetchChannels(); fetchApps() }, ms)
  }
  function stopPolling() { if (interval) { clearInterval(interval); interval = null }; stopVuStream() }

  return {
    allApps, devices, vuLevels, channelDevices, channelVolumes, channelMutes,
    virtualAudioReady, checkingVirtualAudio,
    unassignedApps, channelMap, masterChannel, micChannel, outputDevices, inputDevices,
    loading, error, draggedApp, selectedAppForClickRoute, routingInProgress,
    earBlastEnabled, earBlastChannels, earBlastThreshold, earBlastTarget, earBlastActiveChannels,
    fetchChannels, fetchApps, fetchDevices,
    refreshVirtualAudioStatus, setVirtualAudioReady,
    setVolume, setMute, setAppVolume, routeApp, unrouteApp,
    setChannelDevice, restoreFromPersistence, rehydrate,
    startDrag, endDrag, clearInteractionState, selectAppForRoute, deselectApp, dropOnChannel, dropOnChannelById,
    startPolling, stopPolling,
    toggleEarBlast, setEarBlastChannels, setEarBlastThreshold, setEarBlastTarget, syncEarBlastState,
  }
})

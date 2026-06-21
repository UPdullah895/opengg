<script setup lang="ts">
/**
 * ★ FIX 4: PipeWire state sync architecture.
 *
 * The audio store polls `get_apps` every 2 seconds. This Rust command calls
 * the daemon D-Bus method which returns the CURRENT PipeWire node routing
 * state. If a user routes an app via pavucontrol, the next poll will pick
 * up the change and move the app to the correct column in our UI.
 *
 * For real-time sync without polling, the daemon would need to subscribe
 * to PipeWire events via pw-mon or libpipewire and emit Tauri events.
 * That's a Phase 2 optimization — polling at 2s is sufficient for now.
 */

import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen } from '@tauri-apps/api/event'
import { useAudioStore } from '../stores/audio'
import { usePersistenceStore } from '../stores/persistence'
import ChannelStrip from '../components/ChannelStrip.vue'
import DropZone from '../components/DropZone.vue'
import ChatMix from '../components/ChatMix.vue'
import GraphicEQ from '../components/GraphicEQ.vue'
import DspControls from '../components/DspControls.vue'

const { t } = useI18n()
const audio  = useAudioStore()
const persist = usePersistenceStore()

// ★ Epic 2: Overdrive toggle — expands all faders from 100% max to 150%
const overdriveEnabled = ref(false)
const MIXER_CHANNELS = ['Master', 'Game', 'Chat', 'Media', 'Aux', 'Mic'] as const
watch(overdriveEnabled, (enabled) => {
  if (!enabled) void clampChannelsTo100()
})

type Tab = 'mixer' | 'game' | 'chat' | 'media' | 'aux' | 'mic'
const activeTab = ref<Tab>('mixer')
const TABS: { id: Tab; label: string }[] = [
  { id: 'mixer', label: 'Mixer' },
  { id: 'game',  label: 'Game'  },
  { id: 'chat',  label: 'Chat'  },
  { id: 'media', label: 'Media' },
  { id: 'aux',   label: 'Aux'   },
  { id: 'mic',   label: 'Mic'   },
]

const setupLoading  = ref(false)

// ── Global app-list layout settings (one control for every channel's app box) ──
const appSettingsOpen = ref(false)
const appBoxCount = computed(() => Math.max(1, Math.min(12, persist.state.settings.appBoxCount ?? 3)))
const appBoxPerRow = computed<1 | 2>(() => (persist.state.settings.appBoxPerRow === 2 ? 2 : 1))
function setBoxCount(n: number) { persist.state.settings.appBoxCount = Math.max(1, Math.min(12, n)) }
function setBoxPerRow(n: 1 | 2) { persist.state.settings.appBoxPerRow = n }

function triggerVirtualAudioSetup() {
  window.dispatchEvent(new CustomEvent('openOnboarding', { detail: { step: 1 } }))
}

const COLORS: Record<string, string> = {
  Master: '#94A3B8', Game: '#E94560', Chat: '#3B82F6',
  Media: '#10B981', Aux: '#A855F7', Mic: '#F59E0B',
}

function onChatMix(g: number, c: number) { audio.setVolume('Game', g); audio.setVolume('Chat', c) }

async function clampChannelsTo100() {
  await Promise.all(
    MIXER_CHANNELS.map(async ch => {
      const current = audio.channelVolumes[ch] ?? 100
      if (current > 100) await audio.setVolume(ch, 100)
    }),
  )
}

function devDesc(ch: string, type: 'sink' | 'source') {
  const n = audio.channelDevices[ch]
  if (n) {
    const d = audio.devices.find(d => d.name === n)
    const desc = d?.description || n
    if (ch === 'Mic' && (n.toLowerCase().includes('opengg') || desc.toLowerCase().includes('opengg'))) {
      return 'mic OpenGG (Virtual)'
    }
    return desc
  }
  const defDev = audio.devices.find(d => d.device_type === type && d.is_default)
  if (ch === 'Mic' && defDev && (defDev.name.toLowerCase().includes('opengg') || (defDev.description || '').toLowerCase().includes('opengg'))) {
    return 'mic OpenGG (Virtual)'
  }
  return defDev?.description || 'Default'
}

let unlistenRefresh: (() => void) | null = null

function onMixerClick(e: MouseEvent) {
  // Deselect click-to-route app when clicking outside of any chip or dropzone
  const target = e.target as HTMLElement
  if (!target.closest('.dz-chip') && !target.closest('.dropzone')) {
    audio.deselectApp()
  }
  // Close the global app-settings popover when clicking outside it
  if (appSettingsOpen.value && !target.closest('.app-settings-wrap')) {
    appSettingsOpen.value = false
  }
}

// Click-to-route focus is transient: clear it on Escape and when the window loses focus.
function onMixerKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') { audio.deselectApp(); appSettingsOpen.value = false }
}
function onWindowBlur() { audio.deselectApp() }

// Switching mixer tabs ends the routing interaction → clear the highlight.
watch(activeTab, () => audio.deselectApp())

// Focus-change auto-deactivation: when the pointer leaves the mixer area, clear ALL
// transient highlight (drag + click-to-route) so the channel boxes return to grey
// without needing a page navigation.
function onMixerLeave() { audio.clearInteractionState() }

onMounted(async () => {
  if (!persist.loaded) await persist.load()
  await audio.refreshVirtualAudioStatus()
  // Polling is started/stopped centrally by App.vue based on currentPage.

  // Push-based refresh: backend emits 'audio-mixer-refresh' after every successful
  // route_app call so the UI updates immediately instead of waiting for the next poll.
  unlistenRefresh = await listen('audio-mixer-refresh', () => { audio.fetchApps(); audio.clearInteractionState() })
  document.addEventListener('click', onMixerClick)
  document.addEventListener('keydown', onMixerKeydown)
  window.addEventListener('blur', onWindowBlur)
})
onUnmounted(() => {
  unlistenRefresh?.()
  document.removeEventListener('click', onMixerClick)
  document.removeEventListener('keydown', onMixerKeydown)
  window.removeEventListener('blur', onWindowBlur)
  audio.deselectApp()
})

watch(() => audio.virtualAudioReady, ready => {
  // Polling is managed centrally by App.vue; this watch just ensures
  // we don't try to poll when virtual audio isn't ready.
  if (!ready) audio.stopPolling()
})

watch(overdriveEnabled, enabled => {
  if (!enabled) clampChannelsTo100().catch(() => {})
})
</script>

<template>
  <div class="mixer" @mouseleave="onMixerLeave">
    <div class="mixer-hdr">
        <div><h1 class="t">{{ t('dashboard.audioMixer') }}</h1></div>
      <div class="hdr-actions">
        <!-- Tab bar -->
        <nav class="tab-bar">
          <button
            v-for="tab in TABS" :key="tab.id"
            class="tab-btn"
            :class="{ 'tab-btn--active': activeTab === tab.id }"
            @click="activeTab = tab.id"
          >{{ tab.label }}</button>
        </nav>
        <!-- ★ Epic 2: Overdrive toggle — unlocks faders beyond 100% -->
        <button
          class="rfr"
          :class="{ 'rfr--active': overdriveEnabled }"
          :title="overdriveEnabled ? 'Overdrive ON — faders go to 150%' : 'Enable Overdrive (faders up to 150%)'"
          @click="overdriveEnabled = !overdriveEnabled"
        >
          <!-- Boost/unlock icon -->
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z"/>
          </svg>
        </button>
        <!-- ★ Ear Blast Protection toggle -->
        <button
          class="rfr"
          :class="{ 'rfr--active': audio.earBlastEnabled, 'rfr--limiting': audio.earBlastActiveChannels.size > 0 }"
          :title="audio.earBlastEnabled ? t('dashboard.earBlastOn') : t('dashboard.earBlastOff')"
          @click="audio.toggleEarBlast()"
        >
          <!-- Ear/shield icon -->
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M12 2C7 2 3 6 3 11v3a3 3 0 003 3h1a2 2 0 002-2v-3a2 2 0 00-2-2H6a5 5 0 015-5 5 5 0 015 5h-1a2 2 0 00-2 2v3a2 2 0 002 2h1a3 3 0 003-3v-3C21 6 17 2 12 2z"/>
            <circle v-if="audio.earBlastActiveChannels.size > 0" cx="18" cy="6" r="3" fill="var(--accent)" stroke="none"/>
          </svg>
        </button>
        <!-- ★ Global app-list layout settings (applies to every channel's app box) -->
        <div class="app-settings-wrap">
          <button
            class="rfr"
            :class="{ 'rfr--active': appSettingsOpen }"
            :title="t('devices.appBoxSettings')"
            @click.stop="appSettingsOpen = !appSettingsOpen"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="3"/>
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
            </svg>
          </button>
          <div v-if="appSettingsOpen" class="app-settings-pop" @click.stop>
            <div class="aps-title">{{ t('devices.appBoxSettings') }}</div>
            <div class="aps-row">
              <span class="aps-label">{{ t('devices.appsShown') }}</span>
              <div class="aps-stepper">
                <button @click="setBoxCount(appBoxCount - 1)" :disabled="appBoxCount <= 1">−</button>
                <span class="aps-val">{{ appBoxCount }}</span>
                <button @click="setBoxCount(appBoxCount + 1)" :disabled="appBoxCount >= 12">+</button>
              </div>
            </div>
            <div class="aps-row">
              <span class="aps-label">{{ t('devices.appsPerRow') }}</span>
              <div class="aps-seg">
                <button :class="{ active: appBoxPerRow === 1 }" @click="setBoxPerRow(1)">1</button>
                <button :class="{ active: appBoxPerRow === 2 }" @click="setBoxPerRow(2)">2</button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- ★ Virtual Audio empty state -->
    <div v-if="audio.checkingVirtualAudio" class="empty-state">
      <span class="empty-spin">⟳</span>
      <p>Checking audio engine…</p>
    </div>
    <div v-else-if="!audio.virtualAudioReady" class="empty-state">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="empty-icon"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>
      <p class="empty-title">Virtual Audio Engine not running</p>
      <p class="empty-desc">OpenGG virtual sinks are not loaded. Create them to start routing audio.</p>
      <button class="btn-setup" :disabled="setupLoading" @click="triggerVirtualAudioSetup">
        <svg v-if="!setupLoading" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:16px;height:16px"><path d="M12 5v14M5 12h14"/></svg>
        <span>{{ setupLoading ? 'Creating…' : 'Create Virtual Audio Engine' }}</span>
      </button>
    </div>

    <template v-else>
      <!-- DSP / EQ tab panels -->
      <div v-if="activeTab !== 'mixer'" class="tab-panel">
        <GraphicEQ   v-if="activeTab === 'game'"  channel="Game"  :color="COLORS.Game"  />
        <GraphicEQ   v-if="activeTab === 'media'" channel="Media" :color="COLORS.Media" />
        <GraphicEQ   v-if="activeTab === 'aux'"   channel="Aux"   :color="COLORS.Aux"   />
        <template v-if="activeTab === 'chat'">
          <GraphicEQ   channel="Chat" :color="COLORS.Chat" />
          <DspControls channel="Chat" :color="COLORS.Chat" />
        </template>
        <DspControls v-if="activeTab === 'mic'"   channel="Mic"   :color="COLORS.Mic"   />
      </div>

      <!--
        ★ FIX 3: strips-row now uses min-height:55vh so faders are tall.
        The flex:1 lets it grow if the window is maximized.
      -->
      <div v-else class="strips-row">
      <!-- MASTER -->
      <div class="col">
        <ChannelStrip :channel="audio.masterChannel" :color="COLORS.Master" type="master" :vuLevel="audio.vuLevels['Master'] ?? -60"
          :overdrive="overdriveEnabled"
          :devices="audio.outputDevices" :selectedDevice="devDesc('Master','sink')"
          @update:volume="v=>audio.setVolume('Master',v)" @update:mute="m=>audio.setMute('Master',m)" @update:device="d=>audio.setChannelDevice('Master',d)">
          <template #icon><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M19.07 4.93a10 10 0 010 14.14M15.54 8.46a5 5 0 010 7.07"/></svg></template>
        </ChannelStrip>
        <DropZone channel="Master" :color="COLORS.Master" :apps="audio.masterChannel.apps" />
      </div>

      <!-- OUTPUTS -->
      <div class="col" v-for="ch in ['Game','Chat','Media','Aux']" :key="ch">
        <ChannelStrip
          :channel="audio.channelMap[ch] || { name: ch, volume: 100, muted: false, node_id: 0, apps: [] }"
          :color="COLORS[ch]" type="output" :vuLevel="audio.vuLevels[ch] ?? -60"
          :overdrive="overdriveEnabled"
          :devices="audio.outputDevices" :selectedDevice="devDesc(ch,'sink')"
          @update:volume="v=>audio.setVolume(ch,v)" @update:mute="m=>audio.setMute(ch,m)"
          @update:device="d=>audio.setChannelDevice(ch,d)">
          <template #icon>
            <svg v-if="ch==='Game'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="6" y1="12" x2="10" y2="12"/><line x1="8" y1="10" x2="8" y2="14"/><circle cx="15" cy="13" r="1" fill="currentColor"/><circle cx="18" cy="11" r="1" fill="currentColor"/><path d="M2 6a2 2 0 012-2h16a2 2 0 012 2v10a4 4 0 01-4 4h-2.5L14 18h-4l-1.5 2H6a4 4 0 01-4-4V6z"/></svg>
            <svg v-else-if="ch==='Chat'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 18v-6a9 9 0 0118 0v6"/><path d="M21 19a2 2 0 01-2 2h-1a2 2 0 01-2-2v-3a2 2 0 012-2h3zM3 19a2 2 0 002 2h1a2 2 0 002-2v-3a2 2 0 00-2-2H3z"/></svg>
            <svg v-else-if="ch==='Media'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><polygon points="10 8 16 12 10 16 10 8" fill="currentColor"/></svg>
            <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></svg>
          </template>
        </ChannelStrip>
        <DropZone :channel="ch" :color="COLORS[ch]" :apps="(audio.channelMap[ch]?.apps || [])" />
      </div>

      <!-- INPUT: Mic -->
      <div class="col">
        <ChannelStrip
          :channel="{ ...audio.micChannel, name: 'Mic' }"
          :color="COLORS.Mic" type="input" :vuLevel="audio.vuLevels['Mic'] ?? -60"
          :overdrive="overdriveEnabled"
          :devices="audio.inputDevices" :selectedDevice="devDesc('Mic','source')"
          @update:volume="v=>audio.setVolume('Mic',v)" @update:mute="m=>audio.setMute('Mic',m)"
          @update:device="d=>audio.setChannelDevice('Mic',d)">
          <template #icon><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 1a3 3 0 00-3 3v8a3 3 0 006 0V4a3 3 0 00-3-3z"/><path d="M19 10v2a7 7 0 01-14 0v-2"/><line x1="12" y1="19" x2="12" y2="23"/><line x1="8" y1="23" x2="16" y2="23"/></svg></template>
        </ChannelStrip>
        <DropZone channel="Mic" :color="COLORS.Mic" :apps="audio.micChannel.apps" />
      </div>
      </div><!-- end strips-row -->
    </template><!-- end v-else audioReady -->

    <ChatMix v-if="audio.virtualAudioReady" :gameVolume="audio.channelMap['Game']?.volume ?? 100"
      :chatVolume="audio.channelMap['Chat']?.volume ?? 100" @update:balance="onChatMix" />
  </div>
</template>

<style scoped>
.mixer     { display: flex; flex-direction: column; gap: 14px; height: 100%; }
.mixer-hdr { display: flex; align-items: center; justify-content: space-between; flex-shrink: 0; flex-wrap: wrap; gap: 8px; }
.t   { font-size: 20px; font-weight: 800; letter-spacing: -.3px; line-height: 1.1; }
.sub { font-size: 11px; color: var(--text-muted); }
.hdr-actions { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }

/* Tab navigation */
.tab-bar { display: flex; gap: 2px; background: var(--bg-deep); border: 1px solid var(--border); border-radius: 8px; padding: 2px; }
.tab-btn {
  padding: 5px 12px; border-radius: 6px; border: none; background: transparent;
  color: var(--text-muted); font-size: 12px; font-weight: 600; cursor: pointer;
  transition: all .15s; white-space: nowrap;
}
.tab-btn:hover { color: var(--accent); background: color-mix(in srgb, var(--accent) 10%, transparent); }
.tab-btn--active { background: var(--bg-card); color: var(--text); box-shadow: 0 1px 3px rgba(0,0,0,.2); }

/* DSP / EQ tab panel */
.tab-panel { flex: 1; overflow-y: auto; padding: 4px 2px; display: flex; flex-direction: column; gap: 16px; }
.rfr {
  width: 34px; height: 34px; border-radius: 8px;
  border: 1px solid var(--border); background: var(--bg-card);
  color: var(--text-sec); cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  transition: all .15s;
}
.rfr svg { width: 15px; height: 15px; }
.rfr:hover { background: var(--bg-hover); color: var(--text); }

/* Global app-list settings popover */
.app-settings-wrap { position: relative; }
.app-settings-pop {
  position: absolute; top: calc(100% + 6px); right: 0; z-index: 1000; width: 200px;
  background: var(--bg-card); border: 1px solid var(--border); border-radius: 8px; padding: 10px;
  box-shadow: 0 8px 24px rgba(0,0,0,.45); display: flex; flex-direction: column; gap: 10px;
}
.aps-title { font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: .6px; color: var(--text-muted); }
.aps-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.aps-label { font-size: 12px; color: var(--text-sec); }
.aps-stepper { display: flex; align-items: center; gap: 6px; }
.aps-stepper button { width: 24px; height: 24px; border-radius: 5px; border: 1px solid var(--border); background: var(--bg-deep); color: var(--text); cursor: pointer; font-size: 15px; line-height: 1; display: flex; align-items: center; justify-content: center; }
.aps-stepper button:disabled { opacity: .4; cursor: not-allowed; }
.aps-val { min-width: 18px; text-align: center; font-size: 13px; font-weight: 700; font-variant-numeric: tabular-nums; }
.aps-seg { display: flex; gap: 2px; background: var(--bg-deep); border: 1px solid var(--border); border-radius: 6px; padding: 2px; }
.aps-seg button { width: 28px; height: 22px; border: none; border-radius: 4px; background: transparent; color: var(--text-muted); cursor: pointer; font-size: 12px; font-weight: 700; }
.aps-seg button.active { background: var(--bg-card); color: var(--text); box-shadow: 0 1px 2px rgba(0,0,0,.3); }
/* ★ Epic 2: overdrive active state */
.rfr--active {
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border-color: color-mix(in srgb, var(--accent) 53%, transparent);
  color: var(--accent);
}
.rfr--active:hover { background: color-mix(in srgb, var(--accent) 20%, transparent); }
/* ★ Ear Blast: pulsing border when actively limiting */
.rfr--limiting {
  animation: ear-blast-pulse 1.2s ease-in-out infinite;
}
@keyframes ear-blast-pulse {
  0%, 100% { border-color: color-mix(in srgb, var(--accent) 53%, transparent); box-shadow: 0 0 0 0 color-mix(in srgb, var(--accent) 30%, transparent); }
  50% { border-color: color-mix(in srgb, var(--accent) 80%, transparent); box-shadow: 0 0 6px 0 color-mix(in srgb, var(--accent) 40%, transparent); }
}

/* ★ FIX 3: strips-row fills vertical space, min 55vh for tall faders */
.strips-row {
  display: flex;
  gap: 10px;
  align-items: stretch;   /* ← all cols same height */
  flex: 1;
  min-height: 55vh;       /* ← guarantees tall faders */
}

.col {
  display: flex;
  flex-direction: column;
  gap: 6px;
  align-items: center;
  flex: 1;
  min-width: 110px;
  max-width: 200px;
  min-height: 0; /* ← lets the column contract properly inside strips-row */
}
/* ★ Epic 1: Strict flex rules — strip grows to fill, dropzone is fixed-height scrollable */
.col :deep(.strip)    { width: 100% !important; flex: 1 1 0% !important; min-height: 0 !important; height: auto !important; }
.col :deep(.dropzone) { width: 100% !important; flex: 0 0 var(--box-h, 70px) !important; height: var(--box-h, 70px) !important; overflow-y: auto; scrollbar-width: thin; scrollbar-color: var(--border) transparent; }

/* ★ Empty state */
.empty-state {
  flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
  gap: 12px; color: var(--text-muted); text-align: center; padding: 40px;
}
.empty-icon { width: 48px; height: 48px; opacity: .35; }
.empty-spin { font-size: 32px; opacity: .4; animation: spin 1.2s linear infinite; display: inline-block; }
@keyframes spin { to { transform: rotate(360deg); } }
.empty-title { font-size: 16px; font-weight: 700; color: var(--text-sec); margin: 0; }
.empty-desc  { font-size: 12px; max-width: 300px; margin: 0; line-height: 1.5; }
.btn-setup {
  display: flex; align-items: center; gap: 8px;
  padding: 10px 20px; border-radius: 8px; border: none; cursor: pointer;
  background: var(--accent); color: #fff; font-size: 13px; font-weight: 600;
  transition: opacity .15s;
}
.btn-setup:hover { opacity: .85; }
.btn-setup:disabled { opacity: .5; cursor: not-allowed; }
</style>

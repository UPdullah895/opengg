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

import { onMounted, onUnmounted } from 'vue'
import { useAudioStore } from '../stores/audio'
import { usePersistenceStore } from '../stores/persistence'
import ChannelStrip from '../components/ChannelStrip.vue'
import DropZone from '../components/DropZone.vue'
import ChatMix from '../components/ChatMix.vue'

const audio = useAudioStore()
const persist = usePersistenceStore()

const COLORS: Record<string, string> = {
  Master: '#94A3B8', Game: '#E94560', Chat: '#3B82F6',
  Media: '#10B981', Aux: '#A855F7', Mic: '#F59E0B',
}

function onChatMix(g: number, c: number) { audio.setVolume('Game', g); audio.setVolume('Chat', c) }

function devDesc(ch: string, t: 'sink' | 'source') {
  const n = audio.channelDevices[ch]
  if (n) { const d = audio.devices.find(d => d.name === n); return d?.description || n }
  return audio.devices.find(d => d.device_type === t && d.is_default)?.description || 'Default'
}

onMounted(async () => {
  if (!persist.loaded) await persist.load()
  // ★ FIX 4: Poll every 2s (down from 3s) for tighter PipeWire sync
  audio.startPolling(2000)
})
onUnmounted(() => audio.stopPolling())
</script>

<template>
  <div class="mixer">
    <div class="mixer-hdr">
      <div><h1 class="t">Audio Mixer</h1><span class="sub">OpenGG Virtual Audio Router</span></div>
      <button class="rfr" @click="audio.fetchChannels(); audio.fetchApps()">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 11-2.12-9.36L23 10"/></svg>
      </button>
    </div>

    <!--
      ★ FIX 3: strips-row now uses min-height:55vh so faders are tall.
      The flex:1 lets it grow if the window is maximized.
    -->
    <div class="strips-row">
      <!-- MASTER -->
      <div class="col">
        <div class="col-lbl col-lbl--master">MASTER</div>
        <ChannelStrip :channel="audio.masterChannel" :color="COLORS.Master" type="master" :vuLevel="0.6"
          :devices="audio.outputDevices" :selectedDevice="devDesc('Master','sink')"
          @update:volume="()=>{}" @update:mute="()=>{}" @update:device="d=>audio.setChannelDevice('Master',d)">
          <template #icon><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M19.07 4.93a10 10 0 010 14.14M15.54 8.46a5 5 0 010 7.07"/></svg></template>
        </ChannelStrip>
        <DropZone channel="Master" :color="COLORS.Master" :apps="audio.masterChannel.apps" />
      </div>
      <div class="divider"><div class="dv"></div></div>

      <!-- OUTPUTS -->
      <div class="col" v-for="ch in ['Game','Chat','Media','Aux']" :key="ch">
        <div class="col-lbl">{{ ch.toUpperCase() }}</div>
        <ChannelStrip
          :channel="audio.channelMap[ch] || { name: ch, volume: 100, muted: false, node_id: 0, apps: [] }"
          :color="COLORS[ch]" type="output" :vuLevel="audio.vuLevels[ch] ?? 0"
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
      <div class="divider"><div class="dv"></div></div>

      <!-- INPUT: Mic -->
      <div class="col">
        <div class="col-lbl col-lbl--input">INPUT</div>
        <ChannelStrip
          :channel="audio.channelMap['Mic'] || { name: 'Mic', volume: 100, muted: false, node_id: 0, apps: [] }"
          :color="COLORS.Mic" type="input" :vuLevel="audio.vuLevels['Mic'] ?? 0"
          :devices="audio.inputDevices" :selectedDevice="devDesc('Mic','source')"
          @update:volume="v=>audio.setVolume('Mic',v)" @update:mute="m=>audio.setMute('Mic',m)"
          @update:device="d=>audio.setChannelDevice('Mic',d)">
          <template #icon><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 1a3 3 0 00-3 3v8a3 3 0 006 0V4a3 3 0 00-3-3z"/><path d="M19 10v2a7 7 0 01-14 0v-2"/><line x1="12" y1="19" x2="12" y2="23"/><line x1="8" y1="23" x2="16" y2="23"/></svg></template>
        </ChannelStrip>
      </div>
    </div>

    <ChatMix :gameVolume="audio.channelMap['Game']?.volume ?? 100"
      :chatVolume="audio.channelMap['Chat']?.volume ?? 100" @update:balance="onChatMix" />
  </div>
</template>

<style scoped>
.mixer { display: flex; flex-direction: column; gap: 14px; height: 100%; }
.mixer-hdr { display: flex; align-items: center; justify-content: space-between; flex-shrink: 0; }
.t { font-size: 20px; font-weight: 800; letter-spacing: -.3px; line-height: 1.1; }
.sub { font-size: 11px; color: var(--text-muted); }
.rfr { width: 34px; height: 34px; border-radius: 8px; border: 1px solid var(--border); background: var(--bg-card); color: var(--text-sec); cursor: pointer; display: flex; align-items: center; justify-content: center; }
.rfr svg { width: 15px; height: 15px; } .rfr:hover { background: var(--bg-hover); color: var(--text); }

/* ★ FIX 3: strips-row fills vertical space, min 55vh for tall faders */
.strips-row {
  display: flex;
  gap: 0;
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
}
.col :deep(.strip) { width: 100% !important; }
.col :deep(.dropzone) { width: 100% !important; }

.col-lbl { font-size: 9px; font-weight: 800; letter-spacing: 2px; color: var(--text-muted); text-align: center; min-height: 14px; flex-shrink: 0; }
.col-lbl--input { color: #F59E0B; }
.col-lbl--master { color: #94A3B8; }

.divider { display: flex; align-items: center; padding: 0 4px; align-self: stretch; flex-shrink: 0; }
.dv { width: 1px; height: 100%; background: linear-gradient(180deg, transparent, var(--border) 15%, var(--text-muted) 50%, var(--border) 85%, transparent); opacity: .4; }
</style>

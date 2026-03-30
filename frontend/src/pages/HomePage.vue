<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { usePersistenceStore } from '../stores/persistence'

const emit = defineEmits<{ navigate: [page: string] }>()

const persist = usePersistenceStore()
const recorderStatus = ref('idle')
const clipCount = ref(0)
const gsrWarning = ref(false)

onMounted(async () => {
  try {
    recorderStatus.value = await invoke<string>('get_recorder_status')
  } catch { /* daemon may not be running */ }
  try {
    clipCount.value = await invoke<number>('get_clips_count', { folder: '' })
  } catch { /* clips folder may not exist yet */ }
  // Check if GSR is enabled in settings but not actually running
  if (persist.state.settings.gsrEnabled) {
    try {
      const running = await invoke<boolean>('is_gsr_running')
      gsrWarning.value = !running
    } catch { gsrWarning.value = true }
  }
})

const updates = [
  { version: 'V0.1.0', date: '2026-03-29', items: ['PipeWire routing leak fix — no more dual-device audio bleed', 'Smooth 60 Hz VU meters with attack/release ballistics', 'Overdrive mode — faders extend to 150%', 'System tray with background-run and autostart support', 'Crash log capture → ~/.local/share/opengg/opengg_crash.log'] },
  { version: 'V0.0.9', date: '2026-02-14', items: ['Advanced timeline editor with per-track filters', 'FFmpeg export pipeline with real-time progress', 'Waveform preview in clip editor', 'Dark theme v2 — #16213E base, accent-driven borders'] },
  { version: 'V0.0.8', date: '2026-01-20', items: ['Lazy clip grid rendering via Intersection Observer', 'Mic input strip with NC toggle', 'Per-app volume sliders in drop zones', 'i18n scaffold (Arabic + English)'] },
]
</script>

<template>
  <div class="home">
    <h1 class="page-title">Dashboard</h1>

    <!-- GSR warning banner -->
    <div v-if="gsrWarning" class="gsr-warn">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="gsr-warn-ic"><path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
      <span>GPU Screen Recorder is enabled but not running. Check Settings → Capture &amp; Sound to restart it.</span>
      <button class="gsr-warn-dismiss" @click="gsrWarning=false">✕</button>
    </div>

    <div class="grid">
      <div class="card" @click="emit('navigate', 'mixer')">
        <div class="card-head">
          <span class="card-label">Audio Mixer</span>
          <div class="card-icon accent">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="4" y1="21" x2="4" y2="14"/><line x1="4" y1="10" x2="4" y2="3"/><line x1="12" y1="21" x2="12" y2="12"/><line x1="12" y1="8" x2="12" y2="3"/><line x1="20" y1="21" x2="20" y2="16"/><line x1="20" y1="12" x2="20" y2="3"/></svg>
          </div>
        </div>
        <div class="card-val">5 Channels</div>
        <div class="card-sub">Game · Chat · Media · Aux · Mic</div>
      </div>

      <div class="card">
        <div class="card-head">
          <span class="card-label">Recorder</span>
          <div class="card-icon red">
            <svg viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="12" r="8"/></svg>
          </div>
        </div>
        <div class="card-val">{{ recorderStatus === 'idle' ? 'Idle' : 'Active' }}</div>
        <div class="card-sub">{{ recorderStatus }}</div>
      </div>

      <div class="card" @click="emit('navigate', 'clips')">
        <div class="card-head">
          <span class="card-label">Clips</span>
          <div class="card-icon green">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2"/></svg>
          </div>
        </div>
        <div class="card-val">{{ clipCount }}</div>
        <div class="card-sub">Video clips saved</div>
      </div>

      <div class="card" @click="emit('navigate', 'devices')">
        <div class="card-head">
          <span class="card-label">Devices</span>
          <div class="card-icon purple">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 18v-6a9 9 0 0118 0v6"/><path d="M21 19a2 2 0 01-2 2h-1a2 2 0 01-2-2v-3a2 2 0 012-2h3zM3 19a2 2 0 002 2h1a2 2 0 002-2v-3a2 2 0 00-2-2H3z"/></svg>
          </div>
        </div>
        <div class="card-val">Scan</div>
        <div class="card-sub">Mouse · Keyboard · RGB</div>
      </div>
    </div>

    <h2 class="section-title">Latest Engine Updates</h2>
    <div class="updates">
      <div class="update-card" v-for="u in updates" :key="u.version">
        <div class="update-hdr">
          <span class="update-ver">{{ u.version }}</span>
          <span class="update-date">{{ u.date }}</span>
        </div>
        <ul class="update-list">
          <li v-for="item in u.items" :key="item">{{ item }}</li>
        </ul>
      </div>
    </div>
  </div>
</template>

<style scoped>
.home { display: flex; flex-direction: column; gap: 20px; }
.gsr-warn {
  display: flex; align-items: center; gap: 10px; padding: 10px 14px;
  background: rgba(245,158,11,.1); border: 1px solid rgba(245,158,11,.3);
  border-radius: 8px; font-size: 12px; color: #f59e0b; line-height: 1.4;
}
.gsr-warn-ic { width: 16px; height: 16px; flex-shrink: 0; }
.gsr-warn span { flex: 1; }
.gsr-warn-dismiss { background: transparent; border: none; color: rgba(245,158,11,.6); cursor: pointer; font-size: 14px; padding: 0 2px; line-height: 1; }
.gsr-warn-dismiss:hover { color: #f59e0b; }
.page-title { font-size: 22px; font-weight: 700; }
.grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
.card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 16px;
  cursor: pointer;
  transition: border-color .15s;
}
.card:hover { border-color: var(--text-muted); }
.card-head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px; }
.card-label { font-size: 13px; font-weight: 600; text-transform: uppercase; letter-spacing: .8px; color: var(--text-sec); }
.card-icon { width: 36px; height: 36px; border-radius: var(--radius); display: flex; align-items: center; justify-content: center; }
.card-icon svg { width: 18px; height: 18px; }
.card-icon.accent { background: rgba(233,69,96,.1); color: var(--accent); }
.card-icon.red { background: rgba(239,68,68,.1); color: #EF4444; }
.card-icon.green { background: rgba(16,185,129,.1); color: var(--success); }
.card-icon.purple { background: rgba(168,85,247,.1); color: var(--purple); }
.card-val { font-size: 24px; font-weight: 700; line-height: 1.2; }
.card-sub { font-size: 12px; color: var(--text-muted); margin-top: 3px; }

.section-title { font-size: 15px; font-weight: 700; color: var(--text-sec); letter-spacing: .3px; }

.updates { display: flex; flex-direction: column; gap: 10px; }
.update-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 14px 16px;
}
.update-hdr { display: flex; align-items: center; gap: 10px; margin-bottom: 8px; }
.update-ver { font-size: 13px; font-weight: 700; color: var(--accent); }
.update-date { font-size: 11px; color: var(--text-muted); margin-left: auto; }
.update-list { margin: 0; padding-left: 18px; display: flex; flex-direction: column; gap: 4px; }
.update-list li { font-size: 12px; color: var(--text-sec); line-height: 1.5; }
</style>

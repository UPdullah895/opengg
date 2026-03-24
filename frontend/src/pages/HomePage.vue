<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const emit = defineEmits<{ navigate: [page: string] }>()

const recorderStatus = ref('idle')
const clipCount = ref(0)

onMounted(async () => {
  try {
    recorderStatus.value = await invoke<string>('get_recorder_status')
  } catch { /* daemon may not be running */ }
})
</script>

<template>
  <div>
    <h1 class="page-title">Dashboard</h1>
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
        <div class="card-sub">Video clips</div>
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

    <h2 class="section-title">Quick Actions</h2>
    <div class="actions">
      <button class="btn" @click="emit('navigate', 'mixer')">Open Mixer</button>
      <button class="btn" @click="emit('navigate', 'clips')">Browse Clips</button>
      <button class="btn" @click="emit('navigate', 'settings')">Settings</button>
    </div>
  </div>
</template>

<style scoped>
.page-title { font-size: 22px; font-weight: 700; margin-bottom: 24px; }
.grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; margin-bottom: 28px; }
.card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 20px;
  cursor: pointer;
  transition: border-color .15s;
}
.card:hover { border-color: var(--text-muted); }
.card-head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
.card-label { font-size: 13px; font-weight: 600; text-transform: uppercase; letter-spacing: .8px; color: var(--text-sec); }
.card-icon { width: 40px; height: 40px; border-radius: var(--radius); display: flex; align-items: center; justify-content: center; }
.card-icon svg { width: 20px; height: 20px; }
.card-icon.accent { background: rgba(233,69,96,.1); color: var(--accent); }
.card-icon.red { background: rgba(239,68,68,.1); color: #EF4444; }
.card-icon.green { background: rgba(16,185,129,.1); color: var(--success); }
.card-icon.purple { background: rgba(168,85,247,.1); color: var(--purple); }
.card-val { font-size: 28px; font-weight: 700; line-height: 1.2; }
.card-sub { font-size: 12px; color: var(--text-muted); margin-top: 4px; }
.section-title { font-size: 16px; font-weight: 600; color: var(--text-sec); margin-bottom: 12px; }
.actions { display: flex; gap: 10px; flex-wrap: wrap; }
.btn {
  padding: 8px 16px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--bg-card);
  color: var(--text);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all .15s;
}
.btn:hover { background: var(--bg-hover); border-color: var(--text-muted); }
</style>

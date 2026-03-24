<script setup lang="ts">
import { ref, onMounted } from 'vue'
// import { invoke } from '@tauri-apps/api/core' // TODO: use for get_devices

const devices = ref<any[]>([])
const loading = ref(true)

onMounted(async () => {
  loading.value = false
  // TODO: invoke get_devices from daemon
})
</script>

<template>
  <div>
    <h1 class="page-title">Devices & RGB</h1>

    <div class="section">
      <h3>Gaming Peripherals</h3>
      <p class="hint">Detected via ratbagd (libratbag). Make sure <code>ratbagd.service</code> is running.</p>
      <div v-if="devices.length === 0" class="empty">
        No devices detected. Connect a supported gaming mouse or keyboard.
      </div>
    </div>

    <div class="section">
      <h3>RGB Control</h3>
      <p class="hint">Unified RGB via OpenRGB SDK. Start OpenRGB with <code>--server</code> flag.</p>
      <div class="rgb-grid">
        <div class="rgb-card">
          <div class="rgb-preview" style="background: linear-gradient(135deg, #E94560, #533483)"></div>
          <div class="rgb-info">
            <span>All Devices</span>
            <span class="rgb-mode">Static</span>
          </div>
        </div>
      </div>
    </div>

    <div class="section">
      <h3>Auto-Profile Switching</h3>
      <p class="hint">Automatically switch DPI, audio routing, and RGB when a game launches.</p>
      <div class="profile-list">
        <div class="profile-row">
          <span class="profile-name">CS2 Competitive</span>
          <span class="profile-exes">cs2, csgo_linux64</span>
          <span class="profile-dpi">800 DPI</span>
        </div>
      </div>
      <button class="btn" style="margin-top: 12px">+ Add Profile</button>
    </div>
  </div>
</template>

<style scoped>
.page-title { font-size: 22px; font-weight: 700; margin-bottom: 24px; }
.section {
  background: var(--bg-card); border: 1px solid var(--border);
  border-radius: var(--radius-lg); padding: 24px; margin-bottom: 20px;
}
.section h3 {
  font-size: 15px; font-weight: 700; margin-bottom: 8px;
  padding-bottom: 10px; border-bottom: 1px solid var(--border);
}
.hint { font-size: 13px; color: var(--text-sec); margin-bottom: 16px; }
.hint code { background: var(--bg-deep); padding: 2px 6px; border-radius: 4px; font-size: 12px; }
.empty { color: var(--text-muted); font-size: 13px; }
.rgb-grid { display: flex; gap: 12px; }
.rgb-card {
  width: 160px; border: 1px solid var(--border);
  border-radius: var(--radius); overflow: hidden;
}
.rgb-preview { height: 60px; }
.rgb-info { padding: 10px; display: flex; justify-content: space-between; font-size: 12px; }
.rgb-mode { color: var(--text-muted); }
.profile-list { display: flex; flex-direction: column; gap: 8px; }
.profile-row {
  display: flex; align-items: center; gap: 16px;
  padding: 10px 14px; background: var(--bg-input);
  border: 1px solid var(--border); border-radius: var(--radius);
  font-size: 13px;
}
.profile-name { font-weight: 600; min-width: 160px; }
.profile-exes { color: var(--text-sec); flex: 1; }
.profile-dpi { color: var(--text-muted); font-variant-numeric: tabular-nums; }
.btn {
  padding: 8px 16px; border-radius: var(--radius);
  border: 1px solid var(--border); background: var(--bg-card);
  color: var(--text); font-size: 13px; cursor: pointer; transition: all .15s;
}
.btn:hover { background: var(--bg-hover); }
</style>

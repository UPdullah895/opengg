<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { usePersistenceStore } from '../stores/persistence'
import { loadTheme, saveTheme, getCurrentTheme, type Theme } from '../utils/theme'

const persist = usePersistenceStore()
onMounted(async () => { if (!persist.loaded) await persist.load() })

const modules = computed(() => persist.state.modules)
const settings = computed(() => persist.state.settings)

// Theme editing
const themeAccent = ref('#E94560')
const themeLoading = ref(false)

onMounted(async () => {
  const t = getCurrentTheme()
  if (t?.colors?.['--accent']) themeAccent.value = t.colors['--accent']
})

async function reloadTheme() {
  themeLoading.value = true
  try { await loadTheme() }
  finally { themeLoading.value = false }
}

async function applyAccentColor() {
  const t = getCurrentTheme() || { colors: {}, layout: {} }
  t.colors['--accent'] = themeAccent.value
  await saveTheme(t)
}

async function pickClipsFolder() {
  try {
    const s = await openDialog({ directory: true, multiple: false, title: 'Select Clips Folder' })
    if (s && typeof s === 'string') persist.state.settings.clipsFolder = s
  } catch (e) { console.error(e) }
}

async function openExternal(url: string) {
  try { await openUrl(url) } catch { window.open(url, '_blank') }
}
</script>

<template>
  <div>
    <h1 class="page-title">Settings</h1>

    <!-- Theme -->
    <div class="section">
      <h3>Theme</h3>
      <p class="hint">Customize the UI appearance. Theme stored at ~/.config/opengg/theme.json</p>
      <div class="theme-row">
        <div class="field">
          <label>Accent Color</label>
          <div class="color-row">
            <input type="color" v-model="themeAccent" class="color-picker" />
            <input type="text" v-model="themeAccent" class="color-hex" />
            <button class="btn" @click="applyAccentColor">Apply</button>
          </div>
        </div>
        <div class="field">
          <label>Theme File</label>
          <button class="btn" @click="reloadTheme" :disabled="themeLoading">
            {{ themeLoading ? 'Loading...' : '↻ Reload theme.json' }}
          </button>
        </div>
      </div>
    </div>

    <!-- Modules -->
    <div class="section">
      <h3>Modules</h3>
      <div class="toggle-grid">
        <label class="toggle-row"><input type="checkbox" v-model="modules.audio"><span>Audio Hub</span><span class="desc">PipeWire mixer, EQ, noise cancellation</span></label>
        <label class="toggle-row"><input type="checkbox" v-model="modules.device"><span>Device Manager</span><span class="desc">Mouse/keyboard config, RGB</span></label>
        <label class="toggle-row"><input type="checkbox" v-model="modules.replay"><span>Replay &amp; Clips</span><span class="desc">Recording, replay buffer, clips</span></label>
      </div>
    </div>

    <!-- Recording -->
    <div class="section">
      <h3>Recording</h3>
      <div class="form-grid">
        <div class="field"><label>FPS</label><select v-model.number="settings.fps"><option :value="30">30</option><option :value="60">60</option></select></div>
        <div class="field"><label>Quality</label><select v-model="settings.quality"><option>Low</option><option>Medium</option><option>High</option><option>Ultra</option></select></div>
        <div class="field"><label>Replay Buffer</label><select v-model.number="settings.replayDuration"><option :value="15">15s</option><option :value="30">30s</option><option :value="60">60s</option><option :value="120">120s</option></select></div>
        <div class="field"><label>Clips Folder</label><div class="folder-row"><input type="text" :value="settings.clipsFolder" readonly class="folder-input"><button class="folder-btn" @click="pickClipsFolder"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg></button></div></div>
      </div>
    </div>

    <!-- Clips Behavior -->
    <div class="section">
      <h3>Clips Behavior</h3>
      <div class="form-grid">
        <div class="field"><label>Default Click</label><select v-model="settings.defaultClickAction"><option value="preview">Quick Preview</option><option value="editor">Advanced Editor</option></select></div>
        <div class="field"><label>Columns Per Row</label><select v-model.number="settings.clipsPerRow"><option :value="3">3</option><option :value="4">4</option><option :value="5">5</option></select></div>
      </div>
    </div>

    <!-- Shortcuts -->
    <div class="section">
      <h3>Keyboard Shortcuts</h3>
      <div class="shortcut-list">
        <div class="shortcut-row"><span>Save Replay</span><button class="shortcut-key">{{ settings.shortcuts.saveReplay }}</button></div>
        <div class="shortcut-row"><span>Start/Stop Recording</span><button class="shortcut-key">{{ settings.shortcuts.toggleRecording }}</button></div>
        <div class="shortcut-row"><span>Screenshot</span><button class="shortcut-key">{{ settings.shortcuts.screenshot }}</button></div>
      </div>
    </div>

    <!-- About -->
    <div class="section">
      <h3>About</h3>
      <div class="about">
        <div><strong>OpenGG</strong> v1.0.0</div>
        <div>Open-source Linux gaming hub</div>
        <div class="saved" v-if="persist.loaded">✓ Settings auto-saved</div>
        <div style="margin-top:8px"><a href="#" class="link" @click.prevent="openExternal('https://github.com/UPdullah895/opengg')">GitHub ↗</a></div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.page-title { font-size:22px; font-weight:700; margin-bottom:24px; }
.section { background:var(--bg-card); border:1px solid var(--border); border-radius:var(--radius-lg); padding:24px; margin-bottom:20px; }
.section h3 { font-size:15px; font-weight:700; margin-bottom:16px; padding-bottom:10px; border-bottom:1px solid var(--border); }
.hint { font-size:13px; color:var(--text-sec); margin-bottom:16px; }

/* Theme controls */
.theme-row { display:flex; gap:20px; flex-wrap:wrap; }
.color-row { display:flex; gap:8px; align-items:center; }
.color-picker { width:40px; height:36px; border:1px solid var(--border); border-radius:var(--radius); background:none; cursor:pointer; padding:2px; }
.color-hex { width:90px; padding:7px 10px; background:var(--bg-input); border:1px solid var(--border); border-radius:var(--radius); color:var(--text); font-size:13px; font-family:monospace; outline:none; color-scheme:dark; }
.btn { padding:7px 14px; border:1px solid var(--border); border-radius:var(--radius); background:var(--bg-card); color:var(--text-sec); font-size:12px; cursor:pointer; white-space:nowrap; } .btn:hover { background:var(--bg-hover); color:var(--text); } .btn:disabled { opacity:.5; }

.toggle-grid { display:flex; flex-direction:column; gap:10px; }
.toggle-row { display:flex; align-items:center; gap:12px; padding:10px 14px; background:var(--bg-input); border:1px solid var(--border); border-radius:var(--radius); cursor:pointer; font-size:13px; }
.toggle-row input { accent-color:var(--accent); width:16px; height:16px; }
.toggle-row span:nth-child(2) { font-weight:600; min-width:140px; }
.desc { color:var(--text-sec); flex:1; }
.form-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(220px,1fr)); gap:16px; }
.field label { display:block; font-size:12px; font-weight:600; color:var(--text-sec); margin-bottom:6px; text-transform:uppercase; letter-spacing:.5px; }
.field select,.field input { width:100%; padding:8px 12px; background:var(--bg-input); border:1px solid var(--border); border-radius:var(--radius); color:var(--text); outline:none; color-scheme:dark; }
.folder-row { display:flex; gap:6px; }
.folder-input { flex:1; padding:8px 12px; background:var(--bg-input); border:1px solid var(--border); border-radius:var(--radius); color:var(--text); outline:none; font-size:13px; color-scheme:dark; }
.folder-btn { width:38px; height:38px; border-radius:var(--radius); border:1px solid var(--border); background:var(--bg-card); color:var(--text-sec); cursor:pointer; display:flex; align-items:center; justify-content:center; } .folder-btn svg { width:16px; height:16px; } .folder-btn:hover { background:var(--bg-hover); border-color:var(--accent); }
.shortcut-list { display:flex; flex-direction:column; }
.shortcut-row { display:flex; align-items:center; justify-content:space-between; padding:10px 0; border-bottom:1px solid var(--border); font-size:13px; } .shortcut-row:last-child { border-bottom:none; }
.shortcut-key { padding:6px 14px; background:var(--bg-input); border:1px solid var(--border); border-radius:var(--radius); font-size:12px; font-weight:600; color:var(--text-sec); cursor:pointer; font-family:monospace; }
.about { font-size:13px; color:var(--text-sec); line-height:1.8; } .about strong { color:var(--text); }
.link { color:var(--accent); text-decoration:none; cursor:pointer; } .link:hover { text-decoration:underline; }
.saved { font-size:11px; color:var(--success); margin-top:4px; }
</style>

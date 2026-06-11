<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { usePersistenceStore } from '../../stores/persistence'
import { loadTheme, saveTheme, getCurrentTheme, applyThemeMode, getThemeMode } from '../../utils/theme'
import SelectField from '../SelectField.vue'
import InfoIcon from '../InfoIcon.vue'
import './settings-shared.css'

const { t } = useI18n()
const persist = usePersistenceStore()

const settings = computed(() => persist.state.settings)

// ─── Theme ───
const themeAccent = ref('#E94560')
const themeLoading = ref(false)
const themeDarkMode = ref(true)

onMounted(async () => {
  const th = getCurrentTheme()
  if (th?.colors?.['--accent']) themeAccent.value = th.colors['--accent']
  themeDarkMode.value = getThemeMode() !== 'light'
})

async function reloadTheme() {
  themeLoading.value = true
  try { await loadTheme() } finally { themeLoading.value = false }
}

let _accentTimer: ReturnType<typeof setTimeout> | null = null

async function applyAccentColor() {
  const th = getCurrentTheme() || { colors: {}, layout: {} }
  th.colors['--accent'] = themeAccent.value
  await saveTheme(th)
}

function onAccentInput() {
  if (_accentTimer) clearTimeout(_accentTimer)
  _accentTimer = setTimeout(() => applyAccentColor(), 300)
}

async function onToggleDarkMode() {
  const mode = themeDarkMode.value ? 'dark' : 'light'
  applyThemeMode(mode)
  const th = getCurrentTheme() || { colors: {}, layout: {} }
  th.mode = mode
  await saveTheme(th)
}

// ─── Epic 2: Diagnostics ───
async function openCrashLogsFolder() {
  try { await invoke('open_crash_logs_folder') } catch (e) { console.error(e) }
}

// ─── Epic 4: Daemon / autostart ───
async function onRunAtStartupChange() {
  try { await invoke('set_autostart', { enable: settings.value.runAtStartup }) } catch (e) { console.error(e) }
}

async function onRunInBackgroundChange() {
  try { await invoke('set_run_in_background', { val: settings.value.runInBackground }) } catch (e) { console.error(e) }
}

// SelectField option helpers
const clickOptions = computed(() => [
  { value: 'preview', label: t('settings.clipSettings.defaultClickPreview') },
  { value: 'editor',  label: t('settings.clipSettings.defaultClickEditor') },
])

const dateFormatOptions = [
  { value: 'YMD', label: 'YYYY/MM/DD' },
  { value: 'YDM', label: 'YYYY/DD/MM' },
]

defineEmits<{ navigate: [page: string] }>()
</script>

<template>
  <section class="settings-section">
    <h2 class="sec-title">{{ t('settings.general.title') }}</h2>

    <div class="card">
      <div class="card-head">
        {{ t('settings.general.themeFile') }}
        <InfoIcon :title="t('settings.general.themeHint')" />
        <div class="theme-actions">
          <button
            class="theme-icon-btn"
            :title="themeDarkMode ? 'Switch to Light Mode' : 'Switch to Dark Mode'"
            @click="themeDarkMode = !themeDarkMode; onToggleDarkMode()"
          >
            <svg v-if="!themeDarkMode" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/></svg>
            <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12.79A9 9 0 1111.21 3 7 7 0 0021 12.79z"/></svg>
          </button>
          <button
            class="theme-icon-btn"
            :title="themeLoading ? 'Reloading…' : t('settings.general.reloadTheme')"
            :disabled="themeLoading"
            @click="reloadTheme"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" :class="{ spinning: themeLoading }"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 11-2.12-9.36L23 10"/></svg>
          </button>
        </div>
      </div>
      <div class="color-row">
        <input type="color" v-model="themeAccent" class="color-picker" @input="onAccentInput" />
        <input type="text" v-model="themeAccent" class="color-hex" spellcheck="false" @input="onAccentInput" />
      </div>
    </div>

    <div class="card">
      <div class="card-head">{{ t('settings.clipPreferences.title') }}</div>
      <div class="form-grid">
        <div class="field">
          <label>{{ t('settings.clipSettings.defaultClick') }}</label>
          <SelectField v-model="settings.defaultClickAction" :options="clickOptions" />
        </div>
        <div class="field">
          <label>{{ t('settings.general.dateFormat') }}<InfoIcon :title="t('settings.general.dateFormatHint')" /></label>
          <SelectField v-model="settings.dateFormat" :options="dateFormatOptions" />
        </div>
      </div>
    </div>

    <!-- ★ Epic 4: Daemon & Startup toggles -->
    <div class="card">
      <div class="card-head">{{ t('settings.daemon.title') }}</div>
      <div class="daemon-toggle-row">
        <div class="daemon-toggle-info">
          <span class="tname">
            {{ t('settings.daemon.runAtStartup') }}
            <InfoIcon :title="t('settings.daemon.runAtStartupTooltip')" />
          </span>
        </div>
        <button class="toggle-btn" :class="{ on: settings.runAtStartup }"
                @click="settings.runAtStartup = !settings.runAtStartup; onRunAtStartupChange()">
          <span class="toggle-knob"></span>
        </button>
      </div>
      <div class="daemon-toggle-row">
        <div class="daemon-toggle-info">
          <span class="tname">
            {{ t('settings.daemon.keepInBackground') }}
            <InfoIcon :title="t('settings.daemon.keepInBackgroundTooltip')" />
          </span>
        </div>
        <button class="toggle-btn" :class="{ on: settings.runInBackground }"
                @click="settings.runInBackground = !settings.runInBackground; onRunInBackgroundChange()">
          <span class="toggle-knob"></span>
        </button>
      </div>
    </div>

    <!-- ★ Epic 2: Diagnostics / crash log -->
    <div class="card">
      <div class="card-head">{{ t('settings.diagnostics.title') }} <InfoIcon :title="t('settings.diagnostics.hint')" /></div>
      <div class="action-row">
        <button class="btn btn-accent" @click="openCrashLogsFolder">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>
          {{ t('settings.diagnostics.openCrashLogs') }}
        </button>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* Colors & icons are inherited from parent SettingsPage */
</style>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { usePersistenceStore } from '../stores/persistence'
import { LANGUAGES } from '../i18n'
import { settingsTargetTab } from '../composables/useNavSignal'

// Import all settings child components
import GeneralSettings from '../components/settings/GeneralSettings.vue'
import LanguageSettings from '../components/settings/LanguageSettings.vue'
import ShortcutsSettings from '../components/settings/ShortcutsSettings.vue'
import MixerRoutingSettings from '../components/settings/MixerRoutingSettings.vue'
import CaptureSoundSettings from '../components/settings/CaptureSoundSettings.vue'
import TrackManagementSettings from '../components/settings/TrackManagementSettings.vue'
import StorageSettings from '../components/settings/StorageSettings.vue'
import ExtensionsSettings from '../components/settings/ExtensionsSettings.vue'
import StoreSettings from '../components/settings/StoreSettings.vue'
import AboutSettings from '../components/settings/AboutSettings.vue'
import NotificationsSettings from '../components/settings/NotificationsSettings.vue'

const { t, locale } = useI18n()
const persist = usePersistenceStore()

// mediaPort and mediaToken available via inject() for child components if needed

const settings = computed(() => persist.state.settings)

function syncLocale() {
  if (settings.value.language) locale.value = settings.value.language
}

function disableRtl() {
  settings.value.rtlMode = false
  document.documentElement.dir = 'ltr'
}

watch(() => settings.value.language, (newLang) => {
  const entry = LANGUAGES.find(l => l.code === newLang)
  if (entry?.dir !== 'rtl' && settings.value.rtlMode) disableRtl()
})

// ─── Nav ───
type Section = 'general' | 'language' | 'shortcuts' | 'mixerRouting' | 'captureSound' | 'trackManagement' | 'storage' | 'extensions' | 'store' | 'about' | 'notifications'
type NavItem = { key: Section; label: string; badge?: string }
const active = ref<Section>('general')

const navGroups = computed(() => [
  {
    key: 'general', label: t('settings.groups.general'),
    items: [
      { key: 'general'   as Section, label: t('settings.sections.general')   } as NavItem,
      { key: 'language'  as Section, label: t('settings.sections.language')  } as NavItem,
      { key: 'shortcuts' as Section, label: t('settings.sections.shortcuts') } as NavItem,
    ],
  },
  {
    key: 'audioEngine', label: t('settings.groups.audioEngine'),
    items: [
      { key: 'mixerRouting'  as Section, label: t('settings.sections.mixerRouting')  } as NavItem,
    ],
  },
  {
    key: 'moments', label: t('settings.groups.moments'),
    items: [
      { key: 'captureSound'    as Section, label: t('settings.sections.captureSound') } as NavItem,
      { key: 'trackManagement' as Section, label: t('settings.sections.trackManagement') } as NavItem,
      { key: 'storage'         as Section, label: t('settings.sections.storage')      } as NavItem,
      { key: 'notifications'   as Section, label: t('settings.sections.notifications') } as NavItem,
    ],
  },
  {
    key: 'extensions', label: t('settings.groups.extensions'),
    items: [
      { key: 'extensions' as Section, label: t('settings.sections.extensions'), badge: 'Beta' } as NavItem,
      { key: 'store'      as Section, label: t('settings.sections.store') } as NavItem,
    ],
  },
  {
    key: 'info', label: '',
    items: [
      { key: 'about' as Section, label: t('settings.sections.about') } as NavItem,
    ],
  },
])

defineEmits<{ navigate: [page: string] }>()

onMounted(async () => {
  if (!persist.loaded) await persist.load()
  syncLocale()
  // ★ Epic 4: Sync autostart UI with actual OS state on every open
  try { settings.value.runAtStartup = await invoke<boolean>('get_autostart') } catch { /* ignore */ }
  // ★ Epic 4: Push saved run-in-background flag to Rust state
  try { await invoke('set_run_in_background', { val: settings.value.runInBackground }) } catch { /* ignore */ }
  // ── Cross-page deep link: auto-select tab when navigated from another page ──
  if (settingsTargetTab.value) {
    active.value = settingsTargetTab.value as typeof active.value
    settingsTargetTab.value = null
  }
})
</script>

<template>
  <!-- keydown listener for shortcut recording (global within settings) -->
  <div class="settings-layout" tabindex="-1">

    <!-- ── Left Sidebar Nav ── -->
    <aside class="settings-nav">
      <div v-for="group in navGroups" :key="group.key" class="nav-group">
        <div class="nav-group-label">{{ group.label }}</div>
        <button
          v-for="item in group.items" :key="item.key"
          class="nav-item" :class="{ active: active === item.key }"
          @click="active = item.key"
        >
          {{ item.label }}
          <span v-if="item.badge" class="nav-badge">{{ item.badge }}</span>
        </button>
      </div>
    </aside>

    <!-- ── Content ── -->
    <div class="settings-content">

      <!-- ════════════════════ DYNAMIC SECTION RENDERING ════════════════════ -->
      <GeneralSettings v-if="active === 'general'" @navigate="$emit('navigate', $event)" />
      <LanguageSettings v-else-if="active === 'language'" @navigate="$emit('navigate', $event)" />
      <ShortcutsSettings v-else-if="active === 'shortcuts'" @navigate="$emit('navigate', $event)" />
      <MixerRoutingSettings v-else-if="active === 'mixerRouting'" @navigate="$emit('navigate', $event)" />
      <CaptureSoundSettings v-else-if="active === 'captureSound'" @navigate="$emit('navigate', $event)" />
      <TrackManagementSettings v-else-if="active === 'trackManagement'" @navigate="$emit('navigate', $event)" />
      <StorageSettings v-else-if="active === 'storage'" @navigate="$emit('navigate', $event)" />
      <ExtensionsSettings v-else-if="active === 'extensions'" @navigate="$emit('navigate', $event)" />
      <StoreSettings v-else-if="active === 'store'" @navigate="$emit('navigate', $event)" />
      <AboutSettings v-else-if="active === 'about'" @navigate="$emit('navigate', $event)" />
      <NotificationsSettings v-else-if="active === 'notifications'" @navigate="$emit('navigate', $event)" />

    </div><!-- /settings-content -->
  </div><!-- /settings-layout -->
</template>

<style scoped>
.settings-layout {
  display: flex; height: 100%; overflow: hidden;
  outline: none;
}

/* ── Left nav ── */
.settings-nav {
  width: 196px; flex-shrink: 0;
  border-right: 1px solid var(--border);
  padding: 4px 0; overflow-y: auto;
}
.nav-group { margin-bottom: 2px; }
.nav-group-label {
  font-size: 10px; font-weight: 800; letter-spacing: 1.2px;
  text-transform: uppercase; color: var(--text-muted);
  padding: 12px 16px 5px;
}
.nav-group-label:empty { display: none; padding: 4px 16px; }
.nav-item {
  display: flex; align-items: center; gap: 6px;
  width: 100%; padding: 8px 16px;
  background: transparent; border: none; color: var(--text-sec);
  font-size: 13px; text-align: left; cursor: pointer;
  transition: background .12s, color .12s, border-color .12s;
  border-right: 2px solid transparent;
}
.nav-item:hover { background: color-mix(in srgb, var(--accent) 10%, transparent); color: var(--accent); }
.nav-item.active {
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  color: var(--accent);
  border-right-color: var(--accent);
}
.nav-badge {
  display: inline-block;
  margin-left: auto;
  font-size: 8px; font-weight: 700; letter-spacing: .5px;
  color: var(--accent); background: color-mix(in srgb, var(--accent) 20%, transparent);
  padding: 2px 6px; border-radius: 3px;
}

/* ── Content area ── */
.settings-content {
  flex: 1; overflow-y: auto; padding: 0;
}
.settings-content > * {
  max-width: 100%;
  padding: 20px 28px;
}

/* ── Shared card styles ── */
section {
  display: contents;
}
</style>

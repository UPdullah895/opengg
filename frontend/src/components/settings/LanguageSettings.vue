<script setup lang="ts">
import { computed, watch, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { usePersistenceStore } from '../../stores/persistence'
import { LANGUAGES, registerLocale } from '../../i18n'
import InfoIcon from '../InfoIcon.vue'
import './settings-shared.css'

const { t, locale } = useI18n()
const persist = usePersistenceStore()

const settings = computed(() => persist.state.settings)

const isRtlLanguage = computed(() => {
  const entry = LANGUAGES.find(l => l.code === settings.value.language)
  return entry?.dir === 'rtl'
})

function setLanguage(code: string) {
  settings.value.language = code
  locale.value = code
  // Direction follows the language: Arabic → RTL, others → LTR. Keep rtlMode in sync.
  const entry = LANGUAGES.find(l => l.code === code)
  const isRtl = entry?.dir === 'rtl'
  settings.value.rtlMode = isRtl
  document.documentElement.dir = isRtl ? 'rtl' : 'ltr'
}

function enableRtl() {
  settings.value.rtlMode = true
  document.documentElement.dir = 'rtl'
}

function disableRtl() {
  settings.value.rtlMode = false
  document.documentElement.dir = 'ltr'
}

watch(() => settings.value.language, (newLang) => {
  const entry = LANGUAGES.find(l => l.code === newLang)
  if (entry?.dir !== 'rtl' && settings.value.rtlMode) disableRtl()
})

// ─── Language: open locales folder + dynamic locale reload ───
const localesReloading = ref(false)

async function loadUserLocales() {
  localesReloading.value = true
  try {
    const list = await invoke<Array<{ code: string; json_content: string }>>('list_user_locales')
    for (const ul of list) {
      try {
        const data = JSON.parse(ul.json_content)
        const meta = (data._meta ?? {}) as { name?: string; dir?: 'ltr' | 'rtl' }
        registerLocale(
          ul.code,
          data,
          meta.name ?? (ul.code.charAt(0).toUpperCase() + ul.code.slice(1)),
          meta.dir  ?? 'ltr',
        )
      } catch { /* skip malformed JSON */ }
    }
  } catch { /* dir not created yet */ }
  finally { localesReloading.value = false }
}

async function openLocalesFolder() {
  try { await invoke<string>('open_locales_folder') } catch (e) { console.error(e) }
  // Also reload so files added before clicking the button appear immediately
  await loadUserLocales()
}

defineEmits<{ navigate: [page: string] }>()
</script>

<template>
  <section class="settings-section">
    <h2 class="sec-title">{{ t('settings.language.title') }}</h2>
    <div class="card">
      <div class="card-head">
        {{ t('settings.language.selectLanguage') }}
        <InfoIcon :title="t('settings.language.hint')" />
        <div class="lang-actions">
          <button class="theme-icon-btn" @click="openLocalesFolder" aria-label="Open locales folder">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/><line x1="12" y1="11" x2="12" y2="17"/><line x1="9" y1="14" x2="15" y2="14"/></svg>
            <span class="info-tooltip-wrap"><span class="btn-tooltip">{{ t('settings.language.addLanguage') }}</span></span>
          </button>
          <button class="theme-icon-btn" :disabled="localesReloading" @click="loadUserLocales" aria-label="Reload languages">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" :class="{ spinning: localesReloading }"><path d="M23 4v6h-6"/><path d="M1 20v-6h6"/><path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"/></svg>
            <span class="info-tooltip-wrap"><span class="btn-tooltip">{{ t('settings.language.reloadLanguages') }}</span></span>
          </button>
          <button
            v-if="isRtlLanguage"
            class="theme-icon-btn"
            :class="{ active: settings.rtlMode }"
            @click="settings.rtlMode ? disableRtl() : enableRtl()"
            aria-label="Toggle RTL layout"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 10H3"/><path d="M21 6H3"/><path d="M21 14H3"/><path d="M17 18H3"/><polyline points="21 10 17 14 21 18"/></svg>
            <span class="info-tooltip-wrap"><span class="btn-tooltip">{{ t('settings.language.rtlModeHint') }}</span></span>
          </button>
        </div>
      </div>
      <div class="lang-list">
        <button
          v-for="lang in LANGUAGES" :key="lang.code"
          class="lang-btn" :class="{ active: settings.language === lang.code }"
          @click="setLanguage(lang.code)"
        >
          <span class="lang-left">
            <span class="lang-code">{{ lang.code.toUpperCase() }}</span>
            <span class="lang-label">{{ lang.label }}</span>
          </span>
          <span class="lang-dir-badge">{{ lang.dir === 'rtl' ? 'RTL' : 'LTR' }}</span>
        </button>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* Colors & icons inherited from parent */
</style>

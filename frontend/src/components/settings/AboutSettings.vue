<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { getVersion } from '@tauri-apps/api/app'
import { openUrl } from '@tauri-apps/plugin-opener'

const { t } = useI18n()

const appVersion = ref('')

onMounted(async () => {
  try { appVersion.value = await getVersion() } catch { appVersion.value = '0.1.1' }
})

async function openExternal(url: string) {
  try { await openUrl(url) } catch { window.open(url, '_blank') }
}

defineEmits<{ navigate: [page: string] }>()
</script>

<template>
  <section>
    <h2 class="sec-title">{{ t('settings.sections.about') }}</h2>

    <!-- Hero card -->
    <div class="hero-card">
      <div class="logo-svg">
        <svg viewBox="0 0 32 32" fill="none" stroke="currentColor" stroke-width="2" style="width:48px;height:48px;color:var(--accent)">
          <circle cx="16" cy="8" r="3"/>
          <path d="M16 11v8M8 22a8 8 0 0 0 16 0"/>
        </svg>
      </div>
      <h1 class="about-app-name">OpenGG</h1>
      <p class="version-badge">v{{ appVersion }}</p>
      <p class="about-tagline">{{ t('settings.about.tagline') }}</p>
      <p class="about-desc">{{ t('settings.about.description') }}</p>
    </div>

    <!-- Goals -->
    <div class="goals-card">
      <h3 style="margin:0;font-size:14px;font-weight:700;color:var(--text)">{{ t('settings.about.goals') }}</h3>
      <ul class="goals-list">
        <li>{{ t('settings.about.goal1') }}</li>
        <li>{{ t('settings.about.goal2') }}</li>
        <li>{{ t('settings.about.goal3') }}</li>
      </ul>
    </div>

    <!-- Connect -->
    <div class="connect-card">
      <h3 style="margin:0 0 8px;font-size:14px;font-weight:700;color:var(--text)">{{ t('settings.about.connect') }}</h3>
      <div class="social-links">
        <button class="social-btn" @click="openExternal('https://github.com/opengg-org/opengg')">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="16" height="16"><path d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-3-.5-6.5-.5-6.5 0-1.5-1.5-2.5-1.5-2.5-1.5-.28 1.15-.28 2.35 0 3.5-.73 1.02-1.08 2.25-1 3.5 0 3.5 3 5.5 6 5.5-.5.5-.82 1.2-.82 2v4"/><path d="M9 16c-.25.125-.5.125-.75.125M15 16c.25.125.5.125.75.125"/></svg>
          GitHub
        </button>
        <button class="social-btn" @click="openExternal('https://discord.gg/opengg')">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16"><path d="M6.5 2a1 1 0 0 0-1 1v18a1 1 0 0 0 1.6.8l5.4-4.5 4.4 3.6 2.1-2.1-2-1.8 4-3.3V3a1 1 0 0 0-1-1h-13z"/></svg>
          Discord
        </button>
      </div>
    </div>

    <p class="about-contributors">{{ t('settings.about.credits') }}</p>
  </section>
</template>

<style scoped>
/* Inherited from parent */
</style>

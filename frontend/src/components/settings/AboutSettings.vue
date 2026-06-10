<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { getVersion } from '@tauri-apps/api/app'
import { openUrl } from '@tauri-apps/plugin-opener'
import { deps } from '../../composables/useDependencyStatus'

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

    <!-- System Dependencies -->
    <div class="deps-card">
      <h3 style="margin:0 0 12px;font-size:14px;font-weight:700;color:var(--text)">{{ t('settings.deps.title') }}</h3>
      <div v-if="deps.length === 0" style="font-size:12px;color:var(--text-secondary);padding:8px 0">
        {{ t('settings.deps.loading') }}
      </div>
      <div v-else class="deps-list">
        <div v-for="dep in deps" :key="dep.binary" class="dep-row">
          <div class="dep-check" :class="{ available: dep.available, missing: !dep.available }">
            {{ dep.available ? '✓' : '✗' }}
          </div>
          <div class="dep-info">
            <div class="dep-binary">{{ dep.binary }}</div>
            <div class="dep-feature">{{ t(`settings.deps.feature.${dep.feature}`) }}</div>
          </div>
        </div>
      </div>
    </div>

    <p class="about-contributors">{{ t('settings.about.credits') }}</p>
  </section>
</template>

<style scoped>
.deps-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px;
  margin-bottom: 16px;
}

.deps-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.dep-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px;
  background: var(--bg-secondary, rgba(255, 255, 255, 0.02));
  border-radius: 4px;
  font-size: 12px;
}

.dep-check {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  font-weight: 700;
  font-size: 11px;
  flex-shrink: 0;
}

.dep-check.available {
  background-color: color-mix(in srgb, var(--success, #10b981) 15%, var(--bg-card));
  color: var(--success, #10b981);
}

.dep-check.missing {
  background-color: color-mix(in srgb, var(--danger, #ef4444) 15%, var(--bg-card));
  color: var(--danger, #ef4444);
}

.dep-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.dep-binary {
  font-weight: 500;
  color: var(--text);
  font-family: monospace;
  font-size: 11px;
}

.dep-feature {
  color: var(--text-secondary, rgba(255, 255, 255, 0.6));
  font-size: 11px;
}
</style>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { openUrl } from '@tauri-apps/plugin-opener'
import { appVersion } from '../composables/useAppVersion'

const { t } = useI18n()

function openGithub() { openUrl('https://github.com/UPdullah895/opengg') }
function openDiscord() { openUrl('#') }
function openWebsite() { openUrl('#') }
</script>

<template>
  <div class="about-page">
    <!-- Hero -->
    <div class="hero-card">
      <svg class="logo-svg" viewBox="0 0 512 512" width="120" height="120" style="fill-rule:evenodd;clip-rule:evenodd;">
        <g transform="matrix(2.778643,0,0,2.778643,-447.380743,-285.942888)">
          <path d="M291.671,187.47L332.636,187.47L340.205,195.039L280.739,254.504L259.3,232.814L271.068,221.046L280.97,230.948L307.949,203.97L291.671,203.97L291.671,187.47ZM285.002,195.039L225.537,254.504L166.072,195.039L225.537,135.573L247.24,157.276L235.885,168.632L226.383,159.129L190.473,195.039L226.383,230.948L241.783,215.548L221.274,195.039L280.739,135.573L311.599,166.433L300.04,177.991L280.97,159.129L265.032,175.068L285.002,195.039ZM253.289,211.821L270.072,195.039L253.289,178.256L236.507,195.039L253.289,211.821Z" fill="var(--accent)"/>
        </g>
      </svg>

      <h1 class="app-name">OpenGG</h1>
      <p class="version-badge">v{{ appVersion }}</p>
      <p class="tagline">{{ t('settings.aboutPage.tagline') }}</p>
      <p class="description">{{ t('settings.aboutPage.description') }}</p>
    </div>

    <!-- Project Goals -->
    <div class="card goals-card">
      <div class="card-head">{{ t('settings.aboutPage.projectGoals') }}</div>
      <ul class="goals-list">
        <li>{{ t('settings.aboutPage.goal_1') }}</li>
        <li>{{ t('settings.aboutPage.goal_2') }}</li>
        <li>{{ t('settings.aboutPage.goal_3') }}</li>
      </ul>
    </div>

    <!-- Connect -->
    <div class="card connect-card">
      <div class="card-head">{{ t('settings.aboutPage.connectWithUs') }}</div>
      <div class="social-links">
        <button class="social-btn" @click="openGithub" title="GitHub">
          <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor">
            <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z"/>
          </svg>
          {{ t('settings.aboutPage.github') }}
        </button>
        <button class="social-btn" @click="openDiscord" title="Discord">
          <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor">
            <path d="M20.317 4.37a19.791 19.791 0 00-4.885-1.515.074.074 0 00-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 00-5.487 0 12.64 12.64 0 00-.617-1.25.077.077 0 00-.079-.037A19.736 19.736 0 003.677 4.37a.07.07 0 00-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 00.031.057 19.9 19.9 0 005.993 3.03.078.078 0 00.084-.028 14.09 14.09 0 001.226-1.994.076.076 0 00-.041-.106 13.107 13.107 0 01-1.872-.892.077.077 0 01-.008-.128 10.2 10.2 0 00.372-.292.074.074 0 01.077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 01.078.01c.12.098.246.198.373.292a.077.077 0 01-.006.127 12.299 12.299 0 01-1.873.892.077.077 0 00-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 00.084.028 19.839 19.839 0 006.002-3.03.077.077 0 00.032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 00-.031-.03zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z"/>
          </svg>
          {{ t('settings.aboutPage.discord') }}
        </button>
        <button class="social-btn" @click="openWebsite" title="Website">
          <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="10"/>
            <line x1="2" y1="12" x2="22" y2="12"/>
            <path d="M12 2a15.3 15.3 0 014 10 15.3 15.3 0 01-4 10 15.3 15.3 0 01-4-10 15.3 15.3 0 014-10z"/>
          </svg>
          {{ t('settings.aboutPage.website') }}
        </button>
      </div>
    </div>

    <!-- Footer -->
    <p class="contributors">{{ t('settings.aboutPage.contributors') }}</p>
  </div>
</template>

<style scoped>
.about-page {
  display: flex;
  flex-direction: column;
  gap: 20px;
  padding: 24px;
  max-width: 560px;
  margin: 0 auto;
}

/* Hero */
.hero-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 40px 32px;
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: 8px;
}

.logo-svg {
  margin-bottom: 8px;
}

.app-name {
  font-size: 32px;
  font-weight: 800;
  color: var(--text);
  letter-spacing: -0.5px;
  margin: 0;
}

.version-badge {
  font-size: 13px;
  font-weight: 600;
  color: var(--accent);
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
  padding: 3px 12px;
  border-radius: 20px;
  margin: 0;
}

.tagline {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-sec);
  margin: 4px 0 0;
}

.description {
  font-size: 13px;
  color: var(--text-muted);
  line-height: 1.6;
  margin: 0;
  max-width: 380px;
}

/* Goals */
.goals-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 20px 24px;
}

.goals-list {
  list-style: none;
  padding: 0;
  margin: 8px 0 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.goals-list li {
  font-size: 13.5px;
  color: var(--text-sec);
  line-height: 1.5;
  padding-left: 18px;
  position: relative;
}

.goals-list li::before {
  content: '▸';
  position: absolute;
  left: 0;
  color: var(--accent);
  font-size: 12px;
  top: 2px;
}

/* Connect */
.connect-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 20px 24px;
}

.social-links {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: 8px;
}

.social-btn {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 16px;
  background: var(--bg-deep);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-sec);
  font-size: 13.5px;
  font-weight: 500;
  cursor: pointer;
  transition: all .15s;
  text-align: left;
}

.social-btn:hover {
  background: var(--bg-hover);
  color: var(--text);
  border-color: var(--accent);
}

.social-btn svg {
  flex-shrink: 0;
  opacity: 0.8;
}

.social-btn:hover svg {
  opacity: 1;
  color: var(--accent);
}

/* Footer */
.contributors {
  font-size: 12px;
  color: var(--text-muted);
  text-align: center;
  margin: 0;
  padding-bottom: 16px;
}
</style>

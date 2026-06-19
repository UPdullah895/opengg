<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { openUrl } from '@tauri-apps/plugin-opener'
import { deps, deviceAccess, distroInfo, loadDependencyStatus, loadDeviceAccessStatus, loadDistroInfo, getInstallCommand, getAccessFixCommand, getNormalizedAccessItems } from '../../composables/useDependencyStatus'
import './settings-shared.css'

const { t } = useI18n()

const appVersion = ref('')
const showAllDeps = ref(false)
const expandedDep = ref<string | null>(null)
const showAllAccess = ref(false)
const expandedAccessItem = ref<string | null>(null)

onMounted(async () => {
  try { appVersion.value = await getVersion() } catch { appVersion.value = '0.1.5' }
  await loadDependencyStatus()
  await loadDeviceAccessStatus()
  await loadDistroInfo()
})

async function openExternal(url: string) {
  try { await openUrl(url) } catch { window.open(url, '_blank') }
}

const missingDeps = computed(() => deps.value.filter(d => !d.available))
const allSatisfied = computed(() => deps.value.length > 0 && missingDeps.value.length === 0)

const normalizedAccessItems = computed(() => getNormalizedAccessItems(deviceAccess.value))
const missingAccessItems = computed(() => normalizedAccessItems.value.filter(item => !item.status))
const allAccessGranted = computed(() => normalizedAccessItems.value.length > 0 && missingAccessItems.value.length === 0)

async function copyInstallCommand(cmd: string) {
  try {
    await invoke('write_clipboard', { text: cmd })
    // Brief visual feedback — in a real app, could use toast
  } catch (e) {
    console.error('Failed to copy:', e)
  }
}

function toggleExpanded(binary: string) {
  expandedDep.value = expandedDep.value === binary ? null : binary
}

function toggleExpandedAccess(itemId: string) {
  expandedAccessItem.value = expandedAccessItem.value === itemId ? null : itemId
}

defineEmits<{ navigate: [page: string] }>()
</script>

<template>
  <section class="settings-section">
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
      <div class="deps-header">
        <h3 style="margin:0;font-size:14px;font-weight:700;color:var(--text)">{{ t('settings.deps.title') }}</h3>
        <button v-if="deps.length > 0 && !allSatisfied" class="show-all-btn" @click="showAllDeps = !showAllDeps">
          {{ showAllDeps ? t('settings.deps.hideAll') : t('settings.deps.showAll') }}
        </button>
      </div>

      <div v-if="deps.length === 0" style="font-size:12px;color:var(--text-secondary);padding:8px 0;margin-top:12px">
        {{ t('settings.deps.loading') }}
      </div>

      <!-- All dependencies satisfied -->
      <div v-else-if="allSatisfied" class="deps-list" style="margin-top:12px">
        <div class="dep-row">
          <div class="dep-check available">✓</div>
          <div class="dep-info">
            <div class="dep-binary">{{ t('settings.deps.allSatisfied') }}</div>
          </div>
        </div>
      </div>

      <!-- Missing deps (or all if toggled) -->
      <div v-else class="deps-list" style="margin-top:12px">
        <div v-for="dep in showAllDeps ? deps : missingDeps" :key="dep.binary" class="dep-row-wrapper">
          <div class="dep-row">
            <div class="dep-check" :class="{ available: dep.available, missing: !dep.available }">
              {{ dep.available ? '✓' : '✗' }}
            </div>
            <div class="dep-info">
              <div class="dep-binary">{{ dep.binary }}</div>
              <div class="dep-feature">{{ t(`settings.deps.feature.${dep.feature}`) }}</div>
            </div>
            <button
              v-if="!dep.available"
              class="help-btn"
              :title="t('settings.deps.installHint')"
              @click="toggleExpanded(dep.binary)"
            >
              ?
            </button>
          </div>

          <!-- Inline install help expansion -->
          <div v-if="!dep.available && expandedDep === dep.binary" class="install-help">
            <div class="install-command-wrapper">
              <code>{{ getInstallCommand(dep.binary, distroInfo.id, distroInfo.id_like).command }}</code>
              <button class="copy-btn" @click="copyInstallCommand(getInstallCommand(dep.binary, distroInfo.id, distroInfo.id_like).command)">
                {{ t('common.copy') || '📋' }}
              </button>
            </div>
            <div v-if="getInstallCommand(dep.binary, distroInfo.id, distroInfo.id_like).note" class="install-note">
              {{ t(getInstallCommand(dep.binary, distroInfo.id, distroInfo.id_like).note || '') }}
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Device Access Status -->
    <div class="deps-card">
      <div class="deps-header">
        <h3 style="margin:0;font-size:14px;font-weight:700;color:var(--text)">{{ t('settings.deviceAccess.title') }}</h3>
        <button v-if="normalizedAccessItems.length > 0 && !allAccessGranted" class="show-all-btn" @click="showAllAccess = !showAllAccess">
          {{ showAllAccess ? t('settings.deviceAccess.hideAll') : t('settings.deviceAccess.showAll') }}
        </button>
      </div>

      <div v-if="normalizedAccessItems.length === 0" style="font-size:12px;color:var(--text-secondary);padding:8px 0;margin-top:12px">
        {{ t('settings.deviceAccess.loading') }}
      </div>

      <!-- All access granted -->
      <div v-else-if="allAccessGranted" class="deps-list" style="margin-top:12px">
        <div class="dep-row">
          <div class="dep-check available">✓</div>
          <div class="dep-info">
            <div class="dep-binary">{{ t('settings.deviceAccess.allGranted') }}</div>
          </div>
        </div>
      </div>

      <!-- Missing access items (or all if toggled) -->
      <div v-else class="deps-list" style="margin-top:12px">
        <div v-for="item in showAllAccess ? normalizedAccessItems : missingAccessItems" :key="item.id" class="dep-row-wrapper">
          <div class="dep-row">
            <div class="dep-check" :class="{ available: item.status, missing: !item.status }">
              {{ item.status ? '✓' : '✗' }}
            </div>
            <div class="dep-info">
              <div class="dep-binary">{{ item.label }}</div>
              <div class="dep-feature">{{ t(`settings.deviceAccess.${item.id}`) }}</div>
            </div>
            <button
              v-if="!item.status"
              class="help-btn"
              :title="t('settings.deviceAccess.fixHint')"
              @click="toggleExpandedAccess(item.id)"
            >
              ?
            </button>
          </div>

          <!-- Inline fix help expansion -->
          <div v-if="!item.status && expandedAccessItem === item.id" class="install-help">
            <div v-for="(cmd, idx) in getAccessFixCommand(item.id, distroInfo.id, distroInfo.id_like).commands" :key="idx" class="install-command-wrapper">
              <code>{{ cmd }}</code>
              <button class="copy-btn" @click="copyInstallCommand(cmd)">
                {{ t('common.copy') || '📋' }}
              </button>
            </div>
            <div v-if="getAccessFixCommand(item.id, distroInfo.id, distroInfo.id_like).note" class="install-note">
              {{ t(getAccessFixCommand(item.id, distroInfo.id, distroInfo.id_like).note || '') }}
            </div>
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

.deps-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}

.deps-header h3 {
  margin: 0;
}

.show-all-btn {
  background: none;
  border: none;
  color: var(--accent, #3b82f6);
  font-size: 11px;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 3px;
  transition: background-color 0.2s;
}

.show-all-btn:hover {
  background-color: color-mix(in srgb, var(--accent, #3b82f6) 15%, var(--bg-card));
}

.deps-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.dep-row-wrapper {
  display: flex;
  flex-direction: column;
  gap: 4px;
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

.help-btn {
  background: color-mix(in srgb, var(--danger, #ef4444) 20%, var(--bg-card));
  color: var(--danger, #ef4444);
  border: 1px solid color-mix(in srgb, var(--danger, #ef4444) 30%, transparent);
  border-radius: 50%;
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  font-weight: 700;
  font-size: 12px;
  transition: all 0.2s;
  flex-shrink: 0;
}

.help-btn:hover {
  background-color: color-mix(in srgb, var(--danger, #ef4444) 35%, var(--bg-card));
  border-color: color-mix(in srgb, var(--danger, #ef4444) 50%, transparent);
}

.install-help {
  background: color-mix(in srgb, var(--danger, #ef4444) 8%, var(--bg-card));
  border: 1px solid color-mix(in srgb, var(--danger, #ef4444) 20%, transparent);
  border-radius: 4px;
  padding: 8px;
  margin: 0 28px 0 28px;
  font-size: 11px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.install-command-wrapper {
  display: flex;
  gap: 6px;
  align-items: flex-start;
}

.install-command-wrapper code {
  background: var(--bg-secondary, rgba(0, 0, 0, 0.1));
  padding: 6px 8px;
  border-radius: 3px;
  font-family: 'Monaco', 'Courier New', monospace;
  font-size: 10px;
  color: var(--text);
  white-space: pre-wrap;
  word-break: break-word;
  flex: 1;
  line-height: 1.4;
}

.copy-btn {
  background: color-mix(in srgb, var(--accent, #3b82f6) 20%, var(--bg-card));
  color: var(--accent, #3b82f6);
  border: 1px solid color-mix(in srgb, var(--accent, #3b82f6) 30%, transparent);
  border-radius: 3px;
  padding: 4px 8px;
  cursor: pointer;
  font-size: 10px;
  font-weight: 500;
  transition: all 0.2s;
  flex-shrink: 0;
  white-space: nowrap;
}

.copy-btn:hover {
  background-color: color-mix(in srgb, var(--accent, #3b82f6) 35%, var(--bg-card));
  border-color: color-mix(in srgb, var(--accent, #3b82f6) 50%, transparent);
}

.install-note {
  color: var(--text-secondary, rgba(255, 255, 255, 0.6));
  font-style: italic;
  font-size: 10px;
  line-height: 1.4;
}
</style>

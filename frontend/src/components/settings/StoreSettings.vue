<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'

const { t } = useI18n()

interface RegistryEntry {
  id: string
  name: string
  description: string
  version: string
  author?: string
  repo?: string
  permissions?: string[]
  hasDaemon?: boolean
  icon?: string | null
}

interface Registry {
  version: number
  updated: string
  extensions: RegistryEntry[]
}

const extensions = ref<RegistryEntry[]>([])
const loading = ref(false)
const error = ref<string | null>(null)

async function fetchRegistry() {
  loading.value = true
  error.value = null
  try {
    const result = await invoke<Registry>('fetch_extension_registry')
    extensions.value = result.extensions || []
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

async function openRepository(url: string) {
  try {
    await openUrl(url)
  } catch (e) {
    window.open(url, '_blank')
  }
}

onMounted(() => {
  fetchRegistry()
})

defineEmits<{ navigate: [page: string] }>()
</script>

<template>
  <section>
    <h2 class="sec-title">{{ t('settings.store.title') }}</h2>

    <div class="card browse-card">
      <div class="browse-header">
        <h3 class="browse-title">{{ t('settings.store.browse.title') }}</h3>
        <p class="browse-desc">{{ t('settings.store.browse.description') }}</p>
      </div>

      <!-- Loading State -->
      <div v-if="loading" class="state-container loading-state">
        <svg class="spinner" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="12" cy="12" r="10"/>
        </svg>
        <p>{{ t('settings.store.browse.loading') }}</p>
      </div>

      <!-- Error State -->
      <div v-else-if="error" class="state-container error-state">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="error-icon">
          <circle cx="12" cy="12" r="10"/>
          <line x1="12" y1="8" x2="12" y2="12"/>
          <line x1="12" y1="16" x2="12.01" y2="16"/>
        </svg>
        <p class="error-message">{{ t('settings.store.browse.offline') }}</p>
        <p class="error-detail">{{ error }}</p>
        <button class="retry-btn" @click="fetchRegistry">{{ t('settings.store.browse.retry') }}</button>
      </div>

      <!-- Extensions List -->
      <div v-else-if="extensions.length > 0" class="extensions-list">
        <div v-for="ext in extensions" :key="ext.id" class="extension-card">
          <div class="ext-header">
            <div class="ext-title-row">
              <h4 class="ext-name">{{ ext.name }}</h4>
              <span class="ext-version">{{ t('settings.store.browse.version', { version: ext.version }) }}</span>
            </div>
            <p class="ext-desc">{{ ext.description }}</p>
          </div>

          <div class="ext-meta">
            <div class="meta-item">
              <span v-if="ext.author" class="meta-author">
                {{ t('settings.store.browse.author', { author: ext.author }) }}
              </span>
            </div>
            <div v-if="ext.permissions && ext.permissions.length > 0" class="meta-item">
              <span class="meta-label">{{ t('settings.store.browse.permissions') }}</span>
              <div class="permission-chips">
                <span v-for="perm in ext.permissions" :key="perm" class="permission-chip">
                  {{ t(`ext.consent.permission.${perm.replace(':', '_')}`, perm) }}
                </span>
              </div>
            </div>
            <div class="meta-item">
              <span class="daemon-indicator" :class="{ 'has-daemon': ext.hasDaemon }">
                {{ ext.hasDaemon ? t('settings.store.browse.hasDaemon') : t('settings.store.browse.noDaemon') }}
              </span>
            </div>
          </div>

          <div class="ext-actions">
            <button
              v-if="ext.repo"
              class="view-repo-btn"
              @click="openRepository(ext.repo)"
            >
              {{ t('settings.store.browse.viewRepository') }}
            </button>
          </div>
        </div>

        <p class="install-note">{{ t('settings.store.browse.installNote') }}</p>
      </div>

      <!-- Empty State -->
      <div v-else class="state-container empty-state">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="empty-icon">
          <path d="M6 2 3 6v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6l-3-4z"/>
          <line x1="3" y1="6" x2="21" y2="6"/>
          <circle cx="12" cy="15" r="1"/>
        </svg>
        <p>{{ t('settings.store.browse.noExtensions') }}</p>
      </div>
    </div>
  </section>
</template>

<style scoped>
.browse-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 16px;
}

.browse-header {
  margin-bottom: 16px;
}

.browse-title {
  margin: 0 0 8px;
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
}

.browse-desc {
  margin: 0;
  font-size: 13px;
  color: var(--text-secondary);
}

.state-container {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 32px 24px;
  text-align: center;
}

.loading-state {
  min-height: 200px;
}

.spinner {
  width: 32px;
  height: 32px;
  color: var(--accent);
  margin-bottom: 12px;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

.error-state {
  min-height: 200px;
}

.error-icon {
  width: 40px;
  height: 40px;
  color: var(--danger, #ef4444);
  margin-bottom: 12px;
}

.error-message {
  margin: 0 0 8px;
  font-size: 14px;
  font-weight: 500;
  color: var(--text);
}

.error-detail {
  margin: 0 0 12px;
  font-size: 12px;
  color: var(--text-secondary);
}

.retry-btn {
  padding: 8px 16px;
  background: var(--accent);
  color: white;
  border: none;
  border-radius: 4px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.retry-btn:hover {
  opacity: 0.9;
}

.empty-state {
  min-height: 180px;
}

.empty-icon {
  width: 40px;
  height: 40px;
  color: var(--text-secondary);
  margin-bottom: 12px;
}

.extensions-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.extension-card {
  background: var(--bg-secondary, rgba(255, 255, 255, 0.02));
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 12px;
  transition: all 0.2s;
}

.extension-card:hover {
  background: color-mix(in srgb, var(--accent) 5%, var(--bg-secondary));
  border-color: color-mix(in srgb, var(--accent) 30%, var(--border));
}

.ext-header {
  margin-bottom: 8px;
}

.ext-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}

.ext-name {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
}

.ext-version {
  font-size: 12px;
  color: var(--text-secondary);
  background: var(--bg-card);
  padding: 2px 6px;
  border-radius: 3px;
}

.ext-desc {
  margin: 0;
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.4;
}

.ext-meta {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 8px;
  padding: 8px 0;
  border-top: 1px solid var(--border);
  border-bottom: 1px solid var(--border);
}

.meta-item {
  font-size: 12px;
  color: var(--text-secondary);
}

.meta-author {
  display: block;
  font-weight: 500;
  color: var(--text);
}

.meta-label {
  display: block;
  font-weight: 500;
  color: var(--text);
  margin-bottom: 4px;
}

.permission-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.permission-chip {
  display: inline-block;
  padding: 2px 6px;
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 20%, transparent);
  border-radius: 3px;
  font-size: 11px;
  color: var(--text-secondary);
}

.daemon-indicator {
  display: inline-block;
  padding: 2px 6px;
  background: color-mix(in srgb, var(--text-secondary) 15%, transparent);
  border-radius: 3px;
  font-size: 11px;
  font-weight: 500;
  color: var(--text-secondary);
}

.daemon-indicator.has-daemon {
  background: color-mix(in srgb, var(--success, #10b981) 15%, transparent);
  color: var(--success, #10b981);
}

.ext-actions {
  display: flex;
  gap: 8px;
}

.view-repo-btn {
  padding: 6px 12px;
  background: var(--accent);
  color: white;
  border: none;
  border-radius: 4px;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.view-repo-btn:hover {
  opacity: 0.9;
  transform: translateY(-1px);
}

.install-note {
  margin: 12px 0 0;
  padding-top: 12px;
  border-top: 1px solid var(--border);
  font-size: 12px;
  color: var(--text-secondary);
  text-align: center;
}
</style>

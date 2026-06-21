<script setup lang="ts">
/**
 * Distro-aware install helper for gpu-screen-recorder (the screen recorder the user must
 * install themselves). Detects availability via the shared dependency-status composable and,
 * when missing, shows the per-distro install command with Copy + Recheck. Reused both in
 * Capture & Sound settings and the guided tour's recorder step.
 */
import { ref, computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import {
  missing, distroInfo, loadDistroInfo, reloadDependencyStatus, getInstallCommand,
} from '../../composables/useDependencyStatus'

const { t } = useI18n()

// `compact` trims padding/margins so the helper sits cleanly inside the tour card.
defineProps<{ compact?: boolean }>()

const isMissing = computed(() => missing('recording'))
const install = computed(() =>
  getInstallCommand('gpu-screen-recorder', distroInfo.value.id, distroInfo.value.id_like),
)

const copied = ref(false)
let copiedTimer: ReturnType<typeof setTimeout> | null = null
async function copyCmd() {
  if (!install.value.command) return
  try { await invoke('write_clipboard', { text: install.value.command }) } catch { return }
  copied.value = true
  if (copiedTimer) clearTimeout(copiedTimer)
  copiedTimer = setTimeout(() => { copied.value = false }, 1500)
}

const rechecking = ref(false)
async function recheck() {
  rechecking.value = true
  try { await reloadDependencyStatus() } finally { rechecking.value = false }
}

onMounted(() => { void loadDistroInfo() })
</script>

<template>
  <!-- Installed confirmation (mainly for the tour, so the user sees a positive state) -->
  <div v-if="!isMissing" class="ri ri--ok" :class="{ 'ri--compact': compact }">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
    <span>{{ t('settings.recorderInstall.installed') }}</span>
  </div>

  <!-- Missing → distro-aware install command + Copy + Recheck -->
  <div v-else class="ri ri--missing" :class="{ 'ri--compact': compact }">
    <div class="ri-head">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"/></svg>
      <span>{{ t('settings.recorderInstall.notFound') }}</span>
    </div>
    <div class="ri-cmd">
      <code>{{ install.command }}</code>
      <button class="ri-copy" @click="copyCmd">{{ copied ? t('settings.captureGsr.copied') : t('common.copy') }}</button>
    </div>
    <p v-if="install.note" class="ri-note">{{ t(`settings.${install.note}`) }}</p>
    <button class="ri-recheck" :disabled="rechecking" @click="recheck">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" :class="{ spin: rechecking }"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 11-2.12-9.36L23 10"/></svg>
      {{ rechecking ? t('settings.recorderInstall.rechecking') : t('settings.recorderInstall.recheck') }}
    </button>
  </div>
</template>

<style scoped>
.ri { display: flex; flex-direction: column; gap: 8px; }
.ri--compact { gap: 6px; }
.ri--missing {
  padding: 12px; border-radius: 8px;
  background: color-mix(in srgb, var(--danger) 8%, transparent);
  border: 1px solid color-mix(in srgb, var(--danger) 28%, transparent);
}
.ri--compact.ri--missing { padding: 10px; }
.ri-head { display: flex; align-items: center; gap: 8px; font-size: 13px; font-weight: 600; color: var(--text); }
.ri-head svg { width: 16px; height: 16px; flex-shrink: 0; color: var(--danger); }
.ri-cmd {
  display: flex; align-items: center; gap: 8px;
  background: var(--bg-deep); border: 1px solid var(--border); border-radius: 6px; padding: 6px 8px;
}
.ri-cmd code { flex: 1; min-width: 0; font-family: monospace; font-size: 12px; color: var(--text); white-space: pre-wrap; word-break: break-all; line-height: 1.4; }
.ri-copy {
  flex-shrink: 0; padding: 4px 10px; border-radius: 5px; border: 1px solid var(--border);
  background: var(--bg-card); color: var(--text-sec); font-size: 11px; font-weight: 600; cursor: pointer;
}
.ri-copy:hover { background: var(--bg-hover); color: var(--text); }
.ri-note { font-size: 11px; color: var(--text-sec); line-height: 1.5; margin: 0; }
.ri-recheck {
  align-self: flex-start; display: flex; align-items: center; gap: 6px;
  padding: 5px 12px; border-radius: 6px; border: 1px solid var(--border);
  background: var(--bg-card); color: var(--text-sec); font-size: 11px; font-weight: 600; cursor: pointer;
}
.ri-recheck:hover:not(:disabled) { background: var(--bg-hover); color: var(--text); }
.ri-recheck:disabled { opacity: .5; cursor: not-allowed; }
.ri-recheck svg { width: 13px; height: 13px; }
.ri-recheck svg.spin { animation: ri-spin 1s linear infinite; }
@keyframes ri-spin { to { transform: rotate(360deg); } }
.ri--ok { display: flex; flex-direction: row; align-items: center; gap: 8px; font-size: 13px; font-weight: 600; color: var(--success); }
.ri--ok svg { width: 16px; height: 16px; flex-shrink: 0; }
</style>

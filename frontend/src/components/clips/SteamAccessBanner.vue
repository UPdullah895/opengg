<script setup lang="ts">
defineOptions({ name: 'SteamAccessBanner' })
import { useI18n } from 'vue-i18n'

interface Props {
  steamAccess: string
  steamGamesLoaded: boolean
  steamGamesCount: number
  busy: boolean
}

interface Emits {
  import: []
  retry: []
}

const props = defineProps<Props>()
const emit = defineEmits<Emits>()

const { t } = useI18n()

function getTitle() {
  if (props.steamGamesLoaded) return t('clips.steamImport.readyTitle', { count: props.steamGamesCount })
  if (props.steamAccess === 'denied') return t('clips.steamImport.deniedTitle')
  return t('clips.steamImport.title')
}

function getBody() {
  if (props.steamGamesLoaded) return t('clips.steamImport.readyBody')
  if (props.steamAccess === 'denied') return t('clips.steamImport.deniedBody')
  return t('clips.steamImport.body')
}

function getButtonLabel() {
  if (props.busy) return t('clips.steamImport.loading')
  if (props.steamGamesLoaded) return t('clips.steamImport.refresh')
  if (props.steamAccess === 'granted') return t('clips.steamImport.import')
  return t('clips.steamImport.allowAndImport')
}

function handleClick() {
  if (props.steamAccess === 'denied') emit('retry')
  else emit('import')
}
</script>

<template>
  <div v-if="steamAccess !== 'granted'" class="steam-banner" :class="{ ready: steamGamesLoaded }">
    <div class="steam-banner-copy">
      <div class="steam-banner-title">{{ getTitle() }}</div>
      <div class="steam-banner-body">{{ getBody() }}</div>
    </div>
    <button class="steam-banner-btn" :disabled="busy" @click="handleClick">
      {{ getButtonLabel() }}
    </button>
  </div>
</template>

<style scoped>
.steam-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  margin: 10px 0 14px;
  padding: 12px 14px;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: linear-gradient(135deg, color-mix(in srgb, var(--accent) 10%, var(--bg-card)), var(--bg-card));
}
.steam-banner.ready {
  border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
}
.steam-banner-copy { min-width: 0; }
.steam-banner-title { font-size: 13px; font-weight: 700; color: var(--text); }
.steam-banner-body { margin-top: 3px; font-size: 12px; color: var(--text-sec); }
.steam-banner-btn {
  padding: 8px 12px;
  border: 1px solid color-mix(in srgb, var(--accent) 60%, var(--border));
  border-radius: 8px;
  background: color-mix(in srgb, var(--accent) 16%, var(--bg-surface));
  color: var(--text);
  font-size: 12px;
  font-weight: 700;
  cursor: pointer;
  white-space: nowrap;
}
.steam-banner-btn:hover:not(:disabled) { filter: brightness(1.06); }
.steam-banner-btn:disabled { opacity: .6; cursor: default; }
</style>

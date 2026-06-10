<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

defineEmits<{ navigate: [page: string] }>()
</script>

<template>
  <section>
    <h2 class="sec-title">{{ t('settings.store.title') }}</h2>
    <div class="card store-coming-soon">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="store-icon">
        <path d="M6 2 3 6v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6l-3-4z"/>
        <line x1="3" y1="6" x2="21" y2="6"/>
        <circle cx="12" cy="15" r="1"/>
      </svg>
      <h3 class="store-coming-soon-title">{{ t('settings.store.comingSoon') }}</h3>
    </div>
  </section>
</template>

<style scoped>
/* Inherited from parent */
</style>

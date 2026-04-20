<script setup lang="ts">
defineProps<{ title: string }>()
</script>

<template>
  <div class="page-header">
    <h1 class="page-title">{{ title }}</h1>
    <slot />
  </div>
</template>

<style scoped>
.page-header {
  display: flex;
  align-items: center;
  gap: 12px;
}
.page-title {
  font-size: 20px;
  font-weight: 800;
  color: var(--text);
  margin: 0;
  flex: 1;
}
</style>

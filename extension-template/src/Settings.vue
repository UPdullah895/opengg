<script setup lang="ts">
/**
 * Settings panel component for your extension.
 * This component is loaded into the Settings → Extensions gear dialog
 * when the user clicks the configure button on your extension card.
 *
 * - Use window.Vue for composition API (ref, computed, etc.)
 * - Use window.opengg.invoke() to call whitelisted Tauri commands
 * - Style with scoped CSS — no framework classes are guaranteed
 */
import { ref } from 'vue'

const count = ref(0)
</script>

<template>
  <div class="settings-root">
    <h3 class="settings-title">My Extension Settings</h3>
    <p class="settings-desc">Configure your extension here.</p>

    <div class="row">
      <label>Example counter</label>
      <div class="controls">
        <button @click="count--">−</button>
        <span class="value">{{ count }}</span>
        <button @click="count++">+</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings-root { padding: 16px; font-size: 13px; color: var(--text, #eee); }
.settings-title { margin: 0 0 4px; font-size: 15px; font-weight: 700; }
.settings-desc  { margin: 0 0 16px; color: var(--text-muted, #888); }
.row { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 8px 0; }
.controls { display: flex; align-items: center; gap: 8px; }
.controls button { width: 28px; height: 28px; border-radius: 6px; border: 1px solid var(--border, #333); background: var(--bg-deep, #111); color: var(--text, #eee); cursor: pointer; font-size: 16px; }
.value { min-width: 32px; text-align: center; font-weight: 600; }
</style>

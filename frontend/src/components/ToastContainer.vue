<script setup lang="ts">
import { useToast } from '../composables/useToast'
const { toasts, remove } = useToast()
</script>

<template>
  <Teleport to="body">
    <div class="toast-container">
      <div
        v-for="toast in toasts"
        :key="toast.id"
        :class="['toast', `toast-${toast.type}`]"
        @click="remove(toast.id)"
      >
        <svg class="toast-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <polyline v-if="toast.type === 'success'" points="20 6 9 17 4 12" />
          <template v-else-if="toast.type === 'error'">
            <circle cx="12" cy="12" r="10" />
            <line x1="15" y1="9" x2="9" y2="15" /><line x1="9" y1="9" x2="15" y2="15" />
          </template>
          <template v-else-if="toast.type === 'warning'">
            <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
            <line x1="12" y1="9" x2="12" y2="13" /><line x1="12" y1="17" x2="12.01" y2="17" />
          </template>
          <template v-else>
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="16" x2="12" y2="12" /><line x1="12" y1="8" x2="12.01" y2="8" />
          </template>
        </svg>
        <span class="toast-msg">{{ toast.message }}</span>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-container {
  position: fixed;
  top: 20px;
  right: 20px;
  z-index: 9999;
  display: flex;
  flex-direction: column;
  gap: 8px;
  pointer-events: none;
}

.toast {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;
  border-radius: 8px;
  border: 1px solid;
  background: var(--bg-card);
  color: var(--text);
  font-size: 13px;
  line-height: 1.4;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
  animation: toast-in 0.25s ease;
  pointer-events: auto;
  max-width: 320px;
  cursor: pointer;
}

@keyframes toast-in {
  from { transform: translateX(340px); opacity: 0; }
  to   { transform: translateX(0);     opacity: 1; }
}

.toast-icon { width: 16px; height: 16px; flex-shrink: 0; }
.toast-msg  { word-break: break-word; }

.toast-success { border-color: #10b981; background: color-mix(in srgb, #10b981 12%, var(--bg-card)); }
.toast-success .toast-icon { color: #10b981; }

.toast-error { border-color: var(--danger, #ef4444); background: color-mix(in srgb, var(--danger, #ef4444) 12%, var(--bg-card)); }
.toast-error .toast-icon { color: var(--danger, #ef4444); }

.toast-warning { border-color: #f59e0b; background: color-mix(in srgb, #f59e0b 12%, var(--bg-card)); }
.toast-warning .toast-icon { color: #f59e0b; }

.toast-info { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 12%, var(--bg-card)); }
.toast-info .toast-icon { color: var(--accent); }
</style>

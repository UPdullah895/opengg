<script setup lang="ts">
import { useModalStore } from '../stores/modal'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
const modal = useModalStore()

function label(localeKey: string) {
  return localeKey.includes('.') ? t(localeKey) : localeKey
}
</script>

<template>
  <Teleport to="body">
    <div v-if="modal.isOpen" class="modal-overlay" @click.self="modal.cancel">
      <div class="modal-box">
        <div class="modal-header">
          <div class="modal-icon" :class="modal.kind">
            <svg v-if="modal.kind === 'danger'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/>
              <line x1="12" y1="9" x2="12" y2="13"/>
              <line x1="12" y1="17" x2="12.01" y2="17"/>
            </svg>
            <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="12" cy="12" r="10"/>
              <line x1="12" y1="8" x2="12" y2="12"/>
              <line x1="12" y1="16" x2="12.01" y2="16"/>
            </svg>
          </div>
          <h2 class="modal-title">{{ modal.title || (modal.kind === 'danger' ? t('common.confirmDelete') : t('common.confirm')) }}</h2>
        </div>
        <p class="modal-msg">{{ modal.message }}</p>
        <div class="modal-actions">
          <button class="btn btn-cancel" @click="modal.cancel">
            {{ label(modal.cancelLabel) }}
          </button>
          <button
            class="btn"
            :class="modal.kind === 'danger' ? 'btn-danger' : 'btn-accent'"
            @click="modal.confirm"
          >
            {{ label(modal.confirmLabel) }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.modal-overlay {
  position: fixed; inset: 0; z-index: 9000;
  background: rgba(0,0,0,.78);
  backdrop-filter: blur(6px);
  display: flex; align-items: center; justify-content: center;
}
.modal-box {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 16px;
  padding: 28px 28px 24px;
  width: 400px;
  max-width: 90vw;
  box-shadow: 0 24px 64px rgba(0,0,0,.6);
  display: flex; flex-direction: column; gap: 0;
}
.modal-header {
  display: flex; align-items: center; gap: 14px;
  margin-bottom: 14px;
}
.modal-icon {
  width: 38px; height: 38px; border-radius: 50%;
  display: flex; align-items: center; justify-content: center;
  flex-shrink: 0;
}
.modal-icon.danger {
  background: color-mix(in srgb, var(--danger) 15%, transparent);
  color: var(--danger);
}
.modal-icon.danger svg { width: 18px; height: 18px; }
.modal-icon.info {
  background: color-mix(in srgb, var(--accent) 15%, transparent);
  color: var(--accent);
}
.modal-icon.info svg { width: 18px; height: 18px; }
.modal-title {
  font-size: 16px; font-weight: 700; color: var(--text);
  margin: 0; line-height: 1.3;
}
.modal-msg {
  font-size: 13px; color: var(--text-sec);
  margin: 0 0 24px; line-height: 1.6;
  padding-left: 52px; /* align with title, accounting for icon width + gap */
}
.modal-actions {
  display: flex; gap: 10px; justify-content: flex-end;
  border-top: 1px solid var(--border);
  padding-top: 20px;
}
.btn {
  display: inline-flex; align-items: center; gap: 6px;
  padding: 9px 20px; border-radius: 8px;
  font-size: 13px; font-weight: 600; cursor: pointer;
  transition: background .15s, box-shadow .15s, border-color .15s, color .15s;
  border: 1px solid transparent;
  white-space: nowrap;
}
.btn:disabled { opacity: .45; cursor: not-allowed; }
.btn-cancel {
  background: transparent;
  border-color: color-mix(in srgb, var(--text-sec) 50%, transparent);
  color: var(--text-sec);
}
.btn-cancel:hover:not(:disabled) {
  border-color: var(--text);
  color: var(--text);
}
.btn-danger {
  background: #FF4B61;
  border-color: #FF4B61;
  color: #fff;
}
.btn-danger:hover:not(:disabled) {
  background: color-mix(in srgb, #FF4B61 85%, #000);
  box-shadow: 0 0 14px color-mix(in srgb, #FF4B61 40%, transparent);
}
.btn-accent {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}
.btn-accent:hover:not(:disabled) {
  background: color-mix(in srgb, var(--accent) 85%, #000);
  box-shadow: 0 0 14px color-mix(in srgb, var(--accent) 40%, transparent);
}
</style>

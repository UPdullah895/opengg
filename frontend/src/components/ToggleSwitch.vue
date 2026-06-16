<script setup lang="ts">
defineProps<{
  modelValue: boolean
  disabled?: boolean
  compact?: boolean
}>()

defineEmits<{
  'update:modelValue': [value: boolean]
}>()
</script>

<template>
  <label class="toggle-switch" :class="{ compact }">
    <input
      type="checkbox"
      :checked="modelValue"
      :disabled="disabled"
      @change="$emit('update:modelValue', ($event.target as HTMLInputElement).checked)"
    />
    <span class="switch-track"></span>
  </label>
</template>

<style scoped>
.toggle-switch {
  position: relative;
  display: inline-block;
  width: 40px;
  height: 22px;
  cursor: pointer;
}

.toggle-switch input {
  opacity: 0;
  width: 0;
  height: 0;
  position: absolute;
}

.switch-track {
  position: absolute;
  inset: 0;
  border-radius: 11px;
  background: var(--bg-deep);
  border: 1px solid var(--border);
  transition: background 0.18s, border-color 0.18s;
}

.switch-track::after {
  content: '';
  position: absolute;
  left: 3px;
  top: 50%;
  transform: translateY(-50%);
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--text-muted);
  transition: left 0.18s, background 0.18s;
}

.toggle-switch input:checked + .switch-track {
  background: color-mix(in srgb, var(--accent) 20%, transparent);
  border-color: var(--accent);
}

.toggle-switch input:checked + .switch-track::after {
  left: calc(100% - 17px);
  background: var(--accent);
}

.toggle-switch input:disabled + .switch-track {
  opacity: 0.5;
  cursor: not-allowed;
}

.toggle-switch.compact {
  width: 32px;
  height: 18px;
}

.toggle-switch.compact .switch-track {
  border-radius: 9px;
}

.toggle-switch.compact .switch-track::after {
  width: 12px;
  height: 12px;
  left: 2px;
}

.toggle-switch.compact input:checked + .switch-track::after {
  left: calc(100% - 14px);
}
</style>

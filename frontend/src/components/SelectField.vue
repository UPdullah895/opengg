<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'

export interface SelectOption {
  value: string | number
  label: string
}

const props = defineProps<{
  modelValue: string | number
  options: Array<SelectOption | string>
  disabled?: boolean
  placeholder?: string
}>()
const emit = defineEmits<{ 'update:modelValue': [string | number] }>()

const open = ref(false)
const rootRef = ref<HTMLElement | null>(null)

const normalized = computed<SelectOption[]>(() =>
  props.options.map(o => typeof o === 'string' ? { value: o, label: o } : o)
)
const selectedLabel = computed(() =>
  normalized.value.find(o => o.value === props.modelValue)?.label
  ?? props.placeholder ?? String(props.modelValue)
)

function pick(val: string | number) { emit('update:modelValue', val); open.value = false }

function onWheel(e: WheelEvent) {
  if (!open.value) return
  e.preventDefault()
  const opts = normalized.value
  const cur = opts.findIndex(o => o.value === props.modelValue)
  const next = cur + (e.deltaY > 0 ? 1 : -1)
  if (next >= 0 && next < opts.length) pick(opts[next].value)
}

function onOutside(e: MouseEvent) {
  if (rootRef.value && !rootRef.value.contains(e.target as Node)) open.value = false
}
onMounted(() => document.addEventListener('mousedown', onOutside))
onBeforeUnmount(() => document.removeEventListener('mousedown', onOutside))
</script>

<template>
  <div class="sf-root" ref="rootRef" :class="{ open, disabled }">
    <button
      class="sf-trigger"
      type="button"
      :disabled="disabled"
      @click="open = !open"
    >
      <span class="sf-val">{{ selectedLabel }}</span>
      <svg class="sf-chev" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
        <path d="M6 9l6 6 6-6"/>
      </svg>
    </button>
    <Transition name="sf-drop">
      <div v-if="open" class="sf-dropdown" @wheel.prevent="onWheel">
        <button
          v-for="opt in normalized"
          :key="opt.value"
          type="button"
          class="sf-opt"
          :class="{ active: opt.value === modelValue }"
          @click="pick(opt.value)"
        >
          <svg v-if="opt.value === modelValue" class="sf-check" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6L9 17l-5-5"/></svg>
          <span v-else class="sf-check-placeholder"></span>
          {{ opt.label }}
        </button>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.sf-root { position: relative; display: inline-block; width: 100%; }

.sf-trigger {
  width: 100%; display: flex; align-items: center; justify-content: space-between; gap: 8px;
  padding: 8px 12px;
  background: var(--bg-input); border: 1px solid var(--border); border-radius: var(--radius);
  color: var(--text); font-size: 13px; cursor: pointer;
  transition: border-color .15s;
  outline: none;
}
.sf-trigger:hover:not(:disabled) { border-color: color-mix(in srgb, var(--accent) 50%, var(--border)); }
.sf-root.open .sf-trigger { border-color: var(--accent); }
.sf-trigger:disabled { opacity: .45; cursor: not-allowed; }

.sf-val { flex: 1; text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.sf-chev {
  width: 14px; height: 14px; flex-shrink: 0; opacity: .5;
  transition: transform .15s;
}
.sf-root.open .sf-chev { transform: rotate(180deg); opacity: .9; }

.sf-dropdown {
  position: absolute; top: calc(100% + 4px); left: 0; right: 0; z-index: 200;
  background: var(--bg-card); border: 1px solid var(--border);
  border-radius: var(--radius); padding: 4px;
  box-shadow: 0 8px 24px rgba(0,0,0,.5);
  max-height: 220px; overflow-y: auto;
}

.sf-opt {
  width: 100%; display: flex; align-items: center; gap: 8px;
  padding: 7px 10px; background: transparent; border: none; border-radius: 4px;
  color: var(--text-sec); font-size: 13px; text-align: left; cursor: pointer;
  transition: background .1s, color .1s;
}
.sf-opt:hover { background: var(--bg-hover); color: var(--text); }
.sf-opt.active { color: var(--accent); font-weight: 600; }

.sf-check { width: 13px; height: 13px; flex-shrink: 0; color: var(--accent); }
.sf-check-placeholder { width: 13px; height: 13px; flex-shrink: 0; }

/* Slide-down transition */
.sf-drop-enter-active { transition: opacity .12s, transform .12s; }
.sf-drop-leave-active { transition: opacity .08s, transform .08s; }
.sf-drop-enter-from, .sf-drop-leave-to { opacity: 0; transform: translateY(-4px); }
</style>

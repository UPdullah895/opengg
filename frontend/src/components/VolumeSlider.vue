<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(defineProps<{
  modelValue: number
  color: string
  min?: number
  max?: number
  step?: number
  showValue?: boolean
  disabled?: boolean
  unit?: string
  compact?: boolean
  vertical?: boolean
  length?: number
}>(), {
  min: 0,
  max: 100,
  step: 1,
  showValue: true,
  disabled: false,
  unit: '%',
  compact: false,
  vertical: false,
  length: 84,
})

const emit = defineEmits<{ 'update:modelValue': [number] }>()

function onInput(e: Event) {
  const val = Number((e.target as HTMLInputElement).value)
  emit('update:modelValue', val)
}

const pct = computed(() => {
  const range = props.max - props.min
  if (range <= 0) return '0%'
  const p = ((props.modelValue - props.min) / range) * 100
  return `${p}%`
})
</script>

<template>
  <div class="vs-wrap" :class="{ compact, vertical }" :style="{ '--vs-color': color, '--vs-len': length + 'px' }">
    <input
      type="range"
      class="vs-slider"
      :min="min"
      :max="max"
      :step="step"
      :value="modelValue"
      :disabled="disabled"
      :style="{ '--vol-pct': pct }"
      @input="onInput"
    />
    <span v-if="showValue" class="vs-value" :class="{ muted: modelValue === 0 }">
      {{ modelValue }}{{ unit }}
    </span>
  </div>
</template>

<style scoped>
.vs-wrap {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.vs-wrap.compact {
  flex: none;
  gap: 4px;
}
.vs-wrap.compact .vs-slider {
  width: 52px;
}
.vs-slider {
  flex: 1;
  height: 18px;
  -webkit-appearance: none;
  appearance: none;
  background: transparent;
  cursor: pointer;
  margin: 0;
  padding: 0;
}
.vs-slider:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* WebKit track */
.vs-slider::-webkit-slider-runnable-track {
  height: 5px;
  background: linear-gradient(to right, var(--vs-color) var(--vol-pct, 0%), var(--border) var(--vol-pct, 0%));
  border-radius: 3px;
}
/* WebKit thumb */
.vs-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--vs-color);
  margin-top: -4.5px;
  cursor: pointer;
  box-shadow: 0 0 0 2px var(--bg-deep);
  transition: transform 0.1s;
}
.vs-slider::-webkit-slider-thumb:hover {
  transform: scale(1.15);
}

/* Firefox track */
.vs-slider::-moz-range-track {
  height: 5px;
  background: linear-gradient(to right, var(--vs-color) var(--vol-pct, 0%), var(--border) var(--vol-pct, 0%));
  border-radius: 3px;
}
/* Firefox thumb */
.vs-slider::-moz-range-thumb {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--vs-color);
  border: 2px solid var(--bg-deep);
  cursor: pointer;
  transition: transform 0.1s;
}
.vs-slider::-moz-range-thumb:hover {
  transform: scale(1.15);
}

.vs-value {
  font-size: 10px;
  font-weight: 700;
  color: var(--vs-color);
  font-variant-numeric: tabular-nums;
  min-width: 28px;
  text-align: right;
  flex-shrink: 0;
}
.vs-value.muted {
  color: #E94560;
}

.vs-wrap.compact .vs-value {
  font-size: 9px;
  min-width: 22px;
}

/* ─── Vertical mode (bottom = low, top = high) ───
   WebKitGTK (Tauri/Linux) has no reliable native vertical range, so we rotate a
   horizontal one -90°. The rotated input keeps its un-rotated layout box (len×18),
   so it is absolutely centered inside a box that reserves the vertical length. */
.vs-wrap.vertical {
  flex: none;
  position: relative;
  flex-direction: column;
  width: 30px;
  height: var(--vs-len, 84px);
  gap: 0;
}
.vs-wrap.vertical .vs-slider {
  position: absolute;
  top: 50%;
  left: 50%;
  width: var(--vs-len, 84px);
  height: 18px;
  transform: translate(-50%, -50%) rotate(-90deg);
  transform-origin: center;
}
/* After rotate(-90°) the "to right" track gradient fills from the bottom upward. */
.vs-wrap.vertical .vs-value {
  position: absolute;
  bottom: -18px;
  left: 50%;
  transform: translateX(-50%);
  min-width: 0;
  text-align: center;
}
</style>

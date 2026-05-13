<script setup lang="ts">
import { ref, computed, watch, onUnmounted } from 'vue'
import { useFloating, offset, flip, shift } from '@floating-ui/vue'
import { i18n } from '../i18n'

const props = withDefaults(defineProps<{
  title: string
  placement?: 'top' | 'bottom' | 'left' | 'right'
  trigger?: 'hover' | 'click'
}>(), {
  placement: 'top',
  trigger: 'hover',
})

const triggerEl = ref<HTMLElement | null>(null)
const tooltipEl = ref<HTMLElement | null>(null)
const isVisible = ref(false)

const isRtl = computed(() => {
  if (document.documentElement.dir === 'rtl') return true
  if (document.documentElement.dir === 'ltr') return false
  const lang = String(i18n.global.locale.value)
  const available: Array<{ code: string; dir: 'ltr' | 'rtl' }> = (i18n.global as unknown as { available?: Array<{ code: string; dir: 'ltr' | 'rtl' }> }).available ?? []
  return available.find(l => l.code === lang)?.dir === 'rtl'
})

const effectivePlacement = computed(() => {
  if (isRtl.value) {
    if (props.placement === 'left') return 'right'
    if (props.placement === 'right') return 'left'
  }
  return props.placement
})

const { floatingStyles, update } = useFloating(triggerEl, tooltipEl, {
  middleware: [offset(8), flip(), shift()],
  placement: effectivePlacement,
})

watch(effectivePlacement, () => update())

function show() { isVisible.value = true }
function hide() { isVisible.value = false }

function onClick() {
  if (props.trigger === 'click') isVisible.value = !isVisible.value
}

function onKeydown(e: KeyboardEvent) {
  if (props.trigger === 'click' && (e.key === 'Enter' || e.key === ' ')) {
    e.preventDefault()
    isVisible.value = !isVisible.value
  }
  if (e.key === 'Escape') isVisible.value = false
}

onUnmounted(() => { isVisible.value = false })
</script>

<template>
  <span
    ref="triggerEl"
    class="tooltip-trigger"
    :class="{ 'tooltip-trigger--click': trigger === 'click' }"
    :tabindex="trigger === 'click' ? 0 : -1"
    :aria-describedby="title ? 'smart-tooltip' : undefined"
    @mouseenter="trigger === 'hover' && show()"
    @mouseleave="trigger === 'hover' && hide()"
    @focus="trigger === 'hover' && show()"
    @blur="trigger === 'hover' && hide()"
    @click="onClick"
    @keydown="onKeydown"
  >
    <slot />
  </span>

  <Teleport to="body">
    <Transition name="tip">
      <div
        v-if="isVisible && title"
        ref="tooltipEl"
        id="smart-tooltip"
        class="smart-tooltip"
        :style="floatingStyles"
        role="tooltip"
        :aria-hidden="!isVisible"
      >
        {{ title }}
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.tooltip-trigger {
  display: inline-flex;
  align-items: center;
  cursor: help;
}

.tip-enter-active,
.tip-leave-active {
  transition: opacity 0.15s ease;
}

.tip-enter-from,
.tip-leave-to {
  opacity: 0;
}
</style>

<style>
.smart-tooltip {
  position: absolute;
  z-index: 99999;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 5px 8px;
  font-size: 11px;
  color: var(--text-sec);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
  pointer-events: none;
  max-width: 250px;
  white-space: normal;
  line-height: 1.5;
  font-weight: 400;
  text-transform: none;
  letter-spacing: 0;
}
</style>

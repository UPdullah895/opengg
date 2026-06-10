<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { useI18n } from 'vue-i18n'

const props = defineProps<{
  modelValue: string
  games: string[]
  steamIconUrl?: (game: string) => string
  showCustom?: boolean
  maxItems?: number
  placeholder?: string
}>()

const emit = defineEmits<{
  'update:modelValue': [string]
  select: [string]
}>()

const { t } = useI18n()
const open = ref(false)
const rootRef = ref<HTMLElement | null>(null)

const displayValue = computed({
  get: () => props.modelValue,
  set: (v) => { emit('update:modelValue', v) },
})

const filtered = computed(() => {
  const q = displayValue.value.toLowerCase()
  const list = q ? props.games.filter(g => g.toLowerCase().includes(q)) : props.games
  const max = props.maxItems ?? 10
  return list.slice(0, max)
})

function select(game: string) {
  displayValue.value = game
  open.value = false
  emit('select', game)
}

function onDocClick(e: MouseEvent) {
  if (rootRef.value && !rootRef.value.contains(e.target as Node)) {
    open.value = false
  }
}

onMounted(() => document.addEventListener('mousedown', onDocClick))
onBeforeUnmount(() => document.removeEventListener('mousedown', onDocClick))
</script>

<template>
  <div class="gtd-root" ref="rootRef">
    <input
      v-model="displayValue"
      class="gtd-input"
      :placeholder="placeholder ?? t('clips.gamesFilter.gamePlaceholder')"
      @focus="open = true"
    />
    <div v-if="open && (filtered.length || showCustom)" class="gtd-drop">
      <button
        v-for="g in filtered"
        :key="g"
        class="gtd-opt"
        @mousedown.prevent="select(g)"
      >
        <img v-if="steamIconUrl?.(g)" :src="steamIconUrl(g)" alt="" class="gtd-icon" loading="lazy" />
        <span>{{ g }}</span>
      </button>
      <div
        v-if="showCustom && displayValue && !games.some(g => g.toLowerCase() === displayValue.toLowerCase())"
        class="gtd-custom"
        @mousedown.prevent="select(displayValue)"
      >
        + "{{ displayValue }}"
      </div>
    </div>
  </div>
</template>

<style scoped>
.gtd-root { position: relative; }
.gtd-input {
  width: 100%;
  padding: 3px 8px;
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: 5px;
  color: var(--text-sec);
  font-size: 10px;
  outline: none;
}
.gtd-input:focus { border-color: var(--accent); }
.gtd-drop {
  position: absolute;
  top: 100%;
  left: 0;
  right: 0;
  margin-top: 2px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 2px;
  z-index: 20;
  max-height: 200px;
  overflow-y: auto;
  box-shadow: 0 4px 12px rgba(0,0,0,.3);
}
.gtd-opt {
  width: 100%;
  padding: 5px 8px;
  border: none;
  background: transparent;
  color: var(--text-sec);
  font-size: 10px;
  text-align: left;
  cursor: pointer;
  border-radius: 4px;
  display: flex;
  align-items: center;
  gap: 8px;
}
.gtd-opt:hover { background: var(--bg-hover); color: var(--text); }
.gtd-icon { width: 16px; height: 16px; border-radius: 4px; object-fit: cover; flex-shrink: 0; }
.gtd-custom {
  padding: 5px 8px;
  color: var(--accent);
  font-size: 10px;
  cursor: pointer;
}
</style>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
const props = defineProps<{
  modelValue: string[]
  games: string[]
  clipCounts: Record<string, number>
}>()
const emit = defineEmits<{ 'update:modelValue': [string[]] }>()

const open = ref(false)
const triggerRef = ref<HTMLElement | null>(null)
const popoverRef = ref<HTMLElement | null>(null)

const activeCount = computed(() => props.modelValue.length)

function toggle(game: string) {
  const cur = [...props.modelValue]
  const idx = cur.indexOf(game)
  if (idx >= 0) cur.splice(idx, 1)
  else cur.push(game)
  emit('update:modelValue', cur)
}

function clearAll() { emit('update:modelValue', []) }

function onOutside(e: MouseEvent) {
  const t = e.target as Node
  if (!triggerRef.value?.contains(t) && !popoverRef.value?.contains(t)) open.value = false
}
onMounted(() => document.addEventListener('mousedown', onOutside))
onBeforeUnmount(() => document.removeEventListener('mousedown', onOutside))
</script>

<template>
  <div class="gfd">
    <button ref="triggerRef" class="gfd-trigger" :class="{ active: activeCount > 0, open }" @click="open = !open">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="gfd-ic">
        <path d="M6 11h4m-2-2v4m7-1h.01M18 11h.01M2 6a2 2 0 012-2h16a2 2 0 012 2v10a4 4 0 01-4 4H6a4 4 0 01-4-4V6z"/>
      </svg>
      {{ t('clips.games') }}
      <span v-if="activeCount > 0" class="gfd-badge">{{ activeCount }}</span>
      <span style="flex:1"></span>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="gfd-chev" :class="{ flipped: open }"><path d="M6 9l6 6 6-6"/></svg>
    </button>

    <Teleport to="body">
      <div v-if="open" ref="popoverRef" class="gfd-popover"
        :style="{ top: (triggerRef?.getBoundingClientRect().bottom ?? 0) + 'px', left: (triggerRef?.getBoundingClientRect().left ?? 0) + 'px' }">
        <div class="gfd-list">
          <label
            v-for="game in games"
            :key="game"
            class="gfd-row"
            :class="{ checked: modelValue.includes(game) }"
          >
            <span class="gfd-count">{{ clipCounts[game] ?? 0 }}</span>
            <input type="checkbox" :checked="modelValue.includes(game)" @change="toggle(game)" class="gfd-cb" />
            <span class="gfd-name">{{ game }}</span>
          </label>
          <div v-if="games.length === 0" class="gfd-empty">No games found</div>
        </div>
        <div class="gfd-footer">
          <button class="gfd-clear" @click="clearAll" :disabled="activeCount === 0">Clear all filters</button>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.gfd { position: relative; }

.gfd-trigger {
  display: flex; align-items: center; gap: 5px;
  width: 240px; box-sizing: border-box;
  padding: 7px 10px; border: 1px solid var(--border); border-radius: var(--radius, 7px);
  background: var(--bg-input); color: var(--text-sec); font-size: 13px; cursor: pointer;
  white-space: nowrap; transition: background .15s, color .15s;
}
.gfd-trigger:hover { background: var(--bg-hover); color: var(--text); }
.gfd-trigger.active { border-color: var(--accent); color: var(--accent); }
/* Seamless join with popover when open */
.gfd-trigger.open {
  border-radius: var(--radius, 7px) var(--radius, 7px) 0 0;
  border-bottom-color: transparent;
}

.gfd-ic { width: 14px; height: 14px; flex-shrink: 0; }
.gfd-badge {
  background: var(--accent); color: #fff;
  font-size: 10px; font-weight: 700;
  padding: 0 5px; border-radius: 8px; min-width: 16px; text-align: center;
}
.gfd-chev { width: 12px; height: 12px; flex-shrink: 0; transition: transform .15s; }
.gfd-chev.flipped { transform: rotate(180deg); }

.gfd-popover {
  position: fixed; z-index: 9000;
  width: 240px;
  background: var(--bg-card);
  border: 1px solid var(--border); border-top: none;
  border-radius: 0 0 10px 10px;
  box-shadow: 0 8px 32px rgba(0,0,0,.5);
  overflow: hidden;
}

.gfd-list {
  max-height: 280px; overflow-y: auto; padding: 6px;
  scrollbar-width: thin; scrollbar-color: var(--border) transparent;
}

.gfd-row {
  display: flex; align-items: center; gap: 8px;
  padding: 6px 8px; border-radius: 6px; cursor: pointer;
  transition: background .1s; user-select: none;
}
.gfd-row:hover { background: var(--bg-hover); }
.gfd-row.checked { background: color-mix(in srgb, var(--accent) 10%, transparent); }

.gfd-count {
  min-width: 24px; text-align: right;
  font-size: 11px; font-weight: 700; font-variant-numeric: tabular-nums;
  color: var(--accent);
}

/* ★ Custom dark-theme checkbox — replaces native white background */
.gfd-cb {
  appearance: none;
  -webkit-appearance: none;
  width: 15px; height: 15px; flex-shrink: 0;
  border: 1.5px solid var(--border);
  border-radius: 3px;
  background: var(--bg-deep);
  cursor: pointer;
  position: relative;
  transition: background .15s, border-color .15s;
}
.gfd-cb:hover { border-color: var(--text-muted); }
.gfd-cb:checked { background: var(--accent); border-color: var(--accent); }
.gfd-cb:checked::after {
  content: '';
  position: absolute;
  left: 4px; top: 1px;
  width: 5px; height: 9px;
  border: solid #fff;
  border-width: 0 2px 2px 0;
  transform: rotate(45deg);
}

.gfd-name {
  flex: 1; font-size: 12px; color: var(--text-sec);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.gfd-row.checked .gfd-name { color: var(--text); font-weight: 500; }
.gfd-empty { padding: 12px 8px; font-size: 11px; color: var(--text-muted); text-align: center; }

.gfd-footer { padding: 6px 8px; border-top: 1px solid var(--border); }
.gfd-clear {
  width: 100%; padding: 6px;
  border: 1px dashed var(--border); border-radius: 5px;
  background: transparent; color: var(--text-muted); font-size: 11px; cursor: pointer;
  transition: all .15s;
}
.gfd-clear:hover:not(:disabled) { border-color: var(--danger); color: var(--danger); }
.gfd-clear:disabled { opacity: .4; cursor: default; }
</style>

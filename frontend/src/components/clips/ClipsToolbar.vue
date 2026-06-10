<script setup lang="ts">
defineOptions({ name: 'ClipsToolbar' })
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useReplayStore } from '../../stores/replay'
import SelectField from '../SelectField.vue'
import GameFilterDropdown from '../GameFilterDropdown.vue'

interface Props {
  searchValue: string
  sortOptions: Array<{ value: string; label: string }>
  filterGameNames: string[]
  gameCounts: Record<string, number>
  steamIcons: Record<string, string>
  hasActiveFilters: boolean
  gridSlider: number
  isDragging: boolean
  sliderLabel: string
}

interface Emits {
  'update:searchValue': [value: string]
  'update:gridSlider': [value: number]
  'gridSliderChange': []
  'clearFilters': []
}

const props = defineProps<Props>()
const emit = defineEmits<Emits>()

const { t } = useI18n()
const replay = useReplayStore()

const sortMode = computed({
  get: () => replay.sortMode,
  set: (v) => { replay.sortMode = v as any }
})

function handleGridSliderWheel(e: WheelEvent) {
  e.preventDefault()
  const newVal = Math.max(1, Math.min(4, props.gridSlider + (e.deltaY > 0 ? 1 : -1)))
  emit('update:gridSlider', newVal)
}

function handleGridSliderChange() {
  emit('gridSliderChange')
}
</script>

<template>
  <div class="ctrl-left">
    <slot name="recording" />
    <div class="search-wrap">
      <svg class="search-ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="11" cy="11" r="8"/>
        <line x1="21" y1="21" x2="16.65" y2="16.65"/>
      </svg>
      <input
        :value="searchValue"
        @input="emit('update:searchValue', ($event.target as HTMLInputElement).value)"
        :placeholder="t('clips.search')"
        class="search"
      />
    </div>
    <SelectField class="ctrl-sort" v-model="sortMode" :options="sortOptions" />
    <GameFilterDropdown
      v-model="replay.selectedGames"
      :games="filterGameNames"
      :clipCounts="gameCounts"
      :steam-icons="steamIcons"
    />
    <button class="fav-btn" :class="{ active: replay.filterFav }" @click="replay.filterFav=!replay.filterFav">
      ❤ {{ replay.favCount }}
    </button>
    <Transition name="tip-fade">
      <button v-if="hasActiveFilters" class="clear-filters-btn" @click="emit('clearFilters')">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:12px;height:12px">
          <line x1="18" y1="6" x2="6" y2="18"/>
          <line x1="6" y1="6" x2="18" y2="18"/>
        </svg>
        {{ t('clips.gamesFilter.clear') }}
      </button>
    </Transition>
  </div>
  <div class="ctrl-right">
    <div class="size-slider-wrap">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="size-ic">
        <rect x="3" y="3" width="7" height="7"/>
        <rect x="14" y="3" width="7" height="7"/>
        <rect x="3" y="14" width="7" height="7"/>
        <rect x="14" y="14" width="7" height="7"/>
      </svg>
      <div class="size-range-wrap" @wheel.prevent="handleGridSliderWheel">
        <input
          type="range" min="1" max="4" step="1"
          :value="gridSlider"
          @input="emit('update:gridSlider', Number(($event.target as HTMLInputElement).value))"
          @change="handleGridSliderChange"
          @mousedown="emit('update:gridSlider', gridSlider)"
          @touchstart="emit('update:gridSlider', gridSlider)"
          @mouseup="handleGridSliderChange"
          @touchend="handleGridSliderChange"
          @mouseleave="emit('update:gridSlider', gridSlider)"
          class="size-range"
        />
        <Transition name="tip-fade">
          <div v-if="isDragging" class="size-tip" :style="{ left: ((gridSlider - 1) / 3 * 100) + '%' }">
            {{ sliderLabel }}
          </div>
        </Transition>
      </div>
    </div>
    <slot name="controls" />
  </div>
</template>

<style scoped>
.ctrl-left { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
.ctrl-right { display: flex; align-items: center; gap: 8px; flex-shrink: 0; margin-left: auto; }

.search-wrap { width: 200px; min-width: 160px; max-width: 220px; position: relative; }
.search-ic { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); width: 15px; height: 15px; color: var(--text-muted); pointer-events: none; }
.search { width: 100%; padding: 7px 12px 7px 32px; background: var(--bg-card); border: 1px solid var(--border); border-radius: 7px; color: var(--text); font-size: 13px; outline: none; color-scheme: dark; }
.search:focus { border-color: var(--accent); }
.search::placeholder { color: var(--text-muted); }
.ctrl-sort { width: 115px; }

.size-slider-wrap { display: flex; align-items: center; gap: 6px; padding: 0 10px; height: 32px; background: var(--bg-card); border: 1px solid var(--border); border-radius: 7px; }
.size-ic { width: 14px; height: 14px; color: var(--text-muted); flex-shrink: 0; }
.size-range-wrap { position: relative; display: flex; align-items: center; }

.size-range {
  -webkit-appearance: none; appearance: none;
  width: 72px; height: 6px;
  background: var(--bg-deep); border: 1px solid var(--border); border-radius: 3px;
  outline: none; cursor: pointer;
}
.size-range::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 12px; height: 24px; border-radius: 4px;
  background: var(--accent); border: none;
  cursor: grab; transition: transform .1s;
}
.size-range::-webkit-slider-thumb:active { cursor: grabbing; transform: scaleY(1.1); }
.size-range::-moz-range-thumb {
  width: 12px; height: 24px; border-radius: 4px;
  background: var(--accent); border: none;
  cursor: grab; transition: transform .1s;
}
.size-range::-moz-range-thumb:active { cursor: grabbing; transform: scaleY(1.1); }
.size-range::-moz-range-track { height: 6px; background: var(--bg-deep); border: 1px solid var(--border); border-radius: 3px; }

.size-tip {
  position: absolute; bottom: calc(100% + 10px);
  transform: translateX(-50%);
  padding: 4px 10px; background: var(--bg-card); border: 1px solid var(--border);
  border-radius: 6px; font-size: 11px; font-weight: 600; color: var(--text);
  white-space: nowrap; pointer-events: none;
  box-shadow: 0 4px 12px rgba(0,0,0,.4);
}
.size-tip::after {
  content: ''; position: absolute; top: 100%; left: 50%; transform: translateX(-50%);
  border: 4px solid transparent; border-top-color: var(--border);
}

.tip-fade-enter-active { transition: opacity .1s ease, transform .1s ease; }
.tip-fade-leave-active { transition: opacity .08s ease, transform .08s ease; }
.tip-fade-enter-from, .tip-fade-leave-to { opacity: 0; transform: translateX(-50%) translateY(4px); }

.fav-btn { padding: 7px 12px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg-input); color: var(--text-sec); font-size: 13px; font-weight: 600; cursor: pointer; white-space: nowrap; }
.fav-btn:hover { background: var(--bg-hover); }
.fav-btn.active { background: var(--accent); border-color: var(--accent); color: #fff; }

.clear-filters-btn {
  display: flex; align-items: center; gap: 4px;
  padding: 6px 10px; border: 1px dashed var(--danger); border-radius: var(--radius);
  background: transparent; color: var(--danger); font-size: 12px; font-weight: 600;
  cursor: pointer; white-space: nowrap; transition: all .15s;
}
.clear-filters-btn:hover { background: rgba(220,38,38,.1); }
</style>

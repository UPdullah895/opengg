<script setup lang="ts">
import { ref, computed } from 'vue'

const props = defineProps<{
  gameVolume: number
  chatVolume: number
}>()

const emit = defineEmits<{
  'update:balance': [game: number, chat: number]
}>()

// Balance: -100 (all game) to +100 (all chat), 0 = balanced
const balance = ref(0)

const gameLevel = computed(() => Math.round(Math.max(0, 100 - Math.max(0, balance.value)))) 
const chatLevel = computed(() => Math.round(Math.max(0, 100 - Math.max(0, -balance.value))))

function onSliderInput(e: Event) {
  const val = parseInt((e.target as HTMLInputElement).value)
  balance.value = val
  emit('update:balance', gameLevel.value, chatLevel.value)
}

function resetBalance() {
  balance.value = 0
  emit('update:balance', 100, 100)
}
</script>

<template>
  <div class="chatmix">
    <div class="chatmix-header">
      <span class="chatmix-title">ChatMix</span>
      <button class="chatmix-reset" @click="resetBalance" title="Reset to center">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <polyline points="1 4 1 10 7 10"/><path d="M3.51 15a9 9 0 102.13-9.36L1 10"/>
        </svg>
      </button>
    </div>

    <div class="chatmix-track">
      <div class="chatmix-label chatmix-label--game">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <line x1="6" y1="12" x2="10" y2="12"/><line x1="8" y1="10" x2="8" y2="14"/>
          <circle cx="15" cy="13" r="1" fill="currentColor"/><circle cx="18" cy="11" r="1" fill="currentColor"/>
          <path d="M2 6a2 2 0 012-2h16a2 2 0 012 2v10a4 4 0 01-4 4h-2.5L14 18h-4l-1.5 2H6a4 4 0 01-4-4V6z"/>
        </svg>
        <span>Game</span>
        <span class="chatmix-pct">{{ gameLevel }}%</span>
      </div>

      <input
        type="range"
        class="chatmix-slider"
        min="-100"
        max="100"
        :value="balance"
        @input="onSliderInput"
      />

      <div class="chatmix-label chatmix-label--chat">
        <span class="chatmix-pct">{{ chatLevel }}%</span>
        <span>Chat</span>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M3 18v-6a9 9 0 0118 0v6"/>
          <path d="M21 19a2 2 0 01-2 2h-1a2 2 0 01-2-2v-3a2 2 0 012-2h3zM3 19a2 2 0 002 2h1a2 2 0 002-2v-3a2 2 0 00-2-2H3z"/>
        </svg>
      </div>
    </div>
  </div>
</template>

<style scoped>
.chatmix {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 12px 20px;
}
.chatmix-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 10px;
}
.chatmix-title {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 1.5px;
  color: var(--text-sec);
}
.chatmix-reset {
  background: none; border: none; cursor: pointer;
  color: var(--text-muted);
  width: 20px; height: 20px;
  display: flex; align-items: center; justify-content: center;
  transition: color .15s;
}
.chatmix-reset svg { width: 14px; height: 14px; }
.chatmix-reset:hover { color: var(--text); }

.chatmix-track {
  display: flex;
  align-items: center;
  gap: 12px;
}
.chatmix-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  font-weight: 600;
  min-width: 100px;
  color: var(--text-sec);
}
.chatmix-label svg { width: 16px; height: 16px; flex-shrink: 0; }
.chatmix-label--game { color: #E94560; }
.chatmix-label--chat { color: #3B82F6; justify-content: flex-end; }
.chatmix-pct {
  font-variant-numeric: tabular-nums;
  font-size: 12px;
  font-weight: 800;
  min-width: 32px;
}

.chatmix-slider {
  flex: 1;
  height: 8px;
  -webkit-appearance: none;
  appearance: none;
  background: linear-gradient(to right, #E94560 0%, #E9456040 35%, var(--bg-deep) 45%, var(--bg-deep) 55%, #3B82F640 65%, #3B82F6 100%);
  border-radius: 4px;
  outline: none;
  cursor: pointer;
}
.chatmix-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 18px; height: 18px;
  border-radius: 50%;
  background: linear-gradient(135deg, #fff 0%, #ddd 100%);
  box-shadow: 0 1px 4px rgba(0,0,0,.4), 0 0 0 2px var(--bg-deep);
  cursor: grab;
  transition: transform .1s;
}
.chatmix-slider::-webkit-slider-thumb:active {
  transform: scale(1.15);
  cursor: grabbing;
}
</style>

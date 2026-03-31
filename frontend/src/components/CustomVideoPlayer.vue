<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { resumeAudioContext } from '../utils/audio'

const props = defineProps<{
  src: string
  muted?: boolean
  captureKeyboard?: boolean
  showControls?: boolean  // default true; set false to hide built-in ctrl-bar
}>()

const emit = defineEmits<{
  loadedmetadata: [duration: number]
  play: []
  pause: []
  ended: []
  timeupdate: [currentTime: number]
  'volume-change': [volume: number]
}>()

const videoRef = ref<HTMLVideoElement | null>(null)
const wrapRef  = ref<HTMLElement | null>(null)

const playing     = ref(false)
const currentTime = ref(0)
const duration    = ref(0)
const volume      = ref(1)
const muted       = ref(false)
const isFullscreen = ref(false)
const isHovering   = ref(false)

const ctrlVisible = computed(() =>
  !playing.value || isHovering.value || isFullscreen.value
)

// ── RAF loop for smooth currentTime ──
let raf = 0
function tick() {
  if (videoRef.value) currentTime.value = videoRef.value.currentTime
  raf = requestAnimationFrame(tick)
}

// ── Native video event bridges ──
function onMeta() {
  if (!videoRef.value) return
  duration.value = videoRef.value.duration
  emit('loadedmetadata', videoRef.value.duration)
}
function onPlay()       { playing.value = true;  emit('play') }
function onPause()      { playing.value = false; emit('pause') }
function onEnded()      { playing.value = false; emit('ended') }
function onTimeUpdate() { emit('timeupdate', videoRef.value?.currentTime ?? 0) }

// ── Controls ──
async function togglePlay() {
  if (!videoRef.value) return
  await resumeAudioContext()
  playing.value ? videoRef.value.pause() : videoRef.value.play().catch(e => console.warn('play:', e))
}

function seekTo(s: number) {
  if (!videoRef.value) return
  videoRef.value.currentTime = Math.max(0, Math.min(duration.value, s))
  currentTime.value = videoRef.value.currentTime
}

function skip(d: number) { seekTo(currentTime.value + d) }

function setVolume(v: number) {
  volume.value = v
  muted.value  = v === 0
  if (videoRef.value) videoRef.value.volume = v
  emit('volume-change', v)
}

function toggleMute() {
  if (muted.value) {
    const restore = volume.value > 0 ? volume.value : 1
    muted.value = false
    volume.value = restore
    if (videoRef.value) videoRef.value.volume = restore
    emit('volume-change', restore)
  } else {
    muted.value = true
    if (videoRef.value) videoRef.value.volume = 0
    emit('volume-change', 0)
  }
}

function onProgressClick(e: MouseEvent) {
  const el = e.currentTarget as HTMLElement
  const r  = el.getBoundingClientRect()
  seekTo(((e.clientX - r.left) / r.width) * duration.value)
}

// ── Fullscreen ──
const onFsChange = () => { isFullscreen.value = !!document.fullscreenElement }
function toggleFullscreen() {
  isFullscreen.value ? document.exitFullscreen() : wrapRef.value?.requestFullscreen()
}

// ── Keyboard (opt-in) ──
function onKeydown(e: KeyboardEvent) {
  if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return
  if (e.code === 'Space')      { e.preventDefault(); togglePlay() }
  else if (e.code === 'ArrowRight') { e.preventDefault(); skip(5) }
  else if (e.code === 'ArrowLeft')  { e.preventDefault(); skip(-5) }
  else if (e.code === 'ArrowUp')    { e.preventDefault(); setVolume(Math.min(1, volume.value + 0.1)) }
  else if (e.code === 'ArrowDown')  { e.preventDefault(); setVolume(Math.max(0, volume.value - 0.1)) }
}

onMounted(() => {
  raf = requestAnimationFrame(tick)
  document.addEventListener('fullscreenchange', onFsChange)
  if (props.captureKeyboard) document.addEventListener('keydown', onKeydown)
})

onBeforeUnmount(() => {
  cancelAnimationFrame(raf)
  document.removeEventListener('fullscreenchange', onFsChange)
  document.removeEventListener('keydown', onKeydown)
})

function fmt(s: number) {
  return `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}.${Math.floor((s % 1) * 10)}`
}

defineExpose({ videoRef, playing, currentTime, duration, isFullscreen, seekTo, togglePlay, skip, setVolume, toggleFullscreen })
</script>

<template>
  <div class="cvp-wrap" ref="wrapRef"
    @mouseenter="isHovering = true" @mouseleave="isHovering = false">
    <video
      ref="videoRef"
      :src="src"
      :muted="muted ?? false"
      preload="metadata"
      class="cvp-video"
      @loadedmetadata="onMeta"
      @timeupdate="onTimeUpdate"
      @play="onPlay"
      @pause="onPause"
      @ended="onEnded"
      @click="togglePlay"
    />

    <!-- Slot for overlays / custom ctrl-bar injected by parent -->
    <slot :isFullscreen="isFullscreen" :toggleFullscreen="toggleFullscreen" />

    <!-- Built-in controls (hidden when showControls === false) -->
    <div v-if="showControls !== false" class="cvp-ctrl" :class="{ 'cvp-ctrl-vis': ctrlVisible }">
      <div class="cvp-prog" @click="onProgressClick">
        <div class="cvp-prog-fill" :style="{ width: duration ? (currentTime/duration*100)+'%' : '0%' }">
          <div class="cvp-prog-thumb"></div>
        </div>
      </div>
      <div class="cvp-row">
        <button class="cvp-btn" @click.stop="skip(-5)" title="-5s">
          <svg viewBox="0 0 24 24" fill="currentColor" style="width:12px;height:12px">
            <polygon points="11 19 2 12 11 5"/><polygon points="22 19 13 12 22 5"/>
          </svg>
        </button>
        <button class="cvp-btn" @click.stop="togglePlay">
          <svg v-if="playing" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>
          <svg v-else viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21"/></svg>
        </button>
        <button class="cvp-btn" @click.stop="skip(5)" title="+5s">
          <svg viewBox="0 0 24 24" fill="currentColor" style="width:12px;height:12px">
            <polygon points="13 19 22 12 13 5"/><polygon points="2 19 11 12 2 5"/>
          </svg>
        </button>
        <span class="cvp-time">{{ fmt(currentTime) }} / {{ fmt(duration) }}</span>
        <div class="cvp-vol">
          <svg viewBox="0 0 24 24" fill="currentColor" class="cvp-vol-ic" style="cursor:pointer" @click.stop="toggleMute">
            <path d="M11 5L6 9H2v6h4l5 4V5z"/>
            <path v-if="!muted && volume > 0" d="M15.54 8.46a5 5 0 010 7.07" stroke="currentColor" fill="none" stroke-width="2"/>
            <line v-if="muted || volume === 0" x1="23" y1="9" x2="17" y2="15" stroke="currentColor" stroke-width="2"/>
            <line v-if="muted || volume === 0" x1="17" y1="9" x2="23" y2="15" stroke="currentColor" stroke-width="2"/>
          </svg>
          <input type="range" min="0" max="1" step="0.05" :value="muted ? 0 : volume"
            @input="setVolume(+($event.target as HTMLInputElement).value)" class="cvp-vol-sl" />
        </div>
        <button class="cvp-btn cvp-fs" @click.stop="toggleFullscreen()" title="Fullscreen">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M8 3H5a2 2 0 00-2 2v3m18 0V5a2 2 0 00-2-2h-3m0 18h3a2 2 0 002-2v-3M3 16v3a2 2 0 002 2h3"/>
          </svg>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.cvp-wrap {
  position: relative;
  background: #000;
  aspect-ratio: 16 / 9;
  width: 100%;
  height: 100%;
  flex-shrink: 1;
  min-height: 0;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}
.cvp-video { width: 100%; height: 100%; object-fit: contain; display: block; max-width: 100%; max-height: 100%; }

.cvp-wrap:fullscreen,
.cvp-wrap:-webkit-full-screen {
  width: 100vw !important;
  height: 100vh !important;
  aspect-ratio: auto !important;
  display: flex !important;
  align-items: center;
  justify-content: center;
  background: #000;
}
.cvp-wrap:fullscreen .cvp-video,
.cvp-wrap:-webkit-full-screen .cvp-video {
  max-width: 100%;
  max-height: 100%;
  width: auto;
  height: auto;
  object-fit: contain;
}

.cvp-ctrl {
  position: absolute; bottom: 0; left: 0; right: 0;
  width: 100%;
  background: linear-gradient(transparent, rgba(0,0,0,.75));
  padding: 12px 10px 8px;
  opacity: 0; transition: opacity .2s; pointer-events: none;
  box-sizing: border-box;
}
.cvp-ctrl-vis { opacity: 1; pointer-events: auto; }

.cvp-prog {
  height: 3px; background: rgba(255,255,255,.25); border-radius: 2px;
  cursor: pointer; margin-bottom: 8px; transition: height .15s; position: relative;
}
.cvp-prog:hover { height: 5px; }
.cvp-prog-fill {
  height: 100%; background: var(--accent); border-radius: 2px;
  pointer-events: none; position: relative;
}
.cvp-prog-thumb {
  position: absolute; right: -6px; top: 50%; transform: translateY(-50%);
  width: 12px; height: 12px; border-radius: 50%; background: #fff;
  box-shadow: 0 0 4px rgba(0,0,0,.5); opacity: 0; transition: opacity .15s;
}
.cvp-prog:hover .cvp-prog-thumb { opacity: 1; }

.cvp-row { display: flex; align-items: center; gap: 8px; }
.cvp-btn {
  width: 28px; height: 28px; border: none; background: transparent;
  color: #fff; cursor: pointer; display: flex; align-items: center;
  justify-content: center; border-radius: 4px; flex-shrink: 0;
}
.cvp-btn svg { width: 16px; height: 16px; }
.cvp-btn:hover { background: rgba(255,255,255,.15); }
.cvp-time { font-size: 11px; color: rgba(255,255,255,.85); flex: 1; white-space: nowrap; }
.cvp-vol { display: flex; align-items: center; gap: 4px; }
.cvp-vol-ic { width: 16px; height: 16px; color: #fff; flex-shrink: 0; }
.cvp-vol-sl { width: 60px; height: 3px; accent-color: var(--accent); cursor: pointer; }
.cvp-fs svg { width: 14px; height: 14px; }
</style>

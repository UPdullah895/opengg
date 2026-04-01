<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch } from 'vue'

const props = defineProps<{ scrollEl: HTMLElement | null }>()

const thumbRef = ref<HTMLElement | null>(null)
const visible = ref(false)

let hideTimer: ReturnType<typeof setTimeout> | null = null
let dragging = false
let dragStartY = 0
let dragStartScroll = 0
let ro: ResizeObserver | null = null

function updateThumb() {
  const el = props.scrollEl
  const thumb = thumbRef.value
  if (!el || !thumb) return
  const scrollH = el.scrollHeight
  const clientH = el.clientHeight
  if (scrollH <= clientH) { visible.value = false; return }
  const ratio = clientH / scrollH
  const thumbH = Math.max(40, clientH * ratio)
  const maxScroll = scrollH - clientH
  const maxTop = clientH - thumbH
  const top = maxScroll > 0 ? (el.scrollTop / maxScroll) * maxTop : 0
  thumb.style.height = thumbH + 'px'
  thumb.style.top = top + 'px'
  visible.value = true
}

function showThenHide() {
  visible.value = true
  if (hideTimer) clearTimeout(hideTimer)
  hideTimer = setTimeout(() => { if (!dragging) visible.value = false }, 1500)
}

function onScroll() {
  updateThumb()
  showThenHide()
}

function onThumbMouseDown(e: MouseEvent) {
  e.preventDefault()
  e.stopPropagation()
  dragging = true
  dragStartY = e.clientY
  dragStartScroll = props.scrollEl?.scrollTop ?? 0
  visible.value = true
  if (hideTimer) clearTimeout(hideTimer)
}

function onMouseMove(e: MouseEvent) {
  if (!dragging || !props.scrollEl || !thumbRef.value) return
  const clientH = props.scrollEl.clientHeight
  const scrollH = props.scrollEl.scrollHeight
  const thumbH = thumbRef.value.offsetHeight
  const maxTop = clientH - thumbH
  const maxScroll = scrollH - clientH
  if (maxTop <= 0) return
  const dy = e.clientY - dragStartY
  props.scrollEl.scrollTop = dragStartScroll + (dy / maxTop) * maxScroll
}

function onMouseUp() {
  if (!dragging) return
  dragging = false
  showThenHide()
}

function attach(el: HTMLElement) {
  el.addEventListener('scroll', onScroll, { passive: true })
  ro = new ResizeObserver(updateThumb)
  ro.observe(el)
  // Initial position after layout settles
  requestAnimationFrame(updateThumb)
}

function detach(el: HTMLElement) {
  el.removeEventListener('scroll', onScroll)
  ro?.disconnect()
  ro = null
}

onMounted(() => {
  document.addEventListener('mousemove', onMouseMove)
  document.addEventListener('mouseup', onMouseUp)
  if (props.scrollEl) attach(props.scrollEl)
})

onBeforeUnmount(() => {
  document.removeEventListener('mousemove', onMouseMove)
  document.removeEventListener('mouseup', onMouseUp)
  if (props.scrollEl) detach(props.scrollEl)
  if (hideTimer) clearTimeout(hideTimer)
})

watch(() => props.scrollEl, (next, prev) => {
  if (prev) detach(prev)
  if (next) attach(next)
}, { flush: 'post' })
</script>

<template>
  <div class="overlay-track" :class="{ visible, dragging }">
    <div ref="thumbRef" class="overlay-thumb" @mousedown="onThumbMouseDown" />
  </div>
</template>

<style scoped>
/* Track — positioned absolute relative to .scroll-host, not inside the scrollable content */
.overlay-track {
  position: absolute;
  right: 0; top: 0; bottom: 0;
  width: 20px;          /* wide invisible hit area */
  pointer-events: none;
  z-index: 100;
  opacity: 0;
  transition: opacity 0.2s;
}
.overlay-track.visible { opacity: 1; }

.overlay-thumb {
  position: absolute;
  right: 4px;
  width: 6px;
  background: rgba(255, 255, 255, 0.30);
  border-radius: 99px;
  min-height: 40px;
  pointer-events: auto;
  cursor: grab;
  transition: width 0.15s, right 0.15s, background 0.15s;
  user-select: none;
}
.overlay-track:hover .overlay-thumb,
.overlay-track.dragging .overlay-thumb {
  width: 10px;
  right: 3px;
  background: rgba(255, 255, 255, 0.60);
}
.overlay-track.dragging .overlay-thumb {
  cursor: grabbing;
}
</style>

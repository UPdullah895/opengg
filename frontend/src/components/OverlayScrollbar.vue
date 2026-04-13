<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch } from 'vue'

const props = defineProps<{ scrollEl: HTMLElement | null }>()

const thumbRef = ref<HTMLElement | null>(null)
const visible = ref(false)

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

function onScroll() {
  updateThumb()
}

function onThumbMouseDown(e: MouseEvent) {
  e.preventDefault()
  e.stopPropagation()
  dragging = true
  dragStartY = e.clientY
  dragStartScroll = props.scrollEl?.scrollTop ?? 0
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
  dragging = false
}

let mo: MutationObserver | null = null

function observeChildren(el: HTMLElement) {
  // Observe every direct child's box size. ResizeObserver on the scroll container
  // alone is NOT enough — resizing cards changes el.scrollHeight but el's own box
  // stays flex:1 constant, so RO on el never fires. Observing the inner content
  // (the grid/list/grouped wrapper) catches those reflows and updates the thumb
  // position immediately instead of waiting for the next scroll event.
  if (!ro) return
  for (const child of Array.from(el.children)) {
    if (child instanceof HTMLElement) ro.observe(child)
  }
}

function attach(el: HTMLElement) {
  el.addEventListener('scroll', onScroll, { passive: true })
  ro = new ResizeObserver(updateThumb)
  ro.observe(el)
  observeChildren(el)
  // Re-observe newly added children (e.g., when switching between viewMode variants
  // or when the grid v-for initially populates) so the RO always tracks current content.
  mo = new MutationObserver(() => {
    if (!ro) return
    ro.disconnect()
    ro.observe(el)
    observeChildren(el)
    updateThumb()
  })
  mo.observe(el, { childList: true })
  // Initial position after layout settles
  requestAnimationFrame(updateThumb)
}

function detach(el: HTMLElement) {
  el.removeEventListener('scroll', onScroll)
  ro?.disconnect()
  ro = null
  mo?.disconnect()
  mo = null
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

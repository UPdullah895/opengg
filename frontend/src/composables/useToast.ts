import { ref } from 'vue'

export interface Toast {
  id: string
  type: 'success' | 'error' | 'info' | 'warning'
  message: string
}

const toasts = ref<Toast[]>([])
let nextId = 0

export function useToast() {
  function add(message: string, type: Toast['type'] = 'info', duration = 4000) {
    const id = `toast_${nextId++}`
    toasts.value.push({ id, type, message })
    if (duration > 0) setTimeout(() => remove(id), duration)
    return id
  }

  function success(message: string, duration?: number) { return add(message, 'success', duration) }
  function error(message: string, duration?: number)   { return add(message, 'error', duration) }
  function info(message: string, duration?: number)    { return add(message, 'info', duration) }
  function warning(message: string, duration?: number) { return add(message, 'warning', duration) }

  function remove(id: string) {
    toasts.value = toasts.value.filter(t => t.id !== id)
  }

  return { toasts, add, success, error, info, warning, remove }
}

import { defineStore } from 'pinia'
import { ref } from 'vue'

type ModalKind = 'danger' | 'info'

interface ModalOptions {
  title?: string
  message: string
  confirmLabel?: string
  cancelLabel?: string
  kind?: ModalKind
  onConfirm: () => void | Promise<void>
}

export const useModalStore = defineStore('modal', () => {
  const isOpen         = ref(false)
  const message        = ref('')
  const title          = ref('')
  const confirmLabel   = ref('Confirm')
  const cancelLabel    = ref('Cancel')
  const kind           = ref<ModalKind>('info')
  const onConfirmCb    = ref<(() => void) | null>(null)

  function showConfirm(opts: ModalOptions) {
    title.value        = opts.title    ?? ''
    message.value     = opts.message
    confirmLabel.value = opts.confirmLabel ?? 'Confirm'
    cancelLabel.value  = opts.cancelLabel  ?? 'Cancel'
    kind.value         = opts.kind     ?? 'info'
    onConfirmCb.value  = opts.onConfirm
    isOpen.value       = true
  }

  async function confirm() {
    isOpen.value = false
    const cb = onConfirmCb.value
    onConfirmCb.value = null
    if (cb) await cb()
  }

  function cancel() {
    isOpen.value = false
    onConfirmCb.value = null
  }

  return { isOpen, message, title, confirmLabel, cancelLabel, kind, showConfirm, confirm, cancel }
})

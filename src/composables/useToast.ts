/**
 * Toast 通知 composable
 * 提取自 DanmakuTab 的 toast 逻辑
 */

import { ref, onUnmounted } from 'vue'

export function useToast() {
  const toastMessage = ref('')
  const toastType = ref<'success' | 'error' | 'info'>('info')
  const showToast = ref(false)
  let toastTimer: number | null = null

  const showToastMessage = (msg: string, type: 'success' | 'error' | 'info' = 'info', duration = 3000) => {
    toastMessage.value = msg
    toastType.value = type
    showToast.value = true
    if (toastTimer) clearTimeout(toastTimer)
    toastTimer = window.setTimeout(() => {
      showToast.value = false
      toastTimer = null
    }, duration)
  }

  onUnmounted(() => {
    if (toastTimer) {
      clearTimeout(toastTimer)
      toastTimer = null
    }
  })

  return { showToast, toastMessage, toastType, showToastMessage }
}

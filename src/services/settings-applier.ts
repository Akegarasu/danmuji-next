/**
 * 设置应用服务
 * 负责将设置实时应用到 UI（CSS 变量等）
 */

import { watch } from 'vue'
import { storeToRefs } from 'pinia'
import { useSettingsStore } from '@/stores/settings'

let initialized = false

/**
 * 应用透明度设置
 */
export const applyOpacity = (opacity: number) => {
  // 确保 opacity 在有效范围内
  const validOpacity = Math.max(0, Math.min(1, opacity))
  document.documentElement.style.setProperty('--window-opacity', validOpacity.toString())
  console.log('[Settings] Applied opacity:', validOpacity)
}

/**
 * 应用隐藏边框设置
 */
export const applyHideBorder = (hide: boolean) => {
  document.documentElement.style.setProperty('--window-border', hide ? 'none' : '1px solid rgba(80, 80, 80, 0.5)')
  console.log('[Settings] Applied hideBorder:', hide)
}

/**
 * 应用字体大小设置
 */
export const applyFontSize = (fontSize: number) => {
  // 确保 fontSize 在有效范围内
  const validSize = Math.max(10, Math.min(48, fontSize))
  document.documentElement.style.setProperty('--user-font-size', `${validSize}px`)
  document.documentElement.style.setProperty('--font-size-base', `${validSize}px`)
  document.documentElement.style.setProperty('--font-size-sm', `${validSize - 2}px`)
  document.documentElement.style.setProperty('--font-size-xs', `${validSize - 3}px`)
  document.documentElement.style.setProperty('--font-size-lg', `${validSize + 2}px`)
  console.log('[Settings] Applied font size:', validSize)
}

/**
 * 从 store 获取并应用当前设置
 */
export const applyCurrentSettings = () => {
  const settingsStore = useSettingsStore()
  const mainSettings = settingsStore.getWindowSettings('main')
  
  if (mainSettings) {
    applyOpacity(mainSettings.opacity)
    applyFontSize(mainSettings.fontSize)
    applyHideBorder(mainSettings.hideBorder)
  }
}

/**
 * 初始化设置监听器，确保设置变化时实时应用到 UI
 */
export const initSettingsApplier = () => {
  if (initialized) return
  initialized = true
  
  const settingsStore = useSettingsStore()
  
  // 使用 storeToRefs 获取响应式引用
  const { mainWindowSettings } = storeToRefs(settingsStore)
  
  // 监听主窗口设置变化（透明度、字体大小）
  watch(
    mainWindowSettings,
    (newSettings) => {
      if (newSettings) {
        applyOpacity(newSettings.opacity)
        applyFontSize(newSettings.fontSize)
        applyHideBorder(newSettings.hideBorder)
      }
    },
    { deep: true, immediate: true }
  )
  
  // 监听设置加载完成
  watch(
    () => settingsStore.isLoaded,
    (loaded) => {
      if (loaded) {
        applyCurrentSettings()
      }
    },
    { immediate: true }
  )
  
  // 立即应用一次当前设置
  applyCurrentSettings()
}

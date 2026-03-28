/**
 * 设置同步服务（简化版）
 * 
 * 原理：
 * - 后端文件是唯一的真相来源
 * - 修改设置后保存到后端，并广播"设置已更新"事件
 * - 其他窗口收到事件后从后端重新加载
 */

import { emit, listen } from '@tauri-apps/api/event'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useSettingsStore } from '@/stores/settings'
import { applyCurrentSettings } from './settings-applier'

// 事件名称
const SETTINGS_UPDATED_EVENT = 'settings-updated'

let unlistenFn: UnlistenFn | null = null
let currentWindowLabel = ''

/**
 * 广播设置已更新（通知其他窗口重新加载）
 */
export const broadcastSettingsUpdate = async () => {
  try {
    await emit(SETTINGS_UPDATED_EVENT, { source: currentWindowLabel })
    console.log('[SettingsSync] Broadcasted settings update')
  } catch (e) {
    console.error('[SettingsSync] Failed to broadcast:', e)
  }
}

/**
 * 处理设置更新事件
 */
const handleSettingsUpdate = async (event: { payload: { source: string } }) => {
  // 忽略自己发送的事件
  if (event.payload.source === currentWindowLabel) {
    return
  }
  
  console.log('[SettingsSync] Received settings update from:', event.payload.source)
  
  // 从后端重新加载设置
  const settingsStore = useSettingsStore()
  await settingsStore.loadSettings(true)
  
  // 应用到 UI
  applyCurrentSettings()
}

/**
 * 初始化设置同步服务
 */
export const initSettingsSync = async () => {
  if (unlistenFn) {
    return // 已经初始化
  }
  
  try {
    const window = getCurrentWindow()
    currentWindowLabel = window.label
    
    unlistenFn = await listen(SETTINGS_UPDATED_EVENT, handleSettingsUpdate)
    
    console.log('[SettingsSync] Initialized for window:', currentWindowLabel)
  } catch (e) {
    console.error('[SettingsSync] Failed to initialize:', e)
  }
}

/**
 * 停止设置同步服务
 */
export const stopSettingsSync = () => {
  if (unlistenFn) {
    unlistenFn()
    unlistenFn = null
  }
}

/**
 * 窗口管理服务
 * 负责管理窗口位置、大小的保存和恢复
 * 独立于设置系统，使用单独的 KV 存储
 */

import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { UnlistenFn } from '@tauri-apps/api/event'

// ==================== 类型定义 ====================

export interface WindowState {
  x: number
  y: number
  width: number
  height: number
  isOpen: boolean
}

export interface WindowInfo {
  label: string
  state: WindowState
}

interface WindowListener {
  unlistenMove: UnlistenFn | null
  unlistenResize: UnlistenFn | null
  debounceTimer: ReturnType<typeof setTimeout> | null
}

/** Tab 类型到标题的映射 */
const TAB_TITLES: Record<string, string> = {
  'interaction': '互动',
  'danmaku': '弹幕',
  'gift': '礼物',
  'superchat': 'SC',
  'audience': '观众'
}

// ==================== 常量配置 ====================

/** 去抖延迟时间（毫秒） */
const DEBOUNCE_DELAY = 2000

/** 最小有效窗口尺寸 */
const MIN_VALID_SIZE = 100

// ==================== 内部状态 ====================

/** 活动的窗口监听器 */
const activeListeners = new Map<string, WindowListener>()

// ==================== 窗口状态操作 ====================

/**
 * 从 KV 存储获取保存的窗口状态
 */
export const getSavedWindowState = async (label: string): Promise<WindowState | null> => {
  try {
    return await invoke<WindowState | null>('get_saved_window_state', { label })
  } catch (e) {
    console.error(`[WindowManager] Failed to get saved state for ${label}:`, e)
    return null
  }
}

/**
 * 保存窗口状态到 KV 存储
 */
export const saveWindowState = async (label: string): Promise<boolean> => {
  try {
    // 获取当前窗口状态
    const state = await invoke<WindowState>('get_current_window_state', { label })
    
    // 保存到 KV 存储
    await invoke('save_window_state', { label, state })
    
    console.log(`[WindowManager] Saved state for ${label}:`, state)
    return true
  } catch (e) {
    console.error(`[WindowManager] Failed to save state for ${label}:`, e)
    return false
  }
}

/**
 * 恢复窗口状态（位置和大小）
 */
export const restoreWindowState = async (label: string): Promise<boolean> => {
  try {
    const state = await getSavedWindowState(label)
    
    if (!state || state.width < MIN_VALID_SIZE || state.height < MIN_VALID_SIZE) {
      console.log(`[WindowManager] No valid saved state for ${label}`)
      return false
    }
    
    await invoke('set_window_state', { label, state })
    console.log(`[WindowManager] Restored state for ${label}:`, state)
    return true
  } catch (e) {
    console.error(`[WindowManager] Failed to restore state for ${label}:`, e)
    return false
  }
}

// ==================== 窗口状态监听 ====================

/**
 * 启动窗口状态监听（移动和调整大小）
 * 移动或调整大小后，去抖延迟后保存状态
 */
export const startWindowStateTracking = async (label: string): Promise<void> => {
  // 如果已经在监听，先停止
  if (activeListeners.has(label)) {
    await stopWindowStateTracking(label)
  }
  
  try {
    const window = getCurrentWindow()
    
    // 去抖保存函数
    const debouncedSave = () => {
      const listener = activeListeners.get(label)
      if (!listener) return
      
      // 清除之前的定时器
      if (listener.debounceTimer !== null) {
        clearTimeout(listener.debounceTimer)
      }
      
      // 设置新的定时器
      listener.debounceTimer = setTimeout(() => {
        saveWindowState(label)
        if (listener) {
        listener.debounceTimer = null
        }
      }, DEBOUNCE_DELAY)
    }
    
    // 监听窗口移动和调整大小
    const unlistenMove = await window.onMoved(() => {
      // console.log(`[WindowManager] Window ${label} moved`)
      debouncedSave()
    })
    
    const unlistenResize = await window.onResized(() => {
      console.log(`[WindowManager] Window ${label} resized`)
      debouncedSave()
    })
    
    // 保存监听器
    activeListeners.set(label, {
      unlistenMove,
      unlistenResize,
      debounceTimer: null
    })
    
    console.log(`[WindowManager] Started tracking for ${label}`)
  } catch (e) {
    console.error(`[WindowManager] Failed to start tracking for ${label}:`, e)
  }
}

/**
 * 停止窗口状态监听
 */
export const stopWindowStateTracking = async (label: string): Promise<void> => {
  const listener = activeListeners.get(label)
  if (!listener) return
  
  // 清除定时器
  if (listener.debounceTimer !== null) {
    clearTimeout(listener.debounceTimer)
  }
  
  // 取消事件监听
  listener.unlistenMove?.()
  listener.unlistenResize?.()
  
  activeListeners.delete(label)
  console.log(`[WindowManager] Stopped tracking for ${label}`)
}

// ==================== 窗口生命周期管理 ====================

/**
 * 初始化窗口管理器（恢复状态并开始监听）
 */
export const initWindowManager = async (label: string): Promise<void> => {
  console.log(`[WindowManager] Initializing for ${label}`)
  
  try {
    // 恢复窗口状态
    await restoreWindowState(label)
    
    // 开始监听窗口变化
    await startWindowStateTracking(label)
  } catch (e) {
    console.error(`[WindowManager] Failed to initialize for ${label}:`, e)
  }
}

/**
 * 清理窗口管理器（保存状态并停止监听）
 */
export const cleanupWindowManager = async (label: string): Promise<void> => {
  console.log(`[WindowManager] Cleaning up for ${label}`)
  
  try {
    // 停止监听（会清除去抖定时器）
    await stopWindowStateTracking(label)
    
    // 最后保存一次状态
    await saveWindowState(label)
  } catch (e) {
    console.error(`[WindowManager] Failed to cleanup for ${label}:`, e)
  }
}

// ==================== 窗口打开状态管理 ====================

/**
 * 设置窗口的打开状态
 */
export const setWindowOpenState = async (label: string, isOpen: boolean): Promise<void> => {
  try {
    await invoke('set_window_open_state', { label, isOpen })
    console.log(`[WindowManager] Set ${label} open state to ${isOpen}`)
  } catch (e) {
    console.error(`[WindowManager] Failed to set open state for ${label}:`, e)
  }
}

/**
 * 获取之前打开的窗口列表
 */
export const getPreviouslyOpenWindows = async (): Promise<WindowInfo[]> => {
  try {
    return await invoke<WindowInfo[]>('get_previously_open_windows')
  } catch (e) {
    console.error(`[WindowManager] Failed to get previously open windows:`, e)
    return []
  }
}

// ==================== 窗口创建 ====================

/**
 * 创建 Tab 窗口
 */
export const createTabWindow = async (
  tabType: string,
  title: string
): Promise<void> => {
  const label = `tab-${tabType}`
  
  try {
    await invoke('create_tab_window', { label, title, tabType })
    // 标记窗口为打开状态
    await setWindowOpenState(label, true)
    console.log(`[WindowManager] Created tab window: ${label}`)
  } catch (e) {
    console.error(`[WindowManager] Failed to create tab window ${label}:`, e)
    throw e
  }
}

/**
 * 创建设置窗口
 */
export const createSettingsWindow = async (): Promise<void> => {
  try {
    await invoke('create_settings_window')
    // 标记窗口为打开状态
    await setWindowOpenState('settings', true)
    console.log(`[WindowManager] Created settings window`)
  } catch (e) {
    console.error(`[WindowManager] Failed to create settings window:`, e)
    throw e
  }
}

/**
 * 创建存档窗口
 */
export const createArchiveWindow = async (): Promise<void> => {
  try {
    await invoke('create_archive_window')
    await setWindowOpenState('archive', true)
    console.log(`[WindowManager] Created archive window`)
  } catch (e) {
    console.error(`[WindowManager] Failed to create archive window:`, e)
    throw e
  }
}

/**
 * 创建扩展窗口
 */
export const createExtensionWindow = async (): Promise<void> => {
  try {
    await invoke('create_extension_window')
    await setWindowOpenState('extension', true)
    console.log(`[WindowManager] Created extension window`)
  } catch (e) {
    console.error(`[WindowManager] Failed to create extension window:`, e)
    throw e
  }
}

/**
 * 关闭窗口
 */
export const closeWindow = async (label: string): Promise<void> => {
  try {
    // 先清理窗口管理器
    await cleanupWindowManager(label)
    
    // 标记窗口为关闭状态
    await setWindowOpenState(label, false)
    
    // 关闭窗口
    await invoke('close_window', { label })
    
    console.log(`[WindowManager] Closed window: ${label}`)
  } catch (e) {
    console.error(`[WindowManager] Failed to close window ${label}:`, e)
    throw e
  }
}

// ==================== 窗口恢复 ====================

/**
 * 恢复之前打开的窗口
 */
export const restorePreviouslyOpenWindows = async (): Promise<void> => {
  try {
    const windows = await getPreviouslyOpenWindows()
    
    console.log(`[WindowManager] Found ${windows.length} previously open windows to restore`)
    
    for (const { label } of windows) {
      // 解析窗口类型
      if (label.startsWith('tab-')) {
        const tabType = label.replace('tab-', '')
        const title = TAB_TITLES[tabType] || tabType
        await createTabWindow(tabType, title)
        console.log(`[WindowManager] Restored tab window: ${label}`)
      } else if (label === 'settings') {
        await createSettingsWindow()
        console.log(`[WindowManager] Restored settings window`)
      } else if (label === 'archive') {
        await createArchiveWindow()
        console.log(`[WindowManager] Restored archive window`)
      } else if (label === 'extension') {
        await createExtensionWindow()
        console.log(`[WindowManager] Restored extension window`)
      }
    }
  } catch (e) {
    console.error(`[WindowManager] Failed to restore previously open windows:`, e)
  }
}

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { 
  AppSettings, 
  WindowSettings, 
  DisplaySettings,
  AudienceSortType,
  UserLoginInfo
} from '@/types'
import { DEFAULT_DISPLAY_SETTINGS, DEFAULT_WINDOW_SETTINGS } from '@/types'

const DEFAULT_SETTINGS: AppSettings = {
  roomId: '',
  cookie: '',
  user: null,
  windows: {
    main: { ...DEFAULT_WINDOW_SETTINGS }
  },
  display: { ...DEFAULT_DISPLAY_SETTINGS },
  tabOrder: ['interaction', 'danmaku', 'gift', 'superchat', 'audience'],
  specialFollowUids: []
}

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<AppSettings>(JSON.parse(JSON.stringify(DEFAULT_SETTINGS)))
  const isLoaded = ref(false)
  const isSaving = ref(false)

  // ==================== 响应式计算属性 ====================
  
  const displaySettings = computed(() => settings.value.display)
  const mainWindowSettings = computed(() => settings.value.windows.main || DEFAULT_WINDOW_SETTINGS)
  
  // 弹幕设置
  const danmakuShowMedal = computed(() => settings.value.display.danmakuShowMedal)
  const danmakuShowGuard = computed(() => settings.value.display.danmakuShowGuard)
  const danmakuShowAdmin = computed(() => settings.value.display.danmakuShowAdmin)
  const danmakuShowTime = computed(() => settings.value.display.danmakuShowTime)
  const danmakuShowGuardBorder = computed(() => settings.value.display.danmakuShowGuardBorder)
  const danmakuEmoticonSize = computed(() => settings.value.display.danmakuEmoticonSize)
  
  // 礼物设置
  const giftMergeDisplay = computed(() => settings.value.display.giftMergeDisplay)
  const giftShowFree = computed(() => settings.value.display.giftShowFree)
  const giftMinPrice = computed(() => settings.value.display.giftMinPrice)
  const giftShowTime = computed(() => settings.value.display.giftShowTime)
  const giftShowMedal = computed(() => settings.value.display.giftShowMedal)
  const giftExpireEnabled = computed(() => settings.value.display.giftExpireEnabled)
  const giftExpireMinutes = computed(() => settings.value.display.giftExpireMinutes)
  const scMergeWithGift = computed(() => settings.value.display.scMergeWithGift)
  
  // 观众设置
  const audienceSortType = computed(() => settings.value.display.audienceSortType)
  const audienceShowEnterMsg = computed(() => settings.value.display.audienceShowEnterMsg)
  const audienceShowMedal = computed(() => settings.value.display.audienceShowMedal)

  // ==================== 加载/保存 ====================

  /**
   * 从后端加载设置
   * @param force 强制重新加载（设置窗口使用）
   */
  const loadSettings = async (force = false): Promise<boolean> => {
    if (!force && isLoaded.value) return true
    
    try {
      const configStr = await invoke<string>('load_config')
      if (configStr && configStr !== '{}') {
        const saved = JSON.parse(configStr)
        // 迁移：旧配置可能没有 interaction tab
        if (saved.tabOrder && !saved.tabOrder.includes('interaction')) {
          saved.tabOrder.unshift('interaction')
        }
        settings.value = {
          ...JSON.parse(JSON.stringify(DEFAULT_SETTINGS)),
          ...saved,
          user: saved.user || null,
          display: { ...DEFAULT_DISPLAY_SETTINGS, ...saved.display },
          windows: {
            main: { ...DEFAULT_WINDOW_SETTINGS, ...(saved.windows?.main || {}) },
            ...saved.windows
          },
          specialFollowUids: saved.specialFollowUids ?? []
        }
      }
      isLoaded.value = true
      console.log('[Settings] Loaded', force ? '(forced)' : '')
      return true
    } catch (e) {
      console.error('[Settings] Load failed:', e)
      isLoaded.value = true
      return false
    }
  }

  /**
   * 保存设置到后端（并广播更新）
   */
  const saveSettings = async (): Promise<boolean> => {
    if (isSaving.value) return false
    isSaving.value = true
    
    try {
      await invoke('save_config', { config: JSON.stringify(settings.value, null, 2) })
      console.log('[Settings] Saved')
      
      // 动态导入避免循环依赖
      const { broadcastSettingsUpdate } = await import('@/services/settings-sync')
      await broadcastSettingsUpdate()
      
      isSaving.value = false
      return true
    } catch (e) {
      console.error('[Settings] Save failed:', e)
      isSaving.value = false
      return false
    }
  }

  // 自动保存（防抖 1 秒）
  let saveTimer: number | null = null
  const autoSave = () => {
    if (!isLoaded.value) return
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = window.setTimeout(saveSettings, 1000)
  }

  // ==================== 设置更新方法 ====================

  const setRoomId = (roomId: string) => {
    settings.value.roomId = roomId
    autoSave()
  }

  const setCookie = (cookie: string) => {
    settings.value.cookie = cookie
    autoSave()
  }

  const getWindowSettings = (label: string): WindowSettings => {
    return settings.value.windows[label] || { ...DEFAULT_WINDOW_SETTINGS }
  }

  const updateWindowSettings = (label: string, updates: Partial<WindowSettings>) => {
    if (!settings.value.windows[label]) {
      settings.value.windows[label] = { ...DEFAULT_WINDOW_SETTINGS }
    }
    settings.value.windows[label] = { ...settings.value.windows[label], ...updates }
    autoSave()
  }

  const updateDisplaySettings = (updates: Partial<DisplaySettings>) => {
    settings.value.display = { ...settings.value.display, ...updates }
    autoSave()
  }

  const setAudienceSortType = (sortType: AudienceSortType) => {
    settings.value.display.audienceSortType = sortType
    autoSave()
  }

  // ==================== 特别关注 ====================

  const specialFollowUids = computed(() => settings.value.specialFollowUids)
  const specialFollowSet = computed(() => new Set(settings.value.specialFollowUids))

  const isSpecialFollow = (uid: number) => specialFollowSet.value.has(uid)

  const addSpecialFollow = (uid: number) => {
    if (!settings.value.specialFollowUids.includes(uid)) {
      settings.value.specialFollowUids.push(uid)
      autoSave()
    }
  }

  const removeSpecialFollow = (uid: number) => {
    const idx = settings.value.specialFollowUids.indexOf(uid)
    if (idx !== -1) {
      settings.value.specialFollowUids.splice(idx, 1)
      autoSave()
    }
  }

  // ==================== 用户登录相关 ====================

  const isLoggedIn = computed(() => !!settings.value.user?.isLogin)
  const userInfo = computed(() => settings.value.user)

  const setUserLogin = (cookie: string, user: UserLoginInfo) => {
    settings.value.cookie = cookie
    settings.value.user = user
    autoSave()
  }

  const logout = () => {
    settings.value.cookie = ''
    settings.value.user = null
    autoSave()
  }

  return {
    settings,
    isLoaded,
    isSaving,
    // 计算属性
    displaySettings,
    mainWindowSettings,
    danmakuShowMedal,
    danmakuShowGuard,
    danmakuShowAdmin,
    danmakuShowTime,
    danmakuShowGuardBorder,
    danmakuEmoticonSize,
    giftMergeDisplay,
    giftShowFree,
    giftMinPrice,
    giftShowTime,
    giftShowMedal,
    giftExpireEnabled,
    giftExpireMinutes,
    scMergeWithGift,
    audienceSortType,
    audienceShowEnterMsg,
    audienceShowMedal,
    // 方法
    loadSettings,
    saveSettings,
    setRoomId,
    setCookie,
    getWindowSettings,
    updateWindowSettings,
    updateDisplaySettings,
    setAudienceSortType,
    // 特别关注
    specialFollowUids,
    specialFollowSet,
    isSpecialFollow,
    addSpecialFollow,
    removeSpecialFollow,
    // 用户登录
    isLoggedIn,
    userInfo,
    setUserLogin,
    logout
  }
})

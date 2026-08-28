import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { 
  AppSettings, 
  WindowSettings, 
  DisplaySettings,
  SpeechSettings,
  AudienceSortType,
  UserLoginInfo
} from '@/types'
import {
  DEFAULT_DISPLAY_SETTINGS,
  DEFAULT_SPEECH_SETTINGS,
  DEFAULT_WINDOW_SETTINGS
} from '@/types'
import { createLogger } from '@/services/logger'

const logger = createLogger('SettingsStore')

const DEFAULT_SETTINGS: AppSettings = {
  roomId: '',
  cookie: '',
  user: null,
  windows: {
    main: { ...DEFAULT_WINDOW_SETTINGS }
  },
  display: { ...DEFAULT_DISPLAY_SETTINGS },
  speech: { ...DEFAULT_SPEECH_SETTINGS },
  tabOrder: ['interaction', 'danmaku', 'gift', 'superchat', 'audience'],
  specialFollowUids: [],
  danmakuFilterUids: []
}

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<AppSettings>(JSON.parse(JSON.stringify(DEFAULT_SETTINGS)))
  const isLoaded = ref(false)
  const isSaving = ref(false)

  // ==================== 响应式计算属性 ====================
  
  const displaySettings = computed(() => settings.value.display)
  const speechSettings = computed(() => settings.value.speech)
  const mainWindowSettings = computed(() => settings.value.windows.main || DEFAULT_WINDOW_SETTINGS)

  // 粉丝勋章通用设置
  const medalShowUnlit = computed(() => settings.value.display.medalShowUnlit)
  const medalShowOtherRoom = computed(() => settings.value.display.medalShowOtherRoom)
  
  // 弹幕设置
  const danmakuShowMedal = computed(() => settings.value.display.danmakuShowMedal)
  const danmakuShowGuard = computed(() => settings.value.display.danmakuShowGuard)
  const danmakuShowAdmin = computed(() => settings.value.display.danmakuShowAdmin)
  const danmakuShowTime = computed(() => settings.value.display.danmakuShowTime)
  const danmakuShowGuardBorder = computed(() => settings.value.display.danmakuShowGuardBorder)
  const danmakuEmoticonSize = computed(() => settings.value.display.danmakuEmoticonSize)
  const contentFontFamily = computed(() => settings.value.display.contentFontFamily)
  const contentFontWeight = computed(() => settings.value.display.contentFontWeight)
  const danmakuFontColor = computed(() => settings.value.display.danmakuFontColor)
  const danmakuUsernameColor = computed(() => settings.value.display.danmakuUsernameColor)
  
  // 礼物设置
  const giftMergeDisplay = computed(() => settings.value.display.giftMergeDisplay)
  const giftShowFree = computed(() => settings.value.display.giftShowFree)
  const giftMinPrice = computed(() => settings.value.display.giftMinPrice)
  const giftShowTime = computed(() => settings.value.display.giftShowTime)
  const giftShowMedal = computed(() => settings.value.display.giftShowMedal)
  const giftExpireEnabled = computed(() => settings.value.display.giftExpireEnabled)
  const giftExpireMinutes = computed(() => settings.value.display.giftExpireMinutes)
  const giftFontColor = computed(() => settings.value.display.giftFontColor)
  const giftUsernameColor = computed(() => settings.value.display.giftUsernameColor)
  const giftPriceColor = computed(() => settings.value.display.giftPriceColor)
  const scMergeWithGift = computed(() => settings.value.display.scMergeWithGift)
  const superChatFontColor = computed(() => settings.value.display.superChatFontColor)
  
  // 观众设置
  const audienceSortType = computed(() => settings.value.display.audienceSortType)
  const audienceShowEnterMsg = computed(() => settings.value.display.audienceShowEnterMsg)
  const audienceShowMedal = computed(() => settings.value.display.audienceShowMedal)
  const audienceAutoRefreshEnabled = computed(() => settings.value.display.audienceAutoRefreshEnabled)
  const audienceAutoRefreshIntervalSeconds = computed(() => settings.value.display.audienceAutoRefreshIntervalSeconds)
  const audienceFontColor = computed(() => settings.value.display.audienceFontColor)
  const audienceScoreColor = computed(() => settings.value.display.audienceScoreColor)

  // 入场通知设置
  const entryShowEnabled = computed(() => settings.value.display.entryShowEnabled)
  const entryPanelShowInInteraction = computed(() => settings.value.display.entryPanelShowInInteraction)
  const entryPanelShowInAudience = computed(() => settings.value.display.entryPanelShowInAudience)
  const entryFilterAll = computed(() => settings.value.display.entryFilterAll)
  const entryFilterCaptain = computed(() => settings.value.display.entryFilterCaptain)
  const entryFilterAdmiral = computed(() => settings.value.display.entryFilterAdmiral)
  const entryFilterGovernor = computed(() => settings.value.display.entryFilterGovernor)
  const entryFilterSpecialFollow = computed(() => settings.value.display.entryFilterSpecialFollow)
  const entryShowMedal = computed(() => settings.value.display.entryShowMedal)
  const entryShowGuard = computed(() => settings.value.display.entryShowGuard)
  const entryPanelHeight = computed(() => settings.value.display.entryPanelHeight)
  const entryFontColor = computed(() => settings.value.display.entryFontColor)
  const entryTimeColor = computed(() => settings.value.display.entryTimeColor)

  // ==================== 语音运行时同步 ====================

  const syncSpeechRuntime = async (): Promise<void> => {
    try {
      await invoke('update_speech_settings', {
        settings: settings.value.speech,
        ignoredUids: settings.value.danmakuFilterUids,
        giftShowFree: settings.value.display.giftShowFree,
        giftMinPrice: settings.value.display.giftMinPrice
      })
    } catch (error) {
      logger.warn('Speech settings sync failed:', error)
    }
  }

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
        const savedDisplay = saved.display ?? {}
        // 迁移：旧配置可能没有 interaction tab
        if (saved.tabOrder && !saved.tabOrder.includes('interaction')) {
          saved.tabOrder.unshift('interaction')
        }
        const display = {
          ...DEFAULT_DISPLAY_SETTINGS,
          ...savedDisplay,
          contentFontFamily: savedDisplay.contentFontFamily
            ?? savedDisplay.danmakuFontFamily
            ?? DEFAULT_DISPLAY_SETTINGS.contentFontFamily,
          contentFontWeight: savedDisplay.contentFontWeight
            ?? savedDisplay.danmakuFontWeight
            ?? DEFAULT_DISPLAY_SETTINGS.contentFontWeight
        }
        settings.value = {
          ...JSON.parse(JSON.stringify(DEFAULT_SETTINGS)),
          ...saved,
          user: saved.user || null,
          display,
          speech: {
            ...DEFAULT_SPEECH_SETTINGS,
            ...(saved.speech ?? {})
          },
          windows: {
            main: { ...DEFAULT_WINDOW_SETTINGS, ...(saved.windows?.main || {}) },
            ...saved.windows
          },
          specialFollowUids: saved.specialFollowUids ?? [],
          danmakuFilterUids: saved.danmakuFilterUids ?? []
        }
      }
      isLoaded.value = true
      await syncSpeechRuntime()
      logger.debug('Loaded', force ? '(forced)' : '')
      return true
    } catch (e) {
      logger.error('Load failed:', e)
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
      logger.debug('Saved')
      
      // 动态导入避免循环依赖
      const { broadcastSettingsUpdate } = await import('@/services/settings-sync')
      await broadcastSettingsUpdate()
      
      isSaving.value = false
      return true
    } catch (e) {
      logger.error('Save failed:', e)
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
    if ('giftShowFree' in updates || 'giftMinPrice' in updates) {
      void syncSpeechRuntime()
    }
    autoSave()
  }

  const updateSpeechSettings = (updates: Partial<SpeechSettings>) => {
    settings.value.speech = { ...settings.value.speech, ...updates }
    void syncSpeechRuntime()
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

  // ==================== 弹幕过滤 ====================

  const danmakuFilterUids = computed(() => settings.value.danmakuFilterUids)
  const danmakuFilterSet = computed(() => new Set(settings.value.danmakuFilterUids))

  const isDanmakuFiltered = (uid: number) => danmakuFilterSet.value.has(uid)

  const addDanmakuFilter = (uid: number) => {
    if (!Number.isSafeInteger(uid) || uid <= 0) return
    if (!settings.value.danmakuFilterUids.includes(uid)) {
      settings.value.danmakuFilterUids.push(uid)
      void syncSpeechRuntime()
      autoSave()
    }
  }

  const removeDanmakuFilter = (uid: number) => {
    const idx = settings.value.danmakuFilterUids.indexOf(uid)
    if (idx !== -1) {
      settings.value.danmakuFilterUids.splice(idx, 1)
      void syncSpeechRuntime()
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
    speechSettings,
    mainWindowSettings,
    medalShowUnlit,
    medalShowOtherRoom,
    danmakuShowMedal,
    danmakuShowGuard,
    danmakuShowAdmin,
    danmakuShowTime,
    danmakuShowGuardBorder,
    danmakuEmoticonSize,
    contentFontFamily,
    contentFontWeight,
    danmakuFontColor,
    danmakuUsernameColor,
    giftMergeDisplay,
    giftShowFree,
    giftMinPrice,
    giftShowTime,
    giftShowMedal,
    giftExpireEnabled,
    giftExpireMinutes,
    giftFontColor,
    giftUsernameColor,
    giftPriceColor,
    scMergeWithGift,
    superChatFontColor,
    audienceSortType,
    audienceShowEnterMsg,
    audienceShowMedal,
    audienceAutoRefreshEnabled,
    audienceAutoRefreshIntervalSeconds,
    audienceFontColor,
    audienceScoreColor,
    entryShowEnabled,
    entryPanelShowInInteraction,
    entryPanelShowInAudience,
    entryFilterAll,
    entryFilterCaptain,
    entryFilterAdmiral,
    entryFilterGovernor,
    entryFilterSpecialFollow,
    entryShowMedal,
    entryShowGuard,
    entryPanelHeight,
    entryFontColor,
    entryTimeColor,
    // 方法
    loadSettings,
    saveSettings,
    setRoomId,
    setCookie,
    getWindowSettings,
    updateWindowSettings,
    updateDisplaySettings,
    updateSpeechSettings,
    syncSpeechRuntime,
    setAudienceSortType,
    // 特别关注
    specialFollowUids,
    specialFollowSet,
    isSpecialFollow,
    addSpecialFollow,
    removeSpecialFollow,
    // 弹幕过滤
    danmakuFilterUids,
    danmakuFilterSet,
    isDanmakuFiltered,
    addDanmakuFilter,
    removeDanmakuFilter,
    // 用户登录
    isLoggedIn,
    userInfo,
    setUserLogin,
    logout
  }
})

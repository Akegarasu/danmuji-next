<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { storeToRefs } from 'pinia'
import TitleBar from '@/components/common/TitleBar.vue'
import LoginDialog from '@/components/common/LoginDialog.vue'
import ConfirmDialog from '@/components/common/ConfirmDialog.vue'
import UpdateToast from '@/components/common/UpdateToast.vue'
import { useSettingsStore } from '@/stores/settings'
import { useDanmakuStore } from '@/stores/danmaku'
import { applyCurrentSettings, initSettingsApplier } from '@/services/settings-applier'
import { initSettingsSync } from '@/services/settings-sync'
import { initWindowManager, cleanupWindowManager } from '@/services/window-manager'
import {
  connectRoom,
  disconnectRoom,
  getConnectionStatus,
  getCurrentRoomInfo,
  type ConnectionStatus,
  type RoomInfoResponse
} from '@/services/blive-client'
import { lookupArchiveUserNames } from '@/services/archive'
import { checkForUpdate, getAppVersion, isPortable, type UpdateInfo } from '@/services/updater'
import { getSpeechStatus, getSpeechVoices, previewSpeech } from '@/services/speech'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type {
  AudienceSortType,
  ContentFontWeight,
  DisplaySettings,
  SpeechStatus,
  SpeechVoice
} from '@/types'
import type { UserInfo } from '@/services/auth'
import { createLogger } from '@/services/logger'

const settingsStore = useSettingsStore()
const danmakuStore = useDanmakuStore()
const { settings, isSaving, isLoggedIn, userInfo } = storeToRefs(settingsStore)
const logger = createLogger('SettingsWindow')

// ==================== 登录相关 ====================

const showLoginDialog = ref(false)

const openLoginDialog = () => {
  showLoginDialog.value = true
}

const handleLoginSuccess = (cookie: string, info: UserInfo) => {
  settingsStore.setUserLogin(cookie, {
    uid: info.uid,
    uname: info.uname,
    face: info.face,
    isLogin: info.is_login
  })
  showLoginDialog.value = false
}

const showLogoutConfirm = ref(false)
const showAlertDialog = ref(false)
const alertMessage = ref('')

const handleLogout = () => {
  showLogoutConfirm.value = true
}

const doLogout = () => {
  settingsStore.logout()
}

const showAlert = (message: string) => {
  alertMessage.value = message
  showAlertDialog.value = true
}

// ==================== 连接状态 ====================

const connectionStatus = ref<ConnectionStatus>('disconnected')
const isConnecting = computed(() => connectionStatus.value === 'connecting')
const isConnected = computed(() => connectionStatus.value === 'connected')
const isReconnecting = computed(() => connectionStatus.value === 'reconnecting')
const hasError = computed(() => typeof connectionStatus.value === 'object' && 'error' in connectionStatus.value)
const errorMessage = computed(() => {
  if (typeof connectionStatus.value === 'object' && 'error' in connectionStatus.value) {
    return connectionStatus.value.error.message
  }
  return ''
})

// 连接状态文本
const statusText = computed(() => {
  if (isConnecting.value) return '连接中...'
  if (isConnected.value) return '已连接'
  if (isReconnecting.value) return '重连中...'
  if (hasError.value) return '连接失败'
  return '未连接'
})

// 连接按钮文本
const connectBtnText = computed(() => {
  if (isConnecting.value) return '连接中...'
  if (isConnected.value || isReconnecting.value) return '断开连接'
  return '连接直播间'
})

// 状态事件监听器
let statusUnlisten: UnlistenFn | null = null

const applyRoomInfo = (roomInfo: RoomInfoResponse) => {
  danmakuStore.updateRoomInfo({
    roomId: roomInfo.room_id.toString(),
    title: roomInfo.title,
    liveStatus: roomInfo.live_status,
    streamerUid: roomInfo.uid
  })
}

const syncCurrentRoomInfo = async () => {
  try {
    const roomInfo = await getCurrentRoomInfo()
    if (roomInfo) applyRoomInfo(roomInfo)
  } catch (e) {
    logger.warn('Failed to sync room info:', e)
  }
}

// 初始化连接状态（从后端获取）
const initConnectionStatus = async () => {
  try {
    const status = await getConnectionStatus()
    connectionStatus.value = status

    // 同步到 store
    if (status === 'connected') {
      danmakuStore.setConnected(true)

      // 获取房间信息
      await syncCurrentRoomInfo()
    } else if (status === 'disconnected') {
      danmakuStore.setConnected(false)
    }
  } catch (e) {
    logger.error('Failed to get connection status:', e)
  }
}

// 监听连接状态变化事件
const initStatusListener = async () => {
  statusUnlisten = await listen<ConnectionStatus>('blive-status', (event) => {
    connectionStatus.value = event.payload

    if (event.payload === 'connected') {
      danmakuStore.setConnected(true)
      void syncCurrentRoomInfo()
    } else if (event.payload === 'disconnected') {
      danmakuStore.setConnected(false)
    } else if (typeof event.payload === 'object' && 'error' in event.payload) {
      danmakuStore.setConnected(false)
    }
  })
}

// 连接/断开
const toggleConnection = async () => {
  if (isConnecting.value) return

  if (isConnected.value || isReconnecting.value) {
    // 断开连接
    await disconnectRoom()
    connectionStatus.value = 'disconnected'
    danmakuStore.setConnected(false)
  } else {
    // 连接
    const roomIdStr = settings.value.roomId
    const roomIdNum = parseInt(roomIdStr, 10)
    if (!roomIdNum || roomIdNum <= 0) {
      showAlert('请输入有效的房间号')
      return
    }

    const cookieVal = settings.value.cookie
    if (!cookieVal) {
      showAlert('请先登录账号')
      return
    }

    connectionStatus.value = 'connecting'

    try {
      const result = await connectRoom(roomIdNum, cookieVal)

      if (result.success) {
        connectionStatus.value = 'connected'
        danmakuStore.setConnected(true)

        if (result.room_info) {
          applyRoomInfo(result.room_info)
        }
      } else {
        connectionStatus.value = { error: { message: result.message } }
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      connectionStatus.value = { error: { message: msg } }
    }
  }
}

// ==================== 初始化 ====================

onMounted(async () => {
  // 从后端加载最新配置
  await settingsStore.loadSettings(true)

  // 初始化服务
  initSettingsApplier()
  await initSettingsSync()
  await initWindowManager('settings')

  // 初始化连接状态（从后端同步）
  await initConnectionStatus()
  await initStatusListener()

  // 应用设置到 UI
  applyCurrentSettings()

  // 加载语音引擎及关于信息
  await initSpeechInfo()
  initAboutInfo()
})

onUnmounted(async () => {
  // 清理状态监听
  if (statusUnlisten) {
    statusUnlisten()
    statusUnlisten = null
  }
  if (speechStatusTimer) {
    clearInterval(speechStatusTimer)
    speechStatusTimer = null
  }

  await cleanupWindowManager('settings')
})

// ==================== UI 状态 ====================

const activeSection = ref('connection')

const sections = [
  { id: 'connection', label: '连接' },
  { id: 'general', label: '通用' },
  { id: 'speech', label: '语音' },
  { id: 'font', label: '字体' },
  { id: 'danmaku', label: '弹幕' },
  { id: 'gift', label: '礼物' },
  { id: 'audience', label: '观众' },
  { id: 'special-follow', label: '特别关注' },
  { id: 'danmaku-filter', label: '弹幕过滤' },
  { id: 'shield-keyword', label: '屏蔽词' },
  { id: 'about', label: '关于' }
]

const fontFamilyOptions = [
  { label: '跟随界面字体', value: 'var(--font-family)' },
  { label: '微软雅黑', value: "'Microsoft YaHei', sans-serif" },
  { label: '苹方', value: "'PingFang SC', sans-serif" },
  { label: '思源黑体', value: "'Source Han Sans SC', 'Noto Sans CJK SC', sans-serif" },
  { label: '黑体', value: 'SimHei, sans-serif' },
  { label: '宋体', value: 'SimSun, serif' },
  { label: '等宽字体', value: "'Cascadia Mono', Consolas, monospace" }
]

const fontWeightOptions: Array<{ label: string; value: ContentFontWeight }> = [
  { label: '细体 300', value: 300 },
  { label: '常规 400', value: 400 },
  { label: '中等 500', value: 500 },
  { label: '半粗 600', value: 600 },
  { label: '粗体 700', value: 700 },
  { label: '特粗 800', value: 800 }
]

const neutralColorPresets = ['#ffffff', '#ebebeb', '#d6d6d6', '#b8b8b8', '#9b9b9b', '#6b6b6b']
const accentColorPresets = ['#5c9eff', '#7dd3fc', '#ff7eb3', '#f5c842', '#34d399', '#f97316']
const lightOnColorPresets = ['#ffffff', '#fff7cc', '#dff6ff', '#ffe4f0', '#e8fdf3', '#111827']

type DisplayColorKey =
  | 'danmakuFontColor'
  | 'danmakuUsernameColor'
  | 'giftFontColor'
  | 'giftUsernameColor'
  | 'giftPriceColor'
  | 'superChatFontColor'
  | 'audienceFontColor'
  | 'audienceScoreColor'
  | 'entryFontColor'
  | 'entryTimeColor'

const updateDisplayColor = (key: DisplayColorKey, value: string) => {
  settingsStore.updateDisplaySettings({ [key]: value } as Partial<DisplaySettings>)
}

const getInputValue = (event: Event) => (event.target as HTMLInputElement).value

const fontColorSettings = computed<Array<{
  key: DisplayColorKey
  label: string
  value: string
  presets: string[]
}>>(() => [
  {
    key: 'danmakuFontColor',
    label: '弹幕正文',
    value: settings.value.display.danmakuFontColor,
    presets: [...neutralColorPresets, ...accentColorPresets]
  },
  {
    key: 'danmakuUsernameColor',
    label: '弹幕用户名',
    value: settings.value.display.danmakuUsernameColor,
    presets: [...neutralColorPresets, ...accentColorPresets]
  },
  {
    key: 'giftFontColor',
    label: '礼物名称',
    value: settings.value.display.giftFontColor,
    presets: [...neutralColorPresets, ...accentColorPresets]
  },
  {
    key: 'giftUsernameColor',
    label: '礼物用户名',
    value: settings.value.display.giftUsernameColor,
    presets: [...neutralColorPresets, ...accentColorPresets]
  },
  {
    key: 'giftPriceColor',
    label: '礼物价格',
    value: settings.value.display.giftPriceColor,
    presets: [...accentColorPresets, ...neutralColorPresets]
  },
  {
    key: 'superChatFontColor',
    label: 'SC 正文',
    value: settings.value.display.superChatFontColor,
    presets: lightOnColorPresets
  },
  {
    key: 'audienceFontColor',
    label: '观众名称',
    value: settings.value.display.audienceFontColor,
    presets: [...neutralColorPresets, ...accentColorPresets]
  },
  {
    key: 'audienceScoreColor',
    label: '观众数值',
    value: settings.value.display.audienceScoreColor,
    presets: [...accentColorPresets, ...neutralColorPresets]
  },
  {
    key: 'entryFontColor',
    label: '入场名称',
    value: settings.value.display.entryFontColor,
    presets: [...neutralColorPresets, ...accentColorPresets]
  },
  {
    key: 'entryTimeColor',
    label: '入场时间',
    value: settings.value.display.entryTimeColor,
    presets: [...neutralColorPresets, ...accentColorPresets]
  }
])

// const sortOptions = [
//   { value: 'enterTime', label: '进入时间' },
//   { value: 'giftValue', label: '礼物价值' },
//   { value: 'medalLevel', label: '粉丝牌等级' }
// ]

// ==================== 设置读写（带转换）====================

// 房间号
const roomId = computed({
  get: () => settings.value.roomId,
  set: (v) => settingsStore.setRoomId(v)
})

// 透明度（UI 显示为百分比）
const opacity = computed({
  get: () => Math.round((settings.value.windows.main?.opacity ?? 0.9) * 100),
  set: (v) => settingsStore.updateWindowSettings('main', { opacity: v / 100 })
})

// 内容字体大小
const fontSize = computed({
  get: () => settings.value.windows.main?.fontSize ?? 14,
  set: (v) => settingsStore.updateWindowSettings('main', { fontSize: v })
})

// UI 字体大小
const uiFontSize = computed({
  get: () => settings.value.windows.main?.uiFontSize ?? 14,
  set: (v) => settingsStore.updateWindowSettings('main', { uiFontSize: v })
})

// 窗口置顶
const alwaysOnTop = computed({
  get: () => settings.value.windows.main?.alwaysOnTop ?? true,
  set: (v) => settingsStore.updateWindowSettings('main', { alwaysOnTop: v })
})

// 隐藏窗口边框
const hideBorder = computed({
  get: () => settings.value.windows.main?.hideBorder ?? false,
  set: (v) => settingsStore.updateWindowSettings('main', { hideBorder: v })
})

// 失焦时隐藏标题栏
const autoHideUi = computed({
  get: () => settings.value.windows.main?.autoHideUi ?? false,
  set: (v) => settingsStore.updateWindowSettings('main', { autoHideUi: v })
})

// 语音播报
const createUnavailableSpeechStatus = (error: string | null = null): SpeechStatus => ({
  available: false,
  speaking: false,
  danmaku_suspended: false,
  queue_depth: 0,
  error
})

const getErrorMessage = (error: unknown) =>
  error instanceof Error ? error.message : String(error)

const speechVoices = ref<SpeechVoice[]>([])
const speechVoicesLoading = ref(false)
const speechPreviewing = ref(false)
const speechLoadError = ref('')
const speechStatus = ref<SpeechStatus>(createUnavailableSpeechStatus())
const speechVoiceControlsDisabled = computed(() =>
  speechVoicesLoading.value || speechVoices.value.length === 0
)
let speechStatusTimer: ReturnType<typeof setInterval> | null = null

const speechEnabled = computed({
  get: () => settings.value.speech.enabled,
  set: (v) => settingsStore.updateSpeechSettings({ enabled: v })
})

const speechVoiceId = computed({
  get: () => settings.value.speech.voiceId ?? '',
  set: (v: string) => settingsStore.updateSpeechSettings({ voiceId: v || null })
})

const speechRate = computed({
  get: () => settings.value.speech.rate,
  set: (v: number) => settingsStore.updateSpeechSettings({ rate: Math.min(10, Math.max(-10, v)) })
})

const speechSpeakDanmaku = computed({
  get: () => settings.value.speech.speakDanmaku,
  set: (v) => settingsStore.updateSpeechSettings({ speakDanmaku: v })
})

const speechSpeakGift = computed({
  get: () => settings.value.speech.speakGift,
  set: (v) => settingsStore.updateSpeechSettings({ speakGift: v })
})

const speechSpeakSuperChat = computed({
  get: () => settings.value.speech.speakSuperChat,
  set: (v) => settingsStore.updateSpeechSettings({ speakSuperChat: v })
})

const speechRateLabel = computed(() => {
  if (speechRate.value === 0) return '正常'
  return speechRate.value > 0 ? `加快 +${speechRate.value}` : `减慢 ${speechRate.value}`
})

const speechStatusText = computed(() => {
  const status = speechStatus.value
  if (!status.available) return status.error || '语音引擎不可用'
  if (status.error) return status.error
  if (!speechEnabled.value) return '语音播报未启用'
  if (status.danmaku_suspended) return '弹幕过多，已临时暂停弹幕播报'
  if (status.speaking) return `正在播报（队列 ${status.queue_depth}）`
  if (status.queue_depth > 0) return `等待播报（队列 ${status.queue_depth}）`
  return '语音播报正常'
})

const refreshSpeechStatus = async () => {
  try {
    speechStatus.value = await getSpeechStatus()
  } catch (error) {
    speechStatus.value = createUnavailableSpeechStatus(getErrorMessage(error))
  }
}

const loadSpeechVoices = async () => {
  speechVoicesLoading.value = true
  speechLoadError.value = ''
  try {
    speechVoices.value = await getSpeechVoices()
  } catch (error) {
    speechVoices.value = []
    speechLoadError.value = getErrorMessage(error)
  } finally {
    speechVoicesLoading.value = false
  }
}

const initSpeechInfo = async () => {
  await Promise.all([loadSpeechVoices(), refreshSpeechStatus()])
  speechStatusTimer = setInterval(refreshSpeechStatus, 1000)
}

const handleSpeechPreview = async () => {
  if (speechPreviewing.value) return
  speechPreviewing.value = true
  speechLoadError.value = ''
  try {
    await previewSpeech(settings.value.speech.voiceId, settings.value.speech.rate)
  } catch (error) {
    speechLoadError.value = getErrorMessage(error)
  } finally {
    speechPreviewing.value = false
  }
}

// 显示设置 - 弹幕
const danmakuShowMedal = computed({
  get: () => settings.value.display.danmakuShowMedal,
  set: (v) => settingsStore.updateDisplaySettings({ danmakuShowMedal: v })
})

const danmakuShowWealthLevel = computed({
  get: () => settings.value.display.danmakuShowWealthLevel,
  set: (v) => settingsStore.updateDisplaySettings({ danmakuShowWealthLevel: v })
})

const medalShowUnlit = computed({
  get: () => settings.value.display.medalShowUnlit,
  set: (v) => settingsStore.updateDisplaySettings({ medalShowUnlit: v })
})

const medalShowOtherRoom = computed({
  get: () => settings.value.display.medalShowOtherRoom,
  set: (v) => settingsStore.updateDisplaySettings({ medalShowOtherRoom: v })
})

const danmakuShowGuard = computed({
  get: () => settings.value.display.danmakuShowGuard,
  set: (v) => settingsStore.updateDisplaySettings({ danmakuShowGuard: v })
})

const danmakuShowAdmin = computed({
  get: () => settings.value.display.danmakuShowAdmin,
  set: (v) => settingsStore.updateDisplaySettings({ danmakuShowAdmin: v })
})

const danmakuShowTime = computed({
  get: () => settings.value.display.danmakuShowTime,
  set: (v) => settingsStore.updateDisplaySettings({ danmakuShowTime: v })
})

const danmakuShowGuardBorder = computed({
  get: () => settings.value.display.danmakuShowGuardBorder,
  set: (v) => settingsStore.updateDisplaySettings({ danmakuShowGuardBorder: v })
})

const danmakuEmoticonSize = computed({
  get: () => settings.value.display.danmakuEmoticonSize,
  set: (v) => settingsStore.updateDisplaySettings({ danmakuEmoticonSize: v })
})

const contentFontFamily = computed({
  get: () => settings.value.display.contentFontFamily,
  set: (v) => settingsStore.updateDisplaySettings({ contentFontFamily: v })
})

const contentFontWeight = computed({
  get: () => settings.value.display.contentFontWeight,
  set: (v: ContentFontWeight) => settingsStore.updateDisplaySettings({ contentFontWeight: v })
})

// 显示设置 - 礼物
const giftMergeDisplay = computed({
  get: () => settings.value.display.giftMergeDisplay,
  set: (v) => settingsStore.updateDisplaySettings({ giftMergeDisplay: v })
})

const giftShowFree = computed({
  get: () => settings.value.display.giftShowFree,
  set: (v) => settingsStore.updateDisplaySettings({ giftShowFree: v })
})

const giftMinPrice = computed({
  get: () => settings.value.display.giftMinPrice,
  set: (v) => settingsStore.updateDisplaySettings({ giftMinPrice: v })
})

const giftShowTime = computed({
  get: () => settings.value.display.giftShowTime,
  set: (v) => settingsStore.updateDisplaySettings({ giftShowTime: v })
})

const giftShowMedal = computed({
  get: () => settings.value.display.giftShowMedal,
  set: (v) => settingsStore.updateDisplaySettings({ giftShowMedal: v })
})

const giftExpireEnabled = computed({
  get: () => settings.value.display.giftExpireEnabled,
  set: (v) => settingsStore.updateDisplaySettings({ giftExpireEnabled: v })
})

const giftExpireMinutes = computed({
  get: () => settings.value.display.giftExpireMinutes,
  set: (v) => settingsStore.updateDisplaySettings({ giftExpireMinutes: v })
})

const scMergeWithGift = computed({
  get: () => settings.value.display.scMergeWithGift,
  set: (v) => settingsStore.updateDisplaySettings({ scMergeWithGift: v })
})

// 显示设置 - 观众
const audienceSortType = computed({
  get: () => settings.value.display.audienceSortType,
  set: (v: AudienceSortType) => settingsStore.updateDisplaySettings({ audienceSortType: v })
})

const audienceShowEnterMsg = computed({
  get: () => settings.value.display.audienceShowEnterMsg,
  set: (v) => settingsStore.updateDisplaySettings({ audienceShowEnterMsg: v })
})

const audienceShowMedal = computed({
  get: () => settings.value.display.audienceShowMedal,
  set: (v) => settingsStore.updateDisplaySettings({ audienceShowMedal: v })
})

const audienceAutoRefreshEnabled = computed({
  get: () => settings.value.display.audienceAutoRefreshEnabled,
  set: (v) => settingsStore.updateDisplaySettings({ audienceAutoRefreshEnabled: v })
})

const clampAudienceRefreshInterval = (seconds: number) =>
  Math.min(300, Math.max(10, Number.isFinite(seconds) ? Math.round(seconds) : 120))

const audienceAutoRefreshIntervalSeconds = computed({
  get: () => settings.value.display.audienceAutoRefreshIntervalSeconds,
  set: (v) => settingsStore.updateDisplaySettings({
    audienceAutoRefreshIntervalSeconds: clampAudienceRefreshInterval(v)
  })
})

const audienceAutoRefreshIntervalLabel = computed(() => {
  const seconds = audienceAutoRefreshIntervalSeconds.value
  if (seconds % 60 === 0) return `${seconds / 60} 分钟`
  return `${seconds} 秒`
})

// 入场通知设置
const entryShowEnabled = computed({
  get: () => settings.value.display.entryShowEnabled,
  set: (v) => settingsStore.updateDisplaySettings({ entryShowEnabled: v })
})

const entryPanelShowInInteraction = computed({
  get: () => settings.value.display.entryPanelShowInInteraction,
  set: (v) => settingsStore.updateDisplaySettings({ entryPanelShowInInteraction: v })
})

const entryPanelShowInAudience = computed({
  get: () => settings.value.display.entryPanelShowInAudience,
  set: (v) => settingsStore.updateDisplaySettings({ entryPanelShowInAudience: v })
})

const entryShowMedal = computed({
  get: () => settings.value.display.entryShowMedal,
  set: (v) => settingsStore.updateDisplaySettings({ entryShowMedal: v })
})

const entryShowGuard = computed({
  get: () => settings.value.display.entryShowGuard,
  set: (v) => settingsStore.updateDisplaySettings({ entryShowGuard: v })
})

const entryFilterAll = computed({
  get: () => settings.value.display.entryFilterAll,
  set: (v) => settingsStore.updateDisplaySettings({ entryFilterAll: v })
})

const entryFilterCaptain = computed({
  get: () => settings.value.display.entryFilterCaptain,
  set: (v) => settingsStore.updateDisplaySettings({ entryFilterCaptain: v })
})

const entryFilterAdmiral = computed({
  get: () => settings.value.display.entryFilterAdmiral,
  set: (v) => settingsStore.updateDisplaySettings({ entryFilterAdmiral: v })
})

const entryFilterGovernor = computed({
  get: () => settings.value.display.entryFilterGovernor,
  set: (v) => settingsStore.updateDisplaySettings({ entryFilterGovernor: v })
})

const entryFilterSpecialFollow = computed({
  get: () => settings.value.display.entryFilterSpecialFollow,
  set: (v) => settingsStore.updateDisplaySettings({ entryFilterSpecialFollow: v })
})

// ==================== 特别关注 ====================

const newSpecialUid = ref('')
const specialFollowNames = ref<Record<number, string>>({})
const specialFollowNamesLoading = ref(false)
let specialFollowNamesLoadId = 0

const addSpecialUid = () => {
  const uid = parseInt(newSpecialUid.value.trim(), 10)
  if (!uid || uid <= 0) return
  settingsStore.addSpecialFollow(uid)
  newSpecialUid.value = ''
}

const removeSpecialUid = (uid: number) => {
  settingsStore.removeSpecialFollow(uid)
}

const getSpecialFollowLabel = (uid: number) => {
  const name = specialFollowNames.value[uid]
  return name ? `${name}（${uid}）` : `UID: ${uid}`
}

const loadSpecialFollowNames = async () => {
  const uids = [...settingsStore.specialFollowUids]
  const loadId = ++specialFollowNamesLoadId

  if (uids.length === 0) {
    specialFollowNames.value = {}
    specialFollowNamesLoading.value = false
    return
  }

  specialFollowNamesLoading.value = true

  try {
    const users = await lookupArchiveUserNames(uids)
    if (loadId !== specialFollowNamesLoadId) return

    const nextNames: Record<number, string> = {}
    for (const user of users) {
      nextNames[user.uid] = user.name
    }
    specialFollowNames.value = nextNames
  } catch (e) {
    logger.warn('查询特别关注用户名失败:', e)
  } finally {
    if (loadId === specialFollowNamesLoadId) {
      specialFollowNamesLoading.value = false
    }
  }
}

watch(
  () => [activeSection.value, settingsStore.specialFollowUids.join(',')],
  () => {
    if (activeSection.value === 'special-follow') {
      void loadSpecialFollowNames()
    }
  },
  { immediate: true }
)

// ==================== 弹幕过滤 ====================

const newDanmakuFilterUid = ref('')
const danmakuFilterNames = ref<Record<number, string>>({})
const danmakuFilterNamesLoading = ref(false)
let danmakuFilterNamesLoadId = 0

const addDanmakuFilterUid = () => {
  const uid = parseInt(newDanmakuFilterUid.value.trim(), 10)
  if (!uid || uid <= 0) return
  settingsStore.addDanmakuFilter(uid)
  newDanmakuFilterUid.value = ''
}

const removeDanmakuFilterUid = (uid: number) => {
  settingsStore.removeDanmakuFilter(uid)
}

const getDanmakuFilterLabel = (uid: number) => {
  const name = danmakuFilterNames.value[uid]
  return name ? `${name}（${uid}）` : `UID: ${uid}`
}

const loadDanmakuFilterNames = async () => {
  const uids = [...settingsStore.danmakuFilterUids]
  const loadId = ++danmakuFilterNamesLoadId

  if (uids.length === 0) {
    danmakuFilterNames.value = {}
    danmakuFilterNamesLoading.value = false
    return
  }

  danmakuFilterNamesLoading.value = true

  try {
    const users = await lookupArchiveUserNames(uids)
    if (loadId !== danmakuFilterNamesLoadId) return

    const nextNames: Record<number, string> = {}
    for (const user of users) {
      nextNames[user.uid] = user.name
    }
    danmakuFilterNames.value = nextNames
  } catch (e) {
    logger.warn('查询弹幕过滤用户名失败:', e)
  } finally {
    if (loadId === danmakuFilterNamesLoadId) {
      danmakuFilterNamesLoading.value = false
    }
  }
}

watch(
  () => [activeSection.value, settingsStore.danmakuFilterUids.join(',')],
  () => {
    if (activeSection.value === 'danmaku-filter') {
      void loadDanmakuFilterNames()
    }
  },
  { immediate: true }
)

// ==================== 屏蔽词 ====================

interface ShieldKeyword {
  keyword: string
  uid: number
  name: string
  is_anchor: number
}

const shieldKeywords = ref<ShieldKeyword[]>([])
const shieldMaxLimit = ref(0)
const shieldLoading = ref(false)
const shieldError = ref('')
const shieldLoaded = ref(false)

watch(activeSection, (val) => {
  if (val === 'shield-keyword' && !shieldLoaded.value) {
    loadShieldKeywords()
  }
})

const loadShieldKeywords = async () => {
  const roomIdStr = settings.value.roomId
  const roomIdNum = parseInt(roomIdStr, 10)
  if (!roomIdNum || roomIdNum <= 0) {
    shieldError.value = '请先设置房间号'
    return
  }

  const cookie = settings.value.cookie
  if (!cookie) {
    shieldError.value = '请先登录账号'
    return
  }

  shieldLoading.value = true
  shieldError.value = ''

  try {
    const result = await invoke<{ keyword_list: ShieldKeyword[]; max_limit: number }>(
      'get_shield_keyword_list',
      { roomId: roomIdNum, cookie }
    )
    shieldKeywords.value = result.keyword_list
    shieldMaxLimit.value = result.max_limit
    shieldLoaded.value = true
  } catch (e) {
    shieldError.value = e instanceof Error ? e.message : String(e)
  } finally {
    shieldLoading.value = false
  }
}

const newShieldKeyword = ref('')
const shieldAdding = ref(false)
const shieldDeleting = ref<string | null>(null)
const showShieldDelConfirm = ref(false)

const addShieldKeyword = async () => {
  const keyword = newShieldKeyword.value.trim()
  if (!keyword) return

  const roomIdNum = parseInt(settings.value.roomId, 10)
  const cookie = settings.value.cookie
  if (!roomIdNum || !cookie) return

  shieldAdding.value = true
  shieldError.value = ''

  try {
    const result = await invoke<{ success: boolean; message: string }>(
      'add_shield_keyword',
      { roomId: roomIdNum, keyword, cookie }
    )
    if (result.success) {
      newShieldKeyword.value = ''
      await loadShieldKeywords()
    } else {
      shieldError.value = result.message
    }
  } catch (e) {
    shieldError.value = e instanceof Error ? e.message : String(e)
  } finally {
    shieldAdding.value = false
  }
}

const confirmDelKeyword = (keyword: string) => {
  shieldDeleting.value = keyword
  showShieldDelConfirm.value = true
}

const cancelDel = () => {
  shieldDeleting.value = null
}

const doDelShieldKeyword = async () => {
  const keyword = shieldDeleting.value
  if (!keyword) return

  const roomIdNum = parseInt(settings.value.roomId, 10)
  const cookie = settings.value.cookie
  if (!roomIdNum || !cookie) return

  shieldDeleting.value = null
  shieldError.value = ''

  try {
    const result = await invoke<{ success: boolean; message: string }>(
      'del_shield_keyword',
      { roomId: roomIdNum, keyword, cookie }
    )
    if (result.success) {
      await loadShieldKeywords()
    } else {
      shieldError.value = result.message
    }
  } catch (e) {
    shieldError.value = e instanceof Error ? e.message : String(e)
  }
}

// ==================== 手动保存 ====================

const saveStatus = ref<'idle' | 'saving' | 'saved'>('idle')

const handleSave = async () => {
  saveStatus.value = 'saving'
  const success = await settingsStore.saveSettings()
  saveStatus.value = success ? 'saved' : 'idle'
  if (success) {
    setTimeout(() => { saveStatus.value = 'idle' }, 2000)
  }
}

const saveButtonText = computed(() => {
  if (saveStatus.value === 'saving' || isSaving.value) return '保存中...'
  if (saveStatus.value === 'saved') return '已保存'
  return '保存设置'
})

// ==================== 关于 ====================

const appVersion = ref('')
const portableMode = ref(false)
const updateCheckStatus = ref<'idle' | 'checking' | 'found' | 'latest' | 'error'>('idle')
const aboutUpdateInfo = ref<UpdateInfo | null>(null)
const showUpdateToast = ref(false)

const initAboutInfo = async () => {
  appVersion.value = await getAppVersion()
  portableMode.value = await isPortable()
}

const manualCheckUpdate = async () => {
  if (updateCheckStatus.value === 'checking') return

  updateCheckStatus.value = 'checking'
  showUpdateToast.value = false
  try {
    const info = await checkForUpdate({ silent: false })
    if (info) {
      aboutUpdateInfo.value = info
      updateCheckStatus.value = 'found'
      showUpdateToast.value = true
    } else {
      aboutUpdateInfo.value = null
      updateCheckStatus.value = 'latest'
      setTimeout(() => { updateCheckStatus.value = 'idle' }, 3000)
    }
  } catch (error) {
    logger.error('手动检查更新失败:', error)
    updateCheckStatus.value = 'error'
    showAlert(`检查更新失败：${error instanceof Error ? error.message : String(error)}`)
    setTimeout(() => { updateCheckStatus.value = 'idle' }, 3000)
  }
}

const showUpdateChangelog = () => {
  if (!aboutUpdateInfo.value) return
  const notes = aboutUpdateInfo.value.notes || '暂无更新说明'
  showAlert(`v${aboutUpdateInfo.value.version} 更新日志\n\n${notes}`)
}

const openProjectUrl = async () => {
  await invoke('open_url', { url: 'https://github.com/Akegarasu/danmuji-next' })
}
</script>

<template>
  <div class="settings-window">
    <TitleBar title="设置" :is-sub-window="true" window-label="settings" />

    <div class="settings-layout">
      <!-- 侧边导航 -->
      <nav class="settings-nav">
        <button v-for="section in sections" :key="section.id" class="nav-item"
          :class="{ active: activeSection === section.id }" @click="activeSection = section.id">
          {{ section.label }}
        </button>
      </nav>

      <!-- 设置内容 -->
      <div class="settings-content">
        <!-- 通用设置 -->
        <div v-show="activeSection === 'connection'" class="section">
          <h3 class="section-title">连接设置</h3>

          <!-- 连接状态卡片 -->
          <div class="connection-card" :class="{ connected: isConnected, error: hasError }">
            <div class="connection-status">
              <span class="status-dot">●</span>
              <span class="status-text">{{ statusText }}</span>
              <span v-if="isConnected && danmakuStore.roomInfo.title" class="room-title">
                - {{ danmakuStore.roomInfo.title }}
              </span>
            </div>
            <div v-if="hasError" class="error-message">{{ errorMessage }}</div>
          </div>

          <div class="setting-group">
            <label class="setting-label">房间号</label>
            <input v-model="roomId" type="text" class="setting-input" placeholder="输入直播间房间号"
              :disabled="isConnected || isConnecting" />
          </div>

          <!-- 登录状态 -->
          <div class="setting-group">
            <label class="setting-label">账号登录</label>

            <!-- 已登录状态 -->
            <div v-if="isLoggedIn && userInfo" class="user-card">
              <img v-if="userInfo.face" :src="userInfo.face + '@64w_64h.jpg'" class="user-avatar"
                referrerpolicy="no-referrer" />
              <div class="user-info">
                <span class="user-name">{{ userInfo.uname }}</span>
                <span class="user-uid">UID: {{ userInfo.uid }}</span>
              </div>
              <button class="logout-btn" @click="handleLogout" :disabled="isConnected || isConnecting">
                退出登录
              </button>
            </div>

            <!-- 未登录状态 -->
            <div v-else class="login-prompt">
              <div class="login-info">
                <span class="login-icon">🔐</span>
                <span class="login-text">登录后可获取完整弹幕功能</span>
              </div>
              <button class="login-btn" @click="openLoginDialog" :disabled="isConnected || isConnecting">
                扫码登录
              </button>
            </div>
          </div>

          <div class="setting-group">
            <button class="connect-btn" :class="{
              connected: isConnected,
              connecting: isConnecting,
              reconnecting: isReconnecting,
              error: hasError
            }" @click="toggleConnection" :disabled="isConnecting">
              {{ connectBtnText }}
            </button>
          </div>
        </div>

        <!-- 通用设置 -->
        <div v-show="activeSection === 'general'" class="section">
          <h3 class="section-title">窗口设置</h3>
          <div class="setting-group">
            <label class="setting-label">
              不透明度 <span class="value">{{ opacity }}%</span>
            </label>
            <input v-model.number="opacity" type="range" min="0" max="100" class="setting-slider" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">窗口置顶</label>
            <input v-model="alwaysOnTop" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">隐藏窗口边框</label>
            <input v-model="hideBorder" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">失焦时隐藏标题栏</label>
            <input v-model="autoHideUi" type="checkbox" class="toggle-checkbox" />
          </div>
        </div>

        <!-- 语音播报 -->
        <div v-show="activeSection === 'speech'" class="section">
          <h3 class="section-title">语音播报</h3>

          <div
            class="speech-status-card"
            :class="{
              active: speechEnabled && speechStatus.available,
              suspended: speechStatus.danmaku_suspended,
              error: !speechStatus.available || !!speechStatus.error
            }"
          >
            <span class="speech-status-dot">●</span>
            <span>{{ speechStatusText }}</span>
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">启用语音播报</label>
            <input
              v-model="speechEnabled"
              type="checkbox"
              class="toggle-checkbox"
              :disabled="!speechStatus.available"
            />
          </div>

          <div class="setting-group">
            <label class="setting-label">语音</label>
            <div class="speech-voice-row">
              <select
                v-model="speechVoiceId"
                class="setting-select"
                :disabled="speechVoiceControlsDisabled"
              >
                <option value="">自动选择（优先中文）</option>
                <option v-for="voice in speechVoices" :key="voice.id" :value="voice.id">
                  {{ voice.name }}{{ voice.language ? `（${voice.language}）` : '' }}
                </option>
              </select>
              <button
                type="button"
                class="speech-preview-btn"
                :disabled="speechVoiceControlsDisabled || speechPreviewing"
                @click="handleSpeechPreview"
              >
                {{ speechPreviewing ? '试听中...' : '试听' }}
              </button>
            </div>
            <div v-if="speechLoadError" class="speech-error">{{ speechLoadError }}</div>
            <div v-else-if="speechVoices.length === 0 && !speechVoicesLoading" class="setting-hint">
              未找到系统语音，请先在 Windows 语言或语音设置中安装语音包。
            </div>
          </div>

          <div class="setting-group">
            <label class="setting-label">
              语速 <span class="value">{{ speechRateLabel }}</span>
            </label>
            <input
              v-model.number="speechRate"
              type="range"
              min="-10"
              max="10"
              step="1"
              class="setting-slider"
            />
            <div class="setting-hint">不同系统语音对语速的表现可能略有差异。</div>
          </div>

          <h3 class="section-title section-subtitle">播报内容</h3>
          <div class="setting-group toggle">
            <label class="setting-label">弹幕</label>
            <input v-model="speechSpeakDanmaku" type="checkbox" class="toggle-checkbox" />
          </div>
          <div class="setting-group toggle">
            <label class="setting-label">礼物</label>
            <input v-model="speechSpeakGift" type="checkbox" class="toggle-checkbox" />
          </div>
          <div class="setting-group toggle">
            <label class="setting-label">醒目留言（SC）</label>
            <input v-model="speechSpeakSuperChat" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="info-box">
            <span class="info-icon">🔊</span>
            <span class="info-text">
              弹幕过多或待播队列堆积时会自动暂停弹幕播报，礼物和 SC 不受影响；流量下降后自动恢复。
              礼物播报沿用“显示免费礼物”和“最低显示价格”设置，真实连击礼物只播报最终累计数量。
            </span>
          </div>
        </div>

        <!-- 字体设置 -->
        <div v-show="activeSection === 'font'" class="section">
          <h3 class="section-title">字体设置</h3>

          <div class="setting-group">
            <label class="setting-label">
              内容字体大小 <span class="value">{{ fontSize }}px</span>
            </label>
            <input v-model.number="fontSize" type="range" min="10" max="48" class="setting-slider" />
            <span class="setting-hint">弹幕、礼物、SC、观众和点播等内容区域</span>
          </div>

          <div class="setting-group">
            <label class="setting-label">
              UI 字体大小 <span class="value">{{ uiFontSize }}px</span>
            </label>
            <input v-model.number="uiFontSize" type="range" min="10" max="48" class="setting-slider" />
          </div>

          <div class="setting-group">
            <label class="setting-label">内容字体</label>
            <div class="font-family-grid">
              <button
                v-for="option in fontFamilyOptions"
                :key="option.value"
                type="button"
                class="font-family-option"
                :class="{ active: contentFontFamily === option.value }"
                :style="{ fontFamily: option.value }"
                @click="contentFontFamily = option.value"
              >
                <span class="font-option-name">{{ option.label }}</span>
                <span class="font-option-preview">弹幕 Aa 123</span>
              </button>
            </div>
          </div>

          <div class="setting-group">
            <label class="setting-label">内容字重</label>
            <div class="font-weight-options">
              <button
                v-for="option in fontWeightOptions"
                :key="option.value"
                type="button"
                class="font-weight-option"
                :class="{ active: contentFontWeight === option.value }"
                :style="{ fontWeight: option.value }"
                @click="contentFontWeight = option.value"
              >
                {{ option.label }}
              </button>
            </div>
          </div>

          <h3 class="section-title section-subtitle">颜色</h3>

          <div class="font-color-grid">
            <div
              v-for="item in fontColorSettings"
              :key="item.key"
              class="font-color-card"
            >
              <label class="setting-label">
                {{ item.label }}
                <span class="value">{{ item.value }}</span>
              </label>
              <div class="color-control">
                <div class="color-swatches">
                  <button
                    v-for="color in item.presets"
                    :key="`${item.key}-${color}`"
                    type="button"
                    class="color-swatch"
                    :class="{ active: item.value.toLowerCase() === color.toLowerCase() }"
                    :style="{ backgroundColor: color }"
                    :title="color"
                    @click="updateDisplayColor(item.key, color)"
                  />
                </div>
                <label class="custom-color-btn" :style="{ '--picked-color': item.value }">
                  <input
                    type="color"
                    :value="item.value"
                    @input="updateDisplayColor(item.key, getInputValue($event))"
                  />
                  <span class="custom-color-swatch" />
                  <span>自定义</span>
                </label>
              </div>
            </div>
          </div>
        </div>

        <!-- 弹幕设置 -->
        <div v-show="activeSection === 'danmaku'" class="section">
          <h3 class="section-title">弹幕设置</h3>

          <div class="setting-group toggle">
            <label class="setting-label">显示粉丝勋章</label>
            <input v-model="danmakuShowMedal" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">显示已熄灭的粉丝勋章</label>
            <input v-model="medalShowUnlit" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">显示其他直播间的粉丝勋章</label>
            <input v-model="medalShowOtherRoom" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">显示荣耀等级</label>
            <input v-model="danmakuShowWealthLevel" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">显示大航海标识</label>
            <input v-model="danmakuShowGuard" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">显示大航海左侧高亮边界</label>
            <input v-model="danmakuShowGuardBorder" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">显示房管标识</label>
            <input v-model="danmakuShowAdmin" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">显示弹幕时间</label>
            <input v-model="danmakuShowTime" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group">
            <label class="setting-label">
              表情大小 <span class="value">{{ danmakuEmoticonSize }}px</span>
            </label>
            <input v-model.number="danmakuEmoticonSize" type="range" min="18" max="96" step="2"
              class="setting-slider" />
          </div>
        </div>

        <!-- 礼物设置 -->
        <div v-show="activeSection === 'gift'" class="section">
          <h3 class="section-title">礼物设置</h3>

          <div class="setting-group toggle">
            <label class="setting-label">合并相同礼物</label>
            <input v-model="giftMergeDisplay" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">显示免费礼物</label>
            <input v-model="giftShowFree" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">显示粉丝牌勋章</label>
            <input v-model="giftShowMedal" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">SC与礼物合并展示</label>
            <input v-model="scMergeWithGift" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">显示礼物时间</label>
            <input v-model="giftShowTime" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group">
            <label class="setting-label">
              最低显示价格
              <span class="value">{{ giftMinPrice }} 电池 (¥{{ (giftMinPrice / 10).toFixed(1) }})</span>
            </label>
            <input v-model.number="giftMinPrice" type="range" min="0" max="1000" step="10" class="setting-slider" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">礼物过期灰显</label>
            <input v-model="giftExpireEnabled" type="checkbox" class="toggle-checkbox" />
          </div>

          <div v-if="giftExpireEnabled" class="setting-group">
            <label class="setting-label">
              过期时间
              <span class="value">{{ giftExpireMinutes }} 分钟</span>
            </label>
            <input v-model.number="giftExpireMinutes" type="range" min="1" max="30" step="1" class="setting-slider" />
          </div>
        </div>

        <!-- 观众设置 -->
        <div v-show="activeSection === 'audience'" class="section">
          <h3 class="section-title">观众设置</h3>

          <div class="setting-group toggle">
            <label class="setting-label">显示粉丝勋章</label>
            <input v-model="audienceShowMedal" type="checkbox" class="toggle-checkbox" />
          </div>

          <div class="setting-group toggle">
            <label class="setting-label">自动刷新贡献榜</label>
            <input v-model="audienceAutoRefreshEnabled" type="checkbox" class="toggle-checkbox" />
          </div>

          <div v-if="audienceAutoRefreshEnabled" class="setting-group">
            <label class="setting-label">
              刷新间隔
              <span class="value">{{ audienceAutoRefreshIntervalLabel }}</span>
            </label>
            <div class="range-input-row">
              <input
                v-model.number="audienceAutoRefreshIntervalSeconds"
                type="range"
                min="10"
                max="300"
                step="10"
                class="setting-slider"
              />
              <input
                v-model.number="audienceAutoRefreshIntervalSeconds"
                type="number"
                min="10"
                max="300"
                step="10"
                class="setting-number-input"
              />
              <span class="interval-unit">秒</span>
            </div>
            <div class="setting-hint">可设置 10 秒到 5 分钟。</div>
          </div>

          <h3 class="section-title" style="margin-top: 24px;">入场通知</h3>

          <div class="setting-group toggle">
            <label class="setting-label">启用进房面板</label>
            <input v-model="entryShowEnabled" type="checkbox" class="toggle-checkbox" />
          </div>

          <template v-if="entryShowEnabled">
            <div class="setting-group toggle">
              <label class="setting-label">在互动栏显示</label>
              <input v-model="entryPanelShowInInteraction" type="checkbox" class="toggle-checkbox" />
            </div>

            <div class="setting-group toggle">
              <label class="setting-label">在观众栏显示</label>
              <input v-model="entryPanelShowInAudience" type="checkbox" class="toggle-checkbox" />
            </div>

            <div class="setting-group toggle">
              <label class="setting-label">显示粉丝勋章</label>
              <input v-model="entryShowMedal" type="checkbox" class="toggle-checkbox" />
            </div>

            <div class="setting-group toggle">
              <label class="setting-label">显示舰队标识</label>
              <input v-model="entryShowGuard" type="checkbox" class="toggle-checkbox" />
            </div>

            <div class="setting-group">
              <label class="setting-label">显示范围</label>
              <div class="checkbox-group">
                <label class="checkbox-item">
                  <input type="checkbox" v-model="entryFilterAll" />
                  <span>全部</span>
                </label>
                <label class="checkbox-item" :class="{ disabled: entryFilterAll }">
                  <input type="checkbox" v-model="entryFilterCaptain" :disabled="entryFilterAll" />
                  <span>舰长</span>
                </label>
                <label class="checkbox-item" :class="{ disabled: entryFilterAll }">
                  <input type="checkbox" v-model="entryFilterAdmiral" :disabled="entryFilterAll" />
                  <span>提督</span>
                </label>
                <label class="checkbox-item" :class="{ disabled: entryFilterAll }">
                  <input type="checkbox" v-model="entryFilterGovernor" :disabled="entryFilterAll" />
                  <span>总督</span>
                </label>
                <label class="checkbox-item" :class="{ disabled: entryFilterAll }">
                  <input type="checkbox" v-model="entryFilterSpecialFollow" :disabled="entryFilterAll" />
                  <span>特别关注</span>
                </label>
              </div>
            </div>
          </template>
        </div>

        <!-- 特别关注 -->
        <div v-show="activeSection === 'special-follow'" class="section">
          <h3 class="section-title">特别关注</h3>

          <div class="info-box">
            <span class="info-icon">⭐</span>
            <span class="info-text">本地功能。<br>添加 UID 后，该用户的弹幕和礼物将以高亮显示。可以在弹幕列表中右键添加。</span>
          </div>

          <div class="setting-group">
            <label class="setting-label">添加 UID</label>
            <div class="special-follow-input-row">
              <input
                v-model="newSpecialUid"
                type="text"
                class="setting-input"
                placeholder="输入用户 UID"
                @keydown.enter="addSpecialUid"
              />
              <button class="special-follow-add-btn" @click="addSpecialUid">添加</button>
            </div>
          </div>

          <div class="setting-group">
            <label class="setting-label">
              已关注列表
              <span v-if="specialFollowNamesLoading" class="value">查询中...</span>
            </label>
            <div v-if="settingsStore.specialFollowUids.length === 0" class="special-follow-empty">
              暂无特别关注
            </div>
            <div v-else class="special-follow-list">
              <div
                v-for="uid in settingsStore.specialFollowUids"
                :key="uid"
                class="special-follow-item"
              >
                <span class="special-follow-uid">{{ getSpecialFollowLabel(uid) }}</span>
                <button class="special-follow-remove-btn" @click="removeSpecialUid(uid)">×</button>
              </div>
            </div>
          </div>
        </div>

        <!-- 弹幕过滤 -->
        <div v-show="activeSection === 'danmaku-filter'" class="section">
          <h3 class="section-title">弹幕过滤</h3>

          <div class="info-box">
            <span class="info-icon">🚫</span>
            <span class="info-text">
              本地功能，只在本机隐藏指定 UID 的弹幕，不会提交到直播间屏蔽词，也不需要房管权限。<br>
              添加 UID 后，该用户的弹幕不会出现在弹幕列表和互动列表中。
            </span>
          </div>

          <div class="setting-group">
            <label class="setting-label">添加 UID</label>
            <div class="special-follow-input-row">
              <input
                v-model="newDanmakuFilterUid"
                type="text"
                class="setting-input"
                placeholder="输入用户 UID"
                @keydown.enter="addDanmakuFilterUid"
              />
              <button class="special-follow-add-btn" @click="addDanmakuFilterUid">添加</button>
            </div>
          </div>

          <div class="setting-group">
            <label class="setting-label">
              已过滤列表
              <span v-if="danmakuFilterNamesLoading" class="value">查询中...</span>
            </label>
            <div v-if="settingsStore.danmakuFilterUids.length === 0" class="special-follow-empty">
              暂无弹幕过滤
            </div>
            <div v-else class="special-follow-list">
              <div
                v-for="uid in settingsStore.danmakuFilterUids"
                :key="uid"
                class="special-follow-item danmaku-filter-item"
              >
                <span class="special-follow-uid">{{ getDanmakuFilterLabel(uid) }}</span>
                <button class="special-follow-remove-btn" @click="removeDanmakuFilterUid(uid)">×</button>
              </div>
            </div>
          </div>
        </div>

        <!-- 屏蔽词 -->
        <div v-show="activeSection === 'shield-keyword'" class="section">
          <h3 class="section-title">
            屏蔽词
            <span v-if="shieldMaxLimit" class="shield-count">{{ shieldKeywords.length }} / {{ shieldMaxLimit }}</span>
          </h3>

          <div class="info-box">
            <span class="info-icon">❗</span>
            <span class="info-text">直播间屏蔽词列表，需房管权限设置。</span>
          </div>

          <div class="setting-group">
            <label class="setting-label">添加屏蔽词</label>
            <div class="special-follow-input-row">
              <input
                v-model="newShieldKeyword"
                type="text"
                class="setting-input"
                placeholder="输入关键词"
                :disabled="shieldAdding"
                @keydown.enter="addShieldKeyword"
              />
              <button class="special-follow-add-btn" @click="addShieldKeyword" :disabled="shieldAdding">
                {{ shieldAdding ? '添加中...' : '添加' }}
              </button>
              <button class="shield-refresh-btn-inline" @click="loadShieldKeywords" :disabled="shieldLoading">
                {{ shieldLoading ? '刷新中' : '刷新' }}
              </button>
            </div>
          </div>

          <div v-if="shieldError" class="shield-error">
            {{ shieldError }}
          </div>

          <div v-if="shieldLoading && shieldKeywords.length === 0" class="shield-loading">
            加载中...
          </div>

          <div v-else-if="shieldKeywords.length === 0 && !shieldError" class="special-follow-empty">
            暂无屏蔽词
          </div>

          <div v-else class="shield-keyword-list" :class="{ 'is-loading': shieldLoading }">
            <div
              v-for="item in shieldKeywords"
              :key="item.keyword"
              class="shield-keyword-tag"
              :title="`添加者: ${item.name}${item.is_anchor ? ' (主播)' : ''}`"
            >
              <span>{{ item.keyword }}</span>
              <button class="shield-keyword-del" @click="confirmDelKeyword(item.keyword)">×</button>
            </div>
          </div>
        </div>

        <!-- 关于 -->
        <div v-show="activeSection === 'about'" class="section">
          <h3 class="section-title">关于</h3>

          <div class="about-info-card">
            <div class="about-row">
              <span class="about-label">版本</span>
              <span class="about-value">v{{ appVersion }}</span>
              <span v-if="portableMode" class="about-badge">便携版</span>
            </div>
            <div class="about-row">
              <span class="about-label">项目地址</span>
              <a class="about-link" @click.prevent="openProjectUrl">
                github.com/Akegarasu/danmuji-next
              </a>
            </div>
          </div>

          <div class="about-update-section">
            <button
              class="about-check-btn"
              :disabled="updateCheckStatus === 'checking'"
              @click="manualCheckUpdate"
            >
              <template v-if="updateCheckStatus === 'checking'">检查中...</template>
              <template v-else-if="updateCheckStatus === 'latest'">已是最新版本</template>
              <template v-else-if="updateCheckStatus === 'error'">检查失败</template>
              <template v-else>检查更新</template>
            </button>
            <div v-if="updateCheckStatus === 'found' && aboutUpdateInfo" class="about-update-found">
              <span>发现新版本 v{{ aboutUpdateInfo.version }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div class="settings-footer">
      <span class="hint">右键 Tab 可以拆分窗口哦！</span>
      <button class="save-btn" :class="{ saving: saveStatus === 'saving', saved: saveStatus === 'saved' }"
        :disabled="saveStatus === 'saving' || isSaving" @click="handleSave">
        {{ saveButtonText }}
      </button>
    </div>

    <UpdateToast
      v-if="showUpdateToast && aboutUpdateInfo"
      :update-info="aboutUpdateInfo"
      :pause-auto-close="showAlertDialog"
      @close="showUpdateToast = false"
      @show-changelog="showUpdateChangelog"
    />

    <!-- 登录对话框 -->
    <LoginDialog v-model:visible="showLoginDialog" @login-success="handleLoginSuccess" />

    <!-- 确认对话框 -->
    <ConfirmDialog
      v-model:visible="showAlertDialog"
      title="提示"
      :message="alertMessage"
      confirm-text="知道了"
      :show-cancel="false"
    />
    <ConfirmDialog
      v-model:visible="showLogoutConfirm"
      title="退出登录"
      message="确定要退出登录吗？"
      confirm-text="退出"
      :danger="true"
      @confirm="doLogout"
    />
    <ConfirmDialog
      v-model:visible="showShieldDelConfirm"
      title="删除屏蔽词"
      :message="`确定要删除屏蔽词「${shieldDeleting}」吗？`"
      confirm-text="删除"
      :danger="true"
      @confirm="doDelShieldKeyword"
      @cancel="cancelDel"
    />
  </div>
</template>

<style scoped lang="scss">
.settings-window {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  background: var(--bg-primary);
  border-radius: var(--border-radius);
  border: 1px solid var(--border-color);
  overflow: hidden;
}

.settings-layout {
  flex: 1;
  display: flex;
  overflow: hidden;
}

.settings-nav {
  width: 100px;
  background: var(--bg-secondary);
  border-right: 1px solid var(--border-color);
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  overflow-y: auto;
}

.nav-item {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 12px 8px;
  background: transparent;
  border: none;
  color: var(--text-secondary);
  font-size: var(--font-size-xs);
  cursor: pointer;
  border-radius: var(--border-radius-sm);
  transition: background 0.15s, color 0.15s;

  &:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  &.active {
    background: var(--bg-active);
    color: var(--text-primary);
  }
}

.settings-content {
  flex: 1;
  padding: 16px;
  overflow-y: auto;
}

.section {
  animation: fadeIn 0.2s ease;
}

.section-title {
  font-size: var(--font-size-base);
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 16px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border-color);
}

.info-box {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 10px 12px;
  background: var(--bg-card);
  border-left: 3px solid var(--accent-primary);
  border-radius: var(--border-radius-sm);
  margin-bottom: 16px;

  .info-icon {
    font-size: 16px;
  }

  .info-text {
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    line-height: 1.5;
  }
}

.speech-status-card {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  margin-bottom: 16px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-secondary);
  font-size: var(--font-size-xs);

  .speech-status-dot {
    color: var(--text-muted);
    font-size: 9px;
  }

  &.active {
    border-color: rgba(34, 197, 94, 0.35);
    background: rgba(34, 197, 94, 0.08);

    .speech-status-dot {
      color: #22c55e;
    }
  }

  &.suspended {
    border-color: rgba(245, 200, 66, 0.35);
    background: rgba(245, 200, 66, 0.08);

    .speech-status-dot {
      color: var(--accent-gold);
    }
  }

  &.error {
    border-color: rgba(239, 68, 68, 0.3);
    background: rgba(239, 68, 68, 0.08);

    .speech-status-dot {
      color: #ef4444;
    }
  }
}

.speech-voice-row {
  display: flex;
  align-items: center;
  gap: 8px;

  .setting-select {
    flex: 1;
  }
}

.speech-preview-btn {
  flex-shrink: 0;
  min-width: 68px;
  padding: 8px 12px;
  background: var(--bg-active);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-primary);
  font-size: var(--font-size-sm);
  cursor: pointer;

  &:hover:not(:disabled) {
    background: var(--bg-hover);
  }

  &:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
}

.speech-error {
  margin-top: 6px;
  color: #ef4444;
  font-size: var(--font-size-xs);
}

.setting-group {
  margin-bottom: 16px;

  &.toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
}

.setting-label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 6px;
  font-size: var(--font-size-sm);
  color: var(--text-secondary);

  .value {
    color: var(--accent-primary);
    font-weight: 500;
  }
}

.setting-input {
  width: 100%;
  padding: 8px 12px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-primary);
  font-size: var(--font-size-sm);
  outline: none;
  transition: border-color 0.2s;

  &:focus {
    border-color: var(--accent-primary);
  }

  &::placeholder {
    color: var(--text-muted);
  }
}

.setting-textarea {
  width: 100%;
  padding: 8px 12px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-primary);
  font-size: var(--font-size-xs);
  font-family: monospace;
  outline: none;
  resize: vertical;
  min-height: 60px;
  transition: border-color 0.2s;

  &:focus {
    border-color: var(--accent-primary);
  }

  &::placeholder {
    color: var(--text-muted);
  }
}

.setting-hint {
  margin-top: 6px;
  font-size: var(--font-size-xs);
  color: var(--text-muted);
}

.section-subtitle {
  margin-top: 24px;
}

.connection-card {
  padding: 12px 16px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  margin-bottom: 16px;

  .connection-status {
    display: flex;
    align-items: center;
    gap: 8px;

    .status-dot {
      font-size: 10px;
      color: var(--text-muted);
      animation: pulse 2s infinite;
    }

    .status-text {
      font-size: var(--font-size-sm);
      color: var(--text-secondary);
      font-weight: 500;
    }

    .room-title {
      font-size: var(--font-size-xs);
      color: var(--text-muted);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .error-message {
    margin-top: 8px;
    font-size: var(--font-size-xs);
    color: #ef4444;
  }

  &.connected {
    background: rgba(34, 197, 94, 0.1);
    border-color: rgba(34, 197, 94, 0.3);

    .status-dot {
      color: #22c55e;
    }

    .status-text {
      color: #22c55e;
    }
  }

  &.error {
    background: rgba(239, 68, 68, 0.1);
    border-color: rgba(239, 68, 68, 0.3);

    .status-dot {
      color: #ef4444;
      animation: none;
    }

    .status-text {
      color: #ef4444;
    }
  }
}

.connect-btn {
  width: 100%;
  padding: 10px 16px;
  font-size: var(--font-size-sm);
  font-weight: 500;
  background: var(--accent-primary);
  border: none;
  border-radius: var(--border-radius-sm);
  color: white;
  cursor: pointer;
  transition: all 0.15s;

  &:hover:not(:disabled) {
    opacity: 0.9;
  }

  &:disabled {
    cursor: not-allowed;
    opacity: 0.7;
  }

  &.connecting {
    background: var(--text-muted);
  }

  &.connected,
  &.reconnecting {
    background: rgba(239, 68, 68, 0.8);

    &:hover:not(:disabled) {
      background: #ef4444;
    }
  }

  &.error {
    background: var(--accent-primary);
  }
}

// 用户卡片样式
.user-card {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);

  .user-avatar {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .user-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;

    .user-name {
      font-size: var(--font-size-sm);
      font-weight: 500;
      color: var(--text-primary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .user-uid {
      font-size: var(--font-size-xs);
      color: var(--text-muted);
    }
  }

  .logout-btn {
    padding: 6px 12px;
    background: transparent;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-sm);
    color: var(--text-secondary);
    font-size: var(--font-size-xs);
    cursor: pointer;
    transition: all 0.15s;
    flex-shrink: 0;

    &:hover:not(:disabled) {
      background: rgba(239, 68, 68, 0.1);
      border-color: rgba(239, 68, 68, 0.3);
      color: #ef4444;
    }

    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }
}

.login-prompt {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);

  .login-info {
    display: flex;
    align-items: center;
    gap: 8px;

    .login-icon {
      font-size: 18px;
    }

    .login-text {
      font-size: var(--font-size-sm);
      color: var(--text-secondary);
    }
  }

  .login-btn {
    padding: 8px 16px;
    background: var(--accent-primary);
    border: none;
    border-radius: var(--border-radius-sm);
    color: white;
    font-size: var(--font-size-sm);
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;

    &:hover:not(:disabled) {
      opacity: 0.9;
    }

    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }
}

.setting-select {
  width: 100%;
  padding: 8px 12px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-primary);
  font-size: var(--font-size-sm);
  outline: none;
  cursor: pointer;

  &:focus {
    border-color: var(--accent-primary);
  }

  option {
    background: var(--bg-secondary);
    color: var(--text-primary);
  }
}

.font-family-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  gap: 8px;
}

.font-family-option {
  min-height: 58px;
  padding: 10px 12px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-secondary);
  cursor: pointer;
  text-align: left;
  transition: background 0.15s, border-color 0.15s, color 0.15s;

  &:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  &.active {
    border-color: var(--accent-primary);
    color: var(--text-primary);
    background: rgba(92, 158, 255, 0.12);
  }
}

.font-option-name,
.font-option-preview {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.font-option-name {
  font-size: var(--font-size-sm);
  font-weight: 600;
}

.font-option-preview {
  margin-top: 4px;
  font-size: var(--font-size-xs);
  color: var(--text-muted);
}

.font-weight-options {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(92px, 1fr));
  gap: 8px;
}

.font-weight-option {
  height: 34px;
  padding: 0 10px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-secondary);
  font-size: var(--font-size-xs);
  cursor: pointer;
  transition: background 0.15s, border-color 0.15s, color 0.15s;

  &:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  &.active {
    border-color: var(--accent-primary);
    color: var(--text-primary);
    background: rgba(92, 158, 255, 0.12);
  }
}

.font-color-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(230px, 1fr));
  gap: 12px;
}

.font-color-card {
  padding: 12px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
}

.color-control {
  display: flex;
  align-items: center;
  gap: 10px;
}

.color-swatches {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  flex: 1;
  min-width: 0;
}

.color-swatch {
  width: 22px;
  height: 22px;
  border: 1px solid rgba(255, 255, 255, 0.18);
  border-radius: 50%;
  cursor: pointer;
  box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.2);
  transition: transform 0.15s, box-shadow 0.15s, border-color 0.15s;

  &:hover,
  &.active {
    transform: translateY(-1px);
    border-color: var(--accent-primary);
    box-shadow: 0 0 0 2px rgba(92, 158, 255, 0.25), inset 0 0 0 1px rgba(0, 0, 0, 0.25);
  }
}

.custom-color-btn {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 10px;
  background: var(--bg-active);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-secondary);
  font-size: var(--font-size-xs);
  cursor: pointer;
  overflow: hidden;
  flex-shrink: 0;
  transition: background 0.15s, color 0.15s, border-color 0.15s;

  &:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--accent-primary);
  }

  input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }
}

.custom-color-swatch {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--picked-color);
  border: 1px solid rgba(255, 255, 255, 0.25);
  box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.2);
}

.setting-slider {
  width: 100%;
  height: 4px;
  appearance: none;
  background: var(--bg-card);
  border-radius: 2px;
  outline: none;

  &::-webkit-slider-thumb {
    appearance: none;
    width: 14px;
    height: 14px;
    background: var(--accent-primary);
    border-radius: 50%;
    cursor: pointer;
  }
}

.range-input-row {
  display: flex;
  align-items: center;
  gap: 8px;

  .setting-slider {
    flex: 1;
  }
}

.setting-number-input {
  width: 64px;
  padding: 6px 8px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-primary);
  font-size: var(--font-size-sm);
  outline: none;

  &:focus {
    border-color: var(--accent-primary);
  }
}

.interval-unit {
  font-size: var(--font-size-xs);
  color: var(--text-muted);
  flex-shrink: 0;
}

.checkbox-group {
  display: flex;
  flex-wrap: wrap;
  gap: 10px 16px;
}

.checkbox-item {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--font-size-sm);
  color: var(--text-secondary);
  cursor: pointer;

  input[type="checkbox"] {
    accent-color: var(--accent-primary);
    cursor: pointer;
  }

  &.disabled {
    opacity: 0.4;
    cursor: not-allowed;

    input { cursor: not-allowed; }
  }
}

.toggle-checkbox {
  appearance: none;
  width: 40px;
  height: 20px;
  background: var(--bg-card);
  border-radius: 10px;
  position: relative;
  cursor: pointer;
  transition: background 0.2s;
  flex-shrink: 0;

  &::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    background: var(--text-muted);
    border-radius: 50%;
    transition: all 0.2s;
  }

  &:checked {
    background: var(--accent-primary);

    &::after {
      left: 22px;
      background: white;
    }
  }
}

.settings-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 16px;
  border-top: 1px solid var(--border-color);
  background: var(--bg-secondary);

  .hint {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
}

.save-btn {
  padding: 8px 20px;
  background: var(--bg-active);
  color: var(--text-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  font-size: var(--font-size-sm);
  font-weight: 500;
  cursor: pointer;
  transition: background 0.15s;

  &:hover:not(:disabled) {
    background: var(--bg-hover);
  }

  &:disabled {
    opacity: 0.7;
    cursor: not-allowed;
  }

  &.saving {
    background: var(--text-muted);
  }

  &.saved {
    background: #22c55e;
    border-color: #22c55e;
    color: white;
  }
}

// ==================== 特别关注 ====================

.special-follow-input-row {
  display: flex;
  gap: 8px;

  .setting-input {
    flex: 1;
    margin-bottom: 0;
  }
}

.special-follow-add-btn {
  padding: 8px 16px;
  background: var(--accent-primary);
  border: none;
  border-radius: var(--border-radius-sm);
  color: white;
  font-size: var(--font-size-sm);
  font-weight: 500;
  cursor: pointer;
  flex-shrink: 0;
  transition: opacity 0.15s;

  &:hover {
    opacity: 0.9;
  }
}

.special-follow-empty {
  padding: 16px;
  text-align: center;
  color: var(--text-muted);
  font-size: var(--font-size-sm);
  background: var(--bg-card);
  border-radius: var(--border-radius-sm);
}

.special-follow-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.special-follow-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-left: 3px solid #f5c842;
  border-radius: var(--border-radius-sm);

  &.danmaku-filter-item {
    border-left-color: #ef4444;
  }

  .special-follow-uid {
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .special-follow-remove-btn {
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 16px;
    cursor: pointer;
    border-radius: var(--border-radius-sm);
    transition: background 0.15s, color 0.15s;

    &:hover {
      background: rgba(239, 68, 68, 0.15);
      color: #ef4444;
    }
  }
}

@keyframes fadeIn {
  from {
    opacity: 0;
  }

  to {
    opacity: 1;
  }
}

// ==================== 屏蔽词 ====================

.shield-count {
  font-size: var(--font-size-xs);
  color: var(--text-muted);
  font-weight: 400;
  margin-left: 8px;
}

.shield-error {
  padding: 10px 12px;
  margin-bottom: 12px;
  background: rgba(239, 68, 68, 0.1);
  border: 1px solid rgba(239, 68, 68, 0.3);
  border-radius: var(--border-radius-sm);
  color: #ef4444;
  font-size: var(--font-size-sm);
}

.shield-loading {
  padding: 16px;
  text-align: center;
  color: var(--text-muted);
  font-size: var(--font-size-sm);
}

.shield-keyword-list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 12px;
  max-height: 300px;
  overflow-y: auto;
  width: 100%;
  transition: opacity 0.2s;

  &.is-loading {
    opacity: 0.5;
    pointer-events: none;
  }
}

.shield-keyword-tag {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 6px 4px 10px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  font-size: var(--font-size-xs);
  color: var(--text-primary);
  cursor: default;
  transition: background 0.15s, border-color 0.15s;

  &:hover {
    background: var(--bg-hover);
    border-color: var(--accent-primary);
  }

  .shield-keyword-del {
    width: 16px;
    height: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 12px;
    cursor: pointer;
    border-radius: 50%;
    padding: 0;
    line-height: 1;
    transition: background 0.15s, color 0.15s;

    &:hover {
      background: rgba(239, 68, 68, 0.15);
      color: #ef4444;
    }
  }
}

.shield-refresh-btn-inline {
  padding: 8px 12px;
  background: var(--bg-active);
  color: var(--text-secondary);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  font-size: var(--font-size-sm);
  cursor: pointer;
  flex-shrink: 0;
  transition: background 0.15s, color 0.15s;

  &:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  &:disabled {
    opacity: 0.7;
    cursor: not-allowed;
  }
}

// ==================== 关于 ====================

.about-info-card {
  padding: 14px 16px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  margin-bottom: 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.about-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.about-label {
  font-size: var(--font-size-sm);
  color: var(--text-muted);
  flex-shrink: 0;
  min-width: 56px;
}

.about-value {
  font-size: var(--font-size-sm);
  color: var(--text-primary);
  font-weight: 500;
}

.about-badge {
  font-size: var(--font-size-xs);
  color: var(--text-muted);
  padding: 1px 6px;
  background: var(--bg-hover);
  border-radius: 8px;
}

.about-link {
  font-size: var(--font-size-sm);
  color: var(--accent-primary);
  cursor: pointer;
  text-decoration: none;

  &:hover {
    text-decoration: underline;
  }
}

.about-update-section {
  display: flex;
  align-items: center;
  gap: 12px;
}

.about-check-btn {
  padding: 8px 20px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-secondary);
  font-size: var(--font-size-sm);
  cursor: pointer;
  transition: background 0.15s, color 0.15s;

  &:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  &:disabled {
    opacity: 0.7;
    cursor: not-allowed;
  }
}

.about-update-found {
  font-size: var(--font-size-sm);
  color: var(--accent-primary);
  font-weight: 500;
}
</style>

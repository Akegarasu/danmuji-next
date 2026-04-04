/**
 * Bilibili 弹幕客户端服务
 * 封装与后端的通信，处理数据更新和事件订阅
 */

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useDanmakuStore } from '@/stores/danmaku'
import { useSettingsStore } from '@/stores/settings'
import { useVideoRequestStore } from '@/stores/video-request'
import type { DataUpdate, DataSnapshot, EventType } from '@/types'

// ==================== 后端类型定义 ====================

/** 连接状态 */
export type ConnectionStatus =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'reconnecting'
  | { error: { message: string } }

/** 房间信息 */
export interface RoomInfoResponse {
  room_id: number
  short_id: number
  uid: number
  title: string
  live_status: number
}

/** 连接结果 */
export interface ConnectResult {
  success: boolean
  message: string
  room_info: RoomInfoResponse | null
}

/** 禁言响应 */
export interface SilentUserResponse {
  success: boolean
  code: number
  message: string
}

/** 禁言时长选项 */
export type SilentDuration = 'scene' | '2h' | '4h' | '24h' | '7d' | 'forever'

// ==================== 客户端状态 ====================

let statusUnlisten: UnlistenFn | null = null
let dataUnlisten: UnlistenFn | null = null
let currentWindowLabel: string | null = null

// ==================== 连接相关 API ====================

/** 连接到直播间 */
export async function connectRoom(roomId: number, cookie?: string): Promise<ConnectResult> {
  return await invoke<ConnectResult>('connect_room', {
    roomId,
    cookie: cookie || null
  })
}

/** 断开连接 */
export async function disconnectRoom(): Promise<void> {
  await invoke('disconnect_room')
}

/** 获取连接状态 */
export async function getConnectionStatus(): Promise<ConnectionStatus> {
  return await invoke<ConnectionStatus>('get_connection_status')
}

/** 获取当前房间信息 */
export async function getCurrentRoomInfo(): Promise<RoomInfoResponse | null> {
  return await invoke<RoomInfoResponse | null>('get_current_room_info')
}

// ==================== 事件订阅 API ====================

/** 订阅事件 */
export async function subscribeEvents(windowLabel: string, eventTypes: EventType[]): Promise<void> {
  await invoke('subscribe_events', { windowLabel, eventTypes })
}

/** 取消订阅 */
export async function unsubscribeEvents(windowLabel: string): Promise<void> {
  await invoke('unsubscribe_events', { windowLabel })
}

/** 获取数据快照 */
export async function getDataSnapshot(eventTypes: EventType[]): Promise<DataSnapshot> {
  return await invoke<DataSnapshot>('get_data_snapshot', { eventTypes })
}

/** 刷新贡献排行榜 */
export async function refreshContributionRank(cookie: string): Promise<void> {
  await invoke('refresh_contribution_rank', { cookie })
}

// ==================== 直播间管理 API ====================

/** 时长到 B站 API 参数的映射 */
function mapSilentDuration(duration: SilentDuration): { type: number; hour: number } {
  switch (duration) {
    case 'scene':   return { type: 2,  hour: 0 }        // 仅本场
    case '2h':      return { type: 1,  hour: 2 }
    case '4h':      return { type: 1,  hour: 4 }
    case '24h':     return { type: 1,  hour: 24 }
    case '7d':      return { type: 1,  hour: 24 * 7 }
    case 'forever': return { type: 1, hour: -1 }        // 永久
  }
}

/** 禁言用户 */
export async function addSilentUser(params: {
  roomId: number
  tuid: number
  cookie: string
  duration: SilentDuration
  msg?: string
}): Promise<SilentUserResponse> {
  const { type, hour } = mapSilentDuration(params.duration)

  return await invoke<SilentUserResponse>('add_silent_user', {
    roomId: params.roomId,
    tuid: params.tuid,
    cookie: params.cookie,
    type,
    hour,
    msg: params.msg || null
  })
}

// ==================== 事件监听 ====================

/** 所有支持的事件类型 */
const ALL_EVENT_TYPES: EventType[] = [
  'danmaku',
  'gift',
  'super_chat',
  'contribution_rank',
  'stats',
  'live_status'
]

/**
 * 初始化 Blive 客户端
 * @param eventTypes 要订阅的事件类型（可选，默认订阅所有）
 */
export async function initBliveClient(eventTypes?: EventType[]): Promise<void> {
  const danmakuStore = useDanmakuStore()
  const appWindow = getCurrentWindow()
  currentWindowLabel = appWindow.label

  // 确定要订阅的事件类型（未指定则订阅所有）
  const typesToSubscribe = eventTypes && eventTypes.length > 0 ? eventTypes : ALL_EVENT_TYPES

  // 始终订阅事件，确保后端使用 emit_to 发送给特定窗口
  await subscribeEvents(currentWindowLabel, typesToSubscribe)

  // 获取初始快照
  const snapshot = await getDataSnapshot(typesToSubscribe)
  applySnapshot(snapshot, danmakuStore)

  // 监听连接状态变化（全局广播，所有窗口都需要）
  statusUnlisten = await listen<ConnectionStatus>('blive-status', (event) => {
    if (event.payload === 'connected') {
      danmakuStore.setConnected(true)
    } else if (event.payload === 'disconnected') {
      danmakuStore.setConnected(false)
    } else if (typeof event.payload === 'object' && 'error' in event.payload) {
      danmakuStore.setConnected(false)
      console.error('[BliveClient] Connection error:', event.payload.error.message)
    }

    console.log('[BliveClient] Status changed:', event.payload)
  })

  // 监听数据更新（使用带窗口标签的事件名，确保只接收发给当前窗口的事件）
  const dataEventName = `blive-data:${currentWindowLabel}`
  dataUnlisten = await listen<DataUpdate[]>(dataEventName, (event) => {
    const updates = event.payload

    for (const update of updates) {
      processDataUpdate(update, danmakuStore)
    }
  })

  console.log(`[BliveClient] Initialized for window ${currentWindowLabel}`,
    eventTypes ? `with events: ${eventTypes.join(', ')}` : '(all events)')
}

/** 应用数据快照 */
function applySnapshot(snapshot: DataSnapshot, store: ReturnType<typeof useDanmakuStore>) {
  if (snapshot.danmaku_list) {
    store.setDanmakuList(snapshot.danmaku_list)
  }
  if (snapshot.gift_list) {
    store.setGiftList(snapshot.gift_list)
  }
  if (snapshot.superchat_list) {
    store.setSuperChatList(snapshot.superchat_list)
  }
  if (snapshot.contribution_rank_live) {
    store.updateContributionRankLive(snapshot.contribution_rank_live)
  }
  if (snapshot.contribution_rank_full) {
    store.updateContributionRankFull(snapshot.contribution_rank_full)
  }
  if (snapshot.contributions) {
    store.updateContributions(snapshot.contributions)
  }
  if (snapshot.stats) {
    store.updateStats(snapshot.stats)
  }
  if (snapshot.video_requests) {
    const videoStore = useVideoRequestStore()
    videoStore.syncRequests(snapshot.video_requests)
  }

  console.log('[BliveClient] Applied snapshot')
}

/** 处理数据更新 */
function processDataUpdate(update: DataUpdate, store: ReturnType<typeof useDanmakuStore>) {
  switch (update.type) {
    case 'DanmakuAppend':
      store.appendDanmaku(update.data)
      break

    case 'GiftUpsert':
      store.upsertGifts(update.data)
      break

    case 'SuperChatAppend':
      store.appendSuperChat(update.data)
      break

    case 'ContributionRankLive':
      store.updateContributionRankLive(update.data)
      break

    case 'ContributionRankFull':
      store.updateContributionRankFull(update.data)
      break

    case 'StatsUpdate':
      store.updateStats(update.data)
      break

    case 'ContributionsUpdate':
      store.updateContributions(update.data)
      break

    case 'LiveStart':
      console.log('[BliveClient] Live started')
      store.updateRoomInfo({ liveStatus: 1 })
      break

    case 'LiveStop':
      console.log('[BliveClient] Live stopped')
      store.updateRoomInfo({ liveStatus: 0 })
      break

    case 'VideoRequestAppend': {
      const videoStore = useVideoRequestStore()
      videoStore.appendRequest(update.data)
      break
    }

    case 'VideoRequestUpdate': {
      const videoStore = useVideoRequestStore()
      videoStore.updateRequest(update.data)
      break
    }

    case 'VideoRequestSync': {
      const videoStore = useVideoRequestStore()
      videoStore.syncRequests(update.data)
      break
    }
  }
}

/** 清理事件监听 */
export async function cleanupBliveClient(): Promise<void> {
  // 取消订阅
  if (currentWindowLabel) {
    try {
      await unsubscribeEvents(currentWindowLabel)
    } catch (e) {
      console.error('[BliveClient] Failed to unsubscribe:', e)
    }
  }

  if (statusUnlisten) {
    statusUnlisten()
    statusUnlisten = null
  }
  if (dataUnlisten) {
    dataUnlisten()
    dataUnlisten = null
  }

  currentWindowLabel = null

  console.log('[BliveClient] Cleaned up')
}

/** 自动连接（如果有保存的房间号和 Cookie） */
export async function autoConnect(): Promise<void> {
  const settingsStore = useSettingsStore()
  await settingsStore.loadSettings()

  const roomId = settingsStore.settings.roomId
  const cookie = settingsStore.settings.cookie

  if (!roomId || !cookie) {
    console.log('[BliveClient] Auto connect skipped: missing roomId or cookie')
    return
  }

  const roomIdNum = parseInt(roomId, 10)
  if (isNaN(roomIdNum) || roomIdNum <= 0) {
    console.log('[BliveClient] Auto connect skipped: invalid roomId')
    return
  }

  console.log('[BliveClient] Auto connecting to room:', roomIdNum)
  const result = await connectRoom(roomIdNum, cookie)

  if (result.success && result.room_info) {
    const danmakuStore = useDanmakuStore()
    danmakuStore.updateRoomInfo({
      roomId: result.room_info.room_id.toString(),
      title: result.room_info.title,
      liveStatus: result.room_info.live_status
    })
  } else {
    console.error('[BliveClient] Auto connect failed:', result.message)
  }
}

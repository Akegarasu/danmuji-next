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
import { useVotingStore } from '@/stores/voting'
import type {
  ContributionRankResponse,
  ContributionRankType,
  DataUpdate,
  DataSnapshot,
  EventType,
  GiftEffectConfig,
  GiftEffectResource,
  GuardTopListResponse
} from '@/types'
import { createLogger } from '@/services/logger'

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
const logger = createLogger('BliveClient')

/** 礼物全屏特效配置缓存，key 为房间号。 */
const giftEffectConfigCache = new Map<number, GiftEffectConfig>()
const giftEffectMapCache = new WeakMap<GiftEffectConfig, Map<number, GiftEffectResource>>()

// ==================== 礼物特效 API ====================

/**
 * 从 Bilibili 官方接口获取房间礼物全屏特效配置。
 *
 * `effect_id`（礼物目录）通过配置项的 `id` 与 `bind_gift_ids` 建立映射；
 * `web_mp4`/`web_mp4_json` 是网页端可直接使用的资源地址。
 */
export async function getGiftEffectConfig(
  roomId: number,
  options: {
    areaParentId?: number
    areaId?: number
    baseVersion?: number
    force?: boolean
  } = {}
): Promise<GiftEffectConfig> {
  if (!Number.isInteger(roomId) || roomId <= 0) {
    throw new Error('无效的房间号')
  }

  if (!options.force && options.baseVersion === undefined) {
    const cached = giftEffectConfigCache.get(roomId)
    if (cached) return cached
  }

  const config = await invoke<GiftEffectConfig>('get_gift_effect_config', {
    roomId,
    areaParentId: options.areaParentId ?? null,
    areaId: options.areaId ?? null,
    baseVersion: options.baseVersion ?? null
  })
  giftEffectConfigCache.set(roomId, config)
  giftEffectMapCache.set(config, buildGiftEffectMap(config))
  return config
}

function buildGiftEffectMap(config: GiftEffectConfig): Map<number, GiftEffectResource> {
  const map = new Map<number, GiftEffectResource>()
  for (const effect of config.full_sc_resource.conf_list) {
    for (const giftId of effect.bind_gift_ids) {
      if (giftId !== 0 && !map.has(giftId)) {
        map.set(giftId, effect)
      }
    }
  }
  return map
}

/** 获取配置对应的礼物 ID → 特效资源映射。 */
export function getGiftEffectMap(
  config: GiftEffectConfig
): ReadonlyMap<number, GiftEffectResource> {
  let map = giftEffectMapCache.get(config)
  if (!map) {
    map = buildGiftEffectMap(config)
    giftEffectMapCache.set(config, map)
  }
  return map
}

/** 按礼物 ID 查找绑定的全屏特效。 */
export function findGiftEffect(
  config: GiftEffectConfig,
  giftId: number
): GiftEffectResource | undefined {
  return getGiftEffectMap(config).get(giftId)
}

/** 清理礼物特效配置缓存。 */
export function clearGiftEffectConfigCache(): void {
  giftEffectConfigCache.clear()
  // WeakMap 不需要手动清理其配置对象对应的映射。
}


/** 连接到直播间 */
export async function connectRoom(roomId: number, cookie?: string): Promise<ConnectResult> {
  const result = await invoke<ConnectResult>('connect_room', {
    roomId,
    cookie: cookie || null
  })

  // 连接成功后预取官方礼物特效配置；失败不影响弹幕连接。
  if (result.success && result.room_info) {
    void getGiftEffectConfig(result.room_info.room_id).catch((error) => {
      logger.warn('Failed to load Bilibili gift effect config:', error)
    })
  }

  return result
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

function applyRoomInfo(
  store: ReturnType<typeof useDanmakuStore>,
  roomInfo: RoomInfoResponse
): void {
  store.updateRoomInfo({
    roomId: roomInfo.room_id.toString(),
    title: roomInfo.title,
    liveStatus: roomInfo.live_status,
    streamerUid: roomInfo.uid
  })
}

async function syncCurrentRoomInfo(store: ReturnType<typeof useDanmakuStore>): Promise<void> {
  try {
    const roomInfo = await getCurrentRoomInfo()
    if (roomInfo) applyRoomInfo(store, roomInfo)
  } catch (e) {
    logger.warn('Failed to sync room info:', e)
  }
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

/** 刷新指定类型的贡献排行榜 */
export async function refreshContributionRank(
  cookie: string,
  rankType: ContributionRankType = 'online'
): Promise<ContributionRankResponse> {
  return await invoke<ContributionRankResponse>('refresh_contribution_rank', { cookie, rankType })
}

/** 刷新大航海榜 */
export async function refreshGuardTopList(cookie: string): Promise<GuardTopListResponse> {
  return await invoke<GuardTopListResponse>('refresh_guard_top_list', { cookie })
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
  'live_status',
  'interact_word'
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

  // 新窗口可能在直播间已连接后才打开，需要主动同步一次当前连接状态
  try {
    const status = await getConnectionStatus()
    if (status === 'connected' || status === 'reconnecting') {
      danmakuStore.setConnected(true)
      await syncCurrentRoomInfo(danmakuStore)
    } else if (status === 'disconnected' || typeof status === 'object') {
      danmakuStore.setConnected(false)
    }
    logger.debug('Initial status synced:', status)
  } catch (e) {
    logger.warn('Failed to sync initial status:', e)
  }

  // 监听连接状态变化（全局广播，所有窗口都需要）
  statusUnlisten = await listen<ConnectionStatus>('blive-status', (event) => {
    if (event.payload === 'connected') {
      danmakuStore.setConnected(true)
      void syncCurrentRoomInfo(danmakuStore)
    } else if (event.payload === 'disconnected') {
      danmakuStore.setConnected(false)
    } else if (typeof event.payload === 'object' && 'error' in event.payload) {
      danmakuStore.setConnected(false)
      logger.error('Connection error:', event.payload.error.message)
    }

    logger.debug('Status changed:', event.payload)
  })

  // 监听数据更新（使用带窗口标签的事件名，确保只接收发给当前窗口的事件）
  const dataEventName = `blive-data:${currentWindowLabel}`
  dataUnlisten = await listen<DataUpdate[]>(dataEventName, (event) => {
    const updates = event.payload
    const contributionRankFull = updates.find((update) => update.type === 'ContributionRankFull')
    if (contributionRankFull?.type === 'ContributionRankFull') {
      logger.debug('ContributionRankFull received:', {
        windowLabel: currentWindowLabel,
        count: contributionRankFull.data.length,
        topUid: contributionRankFull.data[0]?.uid ?? null
      })
    }

    for (const update of updates) {
      processDataUpdate(update, danmakuStore)
    }
  })

  logger.debug(`Initialized for window ${currentWindowLabel}`,
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
  if (snapshot.voting_polls) {
    const votingStore = useVotingStore()
    votingStore.syncPolls(snapshot.voting_polls)
  }
  if (snapshot.interact_word_list) {
    store.setInteractWordList(snapshot.interact_word_list)
  }

  logger.debug('Applied snapshot')
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
      logger.debug('Live started')
      store.updateRoomInfo({ liveStatus: 1 })
      break

    case 'LiveStop':
      logger.debug('Live stopped')
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

    case 'VotingUpdate': {
      const votingStore = useVotingStore()
      votingStore.updatePoll(update.data)
      break
    }

    case 'VotingSync': {
      const votingStore = useVotingStore()
      votingStore.syncPolls(update.data)
      break
    }

    case 'InteractWordAppend':
      store.appendInteractWords(update.data)
      break
  }
}

/** 清理事件监听 */
export async function cleanupBliveClient(): Promise<void> {
  // 取消订阅
  if (currentWindowLabel) {
    try {
      await unsubscribeEvents(currentWindowLabel)
    } catch (e) {
      logger.error('Failed to unsubscribe:', e)
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

  logger.debug('Cleaned up')
}

/** 自动连接（如果有保存的房间号和 Cookie） */
export async function autoConnect(): Promise<void> {
  const settingsStore = useSettingsStore()
  await settingsStore.loadSettings()

  const roomId = settingsStore.settings.roomId
  const cookie = settingsStore.settings.cookie

  if (!roomId || !cookie) {
    logger.debug('Auto connect skipped: missing roomId or cookie')
    return
  }

  const roomIdNum = parseInt(roomId, 10)
  if (isNaN(roomIdNum) || roomIdNum <= 0) {
    logger.debug('Auto connect skipped: invalid roomId')
    return
  }

  logger.debug('Auto connecting to room:', roomIdNum)
  const result = await connectRoom(roomIdNum, cookie)

  if (result.success && result.room_info) {
    const danmakuStore = useDanmakuStore()
    applyRoomInfo(danmakuStore, result.room_info)
  } else {
    logger.error('Auto connect failed:', result.message)
  }
}

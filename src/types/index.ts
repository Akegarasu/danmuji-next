// ==================== B站礼物系统说明 ====================
/**
 * B站礼物系统（2024年后）：
 * - 使用"电池"作为付费货币单位
 * - 1 人民币 = 10 电池
 * - 礼物分为：付费礼物（电池）和 免费礼物（如小心心）
 * - 已废弃：金瓜子、银瓜子
 */

// ==================== 事件类型（用于订阅）====================

/** 事件类型（与后端对应） */
export type EventType = 
  | 'danmaku'        // 弹幕
  | 'gift'           // 礼物
  | 'super_chat'     // SuperChat
  | 'contribution_rank'  // 贡献排行
  | 'stats'          // 统计数据
  | 'live_status'    // 直播状态

/** 所有事件类型 */
export const ALL_EVENT_TYPES: EventType[] = [
  'danmaku',
  'gift',
  'super_chat',
  'contribution_rank',
  'stats',
  'live_status'
]

// ==================== Tab 相关类型 ====================

export type TabType = 'interaction' | 'danmaku' | 'gift' | 'superchat' | 'audience'

/** Tab 类型到订阅事件类型的映射 */
export const TAB_EVENT_TYPES: Record<TabType, EventType[]> = {
  interaction: ['danmaku', 'gift', 'super_chat', 'stats', 'live_status'],
  danmaku: ['danmaku', 'live_status'],
  gift: ['gift', 'super_chat', 'stats', 'live_status'],
  superchat: ['super_chat', 'stats', 'live_status'],
  audience: ['contribution_rank', 'stats', 'live_status']
}

/** 互动 Tab 合并时间线项 */
export type InteractionItem =
  | { kind: 'danmaku'; data: ProcessedDanmaku }
  | { kind: 'gift'; data: ProcessedGift }
  | { kind: 'superchat'; data: ProcessedSuperChat }

// ==================== 设置相关类型 ====================

/** 观众排序方式 */
export type AudienceSortType = 'enterTime' | 'giftValue' | 'medalLevel'

/** 窗口显示设置 */
export interface WindowSettings {
  opacity: number
  alwaysOnTop: boolean
  fontSize: number
  showMedal: boolean
  showAvatar: boolean
  hideBorder: boolean
  /** 失焦时隐藏标题栏和标签栏 */
  autoHideUi: boolean
}

/** 显示设置 */
export interface DisplaySettings {
  // 弹幕设置
  danmakuShowMedal: boolean
  danmakuShowGuard: boolean
  danmakuShowAdmin: boolean
  danmakuShowTime: boolean
  danmakuShowGuardBorder: boolean
  danmakuEmoticonSize: number
  
  // 礼物设置
  giftMergeDisplay: boolean
  giftShowFree: boolean
  giftMinPrice: number
  giftShowTime: boolean
  giftShowMedal: boolean
  /** 启用礼物过期灰显 */
  giftExpireEnabled: boolean
  /** 礼物过期时间（分钟） */
  giftExpireMinutes: number
  
  // SC 设置
  scMergeWithGift: boolean
  
  // 观众设置
  audienceSortType: AudienceSortType
  audienceShowEnterMsg: boolean
  audienceShowMedal: boolean
}

/** 用户登录信息 */
export interface UserLoginInfo {
  uid: number
  uname: string
  face: string
  isLogin: boolean
}

/** 应用设置 */
export interface AppSettings {
  roomId: string
  cookie: string
  user: UserLoginInfo | null
  windows: Record<string, WindowSettings>
  display: DisplaySettings
  tabOrder: TabType[]
  /** 特别关注的 UID 列表 */
  specialFollowUids: number[]
}

/** 默认显示设置 */
export const DEFAULT_DISPLAY_SETTINGS: DisplaySettings = {
  danmakuShowMedal: true,
  danmakuShowGuard: true,
  danmakuShowAdmin: true,
  danmakuShowTime: false,
  danmakuShowGuardBorder: false,
  danmakuEmoticonSize: 32,
  giftMergeDisplay: true,
  giftShowFree: true,
  giftMinPrice: 0,
  giftShowTime: false,
  giftShowMedal: false,
  giftExpireEnabled: true,
  giftExpireMinutes: 3,
  scMergeWithGift: false,
  audienceSortType: 'enterTime',
  audienceShowEnterMsg: true,
  audienceShowMedal: true
}

/** 默认窗口设置 */
export const DEFAULT_WINDOW_SETTINGS: WindowSettings = {
  opacity: 0.9,
  alwaysOnTop: true,
  fontSize: 14,
  showMedal: true,
  showAvatar: true,
  hideBorder: false,
  autoHideUi: false
}

// ==================== 后端数据类型（与 Rust 对应）====================

/** 处理后的弹幕（来自后端） */
export interface ProcessedDanmaku {
  id: string
  content: string
  user: ProcessedUser
  timestamp: number
  is_emoticon: boolean
  emoticon_url?: string
}

/** 处理后的礼物（来自后端，已合并） */
export interface ProcessedGift {
  id: string
  merge_key: string
  gift_id: number
  gift_name: string
  gift_icon?: string
  num: number
  total_value: number
  is_paid: boolean
  user: ProcessedUser
  timestamp: number
  /** 大航海等级（仅大航海购买时有值：1=总督, 2=提督, 3=舰长） */
  guard_level?: number
}

/** 处理后的 SC（来自后端） */
export interface ProcessedSuperChat {
  id: string
  content: string
  /** 价格（电池，1元=10电池） */
  price: number
  user: ProcessedUser
  background_color: string
  duration: number
  start_time: number
}

/** 处理后的用户信息（来自后端） */
export interface ProcessedUser {
  uid: number
  name: string
  face?: string
  medal?: ProcessedMedal
  guard_level: number
  is_admin: boolean
}

/** 处理后的勋章（来自后端） */
export interface ProcessedMedal {
  name: string
  level: number
  color: string
}

/** 高能用户排行（来自后端） */
export interface ProcessedOnlineRankUser {
  uid: number
  name: string
  face?: string
  rank: number
  score: string
  guard_level: number
}

/** 用户贡献统计（来自后端） */
export interface UserContribution {
  uid: number
  name: string
  face?: string
  total_value: number
  guard_level: number
}

/** 直播统计（来自后端） */
export interface LiveStats {
  total_revenue: number
  gift_revenue: number
  sc_revenue: number
  guard_revenue: number
  online_count: number
}

/** 贡献排行榜用户（API 获取，完整信息） */
export interface ContributionRankUser {
  uid: number
  name: string
  face: string
  rank: number
  score: number
  guard_level: number
  medal_name?: string
  medal_level?: number
  medal_color?: string
}

/** 礼物更新操作 */
export interface GiftUpsert {
  merge_key: string
  gift: ProcessedGift
  action: 'insert' | 'update'
}

/** 数据更新类型（来自后端） */
export type DataUpdate =
  | { type: 'DanmakuAppend'; data: ProcessedDanmaku[] }
  | { type: 'GiftUpsert'; data: GiftUpsert[] }
  | { type: 'SuperChatAppend'; data: ProcessedSuperChat }
  | { type: 'ContributionRankLive'; data: ProcessedOnlineRankUser[] }
  | { type: 'ContributionRankFull'; data: ContributionRankUser[] }
  | { type: 'StatsUpdate'; data: LiveStats }
  | { type: 'ContributionsUpdate'; data: UserContribution[] }
  | { type: 'LiveStart' }
  | { type: 'LiveStop' }

/** 数据快照（来自后端） */
export interface DataSnapshot {
  danmaku_list?: ProcessedDanmaku[]
  gift_list?: ProcessedGift[]
  superchat_list?: ProcessedSuperChat[]
  contribution_rank_live?: ProcessedOnlineRankUser[]
  contribution_rank_full?: ContributionRankUser[]
  contributions?: UserContribution[]
  stats?: LiveStats
}

// ==================== 工具函数 ====================

/** 格式化价格显示（电池转人民币） */
export const formatPrice = (battery: number): string => {
  if (battery <= 0) return ''
  const rmb = battery / 10
  if (rmb >= 1000) {
    return `¥${(rmb / 1000).toFixed(1)}k`
  }
  if (rmb >= 1) {
    return `¥${rmb.toFixed(rmb % 1 === 0 ? 0 : 1)}`
  }
  // 小于1元，显示小数（如 ¥0.1）
  return `¥${rmb.toFixed(1)}`
}

/** 格式化事件时间（支持秒/毫秒时间戳） */
export const formatEventTime = (timestamp: number): string => {
  if (!timestamp) return ''
  const ms = timestamp < 1_000_000_000_000 ? timestamp * 1000 : timestamp
  const date = new Date(ms)
  const pad = (num: number) => num.toString().padStart(2, '0')
  return `${pad(date.getHours())}:${pad(date.getMinutes())}`
}

/** 新版粉丝牌背景渐变 */
export const getMedalGradient = (level: number): string => {
  if (level <= 10) {
    return 'linear-gradient(45deg, #5762A799, #5762A799)'
  }
  if (level <= 20) {
    return 'linear-gradient(45deg, #C770A499, #C770A499)'
  }
  if (level <= 30) {
    return 'linear-gradient(45deg, #3FB4F699, #3FB4F699)'
  }
  if (level <= 40) {
    return 'linear-gradient(45deg, #4C7DFF99, #4C7DFF99)'
  }
  if (level <= 50) {
    return 'linear-gradient(45deg, rgba(167, 115, 241, 0.6), rgba(167, 115, 241, 0.6))'
  }
  return 'linear-gradient(45deg, rgba(236, 79, 110, 0.6), rgba(236, 79, 110, 0.6))'
}

// ==================== 存档相关类型 ====================

/** 存档会话 */
export interface ArchiveSession {
  id: number
  room_id: number
  room_title: string
  streamer_uid: number
  start_time: number
  end_time: number | null
  total_revenue: number
  gift_revenue: number
  sc_revenue: number
  guard_revenue: number
  danmaku_count: number
  gift_count: number
  sc_count: number
}

/** 分页结果 */
export interface PagedResult<T> {
  items: T[]
  total: number
  page: number
  page_size: number
}

/** 存档弹幕 */
export interface ArchivedDanmaku {
  id: number
  content: string
  user_uid: number
  user_name: string
  timestamp: number
  is_emoticon: boolean
  emoticon_url?: string
}

/** 存档礼物 */
export interface ArchivedGift {
  id: number
  gift_name: string
  gift_icon?: string
  num: number
  total_value: number
  is_paid: boolean
  user_uid: number
  user_name: string
  timestamp: number
  guard_level?: number
}

/** 存档 SC */
export interface ArchivedSuperChat {
  id: number
  content: string
  price: number
  user_uid: number
  user_name: string
  background_color: string
  duration: number
  start_time: number
}

/** 存档内容类型筛选 */
export type ArchiveContentType = 'all' | 'danmaku' | 'gift' | 'superchat'

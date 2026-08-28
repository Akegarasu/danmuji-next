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
  | 'video_request'  // 点播请求
  | 'voting'         // 投票
  | 'interact_word'  // 进入直播间

/** 所有事件类型 */
export const ALL_EVENT_TYPES: EventType[] = [
  'danmaku',
  'gift',
  'super_chat',
  'contribution_rank',
  'stats',
  'live_status',
  'video_request',
  'voting',
  'interact_word'
]

// ==================== Tab 相关类型 ====================

export type TabType = 'interaction' | 'danmaku' | 'gift' | 'superchat' | 'audience'

/** Tab 类型到订阅事件类型的映射 */
export const TAB_EVENT_TYPES: Record<TabType, EventType[]> = {
  interaction: ['danmaku', 'gift', 'super_chat', 'stats', 'live_status', 'interact_word'],
  danmaku: ['danmaku', 'live_status'],
  gift: ['gift', 'super_chat', 'stats', 'live_status'],
  superchat: ['super_chat', 'stats', 'live_status'],
  audience: ['contribution_rank', 'stats', 'live_status', 'interact_word']
}

/** 互动 Tab 合并时间线项 */
export type InteractionItem =
  | { kind: 'danmaku'; data: ProcessedDanmaku }
  | { kind: 'gift'; data: ProcessedGift }
  | { kind: 'superchat'; data: ProcessedSuperChat }

// ==================== 设置相关类型 ====================

/** 观众排序方式 */
export type AudienceSortType = 'enterTime' | 'giftValue' | 'medalLevel'

/** 内容字重 */
export type ContentFontWeight = 300 | 400 | 500 | 600 | 700 | 800

/** 窗口显示设置 */
export interface WindowSettings {
  opacity: number
  alwaysOnTop: boolean
  fontSize: number
  /** UI 元素字体大小（标题栏、标签栏等） */
  uiFontSize: number
  showMedal: boolean
  showAvatar: boolean
  hideBorder: boolean
  /** 失焦时隐藏标题栏和标签栏 */
  autoHideUi: boolean
}

/** 显示设置 */
export interface DisplaySettings {
  // 粉丝勋章通用设置
  medalShowUnlit: boolean
  medalShowOtherRoom: boolean

  // 弹幕设置
  danmakuShowMedal: boolean
  danmakuShowGuard: boolean
  danmakuShowAdmin: boolean
  danmakuShowTime: boolean
  danmakuShowGuardBorder: boolean
  danmakuEmoticonSize: number
  contentFontFamily: string
  contentFontWeight: ContentFontWeight
  danmakuFontColor: string
  danmakuUsernameColor: string
  
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
  giftFontColor: string
  giftUsernameColor: string
  giftPriceColor: string
  
  // SC 设置
  scMergeWithGift: boolean
  superChatFontColor: string
  
  // 观众设置
  audienceSortType: AudienceSortType
  audienceShowEnterMsg: boolean
  audienceShowMedal: boolean
  /** 自动刷新贡献榜 */
  audienceAutoRefreshEnabled: boolean
  /** 自动刷新间隔（秒） */
  audienceAutoRefreshIntervalSeconds: number
  audienceFontColor: string
  audienceScoreColor: string

  // 入场通知设置
  entryShowEnabled: boolean
  /** 在互动栏显示进房面板 */
  entryPanelShowInInteraction: boolean
  /** 在观众栏显示进房面板 */
  entryPanelShowInAudience: boolean
  entryFilterAll: boolean
  entryFilterCaptain: boolean
  entryFilterAdmiral: boolean
  entryFilterGovernor: boolean
  entryFilterSpecialFollow: boolean
  entryShowMedal: boolean
  entryShowGuard: boolean
  entryPanelHeight: number
  entryFontColor: string
  entryTimeColor: string
}

/** 语音播报设置 */
export interface SpeechSettings {
  enabled: boolean
  /** SAPI voice token ID；为空时自动优先选择中文语音 */
  voiceId: string | null
  /** SAPI 语速，范围 -10 ~ 10 */
  rate: number
  speakDanmaku: boolean
  speakGift: boolean
  speakSuperChat: boolean
}

/** 系统语音 */
export interface SpeechVoice {
  id: string
  name: string
  language: string
}

/** 语音服务状态 */
export interface SpeechStatus {
  available: boolean
  speaking: boolean
  danmaku_suspended: boolean
  queue_depth: number
  error: string | null
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
  speech: SpeechSettings
  tabOrder: TabType[]
  /** 特别关注的 UID 列表 */
  specialFollowUids: number[]
  /** 本地弹幕过滤的 UID 列表 */
  danmakuFilterUids: number[]
}

/** 默认显示设置 */
export const DEFAULT_DISPLAY_SETTINGS: DisplaySettings = {
  medalShowUnlit: false,
  medalShowOtherRoom: false,
  danmakuShowMedal: true,
  danmakuShowGuard: true,
  danmakuShowAdmin: true,
  danmakuShowTime: false,
  danmakuShowGuardBorder: false,
  danmakuEmoticonSize: 32,
  contentFontFamily: 'var(--font-family)',
  contentFontWeight: 400,
  danmakuFontColor: '#ebebeb',
  danmakuUsernameColor: '#adbcd9',
  giftMergeDisplay: true,
  giftShowFree: true,
  giftMinPrice: 0,
  giftShowTime: false,
  giftShowMedal: false,
  giftExpireEnabled: true,
  giftExpireMinutes: 3,
  giftFontColor: '#ebebeb',
  giftUsernameColor: '#9b9b9b',
  giftPriceColor: '#f5c842',
  scMergeWithGift: false,
  superChatFontColor: '#ffffff',
  audienceSortType: 'enterTime',
  audienceShowEnterMsg: true,
  audienceShowMedal: true,
  audienceAutoRefreshEnabled: false,
  audienceAutoRefreshIntervalSeconds: 120,
  audienceFontColor: '#ebebeb',
  audienceScoreColor: '#f5c842',
  entryShowEnabled: true,
  entryPanelShowInInteraction: true,
  entryPanelShowInAudience: true,
  entryFilterAll: true,
  entryFilterCaptain: false,
  entryFilterAdmiral: false,
  entryFilterGovernor: false,
  entryFilterSpecialFollow: false,
  entryShowMedal: true,
  entryShowGuard: true,
  entryPanelHeight: 150,
  entryFontColor: '#9b9b9b',
  entryTimeColor: '#6b6b6b'
}

/** 默认语音设置 */
export const DEFAULT_SPEECH_SETTINGS: SpeechSettings = {
  enabled: false,
  voiceId: null,
  rate: 0,
  speakDanmaku: true,
  speakGift: true,
  speakSuperChat: true
}

/** 默认窗口设置 */
export const DEFAULT_WINDOW_SETTINGS: WindowSettings = {
  opacity: 0.9,
  alwaysOnTop: true,
  fontSize: 14,
  uiFontSize: 14,
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

/** 处理后的礼物（来自后端，仅真实 combo 会聚合） */
export interface ProcessedBlindGift {
  gift_id: number
  gift_name: string
  /** 盲盒实际消费金额（电池） */
  total_value: number
}

export interface ProcessedGift {
  id: string
  merge_key: string
  gift_id: number
  gift_name: string
  gift_icon?: string
  num: number
  /** 展示金额（电池）；盲盒为爆出礼物金额 */
  total_value: number
  /** 实际营收（电池）；盲盒为盲盒消费金额 */
  revenue_value: number
  is_paid: boolean
  combo?: ProcessedGiftCombo
  blind_gift?: ProcessedBlindGift
  user: ProcessedUser
  timestamp: number
  /** 大航海等级（仅大航海购买时有值：1=总督, 2=提督, 3=舰长） */
  guard_level?: number
}

export interface ProcessedGiftCombo {
  batch_combo_id: string
  combo_total_coin?: number
  super_batch_gift_num?: number
  combo_resources_id?: number
  combo_stay_time?: number
  show_batch_combo_send?: boolean
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
  is_light: boolean
  anchor_uid: number
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

/** 处理后的进入直播间消息（来自后端） */
export interface ProcessedInteractWord {
  id: string
  user: ProcessedUser
  timestamp: number
  msg_type: number
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

/** 贡献排行榜类型 */
export type ContributionRankType = 'online' | 'daily' | 'weekly' | 'monthly'

/** 贡献排行榜响应 */
export interface ContributionRankResponse {
  rank_type: ContributionRankType
  count: number
  list: ContributionRankUser[]
}

/** 大航海榜用户 */
export interface GuardTopListUser {
  uid: number
  name: string
  face: string
  rank: number
  accompany: number
  score: number
  guard_level: number
  medal_name?: string
  medal_level?: number
  medal_color?: string
}

/** 大航海榜响应 */
export interface GuardTopListResponse {
  count: number
  total_pages: number
  current_page: number
  list: GuardTopListUser[]
}

/** 礼物更新操作 */
export interface GiftUpsert {
  merge_key: string
  gift: ProcessedGift
  action: 'insert' | 'update'
}

/** 视频信息（来自后端） */
export interface VideoInfo {
  bvid: string
  aid: number
  title: string
  cover: string
  view: number
  owner_name: string
  owner_face: string
  duration: number
}

/** 点播来源 */
export type VideoRequestSource = 'danmaku' | 'superchat'

/** 点播请求项（来自后端） */
export interface VideoRequestItem {
  id: string
  video_id: string
  username: string
  uid: number
  source: VideoRequestSource
  sc_price?: number
  timestamp: number
  watched: boolean
  video_info?: VideoInfo
  loading: boolean
  error?: string
}

// ==================== 投票相关 ====================

/** 投票选项标识类型 */
export type VoteKeyType = 'letter' | 'number'

/** 投票状态 */
export type PollStatus = 'active' | 'ended'

/** 投票人 */
export interface Voter {
  uid: number
  username: string
  timestamp: number
}

/** 投票选项 */
export interface PollOption {
  key: string
  label: string
  vote_count: number
}

/** 投票 */
export interface Poll {
  id: string
  title: string
  key_type: VoteKeyType
  options: PollOption[]
  status: PollStatus
  voted_uids: Record<string, string>
  total_votes: number
  created_at: number
  end_at: number | null
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
  | { type: 'VideoRequestAppend'; data: VideoRequestItem }
  | { type: 'VideoRequestUpdate'; data: VideoRequestItem }
  | { type: 'VideoRequestSync'; data: VideoRequestItem[] }
  | { type: 'VotingUpdate'; data: Poll }
  | { type: 'VotingSync'; data: Poll[] }
  | { type: 'InteractWordAppend'; data: ProcessedInteractWord[] }

/** 数据快照（来自后端） */
export interface DataSnapshot {
  danmaku_list?: ProcessedDanmaku[]
  gift_list?: ProcessedGift[]
  superchat_list?: ProcessedSuperChat[]
  contribution_rank_live?: ProcessedOnlineRankUser[]
  contribution_rank_full?: ContributionRankUser[]
  contributions?: UserContribution[]
  stats?: LiveStats
  video_requests?: VideoRequestItem[]
  voting_polls?: Poll[]
  interact_word_list?: ProcessedInteractWord[]
}

// ==================== 工具函数 ====================

/** 判断粉丝勋章是否符合当前直播间的展示规则 */
export const shouldShowMedal = (
  medal: ProcessedMedal | undefined,
  streamerUid: number,
  showUnlit: boolean,
  showOtherRoom: boolean
): boolean => {
  if (!medal) return false
  if (medal.is_light === false && !showUnlit) return false
  if (
    !showOtherRoom
    && streamerUid > 0
    && medal.anchor_uid > 0
    && medal.anchor_uid !== streamerUid
  ) {
    return false
  }
  return true
}

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

/** 归档聚合统计 */
export interface ArchiveSummary {
  room_count: number
  session_count: number
  live_duration: number
  total_revenue: number
  gift_revenue: number
  sc_revenue: number
  guard_revenue: number
  danmaku_count: number
  gift_count: number
  sc_count: number
}

/** 按直播间聚合的归档摘要 */
export interface ArchiveRoomSummary {
  room_id: number
  room_title: string
  streamer_uid: number
  session_count: number
  live_duration: number
  total_revenue: number
  danmaku_count: number
  gift_count: number
  sc_count: number
  first_live_time: number
  last_live_time: number
}

export interface ArchiveOverview {
  summary: ArchiveSummary
  rooms: ArchiveRoomSummary[]
}

export interface ArchiveDailyStat {
  date: string
  session_count: number
  live_duration: number
  total_revenue: number
  gift_revenue: number
  sc_revenue: number
  guard_revenue: number
  danmaku_count: number
  gift_count: number
  sc_count: number
}

export interface ArchiveStatistics {
  summary: ArchiveSummary
  daily: ArchiveDailyStat[]
}

/** 跨弹幕、礼物与醒目留言的统一搜索结果 */
export interface ArchiveSearchItem {
  event_type: Exclude<ArchiveContentType, 'all'>
  id: number
  session_id: number
  room_id: number
  room_title: string
  content: string
  detail?: string
  user_uid: number
  user_name: string
  timestamp: number
  amount?: number
  quantity?: number
  image_url?: string
  is_emoticon: boolean
  is_paid: boolean
  guard_level?: number
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

/** 存档中的用户名称 */
export interface ArchivedUserName {
  uid: number
  name: string
}

/** 存档礼物 */
export interface ArchivedGift {
  id: number
  gift_name: string
  gift_icon?: string
  num: number
  total_value: number
  revenue_value: number
  is_paid: boolean
  combo?: ProcessedGiftCombo
  blind_gift?: ProcessedBlindGift
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

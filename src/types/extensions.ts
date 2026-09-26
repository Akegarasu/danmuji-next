import type { OvertimeSnapshot, OvertimeRequest } from './overtime'

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
  total_votes: number
  created_at: number
  end_at: number | null
}

export interface ExtensionStates {
  'video-request': VideoRequestItem[]
  voting: Poll[]
  overtime: OvertimeSnapshot
}
export type ExtensionId = keyof ExtensionStates

export interface ExtensionRequests {
  'video-request':
    | { type: 'mark_watched'; request_id: string; watched: boolean }
    | { type: 'remove'; request_id: string }
    | { type: 'clear_watched' | 'clear_all' }
  voting:
    | { type: 'create'; title: string; options: [string, string][]; key_type: VoteKeyType; duration_secs: number | null }
    | { type: 'end' | 'delete'; poll_id: string }
  overtime: OvertimeRequest
}

export interface ExtensionState<K extends ExtensionId> {
  extension_id: K
  revision: number
  state: ExtensionStates[K]
}

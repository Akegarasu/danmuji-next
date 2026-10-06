import type { OverlayStyle } from './overlay-style'

export interface SongPriorities {
  superchat: number
  governor: number
  admiral: number
  captain: number
}

export interface SongConfig {
  overlay_style: OverlayStyle
  enabled: boolean
  accept_danmaku: boolean
  accept_superchat: boolean
  command: string
  deduplicate: boolean
  priorities: SongPriorities
  audience: Record<'normal' | 'fans' | 'captain' | 'admiral' | 'governor', { enabled: boolean; limit: number; cooldown_secs: number }>
  gift_bonus_enabled: boolean
  gift_rules: SongGiftRule[]
  sc_min_price: number
  sc_content_as_song: boolean
  sc_sort_by_price: boolean
  overlay_max_rows: number
  overlay_show_username: boolean
}

export interface SongGiftRule {
  id: string
  gift_id: number | null
  gift_name: string
  gifts_required: number
  extra_requests: number
}

export interface SongRequestItem {
  id: string
  song_name: string
  username: string
  uid: number
  source: 'danmaku' | 'superchat'
  guard_level: number
  sc_price: number | null
  timestamp: number
  sung: boolean
}

export interface SongSnapshot {
  config: SongConfig
  requests: SongRequestItem[]
  session: { room_id: number; streamer_uid: number; live_start: number | null; started_at: number }
}

export type SongRequest =
  | { type: 'configure'; config: SongConfig }
  | { type: 'mark_sung'; request_id: string; sung: boolean }
  | { type: 'move'; request_id: string; target_id: string; placement: 'before' | 'after' }
  | { type: 'remove'; request_id: string }
  | { type: 'clear_sung' | 'clear_all' | 'sort_priority' | 'new_session' }

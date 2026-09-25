export type TimerAction = 'add' | 'subtract' | 'multiply' | 'divide' | 'set_time' | 'set_rate' | 'clear' | 'random'
export type RandomAction = 'add' | 'subtract' | 'multiply' | 'divide'
export interface RandomRange {
  action: RandomAction
  min: number
  max: number
}
export interface AppliedAction {
  action: Exclude<TimerAction, 'random'>
  value: number
  count: number
  random: boolean
  delta_ms: number
}
export interface GiftRule {
  id: string
  enabled: boolean
  gift_id: number | null
  gift_name: string
  action: TimerAction
  value: number
  per_gift: boolean
  random_ranges: RandomRange[]
}
export interface TimerConfig {
  enabled: boolean
  initial_seconds: number
  show_rules: boolean
  show_notice: boolean
  rules: GiftRule[]
}
export interface GiftNotice {
  id: number
  gift_name: string
  sender_name: string
  num: number
  delta_ms: number
  actions: Exclude<TimerAction, 'random'>[]
  results: AppliedAction[]
}
export interface OvertimeSnapshot {
  config: TimerConfig
  remaining_ms: number
  running: boolean
  rate: number
  notices: GiftNotice[]
}
export type OvertimeRequest =
  | { type: 'configure'; config: TimerConfig }
  | { type: 'start' | 'pause' | 'reset' }
  | { type: 'apply'; action: Exclude<TimerAction, 'random'>; value: number }

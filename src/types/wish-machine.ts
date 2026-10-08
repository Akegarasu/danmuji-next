export interface WishGoal {
  id: string
  gift_id: number | null
  gift_name: string
  current: number
  target: number
}
export type WishTheme = 'default' | 'test-a' | 'custom'
export interface WishConfig {
  enabled: boolean
  blind_gift_mode: 'original' | 'revealed'
  theme: WishTheme
  theme_a_font_family: string
  custom_css: string
}
export interface WishSnapshot { config: WishConfig; goals: WishGoal[] }
export type WishRequest =
  | { type: 'configure'; config: WishConfig }
  | ({ type: 'add_goal' } & Omit<WishGoal, 'id'>)
  | ({ type: 'update_goal'; current?: number } & Omit<WishGoal, 'current'>)
  | { type: 'remove_goal'; id: string }
  | { type: 'move_goal'; id: string; direction: -1 | 1 }

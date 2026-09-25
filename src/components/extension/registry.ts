/** 扩展面板注册表；新增扩展无需修改窗口内的分支判断。 */
import { defineAsyncComponent, type Component } from 'vue'
import type { EventType } from '@/types'

export interface ExtensionDefinition {
  id: string
  label: string
  component: Component
  liveEvents: EventType[]
}
export const extensionRegistry: ExtensionDefinition[] = [
  { id: 'video-request', label: '点播', component: defineAsyncComponent(() => import('./VideoRequestTab.vue')), liveEvents: ['video_request'] },
  { id: 'voting', label: '投票', component: defineAsyncComponent(() => import('./VotingTab.vue')), liveEvents: ['voting'] },
  { id: 'overtime', label: '加班机', component: defineAsyncComponent(() => import('./OvertimeTab.vue')), liveEvents: [] },
]

/** 扩展面板注册表；新增扩展无需修改窗口内的分支判断。 */
import { defineAsyncComponent, type Component } from 'vue'
import type { ExtensionId } from '@/types/extensions'

export interface ExtensionDefinition {
  id: ExtensionId
  label: string
  component: Component
}
export const extensionRegistry: ExtensionDefinition[] = [
  { id: 'video-request', label: '点播', component: defineAsyncComponent(() => import('./VideoRequestTab.vue')) },
  { id: 'voting', label: '投票', component: defineAsyncComponent(() => import('./VotingTab.vue')) },
  { id: 'overtime', label: '加班机', component: defineAsyncComponent(() => import('./OvertimeTab.vue')) },
]

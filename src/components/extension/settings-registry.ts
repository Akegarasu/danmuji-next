/** 扩展设置注册表：注册内容组件后，统一窗口会按扩展 ID 加载。 */
import { defineAsyncComponent, type Component } from 'vue'
import type { ExtensionId } from '@/types/extensions'

export interface ExtensionSettingsDefinition {
  title: string
  /** 内容组件使用 ExtensionSettingsLayout，并将 close 事件交给窗口处理。 */
  component: Component
}

export const extensionSettingsRegistry: Partial<Record<ExtensionId, ExtensionSettingsDefinition>> = {
  'song-request': {
    title: '点歌机设置',
    component: defineAsyncComponent(() => import('./SongRequestSettings.vue')),
  },
}

export function getExtensionSettings(id: string): ExtensionSettingsDefinition | undefined {
  return Object.prototype.hasOwnProperty.call(extensionSettingsRegistry, id) ? extensionSettingsRegistry[id as ExtensionId] : undefined
}

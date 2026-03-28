<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useRoute } from 'vue-router'
import { getCurrentWindow } from '@tauri-apps/api/window'
import TitleBar from '@/components/common/TitleBar.vue'
import InteractionTab from '@/components/tabs/InteractionTab.vue'
import DanmakuTab from '@/components/tabs/DanmakuTab.vue'
import GiftTab from '@/components/tabs/GiftTab.vue'
import SuperChatTab from '@/components/tabs/SuperChatTab.vue'
import AudienceTab from '@/components/tabs/AudienceTab.vue'
import { initWindowManager, cleanupWindowManager } from '@/services/window-manager'
import { initBliveClient, cleanupBliveClient } from '@/services/blive-client'
import { useSettingsStore } from '@/stores/settings'
import type { TabType } from '@/types'
import { TAB_EVENT_TYPES } from '@/types'
import type { UnlistenFn } from '@tauri-apps/api/event'

const route = useRoute()
const appWindow = getCurrentWindow()
const windowLabel = appWindow.label
const settingsStore = useSettingsStore()
const isWindowFocused = ref(true)
let unlistenFocus: UnlistenFn | null = null

const tabType = computed(() => route.params.type as TabType)

const tabInfo = computed(() => {
  const map: Record<TabType, { label: string }> = {
    interaction: { label: '互动' },
    danmaku: { label: '弹幕' },
    gift: { label: '礼物' },
    superchat: { label: 'SC' },
    audience: { label: '观众' }
  }
  return map[tabType.value] || { label: '互动' }
})

const currentTabComponent = computed(() => {
  switch (tabType.value) {
    case 'interaction': return InteractionTab
    case 'danmaku': return DanmakuTab
    case 'gift': return GiftTab
    case 'superchat': return SuperChatTab
    case 'audience': return AudienceTab
    default: return InteractionTab
  }
})

const showBars = computed(() =>
  isWindowFocused.value || !settingsStore.mainWindowSettings.autoHideUi
)

onMounted(async () => {
  // 初始化窗口管理器（恢复位置并开始监听）
  await initWindowManager(windowLabel)

  // 根据 Tab 类型订阅对应的事件
  const eventTypes = TAB_EVENT_TYPES[tabType.value] || []
  await initBliveClient(eventTypes)

  // 监听窗口焦点变化
  unlistenFocus = await appWindow.onFocusChanged(({ payload: focused }) => {
    isWindowFocused.value = focused
  })
})

onUnmounted(async () => {
  unlistenFocus?.()

  // 清理弹幕客户端
  await cleanupBliveClient()

  // 清理窗口管理器（保存状态并停止监听）
  await cleanupWindowManager(windowLabel)
})
</script>

<template>
  <div class="tab-window">
    <div class="bars-wrapper" :class="{ hidden: !showBars }">
      <TitleBar
        :title="tabInfo.label"
        :show-settings-btn="false"
        :show-lock-btn="true"
        :is-sub-window="true"
        :window-label="windowLabel"
      />
    </div>

    <div class="content">
      <component :is="currentTabComponent" />
    </div>
  </div>
</template>

<style scoped lang="scss">
.tab-window {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  background: var(--bg-primary);
  border-radius: var(--border-radius);
  border: var(--window-border);
  overflow: hidden;
}

.bars-wrapper {
  flex-shrink: 0;
  overflow: hidden;
  max-height: 36px; // title-bar only
  transition: max-height 0.15s ease;

  &.hidden {
    max-height: 0;
  }
}

.content {
  flex: 1;
  overflow: hidden;
}
</style>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import TitleBar from '@/components/common/TitleBar.vue'
import TabBar from '@/components/common/TabBar.vue'
import InteractionTab from '@/components/tabs/InteractionTab.vue'
import DanmakuTab from '@/components/tabs/DanmakuTab.vue'
import GiftTab from '@/components/tabs/GiftTab.vue'
import SuperChatTab from '@/components/tabs/SuperChatTab.vue'
import AudienceTab from '@/components/tabs/AudienceTab.vue'
import { createTabWindow } from '@/services/window-manager'
import { useSettingsStore } from '@/stores/settings'
import type { TabType } from '@/types'
import type { UnlistenFn } from '@tauri-apps/api/event'

const settingsStore = useSettingsStore()
const activeTab = ref<TabType>('interaction')
const isLocked = ref(false)
const isWindowFocused = ref(true)
let unlistenFocus: UnlistenFn | null = null

const onLockChange = (locked: boolean) => {
  isLocked.value = locked
}

const showBars = computed(() =>
  isWindowFocused.value || !settingsStore.mainWindowSettings.autoHideUi
)

// Tab 配置
const tabs: { type: TabType; label: string; icon: string }[] = [
  { type: 'interaction', label: '互动', icon: '✨' },
  { type: 'danmaku', label: '弹幕', icon: '💬' },
  { type: 'gift', label: '礼物', icon: '🎁' },
  { type: 'superchat', label: 'SC', icon: '💰' },
  { type: 'audience', label: '观众', icon: '👥' }
]

// 拆分 Tab 为独立窗口
const popOutTab = async (tabType: TabType) => {
  const tab = tabs.find(t => t.type === tabType)
  if (!tab) return

  try {
    await createTabWindow(tabType, tab.label)
  } catch (e) {
    console.error('Failed to create tab window:', e)
  }
}

const currentTabComponent = computed(() => {
  switch (activeTab.value) {
    case 'interaction': return InteractionTab
    case 'danmaku': return DanmakuTab
    case 'gift': return GiftTab
    case 'superchat': return SuperChatTab
    case 'audience': return AudienceTab
    default: return InteractionTab
  }
})

onMounted(async () => {
  const appWindow = getCurrentWindow()
  unlistenFocus = await appWindow.onFocusChanged(({ payload: focused }) => {
    isWindowFocused.value = focused
  })
})

onUnmounted(() => {
  unlistenFocus?.()
})
</script>

<template>
  <div class="main-window" :class="{ locked: isLocked }">
    <div class="bars-wrapper" :class="{ hidden: !showBars }">
      <TitleBar
        title="AKI 弹幕姬"
        :show-settings-btn="true"
        :show-lock-btn="true"
        window-label="main"
        @lock-change="onLockChange"
      />

      <TabBar
        :tabs="tabs"
        v-model:active="activeTab"
        @pop-out="popOutTab"
      />
    </div>

    <div class="content">
      <KeepAlive>
        <component :is="currentTabComponent" />
      </KeepAlive>
    </div>
  </div>
</template>

<style scoped lang="scss">
.main-window {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  background: var(--bg-primary);
  border-radius: var(--border-radius);
  border: var(--window-border);
  overflow: hidden;
  transition: opacity 0.2s;
}

.bars-wrapper {
  flex-shrink: 0;
  overflow: hidden;
  max-height: 76px; // title-bar(36) + tab-bar(40)
  transition: max-height 0.15s ease;

  &.hidden {
    max-height: 0;
  }
}

.content {
  flex: 1;
  overflow: hidden;
  position: relative;
}
</style>

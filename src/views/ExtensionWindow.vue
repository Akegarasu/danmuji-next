<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import TitleBar from '@/components/common/TitleBar.vue'
import VideoRequestTab from '@/components/extension/VideoRequestTab.vue'
import VotingTab from '@/components/extension/VotingTab.vue'
import { initWindowManager, cleanupWindowManager } from '@/services/window-manager'
import { initBliveClient, cleanupBliveClient } from '@/services/blive-client'
import type { UnlistenFn } from '@tauri-apps/api/event'

type ExtensionTabType = 'video-request' | 'voting'

const appWindow = getCurrentWindow()
const windowLabel = appWindow.label
const isWindowFocused = ref(true)
let unlistenFocus: UnlistenFn | null = null

const activeTab = ref<ExtensionTabType>('video-request')

/** 扩展 Tab 配置 */
const tabs: { type: ExtensionTabType; label: string }[] = [
  { type: 'video-request', label: '点播' },
  { type: 'voting', label: '投票' },
]

const currentComponent = computed(() => {
  switch (activeTab.value) {
    case 'video-request': return VideoRequestTab
    case 'voting': return VotingTab
    default: return VideoRequestTab
  }
})

onMounted(async () => {
  await initWindowManager(windowLabel)
  await initBliveClient(['video_request', 'voting'])
  unlistenFocus = await appWindow.onFocusChanged(({ payload: focused }) => {
    isWindowFocused.value = focused
  })
})

onUnmounted(async () => {
  unlistenFocus?.()
  await cleanupBliveClient()
  await cleanupWindowManager(windowLabel)
})
</script>

<template>
  <div class="extension-window">
    <TitleBar
      title="扩展"
      :show-settings-btn="false"
      :is-sub-window="true"
      :window-label="windowLabel"
    />

    <!-- 扩展 Tab 栏 -->
    <div class="ext-tab-bar">
      <button
        v-for="tab in tabs"
        :key="tab.type"
        class="ext-tab-item"
        :class="{ active: activeTab === tab.type }"
        @click="activeTab = tab.type"
      >
        {{ tab.label }}
      </button>
    </div>

    <div class="content">
      <KeepAlive>
        <component :is="currentComponent" />
      </KeepAlive>
    </div>
  </div>
</template>

<style scoped lang="scss">
.extension-window {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  background: var(--bg-primary);
  border-radius: var(--border-radius);
  border: var(--window-border);
  overflow: hidden;
}

.ext-tab-bar {
  display: flex;
  height: var(--tab-bar-height);
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  padding: 4px;
  gap: 4px;
  overflow-x: auto;

  &::-webkit-scrollbar {
    height: 0;
  }
}

.ext-tab-item {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0 16px;
  background: transparent;
  border: none;
  border-radius: var(--border-radius-sm);
  color: var(--text-secondary);
  font-size: var(--font-size-sm);
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
  transition: background 0.15s, color 0.15s;

  &:hover {
    color: var(--text-primary);
    background: var(--bg-hover);
  }

  &.active {
    color: var(--text-primary);
    background: var(--bg-active);
  }
}

.content {
  flex: 1;
  overflow: hidden;
  position: relative;
}
</style>

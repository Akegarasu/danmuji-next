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
import UpdateToast from '@/components/common/UpdateToast.vue'
import { createTabWindow } from '@/services/window-manager'
import { useSettingsStore } from '@/stores/settings'
import { startAutoCheck, stopAutoCheck, type UpdateInfo } from '@/services/updater'
import type { TabType } from '@/types'
import type { UnlistenFn } from '@tauri-apps/api/event'

const settingsStore = useSettingsStore()
const activeTab = ref<TabType>('interaction')
const isLocked = ref(false)
const isWindowFocused = ref(true)
let unlistenFocus: UnlistenFn | null = null

// 更新相关
const showUpdateToast = ref(false)
const updateInfo = ref<UpdateInfo | null>(null)

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

// 显示更新日志（简单弹窗展示 notes）
const showChangelogDialog = ref(false)
const onShowChangelog = () => {
  showChangelogDialog.value = true
}

onMounted(async () => {
  const appWindow = getCurrentWindow()
  unlistenFocus = await appWindow.onFocusChanged(({ payload: focused }) => {
    isWindowFocused.value = focused
  })

  // 启动后台更新检查
  startAutoCheck((info) => {
    updateInfo.value = info
    showUpdateToast.value = true
  })
})

onUnmounted(() => {
  unlistenFocus?.()
  stopAutoCheck()
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

      <UpdateToast
        v-if="showUpdateToast && updateInfo"
        :update-info="updateInfo"
        @close="showUpdateToast = false"
        @show-changelog="onShowChangelog"
      />
    </div>

    <!-- 更新日志弹窗 -->
    <Teleport to="body">
      <Transition name="changelog-fade">
        <div v-if="showChangelogDialog && updateInfo" class="changelog-overlay" @click.self="showChangelogDialog = false">
          <div class="changelog-dialog">
            <div class="changelog-header">
              <h3>v{{ updateInfo.version }} 更新日志</h3>
              <button class="changelog-close" @click="showChangelogDialog = false">&times;</button>
            </div>
            <div class="changelog-body">
              <pre class="changelog-content">{{ updateInfo.notes || '暂无更新说明' }}</pre>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
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

<!-- 更新日志弹窗样式（非 scoped，因为 Teleport 到 body） -->
<style lang="scss">
.changelog-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
  backdrop-filter: blur(2px);
}

.changelog-dialog {
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  width: 340px;
  max-height: 400px;
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.25);
  overflow: hidden;
  animation: changelogIn 0.15s ease;
  display: flex;
  flex-direction: column;
}

@keyframes changelogIn {
  from {
    opacity: 0;
    transform: scale(0.95) translateY(-4px);
  }
  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}

.changelog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 16px 10px;

  h3 {
    margin: 0;
    font-size: var(--font-size-base, 14px);
    font-weight: 600;
    color: var(--text-primary);
  }
}

.changelog-close {
  background: none;
  border: none;
  color: var(--text-muted);
  font-size: 18px;
  cursor: pointer;
  padding: 0 4px;
  line-height: 1;
  border-radius: var(--border-radius-sm);

  &:hover {
    color: var(--text-secondary);
    background: var(--bg-hover);
  }
}

.changelog-body {
  padding: 0 16px 16px;
  overflow-y: auto;
  flex: 1;
}

.changelog-content {
  margin: 0;
  font-size: var(--font-size-sm, 13px);
  color: var(--text-secondary);
  line-height: 1.7;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: var(--font-family);
}

.changelog-fade-enter-active,
.changelog-fade-leave-active {
  transition: opacity 0.15s ease;
}

.changelog-fade-enter-from,
.changelog-fade-leave-to {
  opacity: 0;
}
</style>

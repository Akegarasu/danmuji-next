<script setup lang="ts">
import { computed, ref } from 'vue'
import { useDanmakuStore } from '@/stores/danmaku'
import { useSettingsStore } from '@/stores/settings'
import VirtualList from '@/components/common/VirtualList.vue'
import SuperChatItem from '@/components/items/SuperChatItem.vue'
import ContextMenu from '@/components/common/ContextMenu.vue'
import type { MenuItem } from '@/components/common/ContextMenu.vue'
import type { ProcessedSuperChat } from '@/types'
import { formatPrice } from '@/types'
import { invoke } from '@tauri-apps/api/core'

const danmakuStore = useDanmakuStore()
const settingsStore = useSettingsStore()
const contextMenuRef = ref<InstanceType<typeof ContextMenu>>()
const currentSC = ref<ProcessedSuperChat | null>(null)

type VirtualListExpose = {
  scrollToBottom: () => void
}

const virtualListRef = ref<VirtualListExpose | null>(null)
const autoScroll = ref(true)
const orderedSuperChatList = computed(() => [...danmakuStore.superChatList].reverse())
const superChatItemKey = (sc: ProcessedSuperChat) => sc.id
const superChatLayoutVersion = computed(() => settingsStore.mainWindowSettings.fontSize)

const scrollToBottom = () => {
  virtualListRef.value?.scrollToBottom()
}

const menuItems = ref<MenuItem[]>([
  {
    label: '打开用户主页',
    icon: '🔗',
    action: () => openUserPage()
  },
  {
    label: '复制用户名',
    icon: '📋',
    action: () => copyUsername()
  },
  {
    label: '复制内容',
    icon: '📝',
    action: () => copyContent()
  }
])

const handleContextMenu = (e: MouseEvent, sc: ProcessedSuperChat) => {
  e.preventDefault()
  e.stopPropagation()
  currentSC.value = sc
  contextMenuRef.value?.show(e.clientX, e.clientY)
}

const openUserPage = async () => {
  if (!currentSC.value) return
  const url = `https://space.bilibili.com/${currentSC.value.user.uid}`
  try {
    await invoke('open_url', { url })
  } catch (e) {
    window.open(url, '_blank')
  }
}

const copyUsername = () => {
  if (!currentSC.value) return
  navigator.clipboard.writeText(currentSC.value.user.name)
}

const copyContent = () => {
  if (!currentSC.value) return
  navigator.clipboard.writeText(currentSC.value.content)
}
</script>

<template>
  <div class="superchat-tab">
    <!-- 统计栏 -->
    <div class="stats-bar">
      <span class="stat">
        SC 收入: <strong>{{ formatPrice(danmakuStore.stats.sc_revenue) || '¥0' }}</strong>
      </span>
      <span class="stat">
        共 <strong>{{ danmakuStore.superChatList.length }}</strong> 条
      </span>
    </div>
    
    <VirtualList
      ref="virtualListRef"
      v-model:auto-scroll="autoScroll"
      class="sc-list"
      :items="orderedSuperChatList"
      :item-key="superChatItemKey"
      :estimate-size="112"
      :overscan="6"
      :layout-version="superChatLayoutVersion"
    >
      <template #default="{ item: sc }">
        <SuperChatItem
          :superchat="sc"
          @contextmenu="handleContextMenu($event, sc)"
        />
      </template>

      <template #empty>
        <div class="empty-state">
          <span class="text">等待 SuperChat...</span>
        </div>
      </template>
    </VirtualList>

    <!-- 回到底部按钮 -->
    <Transition name="fade">
      <button v-if="!autoScroll" class="scroll-btn" @click="scrollToBottom">
        ↓ 回到底部
      </button>
    </Transition>
    
    <ContextMenu ref="contextMenuRef" :items="menuItems" />
  </div>
</template>

<style scoped lang="scss">
.superchat-tab {
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  position: relative;
}

.stats-bar {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 8px 12px;
  background: var(--bg-card);
  border-bottom: 1px solid var(--border-color);
  font-size: var(--font-size-xs);
  color: var(--text-secondary);
  flex-shrink: 0;
  
  .stat strong {
    color: var(--accent-gold);
  }
}

.sc-list {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 8px;
  min-height: 0;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  min-height: 200px;
  color: var(--text-muted);
  gap: 8px;
  
  .text {
    font-size: var(--font-size-sm);
  }
}

.scroll-btn {
  position: absolute;
  bottom: 12px;
  left: 50%;
  transform: translateX(-50%);
  padding: 6px 16px;
  background: var(--bg-active);
  color: var(--text-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius);
  font-size: var(--font-size-xs);
  cursor: pointer;
  transition: background 0.15s;

  &:hover {
    background: var(--bg-hover);
  }
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>

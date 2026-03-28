<script setup lang="ts">
import { ref } from 'vue'
import { useDanmakuStore } from '@/stores/danmaku'
import SuperChatItem from '@/components/items/SuperChatItem.vue'
import ContextMenu from '@/components/common/ContextMenu.vue'
import type { MenuItem } from '@/components/common/ContextMenu.vue'
import type { ProcessedSuperChat } from '@/types'
import { formatPrice } from '@/types'
import { invoke } from '@tauri-apps/api/core'

const danmakuStore = useDanmakuStore()
const contextMenuRef = ref<InstanceType<typeof ContextMenu>>()
const currentSC = ref<ProcessedSuperChat | null>(null)

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
    
    <div class="sc-list">
      <SuperChatItem
        v-for="sc in danmakuStore.superChatList"
        :key="sc.id"
        :superchat="sc"
        @contextmenu="handleContextMenu($event, sc)"
      />
      
      <div v-if="danmakuStore.superChatList.length === 0" class="empty-state">
        <span class="text">等待 SuperChat...</span>
      </div>
    </div>
    
    <ContextMenu ref="contextMenuRef" :items="menuItems" />
  </div>
</template>

<style scoped lang="scss">
.superchat-tab {
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
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
</style>

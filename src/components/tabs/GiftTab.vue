<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useDanmakuStore } from '@/stores/danmaku'
import { useSettingsStore } from '@/stores/settings'
import GiftItem from '@/components/items/GiftItem.vue'
import SuperChatItem from '@/components/items/SuperChatItem.vue'
import ContextMenu from '@/components/common/ContextMenu.vue'
import type { MenuItem } from '@/components/common/ContextMenu.vue'
import type { ProcessedGift, ProcessedSuperChat } from '@/types'
import { formatPrice } from '@/types'
import { useAutoScroll } from '@/composables/useAutoScroll'
import { useContextMenuActions } from '@/composables/useContextMenuActions'

const danmakuStore = useDanmakuStore()
const settingsStore = useSettingsStore()

// ==================== 礼物过期 ====================

const nowSeconds = ref(Math.floor(Date.now() / 1000))
let nowTimer: number | null = null

const isGiftExpired = (gift: ProcessedGift): boolean => {
  if (!settingsStore.giftExpireEnabled) return false
  return nowSeconds.value - gift.timestamp > settingsStore.giftExpireMinutes * 60
}

// ==================== 过滤与合并 ====================

const filteredGiftList = computed(() => {
  return danmakuStore.giftList.filter(gift => {
    if (!settingsStore.giftShowFree && !gift.is_paid) return false
    if (gift.total_value < settingsStore.giftMinPrice) return false
    return true
  })
})

const mergedList = computed(() => {
  if (!settingsStore.scMergeWithGift) {
    return filteredGiftList.value
  }

  const allItems: (ProcessedGift | ProcessedSuperChat)[] = [
    ...filteredGiftList.value,
    ...danmakuStore.superChatList
  ]

  return allItems.sort((a, b) => {
    const timeA = 'start_time' in a ? a.start_time : a.timestamp
    const timeB = 'start_time' in b ? b.start_time : b.timestamp
    return timeA - timeB
  })
})

const isSuperChat = (item: ProcessedGift | ProcessedSuperChat): item is ProcessedSuperChat => {
  return 'duration' in item
}

// ==================== Composables ====================

const { listRef, autoScroll, onScroll, scrollToBottom: _scrollToBottom } = useAutoScroll(
  () => mergedList.value.length
)
const { openUserPage, copyUsername } = useContextMenuActions()

// ==================== 右键菜单 ====================

const contextMenuRef = ref<InstanceType<typeof ContextMenu>>()
const currentItem = ref<ProcessedGift | ProcessedSuperChat | null>(null)

const menuItems = ref<MenuItem[]>([
  {
    label: '打开用户主页',
    icon: '🔗',
    action: () => currentItem.value && openUserPage(currentItem.value.user.uid)
  },
  {
    label: '复制用户名',
    icon: '📋',
    action: () => currentItem.value && copyUsername(currentItem.value.user.name)
  }
])

const handleContextMenu = (e: MouseEvent, item: ProcessedGift | ProcessedSuperChat) => {
  e.preventDefault()
  e.stopPropagation()
  currentItem.value = item
  contextMenuRef.value?.show(e.clientX, e.clientY)
}

// ==================== 生命周期 ====================

onMounted(() => {
  nowTimer = window.setInterval(() => {
    nowSeconds.value = Math.floor(Date.now() / 1000)
  }, 10000)
})

onUnmounted(() => {
  if (nowTimer) {
    clearInterval(nowTimer)
    nowTimer = null
  }
})
</script>

<template>
  <div class="gift-tab">
    <!-- 统计栏 -->
    <div class="stats-bar">
      <span class="stat">
        收入: <strong>{{ formatPrice(danmakuStore.stats.total_revenue) || '¥0' }}</strong>
      </span>
      <span class="stat">
        礼物: <strong>{{ formatPrice(danmakuStore.stats.gift_revenue) || '¥0' }}</strong>
      </span>
      <span class="stat">
        舰长: <strong>{{ formatPrice(danmakuStore.stats.guard_revenue) || '¥0' }}</strong>
      </span>
      <span class="stat">
        SC: <strong>{{ formatPrice(danmakuStore.stats.sc_revenue) || '¥0' }}</strong>
      </span>
    </div>

    <div
      ref="listRef"
      class="gift-list"
      @scroll="onScroll"
    >
      <template v-for="item in mergedList" :key="item.id">
        <SuperChatItem
          v-if="isSuperChat(item)"
          :superchat="item"
          @contextmenu="handleContextMenu($event, item)"
        />
        <GiftItem
          v-else
          :gift="item"
          :show-time="settingsStore.giftShowTime"
          :show-medal="settingsStore.giftShowMedal"
          :is-special-follow="settingsStore.isSpecialFollow(item.user.uid)"
          :expired="isGiftExpired(item)"
          @contextmenu="handleContextMenu($event, item)"
        />
      </template>

      <div v-if="mergedList.length === 0" class="empty-state">
        <span class="text">等待礼物中...</span>
      </div>
    </div>

    <!-- 回到底部按钮 -->
    <Transition name="fade">
      <button v-if="!autoScroll" class="scroll-btn" @click="_scrollToBottom">
        ↓ 回到底部
      </button>
    </Transition>

    <ContextMenu ref="contextMenuRef" :items="menuItems" />
  </div>
</template>

<style scoped lang="scss">
.gift-tab {
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

.gift-list {
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

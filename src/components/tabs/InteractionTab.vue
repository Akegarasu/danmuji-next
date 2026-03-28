<script setup lang="ts">
/**
 * 互动 Tab - 弹幕 + 礼物 + SC 合并时间线
 */

import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useDanmakuStore } from '@/stores/danmaku'
import { useSettingsStore } from '@/stores/settings'
import DanmakuItem from '@/components/items/DanmakuItem.vue'
import GiftItem from '@/components/items/GiftItem.vue'
import SuperChatItem from '@/components/items/SuperChatItem.vue'
import ContextMenu from '@/components/common/ContextMenu.vue'
import SilentDialog from '@/components/common/SilentDialog.vue'
import type { MenuItem } from '@/components/common/ContextMenu.vue'
import type { ProcessedDanmaku, ProcessedGift, ProcessedSuperChat, InteractionItem } from '@/types'
import { useAutoScroll } from '@/composables/useAutoScroll'
import { useToast } from '@/composables/useToast'
import { useContextMenuActions } from '@/composables/useContextMenuActions'

const danmakuStore = useDanmakuStore()
const settingsStore = useSettingsStore()

// ==================== Composables ====================

const { showToast, toastMessage, toastType, showToastMessage } = useToast()
const { openUserPage, copyUsername, copyContent, toggleSpecialFollow } = useContextMenuActions(showToastMessage)

// ==================== 礼物过滤与过期 ====================

const nowSeconds = ref(Math.floor(Date.now() / 1000))
let nowTimer: number | null = null

const isGiftExpired = (gift: ProcessedGift): boolean => {
  if (!settingsStore.giftExpireEnabled) return false
  return nowSeconds.value - gift.timestamp > settingsStore.giftExpireMinutes * 60
}

const filteredGiftList = computed(() => {
  return danmakuStore.giftList.filter(gift => {
    if (!settingsStore.giftShowFree && !gift.is_paid) return false
    if (gift.total_value < settingsStore.giftMinPrice) return false
    return true
  })
})

// ==================== 合并时间线（三路归并）====================

// 统一时间戳为秒（弹幕是毫秒，礼物和 SC 是秒）
const toSeconds = (ts: number): number =>
  ts > 1_000_000_000_000 ? Math.floor(ts / 1000) : ts

const mergedTimeline = computed<InteractionItem[]>(() => {
  const danmaku = danmakuStore.danmakuList
  const gifts = filteredGiftList.value
  // SC 列表是 unshift 追加的（新在前），需要反转为升序
  const scs = danmakuStore.superChatList
  const scsLen = scs.length

  const result: InteractionItem[] = []
  let di = 0, gi = 0, si = scsLen - 1 // si 从末尾开始（最旧的）

  while (di < danmaku.length || gi < gifts.length || si >= 0) {
    const dt = di < danmaku.length ? toSeconds(danmaku[di].timestamp) : Infinity
    const gt = gi < gifts.length ? toSeconds(gifts[gi].timestamp) : Infinity
    const st = si >= 0 ? toSeconds(scs[si].start_time) : Infinity

    if (dt <= gt && dt <= st) {
      result.push({ kind: 'danmaku', data: danmaku[di++] })
    } else if (gt <= dt && gt <= st) {
      result.push({ kind: 'gift', data: gifts[gi++] })
    } else {
      result.push({ kind: 'superchat', data: scs[si--] })
    }
  }

  return result
})

// ==================== 自动滚动 ====================

const { listRef, autoScroll, onScroll, scrollToBottom } = useAutoScroll(
  () => mergedTimeline.value.length
)

// ==================== 右键菜单 ====================

const contextMenuRef = ref<InstanceType<typeof ContextMenu>>()

type CurrentItem =
  | { kind: 'danmaku'; data: ProcessedDanmaku }
  | { kind: 'gift'; data: ProcessedGift }
  | { kind: 'superchat'; data: ProcessedSuperChat }

const currentItem = ref<CurrentItem | null>(null)

const isCurrentSpecialFollow = computed(() =>
  currentItem.value ? settingsStore.isSpecialFollow(currentItem.value.data.user.uid) : false
)

// ==================== 禁言弹窗 ====================

const showSilentDialog = ref(false)
const silentDialogRef = ref<InstanceType<typeof SilentDialog>>()

const canSilent = computed(() => {
  if (!currentItem.value) return false
  const cookie = settingsStore.settings.cookie
  const roomIdNum = parseInt(settingsStore.settings.roomId, 10)
  return !!cookie && !!roomIdNum && roomIdNum > 0
})

const openSilentDialog = () => {
  if (!currentItem.value) return
  silentDialogRef.value?.resetAndShow()
  showSilentDialog.value = true
}

const onSilentToast = (msg: string, type: 'success' | 'error' | 'info') => {
  showToastMessage(msg, type)
}

// ==================== 动态菜单项 ====================

const dynamicMenuItems = computed<MenuItem[]>(() => {
  if (!currentItem.value) return []

  const items: MenuItem[] = [
    {
      label: '打开用户主页',
      icon: '🔗',
      action: () => currentItem.value && openUserPage(currentItem.value.data.user.uid)
    },
    {
      label: '复制用户名',
      icon: '📋',
      action: () => currentItem.value && copyUsername(currentItem.value.data.user.name)
    }
  ]

  // 弹幕和 SC 可以复制内容
  if (currentItem.value.kind === 'danmaku') {
    items.push({
      label: '复制弹幕内容',
      icon: '📝',
      action: () => currentItem.value?.kind === 'danmaku' && copyContent(currentItem.value.data.content, '弹幕内容')
    })
  } else if (currentItem.value.kind === 'superchat') {
    items.push({
      label: '复制SC内容',
      icon: '📝',
      action: () => currentItem.value?.kind === 'superchat' && copyContent(currentItem.value.data.content, 'SC内容')
    })
  }

  items.push({ divider: true, label: '', action: () => { } })

  items.push({
    label: isCurrentSpecialFollow.value ? '取消特别关注' : '特别关注',
    icon: '⭐',
    action: () => currentItem.value && toggleSpecialFollow(currentItem.value.data.user.uid, currentItem.value.data.user.name)
  })

  // 禁言（所有类型都支持）
  items.push({
    label: '禁言',
    icon: '🔇',
    disabled: !canSilent.value,
    action: () => openSilentDialog()
  })

  return items
})

// ==================== 右键处理 ====================

const handleContextMenu = (e: MouseEvent, item: CurrentItem) => {
  e.preventDefault()
  e.stopPropagation()
  currentItem.value = item
  contextMenuRef.value?.show(e.clientX, e.clientY)
}

// ==================== 随机提示 ====================

const getRandomTip = () => {
  if (!settingsStore.settings.cookie) {
    return 'Tips: 请先在右上设置内登录账号哦！'
  }

  const tips = [
    'Tips: 右键 TAB 栏可以拆分为独立窗口！',
    'Tips: 锁定窗口后需要在电脑任务栏解锁哦！',
  ]
  return tips[Math.floor(Math.random() * tips.length)]
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
  <div class="interaction-tab">
    <div ref="listRef" class="interaction-list" @scroll="onScroll">
      <template v-for="item in mergedTimeline" :key="item.kind + '-' + item.data.id">
        <DanmakuItem
          v-if="item.kind === 'danmaku'"
          :message="item.data"
          :show-medal="settingsStore.danmakuShowMedal"
          :show-guard="settingsStore.danmakuShowGuard"
          :show-admin="settingsStore.danmakuShowAdmin"
          :show-time="settingsStore.danmakuShowTime"
          :show-guard-border="settingsStore.danmakuShowGuardBorder"
          :emoticon-size="settingsStore.danmakuEmoticonSize"
          :is-special-follow="settingsStore.isSpecialFollow(item.data.user.uid)"
          @contextmenu="handleContextMenu($event, { kind: 'danmaku', data: item.data })"
        />
        <GiftItem
          v-else-if="item.kind === 'gift'"
          :gift="item.data"
          :show-time="settingsStore.giftShowTime"
          :show-medal="settingsStore.giftShowMedal"
          :is-special-follow="settingsStore.isSpecialFollow(item.data.user.uid)"
          :expired="isGiftExpired(item.data)"
          @contextmenu="handleContextMenu($event, { kind: 'gift', data: item.data })"
        />
        <SuperChatItem
          v-else
          :superchat="item.data"
          @contextmenu="handleContextMenu($event, { kind: 'superchat', data: item.data })"
        />
      </template>

      <div v-if="mergedTimeline.length === 0" class="empty-state">
        <span class="icon">✨</span>
        <span class="text">等待互动中...</span>
        <span class="text" style="font-size: var(--font-size-xs); color: var(--text-muted);">{{ getRandomTip() }}</span>
      </div>
    </div>

    <!-- 回到底部按钮 -->
    <Transition name="fade">
      <button v-if="!autoScroll" class="scroll-btn" @click="scrollToBottom">
        ↓ 回到底部
      </button>
    </Transition>

    <ContextMenu ref="contextMenuRef" :items="dynamicMenuItems" />

    <!-- Toast 提示 -->
    <Teleport to="body">
      <Transition name="toast">
        <div v-if="showToast" class="toast" :class="toastType">
          <span class="toast-icon">
            {{ toastType === 'success' ? '✓' : toastType === 'error' ? '✗' : 'i' }}
          </span>
          <span class="toast-text">{{ toastMessage }}</span>
        </div>
      </Transition>
    </Teleport>

    <!-- 禁言弹窗 -->
    <SilentDialog
      ref="silentDialogRef"
      v-model:visible="showSilentDialog"
      :user-name="currentItem?.data.user.name ?? ''"
      :user-uid="currentItem?.data.user.uid ?? 0"
      @toast="onSilentToast"
    />
  </div>
</template>

<style scoped lang="scss">
.interaction-tab {
  height: 100%;
  display: flex;
  flex-direction: column;
  position: relative;
  overflow: hidden;
}

.interaction-list {
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

  .icon {
    font-size: 32px;
    opacity: 0.5;
  }

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

// ==================== 过渡动画 ====================

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

// ==================== Toast 提示 ====================

.toast {
  position: fixed;
  top: 48px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 10001;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 20px;
  border-radius: var(--border-radius);
  font-size: var(--font-size-sm);
  font-weight: 500;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  pointer-events: none;

  &.success {
    background: rgba(34, 197, 94, 0.95);
    color: white;
  }

  &.error {
    background: rgba(239, 68, 68, 0.95);
    color: white;
  }

  &.info {
    background: rgba(92, 158, 255, 0.95);
    color: white;
  }
}

.toast-icon {
  font-size: 14px;
  font-weight: 700;
}

.toast-text {
  white-space: nowrap;
}

.toast-enter-active {
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

.toast-leave-active {
  transition: all 0.2s ease-in;
}

.toast-enter-from {
  opacity: 0;
  transform: translateX(-50%) translateY(-12px);
}

.toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(-8px);
}
</style>

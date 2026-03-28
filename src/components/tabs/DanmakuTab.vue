<script setup lang="ts">
import { ref, computed } from 'vue'
import { useDanmakuStore } from '@/stores/danmaku'
import { useSettingsStore } from '@/stores/settings'
import DanmakuItem from '@/components/items/DanmakuItem.vue'
import ContextMenu from '@/components/common/ContextMenu.vue'
import SilentDialog from '@/components/common/SilentDialog.vue'
import type { MenuItem } from '@/components/common/ContextMenu.vue'
import type { ProcessedDanmaku } from '@/types'
import { useAutoScroll } from '@/composables/useAutoScroll'
import { useToast } from '@/composables/useToast'
import { useContextMenuActions } from '@/composables/useContextMenuActions'

const danmakuStore = useDanmakuStore()
const settingsStore = useSettingsStore()

// ==================== Composables ====================

const { listRef, autoScroll, onScroll, scrollToBottom } = useAutoScroll(
  () => danmakuStore.danmakuList.length
)
const { showToast, toastMessage, toastType, showToastMessage } = useToast()
const { openUserPage, copyUsername, copyContent, toggleSpecialFollow } = useContextMenuActions(showToastMessage)

// ==================== 右键菜单 ====================

const contextMenuRef = ref<InstanceType<typeof ContextMenu>>()
const currentDanmaku = ref<ProcessedDanmaku | null>(null)

const isCurrentSpecialFollow = computed(() =>
  currentDanmaku.value ? settingsStore.isSpecialFollow(currentDanmaku.value.user.uid) : false
)

// ==================== 禁言弹窗 ====================

const showSilentDialog = ref(false)
const silentDialogRef = ref<InstanceType<typeof SilentDialog>>()

const canSilent = computed(() => {
  if (!currentDanmaku.value) return false
  const cookie = settingsStore.settings.cookie
  const roomIdNum = parseInt(settingsStore.settings.roomId, 10)
  return !!cookie && !!roomIdNum && roomIdNum > 0
})

const openSilentDialog = () => {
  if (!currentDanmaku.value) return
  silentDialogRef.value?.resetAndShow()
  showSilentDialog.value = true
}

const onSilentToast = (msg: string, type: 'success' | 'error' | 'info') => {
  showToastMessage(msg, type)
}

// ==================== 菜单项 ====================

const menuItems = computed<MenuItem[]>(() => ([
  {
    label: '打开用户主页',
    icon: '🔗',
    action: () => currentDanmaku.value && openUserPage(currentDanmaku.value.user.uid)
  },
  {
    label: '复制用户名',
    icon: '📋',
    action: () => currentDanmaku.value && copyUsername(currentDanmaku.value.user.name)
  },
  {
    label: '复制弹幕内容',
    icon: '📝',
    action: () => currentDanmaku.value && copyContent(currentDanmaku.value.content, '弹幕内容')
  },
  { divider: true, label: '', action: () => { } },
  {
    label: isCurrentSpecialFollow.value ? '取消特别关注' : '特别关注',
    icon: '⭐',
    action: () => currentDanmaku.value && toggleSpecialFollow(currentDanmaku.value.user.uid, currentDanmaku.value.user.name)
  },
  {
    label: '禁言',
    icon: '🔇',
    disabled: !canSilent.value,
    action: () => openSilentDialog()
  }
]))

const handleContextMenu = (e: MouseEvent, msg: ProcessedDanmaku) => {
  e.preventDefault()
  e.stopPropagation()
  currentDanmaku.value = msg
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
</script>

<template>
  <div class="danmaku-tab">
    <div ref="listRef" class="danmaku-list" @scroll="onScroll">
      <DanmakuItem v-for="msg in danmakuStore.danmakuList" :key="msg.id" :message="msg"
        :show-medal="settingsStore.danmakuShowMedal" :show-guard="settingsStore.danmakuShowGuard"
        :show-admin="settingsStore.danmakuShowAdmin" :show-time="settingsStore.danmakuShowTime"
        :show-guard-border="settingsStore.danmakuShowGuardBorder" :emoticon-size="settingsStore.danmakuEmoticonSize"
        :is-special-follow="settingsStore.isSpecialFollow(msg.user.uid)"
        @contextmenu="handleContextMenu($event, msg)" />

      <div v-if="danmakuStore.danmakuList.length === 0" class="empty-state">
        <span class="icon">💬</span>
        <span class="text">等待弹幕中...</span>
        <span class="text" style="font-size: var(--font-size-xs); color: var(--text-muted);">{{ getRandomTip() }}</span>
      </div>
    </div>

    <!-- 回到底部按钮 -->
    <Transition name="fade">
      <button v-if="!autoScroll" class="scroll-btn" @click="scrollToBottom">
        ↓ 回到底部
      </button>
    </Transition>

    <ContextMenu ref="contextMenuRef" :items="menuItems" />

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
      :user-name="currentDanmaku?.user.name ?? ''"
      :user-uid="currentDanmaku?.user.uid ?? 0"
      @toast="onSilentToast"
    />
  </div>
</template>

<style scoped lang="scss">
.danmaku-tab {
  height: 100%;
  display: flex;
  flex-direction: column;
  position: relative;
  overflow: hidden;
}

.danmaku-list {
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

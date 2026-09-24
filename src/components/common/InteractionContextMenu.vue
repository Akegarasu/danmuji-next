<script setup lang="ts">
/** 互动页与归档弹幕共用的右键菜单。 */
import { computed, ref } from 'vue'
import ContextMenu from '@/components/common/ContextMenu.vue'
import SilentDialog from '@/components/common/SilentDialog.vue'
import type { MenuItem } from '@/components/common/ContextMenu.vue'
import type { ProcessedUser } from '@/types'
import { useSettingsStore } from '@/stores/settings'
import { useToast } from '@/composables/useToast'
import { useContextMenuActions } from '@/composables/useContextMenuActions'

const settingsStore = useSettingsStore()
const { showToast, toastMessage, toastType, showToastMessage } = useToast()
const { openUserPage, copyUsername, copyContent, toggleSpecialFollow } = useContextMenuActions(showToastMessage)

const contextMenuRef = ref<InstanceType<typeof ContextMenu>>()

type MenuUser = Pick<ProcessedUser, 'uid' | 'name' | 'face'>

type CurrentItem =
  | { kind: 'danmaku' | 'superchat'; data: { user: MenuUser; content: string } }
  | { kind: 'gift'; data: { user: MenuUser } }

const currentItem = ref<CurrentItem | null>(null)
const selectedRoomId = ref<number>()
const targetRoomId = computed(() => selectedRoomId.value ?? parseInt(settingsStore.settings.roomId, 10))

const isCurrentSpecialFollow = computed(() =>
  currentItem.value ? settingsStore.isSpecialFollow(currentItem.value.data.user.uid) : false
)

// ==================== 禁言弹窗 ====================

const showSilentDialog = ref(false)
const silentDialogRef = ref<InstanceType<typeof SilentDialog>>()

const canSilent = computed(() => {
  if (!currentItem.value) return false
  const cookie = settingsStore.settings.cookie
  const roomIdNum = targetRoomId.value
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

  const currentUser = currentItem.value.data.user
  const items: MenuItem[] = [
    {
      label: currentUser.name,
      avatar: currentUser.face ?? null,
      children: [
        {
          label: '打开用户主页',
          icon: '🔗',
          action: () => currentItem.value && openUserPage(currentItem.value.data.user.uid)
        },
        {
          label: '复制用户名',
          icon: '📋',
          action: () => currentItem.value && copyUsername(currentItem.value.data.user.name)
        },
        {
          label: '复制UID',
          icon: '🪪',
          action: () => currentItem.value && copyContent(String(currentItem.value.data.user.uid), 'UID')
        },
        {
          label: isCurrentSpecialFollow.value ? '取消特别关注' : '特别关注',
          icon: '⭐',
          action: () => currentItem.value && toggleSpecialFollow(currentItem.value.data.user.uid, currentItem.value.data.user.name)
        },
        {
          label: '禁言',
          icon: '🔇',
          disabled: !canSilent.value,
          action: () => openSilentDialog()
        }
      ]
    }
  ]

  // 弹幕和 SC 可以复制内容
  if (currentItem.value.kind === 'danmaku') {
    items.push({
      label: '复制弹幕',
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

  return items
})

// ==================== 右键处理 ====================

const show = (e: MouseEvent, item: CurrentItem, roomId?: number) => {
  e.preventDefault()
  e.stopPropagation()
  if (showSilentDialog.value) return
  selectedRoomId.value = roomId
  currentItem.value = item
  contextMenuRef.value?.show(e.clientX, e.clientY)
}

defineExpose({ show })
</script>

<template>
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
    :room-id="targetRoomId"
    @toast="onSilentToast"
  />
</template>

<style scoped lang="scss">
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

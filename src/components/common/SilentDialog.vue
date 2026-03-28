<script setup lang="ts">
/**
 * 禁言弹窗组件
 * 提取自 DanmakuTab 的禁言功能
 */

import { ref, computed } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import { addSilentUser, type SilentDuration } from '@/services/blive-client'

const props = defineProps<{
  visible: boolean
  userName: string
  userUid: number
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
  'toast': [msg: string, type: 'success' | 'error' | 'info']
}>()

const settingsStore = useSettingsStore()

const silentDuration = ref<SilentDuration>('scene')
const silentReason = ref('')
const silentSubmitting = ref(false)

const silentDurationOptions: { value: SilentDuration; label: string }[] = [
  { value: 'scene', label: '仅本场' },
  { value: '2h', label: '2 小时' },
  { value: '4h', label: '4 小时' },
  { value: '24h', label: '24 小时' },
  { value: '7d', label: '7 天' },
  { value: 'forever', label: '永久' }
]

const canSilent = computed(() => {
  const cookie = settingsStore.settings.cookie
  const roomIdNum = parseInt(settingsStore.settings.roomId, 10)
  return !!cookie && !!roomIdNum && roomIdNum > 0
})

const close = () => {
  if (silentSubmitting.value) return
  emit('update:visible', false)
}

const confirm = async () => {
  const cookie = settingsStore.settings.cookie
  const roomIdNum = parseInt(settingsStore.settings.roomId, 10)

  if (!cookie || !roomIdNum || roomIdNum <= 0) {
    emit('toast', '缺少 Cookie 或房间号，无法禁言', 'error')
    return
  }

  silentSubmitting.value = true
  try {
    const res = await addSilentUser({
      roomId: roomIdNum,
      tuid: props.userUid,
      cookie,
      duration: silentDuration.value,
      msg: silentReason.value || undefined
    })

    if (res.success) {
      emit('toast', `已禁言 ${props.userName}`, 'success')
    } else {
      emit('toast', `禁言失败（${res.code}）：${res.message}`, 'error')
    }
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    emit('toast', `禁言请求失败：${msg}`, 'error')
  } finally {
    silentSubmitting.value = false
    emit('update:visible', false)
  }
}

const resetAndShow = () => {
  silentDuration.value = 'scene'
  silentReason.value = ''
}

defineExpose({ canSilent, resetAndShow })
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog">
      <div v-if="visible" class="dialog-mask" @mousedown.self="close">
        <div class="dialog-card" @mousedown.stop>
          <div class="dialog-header">
            <span class="dialog-title">禁言用户</span>
            <button class="dialog-close" @click="close" :disabled="silentSubmitting">×</button>
          </div>

          <div class="dialog-body">
            <div class="dialog-user">
              <span class="dialog-user-name">{{ userName }}</span>
              <span class="dialog-user-uid">UID: {{ userUid }}</span>
            </div>

            <div class="dialog-field">
              <label class="dialog-label">禁言时长</label>
              <div class="duration-grid">
                <button v-for="opt in silentDurationOptions" :key="opt.value" class="duration-option"
                  :class="{ active: silentDuration === opt.value, danger: opt.value === 'forever' }"
                  :disabled="silentSubmitting" @click="silentDuration = opt.value">
                  {{ opt.label }}
                </button>
              </div>
            </div>

            <div class="dialog-field">
              <label class="dialog-label">禁言原因 <span class="optional">（选填）</span></label>
              <input v-model="silentReason" class="dialog-input" placeholder="输入禁言原因" :disabled="silentSubmitting"
                @keydown.enter="confirm" />
            </div>
          </div>

          <div class="dialog-footer">
            <button class="dialog-btn" @click="close" :disabled="silentSubmitting">
              取消
            </button>
            <button class="dialog-btn primary" @click="confirm" :disabled="silentSubmitting">
              {{ silentSubmitting ? '处理中...' : '确认禁言' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped lang="scss">
.dialog-mask {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  z-index: 10000;
  display: flex;
  align-items: center;
  justify-content: center;
}

.dialog-card {
  width: 360px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius);
  overflow: hidden;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
}

.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 16px 10px;
  border-bottom: 1px solid var(--border-color);
}

.dialog-title {
  font-size: var(--font-size-base);
  font-weight: 600;
  color: var(--text-primary);
}

.dialog-close {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: var(--text-muted);
  font-size: 18px;
  cursor: pointer;
  border-radius: var(--border-radius-sm);
  transition: background 0.15s, color 0.15s;

  &:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  &:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
}

.dialog-body {
  padding: 16px;
}

.dialog-user {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  background: var(--bg-card);
  border-radius: var(--border-radius-sm);
  margin-bottom: 16px;
}

.dialog-user-name {
  font-size: var(--font-size-sm);
  font-weight: 500;
  color: var(--text-primary);
}

.dialog-user-uid {
  font-size: var(--font-size-xs);
  color: var(--text-muted);
}

.dialog-field {
  margin-bottom: 14px;

  &:last-child {
    margin-bottom: 0;
  }
}

.dialog-label {
  display: block;
  font-size: var(--font-size-sm);
  color: var(--text-secondary);
  margin-bottom: 8px;

  .optional {
    color: var(--text-muted);
    font-size: var(--font-size-xs);
  }
}

.duration-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 6px;
}

.duration-option {
  padding: 8px 0;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-secondary);
  font-size: var(--font-size-sm);
  cursor: pointer;
  transition: all 0.15s;
  text-align: center;

  &:hover:not(:disabled):not(.active) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  &.active {
    background: var(--accent-primary);
    border-color: var(--accent-primary);
    color: white;
    font-weight: 500;
  }

  &.danger.active {
    background: rgba(239, 68, 68, 0.85);
    border-color: rgba(239, 68, 68, 0.85);
  }

  &:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
}

.dialog-input {
  width: 100%;
  padding: 8px 12px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  color: var(--text-primary);
  font-size: var(--font-size-sm);
  outline: none;
  transition: border-color 0.2s;

  &:focus {
    border-color: var(--accent-primary);
  }

  &::placeholder {
    color: var(--text-muted);
  }

  &:disabled {
    opacity: 0.5;
  }
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 16px;
  border-top: 1px solid var(--border-color);
}

.dialog-btn {
  padding: 8px 18px;
  background: var(--bg-active);
  color: var(--text-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  font-size: var(--font-size-sm);
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;

  &:hover:not(:disabled) {
    background: var(--bg-hover);
  }

  &:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  &.primary {
    background: var(--accent-primary);
    border-color: var(--accent-primary);
    color: white;

    &:hover:not(:disabled) {
      opacity: 0.9;
    }
  }
}

// 弹窗进出动画
.dialog-enter-active {
  transition: opacity 0.2s ease;

  .dialog-card {
    transition: transform 0.2s cubic-bezier(0.4, 0, 0.2, 1), opacity 0.2s ease;
  }
}

.dialog-leave-active {
  transition: opacity 0.15s ease;

  .dialog-card {
    transition: transform 0.15s ease-in, opacity 0.15s ease;
  }
}

.dialog-enter-from {
  opacity: 0;

  .dialog-card {
    opacity: 0;
    transform: scale(0.95) translateY(-8px);
  }
}

.dialog-leave-to {
  opacity: 0;

  .dialog-card {
    opacity: 0;
    transform: scale(0.97) translateY(4px);
  }
}
</style>

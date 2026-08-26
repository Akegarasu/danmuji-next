<script setup lang="ts">
const props = withDefaults(defineProps<{
  visible: boolean
  title?: string
  message: string
  confirmText?: string
  cancelText?: string
  loadingText?: string
  showCancel?: boolean
  danger?: boolean
  loading?: boolean
  closeOnConfirm?: boolean
}>(), {
  title: '确认',
  confirmText: '确定',
  cancelText: '取消',
  loadingText: '处理中…',
  showCancel: true,
  danger: false,
  loading: false,
  closeOnConfirm: true
})

const emit = defineEmits<{
  (e: 'update:visible', value: boolean): void
  (e: 'confirm'): void
  (e: 'cancel'): void
}>()

const close = () => {
  if (props.loading) return
  emit('update:visible', false)
  emit('cancel')
}

const confirm = () => {
  if (props.loading) return
  if (props.closeOnConfirm) emit('update:visible', false)
  emit('confirm')
}
</script>

<template>
  <Teleport to="body">
    <Transition name="confirm-fade">
      <div v-if="visible" class="confirm-overlay" @click.self="close">
        <div class="confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="confirm-dialog-title">
          <div class="confirm-header">
            <h3 id="confirm-dialog-title">{{ title }}</h3>
          </div>
          <div class="confirm-body">
            {{ message }}
          </div>
          <div class="confirm-footer">
            <button v-if="showCancel" class="confirm-btn cancel" :disabled="loading" @click="close">{{ cancelText }}</button>
            <button
              class="confirm-btn"
              :class="danger ? 'danger' : 'primary'"
              :disabled="loading"
              @click="confirm"
            >
              <span v-if="loading" class="button-spinner" aria-hidden="true" />
              {{ loading ? loadingText : confirmText }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped lang="scss">
.confirm-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
  backdrop-filter: blur(2px);
}

.confirm-dialog {
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  width: 300px;
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.25);
  overflow: hidden;
  animation: dialogIn 0.15s ease;
}

@keyframes dialogIn {
  from {
    opacity: 0;
    transform: scale(0.95) translateY(-4px);
  }
  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}

.confirm-header {
  padding: 16px 20px 0;

  h3 {
    margin: 0;
    font-size: var(--font-size-base, 14px);
    font-weight: 600;
    color: var(--text-primary);
  }
}

.confirm-body {
  padding: 12px 20px 20px;
  font-size: var(--font-size-sm, 13px);
  color: var(--text-secondary);
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
}

.confirm-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 0 16px 16px;
}

.confirm-btn {
  padding: 6px 18px;
  border-radius: 6px;
  font-size: var(--font-size-sm, 13px);
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;

  &:disabled {
    cursor: wait;
    opacity: 0.65;
  }

  &.cancel {
    background: transparent;
    border: 1px solid var(--border-color);
    color: var(--text-secondary);

    &:hover:not(:disabled) {
      background: var(--bg-hover);
      color: var(--text-primary);
    }
  }

  &.primary {
    background: var(--accent-primary);
    border: none;
    color: white;

    &:hover:not(:disabled) {
      opacity: 0.9;
    }
  }

  &.danger {
    background: #ef4444;
    border: none;
    color: white;

    &:hover:not(:disabled) {
      background: #dc2626;
    }
  }
}

.button-spinner {
  display: inline-block;
  width: 11px;
  height: 11px;
  margin-right: 5px;
  vertical-align: -1px;
  border: 2px solid currentColor;
  border-right-color: transparent;
  border-radius: 50%;
  animation: confirmSpin 0.7s linear infinite;
}

@keyframes confirmSpin {
  to { transform: rotate(360deg); }
}

.confirm-fade-enter-active,
.confirm-fade-leave-active {
  transition: opacity 0.15s ease;
}

.confirm-fade-enter-from,
.confirm-fade-leave-to {
  opacity: 0;
}
</style>

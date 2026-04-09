<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { downloadAndInstall, type UpdateInfo, type DownloadProgress } from '@/services/updater'

const props = defineProps<{
  updateInfo: UpdateInfo
}>()

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'show-changelog'): void
}>()

// 状态
const isDownloading = ref(false)
const progress = ref<DownloadProgress>({ downloaded: 0, total: null })
const downloadError = ref('')

// 自动关闭计时器
let autoCloseTimer: ReturnType<typeof setTimeout> | null = null
const AUTO_CLOSE_DELAY = 10_000

const startAutoClose = () => {
  autoCloseTimer = setTimeout(() => {
    if (!isDownloading.value) {
      emit('close')
    }
  }, AUTO_CLOSE_DELAY)
}

const cancelAutoClose = () => {
  if (autoCloseTimer) {
    clearTimeout(autoCloseTimer)
    autoCloseTimer = null
  }
}

onMounted(() => {
  startAutoClose()
})

onUnmounted(() => {
  cancelAutoClose()
})

// 进度文本
const progressText = computed(() => {
  const dl = (progress.value.downloaded / 1024 / 1024).toFixed(1)
  if (progress.value.total) {
    const total = (progress.value.total / 1024 / 1024).toFixed(1)
    const pct = Math.round((progress.value.downloaded / progress.value.total) * 100)
    return `${dl}/${total} MB (${pct}%)`
  }
  return `${dl} MB`
})

const progressPercent = computed(() => {
  if (!progress.value.total) return 0
  return Math.min(100, Math.round((progress.value.downloaded / progress.value.total) * 100))
})

// 点击更新
async function doUpdate() {
  cancelAutoClose()
  isDownloading.value = true
  downloadError.value = ''

  try {
    await downloadAndInstall((p) => {
      progress.value = p
    })
  } catch (e) {
    console.error('[updater] 下载安装失败:', e)
    downloadError.value = String(e)
    isDownloading.value = false
    // 失败后重新启动自动关闭
    startAutoClose()
  }
}
</script>

<template>
  <Transition name="update-toast">
    <div class="update-toast" @mouseenter="cancelAutoClose" @mouseleave="startAutoClose">
      <!-- 下载中：进度条模式 -->
      <template v-if="isDownloading">
        <div class="toast-content">
          <span class="toast-text">正在下载 v{{ props.updateInfo.version }}</span>
          <span class="toast-progress-text">{{ progressText }}</span>
        </div>
        <div class="toast-progress-bar">
          <div class="toast-progress-fill" :style="{ width: progressPercent + '%' }"></div>
        </div>
      </template>

      <!-- 错误提示 -->
      <template v-else-if="downloadError">
        <div class="toast-content">
          <span class="toast-text toast-error-text">更新失败，请稍后重试</span>
          <button class="toast-btn toast-close-btn" @click="emit('close')" title="关闭">&times;</button>
        </div>
      </template>

      <!-- 默认：提示更新 -->
      <template v-else>
        <div class="toast-content">
          <span class="toast-text">检测到弹幕姬更新 v{{ props.updateInfo.version }}</span>
          <div class="toast-actions">
            <button class="toast-btn toast-changelog-btn" @click="emit('show-changelog')">日志</button>
            <button class="toast-btn toast-update-btn" @click="doUpdate">更新</button>
            <button class="toast-btn toast-close-btn" @click="emit('close')" title="关闭">&times;</button>
          </div>
        </div>
      </template>
    </div>
  </Transition>
</template>

<style scoped lang="scss">
.update-toast {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  background: var(--bg-secondary);
  border-top: 1px solid var(--border-color);
  z-index: 1000;
  overflow: hidden;
}

.toast-content {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 12px;
  gap: 8px;
  min-height: 32px;
}

.toast-text {
  font-size: var(--font-size-sm);
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.toast-error-text {
  color: #ef4444;
}

.toast-progress-text {
  font-size: var(--font-size-xs);
  color: var(--text-muted);
  white-space: nowrap;
  flex-shrink: 0;
}

.toast-actions {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}

.toast-btn {
  background: none;
  border: none;
  cursor: pointer;
  font-size: var(--font-size-sm);
  padding: 2px 8px;
  border-radius: var(--border-radius-sm);
  transition: background 0.15s, color 0.15s;
}

.toast-changelog-btn {
  color: var(--text-muted);

  &:hover {
    color: var(--text-secondary);
    background: var(--bg-hover);
  }
}

.toast-update-btn {
  color: var(--accent-primary);
  font-weight: 500;

  &:hover {
    background: rgba(92, 158, 255, 0.1);
  }
}

.toast-close-btn {
  color: var(--text-muted);
  font-size: 16px;
  line-height: 1;
  padding: 2px 4px;

  &:hover {
    color: var(--text-secondary);
    background: var(--bg-hover);
  }
}

.toast-progress-bar {
  height: 2px;
  background: var(--bg-hover);
}

.toast-progress-fill {
  height: 100%;
  background: var(--accent-primary);
  transition: width 0.3s ease;
}

// 动画
.update-toast-enter-active {
  animation: slideUp 0.25s ease-out;
}

.update-toast-leave-active {
  animation: slideUp 0.2s ease-in reverse;
}

@keyframes slideUp {
  from {
    transform: translateY(100%);
    opacity: 0;
  }
  to {
    transform: translateY(0);
    opacity: 1;
  }
}
</style>

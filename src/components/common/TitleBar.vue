<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { exit } from '@tauri-apps/plugin-process'
import { createSettingsWindow, createArchiveWindow, closeWindow } from '@/services/window-manager'
import { useDanmakuStore } from '@/stores/danmaku'

const props = withDefaults(defineProps<{
  title: string
  showSettingsBtn?: boolean
  isSubWindow?: boolean
  windowLabel?: string
  showLockBtn?: boolean
}>(), {
  showSettingsBtn: false,
  isSubWindow: false,
  windowLabel: '',
  showLockBtn: false
})

const emit = defineEmits<{
  (e: 'lock-change', locked: boolean): void
}>()

const danmakuStore = useDanmakuStore()

const appWindow = getCurrentWindow()
const currentLabel = props.windowLabel || appWindow.label

// 是否为主窗口
const isMainWindow = computed(() => currentLabel === 'main')

// 锁定状态（鼠标穿透）
const isLocked = ref(false)
let lockUnlisten: UnlistenFn | null = null

// 连接状态显示
const isConnected = computed(() => danmakuStore.isConnected)
const onlineCount = computed(() => danmakuStore.stats.online_count)
const roomTitle = computed(() => danmakuStore.roomInfo.title)

// 状态显示文本
const statusDisplay = computed(() => {
  if (!isConnected.value) return '未连接'
  if (onlineCount.value > 0) {
    return `👥 ${onlineCount.value}`
  }
  return '已连接'
})

const minimize = () => appWindow.minimize()

const close = async () => {
  if (isMainWindow.value) {
    // 主窗口关闭时退出整个应用
    await exit(0)
  } else if (props.isSubWindow) {
    try {
      await closeWindow(currentLabel)
    } catch (e) {
      await appWindow.close()
    }
  } else {
    await appWindow.close()
  }
}

const openSettings = async () => {
  try {
    await createSettingsWindow()
  } catch (e) {
    console.error('Failed to open settings:', e)
  }
}

const openArchive = async () => {
  try {
    await createArchiveWindow()
  } catch (e) {
    console.error('Failed to open archive:', e)
  }
}

// 切换锁定状态（通过后端命令）
const toggleLock = async () => {
  console.log('toggleLock', isLocked.value)
  console.log('currentLabel', currentLabel)
  try {
    if (isLocked.value) {
      // 当前是锁定状态，解锁
      await invoke('unlock_window', { label: currentLabel })
    } else {
      // 当前是解锁状态，锁定
      await invoke('lock_window', { label: currentLabel })
    }
    // 状态会通过 window-lock-change 事件更新
  } catch (e) {
    console.error('Failed to toggle lock:', e)
  }
}

// 初始化锁定状态监听
const initLockListener = async () => {
  // 获取初始锁定状态
  try {
    const savedLockState = await invoke<boolean>('get_window_lock_state', { label: currentLabel })
    isLocked.value = savedLockState

    // 如果之前是锁定状态，重新应用鼠标穿透
    if (savedLockState) {
      await invoke('lock_window', { label: currentLabel })
      emit('lock-change', true)
    }
  } catch (e) {
    console.error('Failed to get lock state:', e)
  }

  // 监听锁定状态变化事件（托盘解锁时会触发）
  // 使用窗口标签作为事件名后缀，确保只接收发给当前窗口的事件
  const eventName = `window-lock-change:${currentLabel}`
  lockUnlisten = await listen<boolean>(eventName, (event) => {
    isLocked.value = event.payload
    emit('lock-change', event.payload)
  })
}

onMounted(() => {
  if (props.showLockBtn) {
    initLockListener()
  }
})

onUnmounted(() => {
  if (lockUnlisten) {
    lockUnlisten()
  }
})
</script>

<template>
  <div class="title-bar" data-tauri-drag-region>
    <div class="title-section" data-tauri-drag-region>
      <span class="title">{{ title }}</span>

      <!-- 主窗口显示连接状态和人数 -->
      <template v-if="isMainWindow">
        <span class="status-badge" :class="{ connected: isConnected }">
          <span class="status-dot">●</span>
          <span class="status-text">{{ statusDisplay }}</span>
        </span>
        <span v-if="isConnected && roomTitle" class="room-title" :title="roomTitle">
          {{ roomTitle.length > 15 ? roomTitle.slice(0, 15) + '...' : roomTitle }}
        </span>
      </template>
    </div>

    <div class="controls">
      <!-- 锁定按钮 -->
      <button v-if="showLockBtn" class="icon-btn lock-btn" :class="{ locked: isLocked }" @click="toggleLock"
        :title="isLocked ? '点击托盘图标解锁' : '锁定窗口（鼠标穿透）'">
        <svg v-if="isLocked" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
          <path d="M7 11V7a5 5 0 0 1 10 0v4" />
        </svg>
        <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
          <path d="M7 11V7a5 5 0 0 1 9.9-1" />
        </svg>
      </button>
      <button v-if="showSettingsBtn" class="icon-btn archive-btn" @click="openArchive" title="存档">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 8v13H3V8" />
          <rect x="1" y="3" width="22" height="5" />
          <line x1="10" y1="12" x2="14" y2="12" />
        </svg>
      </button>
      <button v-if="showSettingsBtn" class="icon-btn settings-btn" @click="openSettings" title="设置">
        <svg viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
          <g id="SVGRepo_bgCarrier" stroke-width="0"></g>
          <g id="SVGRepo_tracerCarrier" stroke-linecap="round" stroke-linejoin="round"></g>
          <g id="SVGRepo_iconCarrier">
            <circle cx="12" cy="12" r="3" stroke="currentColor" stroke-width="2"></circle>
            <path
              d="M13.7654 2.15224C13.3978 2 12.9319 2 12 2C11.0681 2 10.6022 2 10.2346 2.15224C9.74457 2.35523 9.35522 2.74458 9.15223 3.23463C9.05957 3.45834 9.0233 3.7185 9.00911 4.09799C8.98826 4.65568 8.70226 5.17189 8.21894 5.45093C7.73564 5.72996 7.14559 5.71954 6.65219 5.45876C6.31645 5.2813 6.07301 5.18262 5.83294 5.15102C5.30704 5.08178 4.77518 5.22429 4.35436 5.5472C4.03874 5.78938 3.80577 6.1929 3.33983 6.99993C2.87389 7.80697 2.64092 8.21048 2.58899 8.60491C2.51976 9.1308 2.66227 9.66266 2.98518 10.0835C3.13256 10.2756 3.3397 10.437 3.66119 10.639C4.1338 10.936 4.43789 11.4419 4.43786 12C4.43783 12.5581 4.13375 13.0639 3.66118 13.3608C3.33965 13.5629 3.13248 13.7244 2.98508 13.9165C2.66217 14.3373 2.51966 14.8691 2.5889 15.395C2.64082 15.7894 2.87379 16.193 3.33973 17C3.80568 17.807 4.03865 18.2106 4.35426 18.4527C4.77508 18.7756 5.30694 18.9181 5.83284 18.8489C6.07289 18.8173 6.31632 18.7186 6.65204 18.5412C7.14547 18.2804 7.73556 18.27 8.2189 18.549C8.70224 18.8281 8.98826 19.3443 9.00911 19.9021C9.02331 20.2815 9.05957 20.5417 9.15223 20.7654C9.35522 21.2554 9.74457 21.6448 10.2346 21.8478C10.6022 22 11.0681 22 12 22C12.9319 22 13.3978 22 13.7654 21.8478C14.2554 21.6448 14.6448 21.2554 14.8477 20.7654C14.9404 20.5417 14.9767 20.2815 14.9909 19.902C15.0117 19.3443 15.2977 18.8281 15.781 18.549C16.2643 18.2699 16.8544 18.2804 17.3479 18.5412C17.6836 18.7186 17.927 18.8172 18.167 18.8488C18.6929 18.9181 19.2248 18.7756 19.6456 18.4527C19.9612 18.2105 20.1942 17.807 20.6601 16.9999C21.1261 16.1929 21.3591 15.7894 21.411 15.395C21.4802 14.8691 21.3377 14.3372 21.0148 13.9164C20.8674 13.7243 20.6602 13.5628 20.3387 13.3608C19.8662 13.0639 19.5621 12.558 19.5621 11.9999C19.5621 11.4418 19.8662 10.9361 20.3387 10.6392C20.6603 10.4371 20.8675 10.2757 21.0149 10.0835C21.3378 9.66273 21.4803 9.13087 21.4111 8.60497C21.3592 8.21055 21.1262 7.80703 20.6602 7C20.1943 6.19297 19.9613 5.78945 19.6457 5.54727C19.2249 5.22436 18.693 5.08185 18.1671 5.15109C17.9271 5.18269 17.6837 5.28136 17.3479 5.4588C16.8545 5.71959 16.2644 5.73002 15.7811 5.45096C15.2977 5.17191 15.0117 4.65566 14.9909 4.09794C14.9767 3.71848 14.9404 3.45833 14.8477 3.23463C14.6448 2.74458 14.2554 2.35523 13.7654 2.15224Z"
              stroke="currentColor" stroke-width="2"></path>
          </g>
        </svg>
      </button>
      <button class="icon-btn" @click="minimize" title="最小化">
        ─
      </button>
      <button class="icon-btn danger" @click="close" title="关闭">
        ✕
      </button>
    </div>
  </div>
</template>

<style scoped lang="scss">
.title-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: var(--title-bar-height);
  padding: 0 8px 0 12px;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  -webkit-app-region: drag;
}

.title-section {
  display: flex;
  align-items: center;
  gap: 10px;
  overflow: hidden;
}

.title {
  font-size: var(--font-size-sm);
  font-weight: 500;
  color: var(--text-primary);
  flex-shrink: 0;
}

.status-badge {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  background: var(--bg-card);
  border-radius: 10px;
  font-size: var(--font-size-xs);
  color: var(--text-muted);

  .status-dot {
    font-size: 8px;
    animation: pulse 2s infinite;
  }

  &.connected {
    background: rgba(34, 197, 94, 0.15);
    color: #22c55e;

    .status-dot {
      color: #22c55e;
    }
  }
}

.room-title {
  font-size: var(--font-size-xs);
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 150px;
}

@keyframes pulse {

  0%,
  100% {
    opacity: 1;
  }

  50% {
    opacity: 0.5;
  }
}

.controls {
  display: flex;
  gap: 4px;
  -webkit-app-region: no-drag;
  flex-shrink: 0;

  .icon-btn {
    width: 28px;
    height: 28px;
    font-size: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    border-radius: var(--border-radius-sm);
    transition: background 0.15s, color 0.15s;

    &:hover {
      background: var(--bg-hover);
      color: var(--text-primary);
    }

    &.danger:hover {
      background: rgba(220, 60, 60, 0.3);
      color: #dc3c3c;
    }

    &.settings-btn svg {
      width: 14px;
      height: 14px;
    }

    &.archive-btn svg {
      width: 14px;
      height: 14px;
    }

    &.lock-btn {
      svg {
        width: 14px;
        height: 14px;
      }

      &.locked {
        background: rgba(59, 130, 246, 0.2);
        color: #3b82f6;

        &:hover {
          background: rgba(59, 130, 246, 0.3);
        }
      }
    }
  }
}
</style>

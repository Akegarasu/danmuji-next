<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useSettingsStore } from '@/stores/settings'
import { initSettingsApplier } from '@/services/settings-applier'
import { initSettingsSync } from '@/services/settings-sync'
import { 
  initWindowManager, 
  cleanupWindowManager,
  restorePreviouslyOpenWindows 
} from '@/services/window-manager'
import { 
  initBliveClient, 
  cleanupBliveClient,
  autoConnect 
} from '@/services/blive-client'
import { ALL_EVENT_TYPES } from '@/types'

const settingsStore = useSettingsStore()

onMounted(async () => {
  const appWindow = getCurrentWindow()
  const isMainWindow = appWindow.label === 'main'
  
  // 1. 从后端加载设置
  await settingsStore.loadSettings()
  
  // 2. 初始化设置服务
  initSettingsApplier()
  await initSettingsSync()
  
  // 只有主窗口执行以下操作
  if (isMainWindow) {
    // 3. 初始化弹幕客户端（主窗口订阅所有事件）
    await initBliveClient(ALL_EVENT_TYPES)
    
    // 4. 初始化窗口管理
    await initWindowManager('main')
    await restorePreviouslyOpenWindows()
    
    // 5. 自动连接（如果有保存的房间号和 Cookie）
    await autoConnect()
  }
})

onUnmounted(async () => {
  const appWindow = getCurrentWindow()
  if (appWindow.label === 'main') {
    await cleanupBliveClient()
    cleanupWindowManager('main')
  }
})
</script>

<template>
  <router-view />
</template>

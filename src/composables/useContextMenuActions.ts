/**
 * 右键菜单通用操作 composable
 * 提取自各 Tab 的右键菜单操作
 */

import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '@/stores/settings'

export function useContextMenuActions(
  showToastMessage?: (msg: string, type: 'success' | 'error' | 'info', duration?: number) => void
) {
  const settingsStore = useSettingsStore()

  const openUserPage = async (uid: number) => {
    const url = `https://space.bilibili.com/${uid}`
    try {
      await invoke('open_url', { url })
    } catch (e) {
      window.open(url, '_blank')
    }
  }

  const copyUsername = (name: string) => {
    navigator.clipboard.writeText(name)
    showToastMessage?.('已复制用户名', 'success', 1500)
  }

  const copyContent = (content: string, label = '内容') => {
    navigator.clipboard.writeText(content)
    showToastMessage?.(`已复制${label}`, 'success', 1500)
  }

  const toggleSpecialFollow = (uid: number, name: string) => {
    if (settingsStore.isSpecialFollow(uid)) {
      settingsStore.removeSpecialFollow(uid)
      showToastMessage?.(`已取消特别关注 ${name}`, 'info', 1500)
    } else {
      settingsStore.addSpecialFollow(uid)
      showToastMessage?.(`已特别关注 ${name}`, 'success', 1500)
    }
  }

  return { openUserPage, copyUsername, copyContent, toggleSpecialFollow }
}

/**
 * 自动更新服务
 * 后台异步检查更新，启动后检查一次，之后每小时检查一次
 * 检查失败静默处理，不对用户展示
 */

import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'

/** 更新信息（传递给 UI 组件） */
export interface UpdateInfo {
  version: string
  notes: string
  date: string
}

/** 下载进度回调参数 */
export interface DownloadProgress {
  downloaded: number
  total: number | null
}

// 模块级状态
let checkTimer: ReturnType<typeof setInterval> | null = null
let pendingUpdate: Update | null = null

const CHECK_INTERVAL = 60 * 60 * 5000 // 5小时
const INITIAL_DELAY = 15_000 // 启动后15秒首次检查

/**
 * 检查更新，返回更新信息或 null（已是最新 / 检查失败）
 */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  try {
    const update = await check()
    if (!update) return null

    // 缓存 update 对象，后续下载安装时使用
    pendingUpdate = update

    return {
      version: update.version,
      notes: update.body ?? '',
      date: update.date ?? '',
    }
  } catch (e) {
    // 检查失败静默处理
    console.debug('[updater] 检查更新失败:', e)
    return null
  }
}

/**
 * 下载并安装已发现的更新
 * @param onProgress 下载进度回调
 */
export async function downloadAndInstall(
  onProgress?: (progress: DownloadProgress) => void
): Promise<void> {
  if (!pendingUpdate) {
    throw new Error('没有可用更新')
  }

  let downloaded = 0
  let total: number | null = null

  await pendingUpdate.downloadAndInstall((event) => {
    switch (event.event) {
      case 'Started':
        total = event.data.contentLength ?? null
        break
      case 'Progress':
        downloaded += event.data.chunkLength
        onProgress?.({ downloaded, total })
        break
      case 'Finished':
        break
    }
  })

  // 安装完成后重启应用
  await relaunch()
}

/**
 * 启动后台自动检查
 * @param onUpdateFound 发现更新时的回调
 */
export function startAutoCheck(onUpdateFound: (info: UpdateInfo) => void) {
  // 启动后延迟首次检查，避免影响启动体验
  setTimeout(async () => {
    const info = await checkForUpdate()
    if (info) onUpdateFound(info)
  }, INITIAL_DELAY)

  // 每小时定时检查
  checkTimer = setInterval(async () => {
    const info = await checkForUpdate()
    if (info) onUpdateFound(info)
  }, CHECK_INTERVAL)
}

/**
 * 停止后台自动检查
 */
export function stopAutoCheck() {
  if (checkTimer) {
    clearInterval(checkTimer)
    checkTimer = null
  }
}

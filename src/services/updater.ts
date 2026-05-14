/**
 * 自动更新服务
 *
 * 同时支持安装版（NSIS，通过 tauri-plugin-updater）和便携版（portable，自定义更新流程）。
 * 启动时自动检测运行模式，调用对应的更新逻辑。
 *
 * 更新清单托管 静态存储，
 * 通过 Referer 白名单防盗链保护下载资源。
 *
 * - 后台异步检查：启动后 15s 首次检查，之后每小时检查一次
 * - 检查失败静默处理，不对用户展示
 */

import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { exit } from '@tauri-apps/plugin-process'
import { invoke } from '@tauri-apps/api/core'

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

// ==================== 常量 ====================

/** COS 防盗链 Referer */
const REFERER = 'https://updater.anzu.link'

/** 更新清单地址（与 tauri.conf.json 中 endpoints 一致） */
const UPDATE_ENDPOINT = 'https://akiba-1301838591.cos.ap-shanghai.myqcloud.com/update.json'

const CHECK_INTERVAL = 2 * 60 * 60 * 1000 // 2小时
const INITIAL_DELAY = 10_000 // 启动后10秒首次检查

// ==================== 模块级状态 ====================

let checkTimer: ReturnType<typeof setInterval> | null = null
let pendingUpdate: Update | null = null
/** 便携版更新时缓存的下载 URL */
let portableDownloadUrl: string | null = null
/** 缓存：是否为便携版 */
let _isPortable: boolean | null = null

// ==================== 工具函数 ====================

/**
 * 简单 semver 比较：remote 是否比 current 更新
 * 仅支持 x.y.z 格式
 */
function isNewerVersion(current: string, remote: string): boolean {
  const c = current.split('.').map(Number)
  const r = remote.split('.').map(Number)
  for (let i = 0; i < Math.max(c.length, r.length); i++) {
    if ((r[i] || 0) > (c[i] || 0)) return true
    if ((r[i] || 0) < (c[i] || 0)) return false
  }
  return false
}

// ==================== 检测运行模式 ====================

/** 是否为便携版（结果会缓存） */
export async function isPortable(): Promise<boolean> {
  if (_isPortable === null) {
    _isPortable = await invoke<boolean>('is_portable')
  }
  return _isPortable
}

/** 获取当前应用版本号 */
export async function getAppVersion(): Promise<string> {
  return invoke<string>('get_app_version')
}

// ==================== 安装版更新（tauri-plugin-updater）====================

async function checkInstallerUpdate(): Promise<UpdateInfo | null> {
  const update = await check({
    headers: { Referer: REFERER },
  })
  if (!update) return null

  pendingUpdate = update
  return {
    version: update.version,
    notes: update.body ?? '',
    date: update.date ?? '',
  }
}

async function installInstallerUpdate(
  onProgress?: (progress: DownloadProgress) => void
): Promise<void> {
  if (!pendingUpdate) throw new Error('没有可用更新')

  let downloaded = 0
  let total: number | null = null

  await pendingUpdate.downloadAndInstall(
    (event) => {
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
    },
    { headers: { Referer: REFERER } }
  )

  await relaunch()
}

// ==================== 便携版更新（自定义流程）====================

async function checkPortableUpdate(): Promise<UpdateInfo | null> {
  // COS 静态清单：始终返回最新版本 JSON，需要客户端比较版本号
  const body = await invoke<string | null>('check_portable_update', {
    url: UPDATE_ENDPOINT,
  })

  if (!body) return null

  const data = JSON.parse(body)

  // 客户端 semver 比较（静态 JSON 没有服务端版本过滤）
  const currentVersion = await getAppVersion()
  if (!isNewerVersion(currentVersion, data.version)) {
    return null
  }

  // 缓存 portable 下载 URL
  portableDownloadUrl = data.portable_url || null

  return {
    version: data.version,
    notes: data.notes ?? '',
    date: data.pub_date ?? '',
  }
}

async function installPortableUpdate(
  _onProgress?: (progress: DownloadProgress) => void
): Promise<void> {
  if (!portableDownloadUrl) throw new Error('没有可用的便携版下载地址')

  // 后端下载新 exe + 生成替换脚本 + 启动脚本
  await invoke('install_portable_update', { downloadUrl: portableDownloadUrl })

  // 退出当前进程（脚本会等进程退出后替换文件并重启）
  await exit(0)
}

// ==================== 统一入口 ====================

/**
 * 检查更新（自动判断安装版/便携版）
 */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  try {
    const portable = await isPortable()
    return portable ? await checkPortableUpdate() : await checkInstallerUpdate()
  } catch (e) {
    console.debug('[updater] 检查更新失败:', e)
    return null
  }
}

/**
 * 下载并安装更新（自动判断安装版/便携版）
 */
export async function downloadAndInstall(
  onProgress?: (progress: DownloadProgress) => void
): Promise<void> {
  const portable = await isPortable()
  return portable
    ? await installPortableUpdate(onProgress)
    : await installInstallerUpdate(onProgress)
}

/**
 * 启动后台自动检查
 */
export function startAutoCheck(onUpdateFound: (info: UpdateInfo) => void) {
  // 启动后延迟首次检查
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

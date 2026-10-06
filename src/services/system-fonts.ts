import { invoke } from '@tauri-apps/api/core'

export interface SystemFont { family: string; label: string }

let cached: Promise<SystemFont[]> | undefined
/** 同一窗口复用字体列表；用户安装新字体后可主动刷新。失败不缓存。 */
export function getSystemFonts(refresh = false): Promise<SystemFont[]> {
  if (!cached || refresh) {
    const request = invoke<SystemFont[]>('get_system_fonts')
      .then(fonts => fonts.sort((a, b) => a.label.localeCompare(b.label, 'zh-CN')))
    cached = request
    void request.catch(() => { if (cached === request) cached = undefined })
  }
  return cached
}

export function fontPreviewFamily(family: string) {
  return family ? `"${family.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}", var(--font-family)` : 'var(--font-family)'
}

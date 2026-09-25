/** 桌面端传输适配。领域类型与 OBS 页面均不依赖 Tauri。 */
import { invoke } from '@tauri-apps/api/core'
import type { OvertimeSnapshot, OvertimeRequest } from '@/types/overtime'

export interface OverlayServerInfo {
  port: number
  url: string | null
  error: string | null
  persistence_error: string | null
}
export const getOvertimeSnapshot = () => invoke<OvertimeSnapshot>('get_extension_snapshot', { extensionId: 'overtime' })
export const requestOvertime = (request: OvertimeRequest) => invoke<OvertimeSnapshot>('extension_request', { extensionId: 'overtime', request })
export const getOverlayServer = () => invoke<OverlayServerInfo>('get_overlay_server')
export const startOverlayServer = (port: number) => invoke<OverlayServerInfo>('start_overlay_server', { port })

/**
 * Bilibili 扫码登录服务
 */

import { invoke } from '@tauri-apps/api/core'

/** 二维码数据 */
export interface QRCodeData {
  url: string
  qrcode_key: string
}

/** 扫码状态 */
export interface QRCodeStatus {
  /** 
   * 状态码
   * - 86101: 未扫码
   * - 86090: 已扫码未确认
   * - 86038: 二维码已过期  
   * - 0: 登录成功
   */
  code: number
  message: string
  refresh_token: string | null
  cookie: string | null
}

/** 用户信息 */
export interface UserInfo {
  uid: number
  uname: string
  face: string
  is_login: boolean
}

/** 扫码状态码 */
export const QRCodeStatusCode = {
  NOT_SCANNED: 86101,
  SCANNED_NOT_CONFIRMED: 86090,
  EXPIRED: 86038,
  SUCCESS: 0
} as const

/**
 * 生成登录二维码
 */
export async function generateLoginQRCode(): Promise<QRCodeData> {
  return await invoke<QRCodeData>('generate_login_qrcode')
}

/**
 * 轮询扫码状态
 */
export async function pollLoginStatus(qrcodeKey: string): Promise<QRCodeStatus> {
  return await invoke<QRCodeStatus>('poll_login_status', { qrcodeKey })
}

/**
 * 获取用户信息
 */
export async function getUserInfo(cookie: string): Promise<UserInfo> {
  return await invoke<UserInfo>('get_user_info', { cookie })
}

/**
 * 验证 Cookie 是否有效
 */
export async function validateCookie(cookie: string): Promise<boolean> {
  return await invoke<boolean>('validate_cookie', { cookie })
}

/**
 * 扫码登录管理器
 */
export class QRCodeLoginManager {
  private qrcode: QRCodeData | null = null
  private pollTimer: number | null = null
  private onStatusChange?: (status: QRCodeStatus) => void
  private onQRCodeChange?: (qrcode: QRCodeData) => void
  private onError?: (error: string) => void

  /**
   * 设置状态变更回调
   */
  setStatusCallback(callback: (status: QRCodeStatus) => void): this {
    this.onStatusChange = callback
    return this
  }

  /**
   * 设置二维码变更回调
   */
  setQRCodeCallback(callback: (qrcode: QRCodeData) => void): this {
    this.onQRCodeChange = callback
    return this
  }

  /**
   * 设置错误回调
   */
  setErrorCallback(callback: (error: string) => void): this {
    this.onError = callback
    return this
  }

  /**
   * 开始登录流程
   */
  async start(): Promise<void> {
    this.stop()
    
    try {
      // 生成二维码
      this.qrcode = await generateLoginQRCode()
      this.onQRCodeChange?.(this.qrcode)
      
      // 开始轮询
      this.startPolling()
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      this.onError?.(msg)
    }
  }

  /**
   * 刷新二维码
   */
  async refresh(): Promise<void> {
    await this.start()
  }

  /**
   * 停止登录流程
   */
  stop(): void {
    if (this.pollTimer) {
      clearInterval(this.pollTimer)
      this.pollTimer = null
    }
    this.qrcode = null
  }

  /**
   * 获取当前二维码
   */
  getQRCode(): QRCodeData | null {
    return this.qrcode
  }

  private startPolling(): void {
    if (this.pollTimer) {
      clearInterval(this.pollTimer)
    }

    // 每 2 秒轮询一次
    this.pollTimer = window.setInterval(async () => {
      if (!this.qrcode) {
        this.stop()
        return
      }

      try {
        const status = await pollLoginStatus(this.qrcode.qrcode_key)
        this.onStatusChange?.(status)

        // 登录成功或二维码过期，停止轮询
        if (status.code === QRCodeStatusCode.SUCCESS || 
            status.code === QRCodeStatusCode.EXPIRED) {
          this.stop()
        }
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e)
        this.onError?.(msg)
        this.stop()
      }
    }, 2000)
  }
}


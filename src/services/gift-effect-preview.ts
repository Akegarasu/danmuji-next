import { emitTo, listen } from '@tauri-apps/api/event'

export const GIFT_EFFECT_PREVIEW_REQUEST = 'gift-effect-preview-request'
export const GIFT_EFFECT_PREVIEW_RESULT = 'gift-effect-preview-result'

export interface GiftEffectPreviewRequest {
  id: string
  roomId: number
}

export interface GiftEffectPreviewResult {
  id: string
  status: 'loading' | 'success' | 'error'
  message: string
}

/** 设置窗口请求主窗口播放，并等待播放器返回实际结果。 */
export async function previewGiftEffect(
  roomId: number,
  signal: AbortSignal,
  onProgress: (message: string) => void
): Promise<string> {
  const id = crypto.randomUUID()
  let timer: ReturnType<typeof setTimeout> | undefined
  let abort: () => void = () => {}
  let resolveResult!: (message: string) => void
  let rejectResult!: (error: Error) => void
  const result = new Promise<string>((resolve, reject) => {
    resolveResult = resolve
    rejectResult = reject
  })
  const unlisten = await listen<GiftEffectPreviewResult>(GIFT_EFFECT_PREVIEW_RESULT, ({ payload }) => {
    if (payload.id !== id) return
    clearTimeout(timer)
    if (payload.status === 'loading') {
      onProgress(payload.message)
      // 包含配置、视频加载及播放器最长五分钟的播放时限。
      timer = setTimeout(() => rejectResult(new Error('等待特效播放结果超时')), 7 * 60_000)
    } else if (payload.status === 'success') {
      resolveResult(payload.message)
    } else {
      rejectResult(new Error(payload.message))
    }
  })

  try {
    abort = () => rejectResult(new Error('特效测试已取消'))
    signal.addEventListener('abort', abort, { once: true })
    timer = setTimeout(() => rejectResult(new Error('主窗口播放器未响应，请确认主窗口已打开')), 5000)
    if (signal.aborted) abort()
    else {
      void emitTo('main', GIFT_EFFECT_PREVIEW_REQUEST, { id, roomId } satisfies GiftEffectPreviewRequest)
        .catch(error => rejectResult(new Error(String(error))))
    }
    return await result
  } finally {
    clearTimeout(timer)
    signal.removeEventListener('abort', abort)
    unlisten()
  }
}

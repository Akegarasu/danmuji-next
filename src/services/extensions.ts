/** 扩展统一 IPC 与状态订阅，不依赖直播连接。 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { ExtensionId, ExtensionRequests, ExtensionState, ExtensionStates } from '@/types/extensions'

export interface OverlayServerInfo {
  port: number
  url: string | null
  error: string | null
  persistence_error: string | null
}
export const getOverlayServer = () => invoke<OverlayServerInfo>('get_overlay_server')
export const startOverlayServer = (port: number) => invoke<OverlayServerInfo>('start_overlay_server', { port })

/** 先监听再取快照；版本号阻止较慢的查询覆盖较新的状态事件。 */
export function createExtensionClient<K extends ExtensionId>(
  extensionId: K,
  onState: (state: ExtensionStates[K]) => void,
  onError: (message: string) => void,
) {
  let active = false
  let generation = 0
  let revision = -1
  let unlisten: UnlistenFn | undefined
  let connecting: Promise<void> | undefined

  function apply(update: ExtensionState<K>, token: number) {
    if (!active || token !== generation || update.extension_id !== extensionId || update.revision <= revision) return
    revision = update.revision
    onState(update.state)
  }

  async function refresh() {
    if (!active) return
    const token = generation
    const update = await invoke<ExtensionState<K>>('get_extension_snapshot', { extensionId })
    apply(update, token)
  }

  function connect(): Promise<void> {
    if (connecting) return connecting
    if (!active) {
      active = true
      revision = -1
      ++generation
    }
    const token = generation
    connecting = (async () => {
      try {
        if (!unlisten) {
          const stop = await listen<ExtensionState<K>>(`extension-state:${extensionId}`, event => apply(event.payload, token))
          if (!active || token !== generation) { stop(); return }
          unlisten = stop
        }
        // 已连接时重新同步状态；操作可能已经生效，不能重放创建等请求。
        await refresh()
        if (active && token === generation) onError('')
      } catch (cause) {
        if (token === generation) {
          unlisten?.()
          unlisten = undefined
          active = false
          onError(String(cause))
        }
      }
    })().finally(() => {
      if (token === generation) connecting = undefined
    })
    return connecting
  }

  function disconnect() {
    active = false
    ++generation
    unlisten?.()
    unlisten = undefined
    connecting = undefined
  }

  async function request<R = void>(request: ExtensionRequests[K]): Promise<R> {
    const token = generation
    onError('')
    try {
      const result = await invoke<R>('extension_request', { extensionId, request })
      await refresh()
      return result
    } catch (cause) {
      // 操作可能已生效但保存失败，仍查询当前状态，错误留给页面展示。
      if (active && token === generation) {
        await refresh().catch(() => {})
        if (active && token === generation) onError(String(cause))
      }
      throw cause
    }
  }

  async function query<R>(query: Record<string, unknown>): Promise<R> {
    return invoke<R>('extension_query', { extensionId, query })
  }

  return { connect, disconnect, request, query }
}

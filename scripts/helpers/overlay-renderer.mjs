// 渲染器测试只注入快照；实际重连、缓存和生命周期由 test_overlay_connection / cache 覆盖。
export function rendererConnection(EventSource) {
  return {
    subscribe(id, onSnapshot, onDisconnect = () => {}) {
      const source = new EventSource(`/api/extensions/${id}/events`)
      source.addEventListener('snapshot', event => {
        try { onSnapshot(JSON.parse(event.data)) } catch { /* 模拟无效快照被隔离。 */ }
      })
      source.onerror = onDisconnect
      return () => source.close()
    },
  }
}

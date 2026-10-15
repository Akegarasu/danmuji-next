/* 三个 OBS 页面共用的连接与离线资源更新；不持久化直播数据。 */
(() => {
  let registration
  let updating = false
  let hidden = false
  let reloading = false
  const workers = navigator.serviceWorker
  let controller = workers?.controller

  function reload() {
    if (hidden || reloading) return
    reloading = true
    window.location.reload()
  }
  async function checkForUpdate() {
    if (!workers || updating || hidden) return
    updating = true
    try {
      if (registration) await registration.update()
      else registration = await workers.register('/overlays/sw.js', { scope: '/overlays/', updateViaCache: 'none' })
    } catch { /* 弹幕姬尚未启动或更新下载失败，继续使用已完整安装的资源。 */ }
    finally { updating = false }
  }
  const controllerChanged = () => {
    // 首次安装只接管页面；已有离线版本被替换时才刷新，避免首次加载闪动。
    const previous = controller
    controller = workers.controller
    if (previous && controller !== previous) reload()
  }
  workers?.addEventListener('controllerchange', controllerChanged)
  const updateTimer = setInterval(checkForUpdate, 30000)
  checkForUpdate()
  window.addEventListener('pagehide', () => {
    hidden = true
    clearInterval(updateTimer)
    workers?.removeEventListener('controllerchange', controllerChanged)
  })
  window.addEventListener('pageshow', event => {
    // 从页面往返缓存恢复时，重新初始化已在 pagehide 中释放的渲染定时器。
    if (event.persisted) { hidden = false; reload() }
  })

  window.OverlayConnection = {
    subscribe(id, onSnapshot, onDisconnect = () => {}) {
      let source
      let stopped = false
      let failures = 0
      let retryTimer
      let watchdog

      const armWatchdog = () => {
        clearTimeout(watchdog)
        // 服务端最长每 10 秒发一次心跳；握手或连接假死时主动重建。
        watchdog = setTimeout(disconnect, 25000)
      }
      function disconnect() {
        if (stopped) return
        source?.close()
        source = undefined
        clearTimeout(watchdog)
        clearTimeout(retryTimer)
        onDisconnect()
        retryTimer = setTimeout(connect, Math.min(1000 * 2 ** Math.min(failures++, 3), 5000))
      }
      function connect() {
        if (stopped) return
        clearTimeout(retryTimer)
        source?.close()
        let current
        let received = false
        try { current = new EventSource(`/api/extensions/${id}/events`) }
        catch { disconnect(); return }
        source = current
        armWatchdog()
        current.addEventListener('snapshot', event => {
          if (stopped || source !== current) return
          try {
            onSnapshot(JSON.parse(event.data))
            failures = 0
            armWatchdog()
            if (!received) { received = true; checkForUpdate() }
          } catch { /* 无效快照不覆盖页面；持续异常时由看门狗重连。 */ }
        })
        current.addEventListener('heartbeat', () => {
          if (!stopped && source === current) armWatchdog()
        })
        // 包括 EventSource 进入 CLOSED 的情况，不依赖浏览器是否继续原生重试。
        current.onerror = () => { if (source === current) disconnect() }
      }
      const online = () => { if (!source) connect() }
      window.addEventListener('online', online)
      connect()
      return () => {
        stopped = true
        source?.close()
        source = undefined
        clearTimeout(retryTimer)
        clearTimeout(watchdog)
        window.removeEventListener('online', online)
      }
    },
  }
})()

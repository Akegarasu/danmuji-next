/* VERSION 与 ASSETS 由本地服务根据全部嵌入资源生成，不能手工维护版本号。 */
const CACHE_PREFIX = 'danmuji-overlays-'
const CACHE_NAME = CACHE_PREFIX + VERSION

self.addEventListener('install', event => {
  event.waitUntil((async () => {
    // 完整下载并校验同一版本后才激活。升级中断时旧缓存仍可启动 OBS 页面。
    const responses = await Promise.all(ASSETS.map(async path => {
      const response = await fetch(path, { cache: 'no-store' })
      if (!response.ok || response.headers.get('X-Danmuji-Overlay-Version') !== VERSION) {
        throw new Error('覆盖层资源下载不完整或版本已变化')
      }
      return response
    }))
    const cache = await caches.open(CACHE_NAME)
    await Promise.all(ASSETS.map((path, index) => cache.put(path, responses[index])))
    await self.skipWaiting()
  })())
})

self.addEventListener('activate', event => {
  event.waitUntil((async () => {
    await Promise.all((await caches.keys())
      .filter(name => name.startsWith(CACHE_PREFIX) && name !== CACHE_NAME)
      .map(name => caches.delete(name)))
    await self.clients.claim()
  })())
})

self.addEventListener('fetch', event => {
  const request = event.request
  const url = new URL(request.url)
  // 仅拦截资源白名单；API、SSE、未知路径和其他来源始终访问网络。
  if (request.method !== 'GET' || url.origin !== self.location.origin) return
  const path = url.pathname.replace(/\/index\.html$/, '/')
  if (!ASSETS.includes(path)) return
  event.respondWith((async () => {
    const cached = await (await caches.open(CACHE_NAME)).match(path)
    return cached || fetch(request)
  })())
})

// 验证离线资源完整性、升级失败回退和动态接口不落缓存。
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'

const code = readFileSync(new URL('../public/overlays/sw.js', import.meta.url), 'utf8')
const assets = ['/overlays/overtime/', '/overlays/song-request/', '/overlays/wish-machine/', '/overlays/shared/connection.js']
function fixture(version, stores = new Map(), failure) {
  const handlers = new Map(), fetched = []
  let online = true, activated = false, claimed = false
  const caches = {
    async open(name) {
      if (!stores.has(name)) stores.set(name, new Map())
      const store = stores.get(name)
      return {
        async match(path) { return store.get(path)?.clone() },
        async put(path, response) { store.set(path, response.clone()) },
      }
    },
    async keys() { return [...stores.keys()] },
    async delete(name) { return stores.delete(name) },
  }
  runInNewContext(code, {
    VERSION: version, ASSETS: assets, caches, URL,
    self: {
      location: { origin: 'http://127.0.0.1:17654' },
      addEventListener(name, fn) { handlers.set(name, fn) },
      async skipWaiting() { activated = true },
      clients: { async claim() { claimed = true } },
    },
    async fetch(path, options) {
      fetched.push(path)
      if (!online || failure === path) throw new Error('服务离线')
      assert.equal(options.cache, 'no-store')
      return new Response(`${version}:${path}`, { headers: { 'X-Danmuji-Overlay-Version': failure === 'mixed' ? 'different' : version } })
    },
  })
  return {
    stores, fetched, get activated() { return activated }, get claimed() { return claimed },
    offline() { online = false },
    async lifecycle(name) { let work; handlers.get(name)({ waitUntil(value) { work = value } }); await work },
    request(path, method = 'GET') {
      let result
      handlers.get('fetch')({ request: new Request(new URL(path, 'http://127.0.0.1:17654'), { method }), respondWith(value) { result = value } })
      return result
    },
  }
}

test('完整预缓存后服务关闭，三个页面、显式 index 和带查询参数的地址均可冷启动', async () => {
  const f = fixture('v1')
  await f.lifecycle('install')
  assert.equal(f.activated, true)
  await f.lifecycle('activate')
  f.offline()
  for (const path of assets) assert.equal(await (await f.request(path)).text(), `v1:${path}`)
  assert.equal(await (await f.request('/overlays/wish-machine/index.html?refresh=123')).text(), 'v1:/overlays/wish-machine/')
  assert.equal(f.fetched.length, assets.length)
})

test('状态、事件流、未知资源、其他来源和写请求均不拦截或缓存', async () => {
  const f = fixture('v1')
  await f.lifecycle('install')
  for (const path of ['/api/extensions', '/api/extensions/overtime/state', '/api/extensions/overtime/events', '/overlays/sw.js', '/overlays/unknown/', 'https://example.com/overlays/overtime/']) {
    assert.equal(f.request(path), undefined)
  }
  assert.equal(f.request('/overlays/overtime/', 'POST'), undefined)
})

test('升级下载中断或版本混杂不接管，不破坏旧资源；完整新版接管后只清理自己的旧缓存', async () => {
  const first = fixture('v1')
  await first.lifecycle('install')
  await first.lifecycle('activate')
  first.stores.set('unrelated-cache', new Map())
  for (const failure of ['/overlays/wish-machine/', 'mixed']) {
    const broken = fixture('v2', first.stores, failure)
    await assert.rejects(broken.lifecycle('install'))
    assert.equal(broken.activated, false)
    assert.ok(first.stores.has('danmuji-overlays-v1'))
    first.offline()
    assert.equal(await (await first.request('/overlays/wish-machine/')).text(), 'v1:/overlays/wish-machine/')
  }
  const next = fixture('v2', first.stores)
  await next.lifecycle('install')
  await next.lifecycle('activate')
  assert.equal(next.claimed, true)
  assert.deepEqual([...first.stores.keys()].sort(), ['danmuji-overlays-v2', 'unrelated-cache'])
  next.offline()
  assert.equal(await (await next.request('/overlays/overtime/')).text(), 'v2:/overlays/overtime/')
})

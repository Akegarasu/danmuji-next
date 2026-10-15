// 使用虚拟时间覆盖冷启动重试、连接假死和资源更新，不依赖直播平台。
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'

const code = readFileSync(new URL('../public/overlays/shared/connection.js', import.meta.url), 'utf8')
const settle = () => new Promise(resolve => setImmediate(resolve))
function events() {
  const handlers = new Map()
  return {
    addEventListener(name, fn) { if (!handlers.has(name)) handlers.set(name, new Set()); handlers.get(name).add(fn) },
    removeEventListener(name, fn) { handlers.get(name)?.delete(fn) },
    emit(name, value = {}) { for (const fn of handlers.get(name) || []) fn(value) },
  }
}
function fixture(workers) {
  const timers = new Map(), streams = [], snapshots = []
  let now = 0, sequence = 0, reloads = 0, disconnects = 0
  const window = { ...events(), location: { reload() { reloads++ } } }
  function schedule(fn, delay, interval = false) {
    const id = ++sequence
    timers.set(id, { fn, at: now + delay, delay, interval })
    return id
  }
  class EventSource {
    constructor(url) { this.url = url; Object.assign(this, events()); streams.push(this) }
    close() { this.closed = true }
  }
  runInNewContext(code, {
    window, navigator: { serviceWorker: workers }, EventSource,
    setTimeout: (fn, delay) => schedule(fn, delay), clearTimeout: id => timers.delete(id),
    setInterval: (fn, delay) => schedule(fn, delay, true), clearInterval: id => timers.delete(id),
  })
  const close = window.OverlayConnection.subscribe('wish-machine', value => snapshots.push(value), () => disconnects++)
  return {
    window, streams, snapshots, timers, close,
    get reloads() { return reloads }, get disconnects() { return disconnects },
    send(value, stream = streams.at(-1)) { stream.emit('snapshot', { data: JSON.stringify(value) }) },
    advance(elapsed) {
      const end = now + elapsed
      while (true) {
        const next = [...timers].filter(([, timer]) => timer.at <= end).sort((a, b) => a[1].at - b[1].at)[0]
        if (!next) break
        const [id, timer] = next
        now = timer.at
        if (timer.interval) timer.at += timer.delay
        else timers.delete(id)
        timer.fn()
      }
      now = end
    },
  }
}

test('OBS 先启动：失败后按上限五秒持续重建连接，恢复完整快照且隔离旧连接', () => {
  const f = fixture()
  assert.equal(f.streams[0].url, '/api/extensions/wish-machine/events')
  for (const delay of [1000, 2000, 4000, 5000, 5000]) {
    const count = f.streams.length
    const previous = f.streams.at(-1)
    previous.onerror()
    assert.equal(previous.closed, true)
    f.advance(delay - 1)
    assert.equal(f.streams.length, count)
    f.advance(1)
    assert.equal(f.streams.length, count + 1)
  }
  f.send({ goals: [{ id: 'latest', current: 12 }] })
  f.send({ goals: [{ id: 'outdated' }] }, f.streams[0])
  assert.equal(f.snapshots.length, 1)
  assert.equal(f.snapshots[0].goals[0].current, 12)
  f.streams.at(-1).onerror()
  const count = f.streams.length
  f.advance(1000)
  assert.equal(f.streams.length, count + 1, '成功后重试间隔重置')
})

test('握手假死与无数据连接超时重建；心跳使静止的队列保持连接', () => {
  const f = fixture()
  f.advance(25000)
  assert.equal(f.streams[0].closed, true)
  f.advance(1000)
  assert.equal(f.streams.length, 2)
  f.send({ goals: [] })
  for (let i = 0; i < 6; i++) {
    f.advance(10000)
    f.streams.at(-1).emit('heartbeat')
  }
  assert.equal(f.streams.length, 2)
  f.streams.at(-1).emit('snapshot', { data: 'invalid json' })
  assert.equal(f.snapshots.length, 1)
  f.advance(26000)
  assert.equal(f.streams.length, 3)
})

test('网络恢复立即重连，释放后取消重试、心跳检查及过期回调', () => {
  const f = fixture()
  f.streams[0].onerror()
  f.window.emit('online')
  assert.equal(f.streams.length, 2)
  f.advance(1000)
  assert.equal(f.streams.length, 2)
  f.close()
  f.window.emit('pagehide')
  f.send({ goals: [] })
  f.window.emit('online')
  f.advance(120000)
  assert.equal(f.snapshots.length, 0)
  assert.equal(f.streams.length, 2)
  assert.equal(f.streams[1].closed, true)
  assert.equal(f.timers.size, 0)
})

test('离线注册失败会重试，重连检查更新，同版本不刷新，新控制器只刷新一次', async () => {
  let attempts = 0, updates = 0
  const workers = { ...events(), controller: null, async register(url, options) {
    assert.equal(url, '/overlays/sw.js')
    assert.equal(options.scope, '/overlays/')
    assert.equal(options.updateViaCache, 'none')
    if (++attempts === 1) throw new Error('服务离线')
    return { async update() { updates++ } }
  } }
  const f = fixture(workers)
  await settle()
  f.send({ goals: [] })
  await settle()
  assert.equal(attempts, 2)
  workers.controller = { version: 'first' }
  workers.emit('controllerchange')
  assert.equal(f.reloads, 0)
  f.advance(30000)
  await settle()
  assert.equal(updates, 1)
  assert.equal(f.reloads, 0)
  workers.controller = { version: 'second' }
  workers.emit('controllerchange')
  workers.emit('controllerchange')
  assert.equal(f.reloads, 1)
})

test('更新失败不刷新已有缓存页面，页面隐藏后不再更新，往返缓存恢复会重新初始化', async () => {
  let updates = 0
  const workers = { ...events(), controller: {}, async register() {
    return { async update() { updates++; throw new Error('更新下载中断') } }
  } }
  const f = fixture(workers)
  await settle()
  f.advance(30000)
  await settle()
  assert.equal(updates, 1)
  assert.equal(f.reloads, 0)
  f.close()
  f.window.emit('pagehide')
  f.advance(60000)
  assert.equal(updates, 1)
  f.window.emit('pageshow', { persisted: true })
  assert.equal(f.reloads, 1)
})

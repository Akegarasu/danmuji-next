// 用 Node 的轻量 DOM 替身验证 OBS 入口，不启动浏览器或连接直播平台。
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'

const sourceCode = readFileSync(new URL('../public/overlays/overtime/overlay.js', import.meta.url), 'utf8')
class Element {
  textContent = ''
  children = []
  style = {}
  offsetWidth = 399
  classes = new Set()
  classList = {
    add: value => this.classes.add(value),
    remove: value => this.classes.delete(value),
    toggle: (value, enabled) => enabled ? this.classes.add(value) : this.classes.delete(value),
  }
  append(...children) { this.children.push(...children) }
  replaceChildren(...children) { this.children = children }
}
function page() {
  const elements = Object.fromEntries(['clock', 'connection', 'rules', 'notice-text', 'room'].map(id => [id, new Element()]))
  const intervals = new Map(), timeouts = new Map(), events = new Map()
  let now = 0, sequence = 0, stream
  class EventSource {
    listeners = new Map()
    closed = false
    constructor(url) { assert.equal(url, '/api/extensions/overtime/events'); stream = this }
    addEventListener(name, listener) { this.listeners.set(name, listener) }
    close() { this.closed = true }
  }
  runInNewContext(sourceCode, {
    document: { getElementById: id => elements[id], createElement: () => new Element(), querySelector: () => elements.room },
    window: { innerWidth: 350, addEventListener: (name, fn) => events.set(name, fn), removeEventListener: name => events.delete(name) },
    EventSource, performance: { now: () => now }, console,
    setInterval: fn => { const id = ++sequence; intervals.set(id, fn); return id }, clearInterval: id => intervals.delete(id),
    setTimeout: fn => { const id = ++sequence; timeouts.set(id, fn); return id }, clearTimeout: id => timeouts.delete(id),
  })
  return {
    elements, stream, intervals, timeouts, events,
    nextNotice: () => {
      const next = timeouts.entries().next().value
      assert.ok(next, '应存在提示切换定时器')
      timeouts.delete(next[0]); next[1]()
    },
    send: value => stream.listeners.get('snapshot')({ data: JSON.stringify(value) }),
    advance: elapsed => { now += elapsed; for (const fn of intervals.values()) fn() },
  }
}
function snapshot(overrides = {}) {
  return { config: { enabled: true, show_rules: true, show_notice: true, rules: [] }, remaining_ms: 10000, running: true, rate: 1, notices: [], ...overrides }
}

test('多个快照校准时钟，断线后停止推算，重新连接恢复后端时间', () => {
  const view = page()
  view.send(snapshot())
  assert.equal(view.elements.clock.textContent, '00:00:10')
  view.advance(1100)
  assert.equal(view.elements.clock.textContent, '00:00:09')
  view.stream.onerror()
  view.advance(60000)
  assert.notEqual(view.elements.clock.textContent, '下班啦！')
  assert.match(view.elements.connection.textContent, /断开/)
  view.send(snapshot({ remaining_ms: 42000 }))
  assert.equal(view.elements.clock.textContent, '00:00:42')
  assert.equal(view.elements.connection.textContent, '')
})
test('恢复最新提示并循环，新礼物排队切换，重复快照不打断动画，重置清除提示', () => {
  const view = page()
  const notice = id => ({ id, gift_name: '小心心', sender_name: '测试', num: 1, delta_ms: 1000, actions: ['add'] })
  view.send(snapshot({ notices: [notice(1)] }))
  assert.equal(view.timeouts.size, 1)
  assert.match(view.elements['notice-text'].textContent, /投喂/)
  const current = view.elements['notice-text'].textContent
  view.nextNotice()
  assert.equal(view.timeouts.size, 0)
  assert.equal(view.elements['notice-text'].textContent, current)
  assert.ok(view.elements['notice-text'].classes.has('scrolling'))
  view.send(snapshot({ notices: [notice(1), notice(2)] }))
  assert.equal(view.timeouts.size, 1)
  view.send(snapshot({ notices: [notice(1), notice(2)] }))
  assert.equal(view.timeouts.size, 1)
  view.send(snapshot({ notices: [] }))
  assert.equal(view.timeouts.size, 0)
  assert.equal(view.elements['notice-text'].textContent, '')
})
test('暂停不扣时间，倍率影响显示，超时失联不宣告下班', () => {
  const view = page()
  view.send(snapshot({ running: false }))
  view.advance(2000)
  assert.equal(view.elements.clock.textContent, '00:00:10')
  view.send(snapshot({ rate: 2 }))
  view.advance(1000)
  assert.equal(view.elements.clock.textContent, '00:00:08')
  view.advance(10000)
  assert.match(view.elements.connection.textContent, /断开/)
  view.send(snapshot({ remaining_ms: 0 }))
  assert.equal(view.elements.clock.textContent, '下班啦！')
})
test('仅渲染启用规则，窗口缩放不依赖新 CSS 语法，关闭页面释放连接', () => {
  const view = page()
  view.send(snapshot({ config: { enabled: true, show_rules: true, show_notice: true, rules: [
    { enabled: true, gift_name: '小心心', action: 'add', value: 60, per_gift: true },
    { enabled: false, gift_name: '隐藏礼物', action: 'clear', value: 0, per_gift: false },
  ] } }))
  assert.equal(view.elements.rules.children.length, 1)
  assert.equal(view.elements.room.style.zoom, 350 / 600)
  view.events.get('pagehide')()
  assert.equal(view.stream.closed, true)
  assert.equal(view.intervals.size, 0)
  assert.equal(view.events.has('resize'), false)
})


test('连续礼物轮流展示，随机结果使用服务端数值，重连仅恢复最后一条', () => {
  const view = page()
  const notice = (id, sender) => ({ id, gift_name: '小心心', sender_name: sender, num: 2,
    delta_ms: 5000, actions: ['multiply'], results: [{ action: 'multiply', value: 1.25, count: 2, random: true, delta_ms: 5000 }] })
  const first = notice(1, '第一位')
  const second = notice(2, '第二位')
  view.send(snapshot({ notices: [first] }))
  assert.match(view.elements['notice-text'].textContent, /第一位.*随机×1.25/)
  view.send(snapshot({ notices: [first, second] }))
  assert.match(view.elements['notice-text'].textContent, /第一位/)
  view.nextNotice()
  assert.match(view.elements['notice-text'].textContent, /第二位/)
  view.nextNotice()
  assert.match(view.elements['notice-text'].textContent, /第二位/)
  assert.equal(view.timeouts.size, 0)
  view.stream.onerror()
  view.send(snapshot({ notices: [first, second, notice(3, '最新')] }))
  assert.match(view.elements['notice-text'].textContent, /最新/)
})

test('随机范围呈现在透明规则列表中，关闭提示取消循环', () => {
  const view = page()
  const config = { enabled: true, show_rules: true, show_notice: true, rules: [
    { enabled: true, gift_name: '小心心', action: 'random', value: 0, per_gift: true,
      random_ranges: [{ action: 'add', min: 10, max: 60 }, { action: 'divide', min: 1.2, max: 2 }] },
  ] }
  const notices = [{ id: 1, gift_name: '小心心', sender_name: '测试', num: 1, delta_ms: 10000, actions: ['add'] }]
  view.send(snapshot({ config, notices }))
  const row = view.elements.rules.children[0]
  assert.equal(row.children[0].textContent, '·小心心')
  assert.equal(row.children[1].textContent, '随机')
  assert.equal(row.children[2].textContent, '+10～60秒 / ÷1.2～2')
  view.send(snapshot({ config: { ...config, show_notice: false }, notices }))
  assert.equal(view.elements['notice-text'].textContent, '')
  assert.equal(view.timeouts.size, 0)
  assert.ok(!view.elements['notice-text'].classes.has('scrolling'))
})

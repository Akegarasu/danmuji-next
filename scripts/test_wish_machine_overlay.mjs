// 验证 OBS 心愿列表、稳定选择器、纯文本渲染和断线恢复。
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'

const code = readFileSync(new URL('../public/overlays/wish-machine/overlay.js', import.meta.url), 'utf8')
class Element {
  children = []
  dataset = {}
  textContent = ''
  appendChild(child) { this.children.push(child) }
  replaceChildren(...children) { this.children = children }
}
function fixture() {
  const list = new Element(), events = new Map()
  let stream
  class EventSource {
    listeners = new Map()
    closed = false
    constructor(url) { assert.equal(url, '/api/extensions/wish-machine/events'); stream = this }
    addEventListener(name, handler) { this.listeners.set(name, handler) }
    close() { this.closed = true }
  }
  runInNewContext(code, {
    document: { getElementById(id) { assert.equal(id, 'wish-list'); return list }, createElement: () => new Element() },
    EventSource, window: { addEventListener: (name, handler) => events.set(name, handler) },
  })
  return { list, events, stream, send: goals => stream.listeners.get('snapshot')({ data: JSON.stringify({ goals }) }) }
}
const goal = (id, gift_name, current = 0, target = 5) => ({ id, gift_name, current, target })

test('名称及数量按纯文本显示，超额保留真实数量并标记达成', () => {
  const f = fixture()
  f.send([goal('captain', '舰长', 6), goal('gift', '<img src=x onerror=alert(1)>', 0, 200)])
  const row = f.list.children[0]
  assert.equal(row.dataset.complete, 'true')
  assert.equal(row.id, 'wish-goal-captain')
  assert.deepEqual(row.children[1].children.map(cell => [cell.id, cell.textContent]), [
    ['wish-current-captain', '6'], ['wish-separator-captain', '/'], ['wish-target-captain', '5'],
  ])
  assert.equal(f.list.children[1].children[0].textContent, '<img src=x onerror=alert(1)>')
})

test('按后端顺序显示，改名及排序保留节点 ID，相同快照不重绘', () => {
  const f = fixture()
  const goals = [goal('first', '舰长'), goal('second', '心动盲盒')]
  f.send(goals)
  const first = f.list.children[0]
  f.send(goals)
  assert.equal(f.list.children[0], first)
  f.send([goals[1], { ...goals[0], gift_name: '新名称', current: 2 }])
  assert.equal(f.list.children[1].id, first.id)
  assert.equal(f.list.children[1].children[0].id, 'wish-name-first')
  assert.equal(f.list.children[1].children[0].textContent, '新名称')
})

test('断线及无效数据保留最后有效内容，重连恢复，空列表清空，销毁释放连接', () => {
  const f = fixture()
  f.send([goal('captain', '舰长')])
  f.stream.onerror()
  f.stream.listeners.get('snapshot')({ data: 'invalid json' })
  for (const invalid of [[goal('bad id', '错误')], [goal('x', '错误', -1)], [goal('x', '错误', 1, 0)], [null], null]) f.send(invalid)
  assert.equal(f.list.children[0].id, 'wish-goal-captain')
  f.send([goal('captain', '舰长', 1)])
  assert.equal(f.list.children[0].children[1].children[0].textContent, '1')
  f.send([])
  assert.equal(f.list.children.length, 0)
  f.events.get('pagehide')()
  assert.equal(f.stream.closed, true)
})

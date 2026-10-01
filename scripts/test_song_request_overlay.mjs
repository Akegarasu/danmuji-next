// 验证 OBS 队列顺序、纯文本渲染及断线恢复，不连接直播平台。
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'

const code = readFileSync(new URL('../public/overlays/song-request/overlay.js', import.meta.url), 'utf8')
class Element {
  textContent = ''
  children = []
  hidden = false
  classes = new Set()
  classList = { toggle: (name, enabled) => enabled ? this.classes.add(name) : this.classes.delete(name) }
  appendChild(child) { this.children.push(child) }
  replaceChildren(...children) { this.children = children }
}
function fixture() {
  const elements = { songs: new Element() }
  const events = new Map()
  let stream
  class EventSource {
    listeners = new Map()
    closed = false
    constructor(url) { assert.equal(url, '/api/extensions/song-request/events'); stream = this }
    addEventListener(name, callback) { this.listeners.set(name, callback) }
    close() { this.closed = true }
  }
  runInNewContext(code, { document: { getElementById: id => elements[id], createElement: () => new Element() }, EventSource,
    window: { addEventListener: (name, callback) => events.set(name, callback) } })
  return { elements, events, stream, send: value => stream.listeners.get('snapshot')({ data: JSON.stringify(value) }) }
}
const state = (requests, overrides = {}) => ({ show_username: true, total: requests.length, requests, ...overrides })
const song = (id, name) => ({ id, song_name: name, username: `用户${id}` })

test('仅按后端顺序显示序号、歌名、点歌人，空队列不显示内容', () => {
  const f = fixture()
  f.send(state([song(2, '稻香'), song(1, '晴天')], { total: 5 }))
  assert.deepEqual(f.elements.songs.children.map(row => row.children.map(cell => cell.textContent)), [['01', '稻香', '用户2'], ['02', '晴天', '用户1']])
  f.send(state([song(1, '晴天')]))
  assert.equal(f.elements.songs.children[0].children[0].textContent, '01')
  f.send(state([]))
  assert.equal(f.elements.songs.children.length, 0)
  const html = readFileSync(new URL('../public/overlays/song-request/index.html', import.meta.url), 'utf8')
  assert.doesNotMatch(html, /<header|<footer|id="empty"|id="status"|id="count"/)
  const css = readFileSync(new URL('../public/overlays/song-request/overlay.css', import.meta.url), 'utf8')
  assert.match(css, /html, body[^}]*background: transparent/)
  assert.doesNotMatch(css, /background:(?! transparent)/)
})

test('观众内容按纯文本显示，用户名可隐藏，相同快照不重建列表', () => {
  const f = fixture()
  const snapshot = state([song(1, '<img src=x onerror=alert(1)>')], { show_username: false })
  f.send(snapshot)
  assert.equal(f.elements.songs.children[0].children[1].textContent, '<img src=x onerror=alert(1)>')
  assert.equal(f.elements.songs.children[0].children.length, 2)
  const row = f.elements.songs.children[0]
  f.send(snapshot)
  assert.equal(f.elements.songs.children[0], row)
})

test('断线和无效数据静默保留队列，重连恢复，关闭页面释放连接', () => {
  const f = fixture()
  f.send(state([song(1, '晴天')]))
  f.stream.onerror()
  assert.equal(f.elements.songs.children.length, 1)
  f.send(state([song(1, '晴天')]))
  f.stream.listeners.get('snapshot')({ data: 'invalid json' })
  assert.equal(f.elements.songs.children[0].children[1].textContent, '晴天')
  f.send(state([]))
  assert.equal(f.elements.songs.children.length, 0)
  f.events.get('pagehide')()
  assert.equal(f.stream.closed, true)
})

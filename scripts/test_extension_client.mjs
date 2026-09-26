// 验证扩展客户端的查询/推送竞态与销毁行为，不启动桌面或浏览器。
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'
import { setImmediate } from 'node:timers/promises'
import ts from 'typescript'

const source = readFileSync(new URL('../src/services/extensions.ts', import.meta.url), 'utf8')
const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText
const state = (revision, value) => ({ extension_id: 'voting', revision, state: value })
function deferred() {
  let resolve, reject
  const promise = new Promise((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}
function fixture() {
  const values = [], errors = [], calls = [], registrations = []
  const api = {
    invoke: async () => state(0, []),
    listen: async (_name, callback) => {
      const registration = { callback, stopped: false }
      registrations.push(registration)
      return () => { registration.stopped = true }
    },
  }
  const module = { exports: {} }
  runInNewContext(code, { module, exports: module.exports, require(name) {
    if (name === '@tauri-apps/api/core') return { invoke: (...args) => { calls.push(args); return api.invoke(...args) } }
    if (name === '@tauri-apps/api/event') return { listen: (...args) => api.listen(...args) }
    throw new Error(name)
  } })
  const client = module.exports.createExtensionClient('voting', value => values.push(value), message => errors.push(message))
  const emit = update => registrations.forEach(r => { if (!r.stopped) r.callback({ payload: update }) })
  return { client, api, values, errors, calls, registrations, emit }
}

test('较新的状态事件不会被较慢的初始快照覆盖', async () => {
  const f = fixture(), snapshot = deferred()
  f.api.invoke = () => snapshot.promise
  const connecting = f.client.connect()
  await setImmediate()
  f.emit(state(2, ['新状态']))
  snapshot.resolve(state(1, ['旧快照']))
  await connecting
  f.emit(state(2, ['重复版本']))
  f.emit({ extension_id: 'overtime', revision: 100, state: ['其他扩展'] })
  assert.deepEqual(f.values, [['新状态']])
  f.client.disconnect()
})

test('监听注册尚未返回时关闭页面也会清理监听', async () => {
  const f = fixture(), registration = deferred()
  let stopped = 0
  f.api.listen = () => registration.promise
  const connecting = f.client.connect()
  f.client.disconnect()
  registration.resolve(() => { stopped++ })
  await connecting
  assert.equal(stopped, 1)
  assert.equal(f.calls.length, 0)
  assert.equal(f.values.length, 0)
})

test('重新打开页面不接受上一次连接的查询结果或事件', async () => {
  const f = fixture(), old = deferred()
  let fetches = 0
  f.api.invoke = () => ++fetches === 1 ? old.promise : Promise.resolve(state(3, []))
  const first = f.client.connect()
  await setImmediate()
  f.client.disconnect()
  await f.client.connect()
  old.resolve(state(9, ['已关闭页面的结果']))
  await first
  f.registrations[0].callback({ payload: state(10, ['迟到事件']) })
  assert.deepEqual(f.values, [[]])
  assert.equal(f.registrations[0].stopped, true)
  assert.equal(f.registrations[1].stopped, false)
  f.client.disconnect()
  assert.equal(f.registrations[1].stopped, true)
})

test('操作通过统一请求并刷新状态，只读查询不触发刷新', async () => {
  const f = fixture()
  let revision = 0
  f.api.invoke = async (name, args) => {
    assert.equal(args.extensionId, 'voting')
    if (name === 'get_extension_snapshot') return state(revision, revision ? ['已创建'] : [])
    if (name === 'extension_request') { revision++; return { id: 'poll-1' } }
    if (name === 'extension_query') return [{ uid: 42 }]
    throw new Error(name)
  }
  await f.client.connect()
  const result = await f.client.request({ type: 'create', title: '标题', options: [['A', '甲']], key_type: 'letter', duration_secs: null })
  assert.equal(result.id, 'poll-1')
  assert.deepEqual(f.values, [[], ['已创建']])
  const before = f.calls.length
  assert.deepEqual(await f.client.query({ type: 'voters', poll_id: 'poll-1', option_key: 'A' }), [{ uid: 42 }])
  assert.equal(f.calls.length, before + 1)
  assert.equal(f.calls.at(-1)[0], 'extension_query')
  f.client.disconnect()
})

test('操作生效但保存失败时刷新当前状态并保留错误', async () => {
  const f = fixture()
  let revision = 0
  f.api.invoke = async name => {
    if (name === 'extension_request') { revision++; throw new Error('状态保存失败') }
    return state(revision, [revision])
  }
  await f.client.connect()
  await assert.rejects(f.client.request({ type: 'delete', poll_id: 'poll-1' }), /状态保存失败/)
  assert.deepEqual(f.values, [[0], [1]])
  assert.match(f.errors.at(-1), /状态保存失败/)
  f.client.disconnect()
})

test('初始查询失败后清理监听，重试可获得空状态', async () => {
  const f = fixture()
  f.api.invoke = async () => { throw new Error('读取失败') }
  await f.client.connect()
  assert.equal(f.registrations[0].stopped, true)
  assert.match(f.errors.at(-1), /读取失败/)
  f.api.invoke = async () => state(0, [])
  await f.client.connect()
  assert.deepEqual(f.values, [[]])
  assert.equal(f.errors.at(-1), '')
  f.client.disconnect()
})

test('操作失败后可刷新状态，复用监听且不重放已生效的操作', async () => {
  const f = fixture()
  let revision = 0
  f.api.invoke = async name => {
    if (name === 'extension_request') { revision++; throw new Error('状态保存失败') }
    return state(revision, [revision])
  }
  await f.client.connect()
  await assert.rejects(f.client.request({ type: 'create', title: '标题', options: [['A', '甲']], key_type: 'letter', duration_secs: null }), /状态保存失败/)
  assert.match(f.errors.at(-1), /状态保存失败/)
  const before = f.calls.length
  revision++
  await f.client.connect()
  assert.equal(f.calls.length, before + 1)
  assert.equal(f.calls.at(-1)[0], 'get_extension_snapshot')
  assert.equal(f.calls.filter(([name]) => name === 'extension_request').length, 1)
  assert.equal(f.registrations.length, 1)
  assert.equal(f.registrations[0].stopped, false)
  assert.deepEqual(f.values, [[0], [1], [2]])
  assert.equal(f.errors.at(-1), '')
  f.client.disconnect()
})

test('重复刷新合并为一次查询，失败时保留错误并允许重新连接', async () => {
  const f = fixture(), snapshot = deferred()
  await f.client.connect()
  f.api.invoke = () => snapshot.promise
  const before = f.calls.length
  const first = f.client.connect()
  const second = f.client.connect()
  assert.equal(first, second)
  assert.equal(f.calls.length, before + 1)
  snapshot.reject(new Error('刷新失败'))
  await first
  assert.match(f.errors.at(-1), /刷新失败/)
  assert.equal(f.registrations[0].stopped, true)
  f.api.invoke = async () => state(1, ['恢复'])
  await f.client.connect()
  assert.equal(f.registrations.length, 2)
  assert.deepEqual(f.values, [[], ['恢复']])
  assert.equal(f.errors.at(-1), '')
  f.client.disconnect()
})

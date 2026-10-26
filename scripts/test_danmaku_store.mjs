// 使用真实 Vue/Pinia 调度，验证弹幕批次替换、满缓存淘汰及过滤视图更新。
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'
import * as vue from 'vue'
import * as pinia from 'pinia'
import ts from 'typescript'

const code = ts.transpileModule(readFileSync(new URL('../src/stores/danmaku.ts', import.meta.url), 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText
const module = { exports: {} }
runInNewContext(code, { module, exports: module.exports, require(name) {
  if (name === 'vue') return vue
  if (name === 'pinia') return pinia
  throw new Error(name)
} })

function fixture(t) {
  const instance = pinia.createPinia()
  t.after(() => pinia.disposePinia(instance))
  return module.exports.useDanmakuStore(instance)
}
const message = index => ({
  id: `dm-${index}`, content: `弹幕 ${index}`, timestamp: 1793022281557 + index,
  is_mirror: index % 2 === 0, is_emoticon: false,
  user: { uid: index, name: `观众 ${index}`, is_admin: false, guard_level: 0, wealth_level: 0 },
})
const messages = (start, count) => Array.from({ length: count }, (_, index) => message(start + index))

test('快照和追加都保留最近一万条，按原顺序保留跨房标记', t => {
  const store = fixture(t)
  store.setDanmakuList(messages(0, 10005))
  assert.equal(store.danmakuList.length, 10000)
  assert.equal(store.danmakuList[0].id, 'dm-5')
  store.appendDanmaku(messages(10005, 20))
  assert.equal(store.danmakuList.length, 10000)
  assert.equal(store.danmakuList[0].id, 'dm-25')
  assert.equal(store.danmakuList.at(-1).id, 'dm-10024')
  assert.equal(store.danmakuList.at(-1).is_mirror, true)
  store.appendDanmaku(messages(20000, 10020))
  assert.equal(store.danmakuList.length, 10000)
  assert.equal(store.danmakuList[0].id, 'dm-20020')
  assert.equal(store.danmakuList.at(-1).id, 'dm-30019')
})

test('满缓存时长度不变仍更新过滤视图，清空后可继续接收', async t => {
  const store = fixture(t)
  const minUid = vue.ref(9999)
  const filtered = vue.computed(() => store.danmakuList.filter(item => item.user.uid >= minUid.value))
  const visible = []
  const unwatch = vue.watch(filtered, items => visible.push(Array.from(items, item => item.id)))
  t.after(unwatch)
  store.setDanmakuList(messages(0, 10000))
  await vue.nextTick()
  assert.deepEqual(visible.at(-1), ['dm-9999'])
  store.appendDanmaku(messages(10000, 2))
  await vue.nextTick()
  assert.deepEqual(visible.at(-1), ['dm-9999', 'dm-10000', 'dm-10001'])
  minUid.value = 10001
  await vue.nextTick()
  assert.deepEqual(visible.at(-1), ['dm-10001'])
  store.clearAll()
  await vue.nextTick()
  assert.deepEqual(visible.at(-1), [])
  store.appendDanmaku(messages(10002, 1))
  await vue.nextTick()
  assert.deepEqual(visible.at(-1), ['dm-10002'])
})

test('空批次不刷新视图，相同时间和用户的不同消息不会被合并', async t => {
  const store = fixture(t)
  let changes = 0
  const unwatch = vue.watch(() => store.danmakuList, () => changes++)
  t.after(unwatch)
  const first = message(42)
  const second = { ...first, id: 'second', content: '第二条消息' }
  store.appendDanmaku([first, second])
  await vue.nextTick()
  assert.deepEqual(Array.from(store.danmakuList, item => item.content), ['弹幕 42', '第二条消息'])
  assert.equal(changes, 1)
  store.appendDanmaku([])
  await vue.nextTick()
  assert.equal(changes, 1)
})

// 验证窗口初始化及异步请求竞争，使用真实 Vue 响应式调度。
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'
import * as vue from 'vue'
import ts from 'typescript'

const code = ts.transpileModule(readFileSync(new URL('../src/composables/useAudienceRanks.ts', import.meta.url), 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 }
}).outputText

const settle = async () => { await Promise.resolve(); await vue.nextTick() }
const guardResponse = (uid = 1) => ({ count: 1, list: [{ uid }], total_pages: 1, current_page: 1 })

function fixture(t, { connected = true, cookie = 'test-cookie', mode = 'audience' } = {}) {
  const store = vue.reactive({ isConnected: connected, roomInfo: { roomId: '1' }, contributionRankFull: [] })
  const settings = vue.reactive({ settings: { cookie } })
  const requests = []
  const request = key => new Promise((resolve, reject) => {
    requests.push({ key, roomId: store.roomInfo.roomId, resolve, reject })
  })
  const dependencies = {
    vue,
    '@/stores/danmaku': { useDanmakuStore: () => store },
    '@/stores/settings': { useSettingsStore: () => settings },
    '@/services/blive-client': {
      refreshContributionRank: (_cookie, key) => request(key),
      refreshGuardTopList: () => request('guard')
    },
    '@/services/logger': { createLogger: () => ({ error() {} }) }
  }
  const module = { exports: {} }
  runInNewContext(code, { module, exports: module.exports, require: name => {
    if (!(name in dependencies)) throw new Error(name)
    return dependencies[name]
  } })
  const activeMode = vue.ref(mode)
  const activeRankType = vue.ref('online')
  const scope = vue.effectScope()
  t.after(() => scope.stop())
  const state = scope.run(() => module.exports.useAudienceRanks(activeMode, activeRankType))
  return { store, settings, requests, activeMode, activeRankType, state, scope }
}

test('连接先于 Cookie 就绪时，设置加载后自动获取大航海', async t => {
  const f = fixture(t, { cookie: '', mode: 'guard' })
  assert.equal(f.requests.length, 0)
  f.settings.settings.cookie = 'ready'
  await settle()
  assert.deepEqual(f.requests.map(r => r.key), ['guard'])
  f.requests[0].resolve(guardResponse())
  await settle()
  assert.equal(f.state.guardUsers.value[0].uid, 1)
  assert.equal(f.state.isRefreshing.value, false)
})

test('在线榜慢请求不阻塞大航海预取，切换时复用在途请求', async t => {
  const f = fixture(t)
  assert.deepEqual(f.requests.map(r => r.key), ['online', 'guard'])
  f.activeMode.value = 'guard'
  await settle()
  f.state.handleRefresh()
  assert.equal(f.requests.length, 2)
  f.requests[1].resolve(guardResponse())
  await settle()
  assert.equal(f.state.guardCount.value, 1)
  assert.equal(f.state.isRefreshing.value, false)
  f.activeMode.value = 'audience'
  await settle()
  assert.equal(f.state.isRefreshing.value, true)
})

test('后台榜单失败不污染当前页，进入大航海可重试', async t => {
  const f = fixture(t)
  f.requests[1].reject(new Error('timeout'))
  await settle()
  assert.equal(f.state.loadError.value, '')
  f.activeMode.value = 'guard'
  await settle()
  assert.equal(f.requests[2].key, 'guard')
  f.requests[2].reject(new Error('timeout'))
  await settle()
  assert.match(f.state.loadError.value, /大航海榜加载失败/)
  assert.equal(f.state.isRefreshing.value, false)
  f.state.handleRefresh()
  f.requests[3].resolve(guardResponse())
  await settle()
  assert.equal(f.state.loadError.value, '')
  assert.equal(f.state.guardCount.value, 1)
})

test('换房后旧响应不能回填数据或解除新请求的加载状态', async t => {
  const f = fixture(t, { mode: 'guard' })
  f.store.roomInfo.roomId = '2'
  await settle()
  assert.equal(f.requests.length, 2)
  f.requests[0].resolve(guardResponse(11))
  await settle()
  assert.equal(f.state.guardUsers.value.length, 0)
  assert.equal(f.state.isRefreshing.value, true)
  f.requests[1].resolve(guardResponse(22))
  await settle()
  assert.equal(f.state.guardUsers.value[0].uid, 22)
})

test('断开重连同一房间后忽略旧错误，Cookie 更新也会重新加载', async t => {
  const f = fixture(t, { mode: 'guard' })
  f.store.isConnected = false
  f.store.isConnected = true
  await settle()
  f.requests[0].reject(new Error('old request'))
  await settle()
  assert.equal(f.state.loadError.value, '')
  assert.equal(f.state.isRefreshing.value, true)
  f.requests[1].resolve(guardResponse())
  await settle()
  f.settings.settings.cookie = 'new-cookie'
  await settle()
  assert.equal(f.state.guardCount.value, 0)
  assert.equal(f.requests.length, 3)
})

test('快速切换榜单分别加载，旧榜单失败不显示在当前榜单', async t => {
  const f = fixture(t)
  f.activeRankType.value = 'daily'
  await settle()
  assert.equal(f.requests[2].key, 'daily')
  f.requests[0].reject(new Error('online error'))
  f.requests[2].resolve({ count: 0, list: [], rank_type: 'daily' })
  await settle()
  assert.equal(f.state.loadError.value, '')
  assert.equal(f.state.isRefreshing.value, false)
  assert.equal(f.state.rankLoaded.value.daily, true)
})

test('刷新失败保留上次大航海列表，关闭窗口后丢弃在途响应', async t => {
  const f = fixture(t, { mode: 'guard' })
  f.requests[0].resolve(guardResponse())
  await settle()
  f.state.handleRefresh()
  f.requests[1].reject(new Error('timeout'))
  await settle()
  assert.equal(f.state.guardUsers.value[0].uid, 1)
  f.state.handleRefresh()
  f.scope.stop()
  f.requests[2].resolve(guardResponse(2))
  await settle()
  assert.equal(f.state.guardUsers.value[0].uid, 1)
})

// 使用 Rust 管线实际产生的批次，验证现有客户端和 Pinia 缓存消费协议。
// 此项集成测试需要本地 Rust 工具链及已安装的前端依赖，不连接 Bilibili。
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'
import * as vue from 'vue'
import * as pinia from 'pinia'
import ts from 'typescript'

function load(relativePath, dependencies) {
  const source = readFileSync(new URL(relativePath, import.meta.url), 'utf8')
  const code = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
  }).outputText
  const module = { exports: {} }
  runInNewContext(code, { module, exports: module.exports, require(name) {
    if (!(name in dependencies)) throw new Error(`未提供测试依赖: ${name}`)
    return dependencies[name]
  } })
  return module.exports
}

test('原始盲盒通知经 Rust 聚合、序列化和窗口客户端后保持数量、收入与顺序', async () => {
  const result = spawnSync('cargo', [
    'test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', '--locked', '--offline',
    'gift_pipeline_tests::v1_three_ten_draws_keep_counts_in_updates_and_sqlite',
    '--', '--exact', '--nocapture'
  ], { cwd: fileURLToPath(new URL('..', import.meta.url)), encoding: 'utf8', timeout: 120_000 })
  assert.ifError(result.error)
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
  const payload = result.stdout.split(/\r?\n/).find(line => line.startsWith('GIFT_COMBO_UPDATES='))
  assert.ok(payload, 'Rust 测试应输出真实窗口批次')
  const batches = JSON.parse(payload.slice('GIFT_COMBO_UPDATES='.length))
  assert.equal(batches.length, 3)

  pinia.setActivePinia(pinia.createPinia())
  const stores = load('../src/stores/danmaku.ts', { vue, pinia })
  const handlers = new Map()
  const calls = []
  const client = load('../src/services/blive-client.ts', {
    '@tauri-apps/api/core': { async invoke(name, args) {
      calls.push({ name, args })
      if (name === 'get_data_snapshot') return {}
      if (name === 'get_connection_status') return 'disconnected'
    } },
    '@tauri-apps/api/event': { async listen(name, callback) {
      handlers.set(name, callback)
      return () => handlers.delete(name)
    } },
    '@tauri-apps/api/window': { getCurrentWindow: () => ({ label: 'main' }) },
    '@/stores/danmaku': stores,
    '@/stores/settings': { useSettingsStore: () => ({}) },
    '@/services/logger': { createLogger: () => ({ debug() {}, warn() {}, error() {} }) }
  })
  await client.initBliveClient()
  try {
    for (const batch of batches) handlers.get('blive-data:main')({ payload: batch })
    const store = stores.useDanmakuStore()
    assert.deepEqual(Array.from(store.giftList, gift => [gift.gift_id, gift.num, gift.total_value]), [
      [32128, 11, 1760], [32125, 8, 160], [32126, 11, 990]
    ])
    assert.equal(store.stats.total_revenue, 2910)
    assert.equal(store.contributions[0].total_value, 2910)
    assert.equal(store.giftEffectTriggers.length, 9)
    assert.ok(store.giftList.every((gift, index, items) => !index || items[index - 1].timestamp <= gift.timestamp))
  } finally {
    await client.cleanupBliveClient()
  }
  assert.equal(handlers.size, 0)
  assert.equal(calls.at(-1).name, 'unsubscribe_events')
})

// 验证调色数值与本机字体请求缓存，避免无效颜色或并发刷新污染配置。
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'
import ts from 'typescript'

function load(path, dependencies = {}) {
  const code = ts.transpileModule(readFileSync(new URL(path, import.meta.url), 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 },
  }).outputText
  const module = { exports: {} }
  runInNewContext(code, { module, exports: module.exports, require: name => dependencies[name] })
  return module.exports
}
const color = load('../src/utils/color.ts')

test('颜色输入兼容大小写、简写与无井号值，无效输入不提交', () => {
  assert.equal(color.normalizeHex(' #AbC '), '#aabbcc')
  assert.equal(color.normalizeHex('80DEEA'), '#80deea')
  for (const value of ['', '#12', '#gggggg', '#12345678', 'red', 'url(test)']) assert.equal(color.normalizeHex(value), null)
})

test('HSV 调色往返保留 RGB，覆盖黑白、灰度、色相端点与中间色', () => {
  for (const hex of ['#000000', '#ffffff', '#888888', '#ff0000', '#00ff00', '#0000ff', '#ffb347', '#80deea', '#123456']) {
    assert.equal(color.hsvToHex(color.hexToHsv(hex)), hex)
  }
  for (let rgb = 0; rgb <= 0xffffff; rgb += 7919) {
    const hex = `#${rgb.toString(16).padStart(6, '0')}`
    assert.equal(color.hsvToHex(color.hexToHsv(hex)), hex)
  }
  assert.equal(color.hsvToHex({ h: 360, s: 100, v: 100 }), '#ff0000')
  assert.equal(color.hsvToHex({ h: 0, s: 0, v: 100 }), '#ffffff')
})

test('本机字体查询合并并发请求，可刷新，旧失败不清除较新的缓存', async () => {
  const requests = []
  const service = load('../src/services/system-fonts.ts', { '@tauri-apps/api/core': { invoke: command => {
    assert.equal(command, 'get_system_fonts')
    return new Promise((resolve, reject) => requests.push({ resolve, reject }))
  } } })
  const first = service.getSystemFonts()
  assert.equal(service.getSystemFonts(), first)
  const fresh = service.getSystemFonts(true)
  requests[1].resolve([{family:'KaiTi',label:'楷体'},{family:'Arial',label:'Arial'}])
  assert.equal((await fresh).length, 2)
  requests[0].reject(new Error('旧请求失败'))
  await assert.rejects(first)
  assert.equal(service.getSystemFonts(), fresh)
  const failed = service.getSystemFonts(true)
  requests[2].reject(new Error('刷新失败'))
  await assert.rejects(failed)
  const retry = service.getSystemFonts()
  assert.notEqual(retry, failed)
  requests[3].resolve([])
  await retry
  assert.equal(service.fontPreviewFamily('测试"字体\\'), '"测试\\"字体\\\\", var(--font-family)')
})

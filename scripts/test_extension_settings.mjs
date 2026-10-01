// 验证同一个设置入口可装载不同扩展，并分别保存窗口状态。
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'
import ts from 'typescript'

function load(path, dependencies) {
  const code = ts.transpileModule(readFileSync(new URL(path, import.meta.url), 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 },
  }).outputText
  const module = { exports: {} }
  runInNewContext(code, { module, exports: module.exports, require(name) {
    if (!(name in dependencies)) throw new Error(name)
    return dependencies[name]
  } })
  return module.exports
}

function fixture() {
  const registry = load('../src/components/extension/settings-registry.ts', { vue: { defineAsyncComponent: loader => ({ loader }) } })
  const calls = []
  const manager = load('../src/services/window-manager.ts', {
    '@tauri-apps/api/core': { invoke: async (command, args) => { calls.push([command, JSON.parse(JSON.stringify(args))]) } },
    '@tauri-apps/api/window': {},
    '@/services/logger': { createLogger: () => ({ debug() {}, error() {} }) },
    '@/components/extension/settings-registry': registry,
  })
  return { registry, manager, calls }
}

test('点歌设置通过通用命令传递内容 ID 和标题，使用独立窗口标签', async () => {
  const f = fixture()
  await f.manager.createExtensionSettingsWindow('song-request')
  assert.deepEqual(f.calls, [
    ['create_extension_settings_window', { extensionId: 'song-request', title: '点歌机设置' }],
    ['set_window_open_state', { label: 'extension-settings-song-request', isOpen: true }],
  ])
})

test('注册另一个扩展的内容后复用同一入口，窗口状态与点歌独立', async () => {
  const f = fixture()
  const content = { name: 'VotingSettings' }
  f.registry.extensionSettingsRegistry.voting = { title: '投票设置', component: content }
  assert.equal(f.registry.getExtensionSettings('voting').component, content)
  await f.manager.createExtensionSettingsWindow('voting')
  assert.deepEqual(f.calls, [
    ['create_extension_settings_window', { extensionId: 'voting', title: '投票设置' }],
    ['set_window_open_state', { label: 'extension-settings-voting', isOpen: true }],
  ])
})

test('未注册内容与原型属性不会创建空白窗口', async () => {
  const f = fixture()
  for (const id of ['missing', 'constructor', '__proto__']) {
    assert.equal(f.registry.getExtensionSettings(id), undefined)
    await assert.rejects(f.manager.createExtensionSettingsWindow(id), /尚未提供设置/)
  }
  assert.deepEqual(f.calls, [])
})

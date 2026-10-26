// 编译实际编辑器，验证数字输入与手动校准请求，不依赖桌面环境。
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'
import * as vue from 'vue'
import { parse, compileScript } from 'vue/compiler-sfc'
import ts from 'typescript'

const { descriptor } = parse(readFileSync(new URL('../src/components/extension/WishGoalEditor.vue', import.meta.url), 'utf8'))
const script = compileScript(descriptor, { id: 'wish-goal-editor-test' })
const code = ts.transpileModule(script.content, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText
const module = { exports: {} }
runInNewContext(code, { module, exports: module.exports, require(name) {
  if (name === 'vue') return vue
  if (name === '@/assets/gift.json') return []
  throw new Error(name)
} })

function fixture(t, goal = { id: 'captain', gift_id: null, gift_name: '舰长', current: 2, target: 5 }) {
  const props = vue.reactive({ goal, busy: false }), requests = [], scope = vue.effectScope()
  t.after(() => scope.stop())
  const editor = scope.run(() => module.exports.default.setup(props, {
    expose() {}, emit: (event, request) => { if (event === 'save') requests.push(JSON.parse(JSON.stringify(request))) },
  }))
  return { editor, props, requests }
}

// 使用 Vue 实际的 number 输入指令，覆盖未写 .number 也会自动转成数字的情况。
function inputNumber(model, value) {
  const listeners = new Map()
  const input = { type: 'number', value, addEventListener: (event, fn) => listeners.set(event, fn) }
  vue.vModelText.created(input, { modifiers: {} }, { props: { type: 'number', 'onUpdate:modelValue': value => { model.value = value } } })
  listeners.get('input')({ target: input })
}

test('手填礼物 ID 后修改数量可保存，清空 ID 恢复按名称匹配', t => {
  const { editor, requests } = fixture(t)
  inputNumber(editor.giftId, '10003')
  inputNumber(editor.current, '7')
  editor.countEdited.value = true
  editor.save()
  assert.deepEqual(requests[0], { type: 'update_goal', id: 'captain', gift_id: 10003, gift_name: '舰长', current: 7, target: 5 })
  inputNumber(editor.giftId, '')
  editor.save()
  assert.equal(requests[1].gift_id, null)
})

test('仅改目标不提交旧数量，手动数量不会被实时推送覆盖', async t => {
  const { editor, props, requests } = fixture(t)
  props.goal.current = 3
  await vue.nextTick()
  assert.equal(editor.current.value, 3)
  inputNumber(editor.target, '10')
  editor.save()
  assert.equal('current' in requests[0], false)
  inputNumber(editor.current, '0')
  editor.countEdited.value = true
  props.goal.current = 4
  await vue.nextTick()
  assert.equal(editor.current.value, 0)
  editor.save()
  assert.equal(requests[1].current, 0)
})

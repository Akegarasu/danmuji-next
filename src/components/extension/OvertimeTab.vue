<script setup lang="ts">
import { computed, onActivated, onDeactivated, onBeforeUnmount, ref } from 'vue'
import giftCatalog from '@/assets/gift.json'
import ExtSelect from '@/components/common/ExtSelect.vue'
import SettingsSectionTitle from '@/components/common/SettingsSectionTitle.vue'
import SettingsToggle from '@/components/common/SettingsToggle.vue'
import OverlayStyleSettings from './OverlayStyleSettings.vue'
import { createExtensionClient, getOverlayServer, startOverlayServer, type OverlayServerInfo } from '@/services/extensions'
import type { GiftRule, OvertimeRequest, OvertimeSnapshot, TimerAction, TimerConfig, RandomAction, AppliedAction } from '@/types/overtime'

const state = ref<OvertimeSnapshot | null>(null)
const server = ref<OverlayServerInfo | null>(null)
const config = ref<TimerConfig | null>(null)
const dirty = ref(false)
const busy = ref(false)
const error = ref('')
const message = ref('')
const port = ref(17654)
const manualAction = ref<Exclude<TimerAction, 'random'>>('add')
const manualValue = ref(60)
const preview = ref(false)
const giftSearch = ref('')
let active = false
let interval: ReturnType<typeof setInterval> | undefined
let revision = 0
const actions: { value: TimerAction; label: string }[] = [
  { value: 'add', label: '增加时间（秒）' },
  { value: 'subtract', label: '减少时间（秒）' },
  { value: 'multiply', label: '剩余时间乘以' },
  { value: 'divide', label: '剩余时间除以' },
  { value: 'set_time', label: '设置时间（秒）' },
  { value: 'set_rate', label: '倒计时速度倍率' },
  { value: 'clear', label: '清空时间' },
  { value: 'random', label: '随机加减乘除' },
]
const fixedActions = actions.filter((action): action is { value: Exclude<TimerAction, 'random'>; label: string } => action.value !== 'random')
const blindGiftModes: { value: TimerConfig['blind_gift_mode']; label: string }[] = [
  { value: 'revealed', label: '处理爆出的礼物（默认）' },
  { value: 'original', label: '直接处理盲盒' },
]
function changeBlindGiftMode(value: TimerConfig['blind_gift_mode']) {
  if (!config.value) return
  config.value.blind_gift_mode = value
  dirty.value = true
}
const randomActions: { value: RandomAction; label: string; unit: string }[] = [
  { value: 'add', label: '增加', unit: '秒' },
  { value: 'subtract', label: '减少', unit: '秒' },
  { value: 'multiply', label: '乘以', unit: '倍' },
  { value: 'divide', label: '除以', unit: '倍' },
]
function toggleRandomAction(rule: GiftRule, action: RandomAction, enabled: boolean) {
  if (enabled && !rule.random_ranges.some(range => range.action === action)) {
    const time = action === 'add' || action === 'subtract'
    rule.random_ranges.push({ action, min: time ? 10 : 1, max: time ? 60 : 2 })
  } else if (!enabled) rule.random_ranges = rule.random_ranges.filter(range => range.action !== action)
  dirty.value = true
}
function changeRuleAction(rule: GiftRule, action: TimerAction) {
  rule.action = action
  if (rule.action === 'random' && !rule.random_ranges.length) {
    randomActions.forEach(action => toggleRandomAction(rule, action.value, true))
  }
  dirty.value = true
}
function resultLabel(result: AppliedAction) {
  const symbol: Record<string, string> = { add: '+', subtract: '-', multiply: '×', divide: '÷' }
  return `随机${symbol[result.action] ?? ''}${result.value}${['add', 'subtract'].includes(result.action) ? '秒' : ''}`
}
const gifts = computed(() => {
  const query = giftSearch.value.trim().toLowerCase()
  if (!query) return []
  return giftCatalog.filter(gift => gift.name.toLowerCase().includes(query) || String(gift.id).includes(query)).slice(0, 12)
})
const timeText = computed(() => {
  const seconds = Math.ceil((state.value?.remaining_ms ?? 0) / 1000)
  return [Math.floor(seconds / 3600), Math.floor(seconds / 60) % 60, seconds % 60].map(value => String(value).padStart(2, '0')).join(':')
})
const statusText = computed(() => !state.value?.config.enabled ? '未启用' : state.value.remaining_ms <= 0 ? '下班啦！' : state.value.running ? `计时中 · ${state.value.rate} 倍速` : '已暂停')
const initialMinutes = computed({
  get: () => (config.value?.initial_seconds ?? 0) / 60,
  set: value => { if (config.value) { config.value.initial_seconds = Number(value) * 60; dirty.value = true } },
})

const client = createExtensionClient('overtime', snapshot => {
  state.value = snapshot
  if (!config.value || !dirty.value) config.value = structuredClone(snapshot.config)
}, message => { error.value = message })

async function refreshServer() {
  const token = ++revision
  try {
    const info = await getOverlayServer()
    if (!active || token !== revision) return
    if (!server.value) port.value = info.port
    server.value = info
  } catch (cause) { if (active && token === revision) error.value = String(cause) }
}
async function execute(request: OvertimeRequest) {
  busy.value = true; error.value = ''; message.value = ''
  try {
    await client.request(request)
    if (request.type === 'configure') {
      if (state.value) config.value = JSON.parse(JSON.stringify(state.value.config)) as TimerConfig
      dirty.value = false
      message.value = '设置已保存'
    }
  } catch (cause) { error.value = String(cause) }
  finally { busy.value = false }
}
function save() {
  if (!config.value) return
  // Vue 响应式代理不可直接 structuredClone，IPC 前生成普通 JSON 对象。
  const value = JSON.parse(JSON.stringify(config.value)) as TimerConfig
  value.rules.forEach(rule => { if (rule.action === 'random') rule.value = 0 })
  return execute({ type: 'configure', config: value })
}
function addRule(gift?: { id: number; name: string }) {
  if (!config.value || config.value.rules.length >= 100) return
  config.value.rules.push({
    id: crypto.randomUUID(), enabled: true, gift_id: gift?.id ?? null,
    gift_name: gift?.name ?? '', action: 'add', value: 60, per_gift: true, random_ranges: []
  })
  giftSearch.value = ''; dirty.value = true
}
function removeRule(index: number) { config.value?.rules.splice(index, 1); dirty.value = true }
function changeGiftId(rule: GiftRule, event: Event) {
  const input = (event.target as HTMLInputElement).value.trim()
  rule.gift_id = input ? Number(input) : null
  dirty.value = true
}
async function restartServer() {
  busy.value = true; error.value = ''; message.value = ''
  try {
    server.value = await startOverlayServer(Number(port.value))
    message.value = '本地服务已就绪。更换端口后请同步更新 OBS 地址。'
  } catch (cause) { error.value = String(cause); server.value = await getOverlayServer().catch(() => server.value) }
  finally { busy.value = false }
}
async function copyUrl() {
  if (!server.value?.url) return
  try { await navigator.clipboard.writeText(server.value.url); message.value = 'OBS 地址已复制' }
  catch { error.value = '复制失败，请选中下方地址手动复制。' }
}
function stop() { active = false; ++revision; clearInterval(interval); client.disconnect() }
onActivated(() => {
  active = true; void client.connect(); void refreshServer()
  interval = setInterval(() => { if (!busy.value) void refreshServer() }, 1000)
})
onDeactivated(stop)
onBeforeUnmount(stop)
</script>

<template>
  <div class="overtime-panel">
    <div v-if="error" class="feedback error" role="alert">{{ error }}</div>
    <div v-if="message" class="feedback success" role="status">{{ message }}</div>
    <div v-if="server?.persistence_error" class="feedback error">状态保存异常：{{ server.persistence_error }}</div>
    <template v-if="config && state">
      <section class="clock-panel">
        <div class="clock-caption">加班机 <span>{{ statusText }}</span></div>
        <div class="clock-value">{{ timeText }}</div>
        <div class="button-row">
          <button class="ext-btn ext-btn--primary" :disabled="busy || !state.config.enabled"
            @click="execute({ type: state.running ? 'pause' : 'start' })">{{ state.running ? '暂停' : '开始 / 继续'
            }}</button>
          <button class="ext-btn" :disabled="busy" @click="execute({ type: 'reset' })">重置为初始时间</button>
          <button class="ext-btn ext-btn--danger" :disabled="busy"
            @click="execute({ type: 'apply', action: 'clear', value: 0 })">清空</button>
        </div>
        <div class="manual-row">
          <ExtSelect v-model="manualAction" :options="fixedActions" class="manual-select" aria-label="手动操作" />
          <input v-if="manualAction !== 'clear'" v-model.number="manualValue" type="number" min="0" step="any"
            aria-label="手动操作数值">
          <button class="ext-btn" :disabled="busy"
            @click="execute({ type: 'apply', action: manualAction, value: manualAction === 'clear' ? 0 : Number(manualValue) })">执行</button>
        </div>
      </section>
      <section>
        <SettingsSectionTitle class="section-heading">OBS 浏览器源</SettingsSectionTitle>
        <div class="feedback error" v-if="server?.error">{{ server.error }}</div>
        <input class="url-input" :value="server?.url ?? '服务尚未启动'" readonly aria-label="OBS 浏览器源地址"
          @focus="($event.target as HTMLInputElement).select()">
        <div class="button-row">
          <button class="ext-btn ext-btn--primary" :disabled="!server?.url" @click="copyUrl">复制地址</button>
          <button class="ext-btn" :disabled="!server?.url" @click="preview = !preview">{{ preview ? '收起预览' : '预览皮肤'
            }}</button>
          <label class="port-label">端口 <input v-model.number="port" type="number" min="1024" max="65535"
              aria-label="服务端口"></label>
          <button class="ext-btn" :disabled="busy" @click="restartServer">应用端口 / 重试</button>
        </div>
        <details class="help-details"><summary>OBS 接入说明</summary><p>在 OBS 中添加浏览器源并粘贴地址，建议尺寸 600 × 800。使用时保持弹幕姬运行并连接直播间。</p></details>
        <iframe v-if="preview && server?.url" class="overlay-preview" :src="server.url" title="加班机 OBS 预览" />
        <div class="appearance-settings">
          <OverlayStyleSettings v-model="config.overlay_style" preset="overtime" :disabled="busy" @update:model-value="dirty = true" />
        </div>
      </section>
      <section @input="dirty = true" @change="dirty = true">
        <SettingsSectionTitle class="section-heading">基本设置 <span v-if="dirty" class="unsaved">未保存</span></SettingsSectionTitle>
        <div class="settings-row">
          <SettingsToggle v-model="config.enabled" class="toggle-setting" label="启用加班机" @change="dirty = true" />
          <label class="field-setting">初始时间（分钟）<input v-model.number="initialMinutes" type="number" min="0" max="5256000" step="any"></label>
        </div>
        <div class="settings-row">
          <SettingsToggle v-model="config.show_rules" class="toggle-setting" label="显示礼物规则" @change="dirty = true" />
          <SettingsToggle v-model="config.show_notice" class="toggle-setting" label="显示投喂提示" @change="dirty = true" />
        </div>
        <div class="settings-row">
          <div class="field-setting"><span>盲盒处理方式</span>
            <ExtSelect :model-value="config.blind_gift_mode" :options="blindGiftModes" aria-label="盲盒处理方式"
              @update:model-value="changeBlindGiftMode" />
          </div>
        </div>
        <p v-if="config.blind_gift_mode === 'original'">按“心动盲盒”等盲盒本身匹配，不触发爆出礼物规则。</p>
        <p v-else>按“棉花糖”等爆出礼物匹配，不触发盲盒本身的规则。</p>
      </section>
      <section>
        <SettingsSectionTitle class="section-heading">礼物触发规则 <span>{{ config.rules.length }} / 100</span></SettingsSectionTitle>
        <div class="gift-search">
          <input v-model="giftSearch" placeholder="搜索礼物名称或 ID…" aria-label="搜索礼物">
          <button class="ext-btn" :disabled="config.rules.length >= 100" @click="addRule()">+ 自定义礼物</button>
        </div>
        <div class="gift-results" v-if="giftSearch">
          <button v-for="gift in gifts" :key="gift.id" :disabled="config.rules.length >= 100" @click="addRule(gift)">{{
            gift.name }} <span class="gift-id">#{{ gift.id }}</span></button>
          <p v-if="!gifts.length">未找到礼物，可添加自定义规则；大航海可填写“舰长 / 提督 / 总督”。</p>
        </div>
        <details class="help-details">
          <summary>规则与计时说明</summary>
          <p>填写 ID 时只匹配 ID；留空则匹配完整名称。同一礼物的多条规则按列表顺序执行。</p>
          <p>按个执行时使用本次通知的数量，例如 3 个 ×2 会执行 ×8；取消则每条通知执行一次。十连盲盒可能分成多条通知。设置时间、速度和清空操作始终执行一次。</p>
          <p>随机规则每条通知抽取一次操作和数值，按个执行时复用结果。加减为整数秒，乘除保留两位小数，范围包含两端。</p>
          <p>暂停时仍接收礼物，禁用后忽略礼物。初始时间在重置后应用，重置会暂停并恢复 1 倍速；重启后保留剩余时间并暂停。</p>
        </details>
        <div v-if="!config.rules.length" class="empty-rules">还没有规则，搜索或添加礼物开始配置。</div>
        <article v-for="(rule, index) in config.rules" :key="rule.id" class="rule-card" @input="dirty = true"
          @change="dirty = true">
          <div class="rule-heading">
            <label><input v-model="rule.enabled" type="checkbox">规则 {{ index + 1 }}</label>
            <button class="ext-btn ext-btn--danger" @click="removeRule(index)">删除</button>
          </div>
          <div class="rule-grid">
            <label>礼物名称<input v-model="rule.gift_name" maxlength="80" placeholder="如：小心心、舰长"></label>
            <label>礼物 ID（选填）<input :value="rule.gift_id ?? ''" type="number" min="1" step="1" placeholder="留空按名称匹配"
                @input="changeGiftId(rule, $event)"></label>
            <div class="field-setting"><span>触发操作</span>
              <ExtSelect :model-value="rule.action" :options="actions" :aria-label="`规则 ${index + 1} 触发操作`"
                @update:model-value="changeRuleAction(rule, $event)" />
            </div>
            <label v-if="!['clear', 'random'].includes(rule.action)">数值<input v-model.number="rule.value" type="number"
                min="0" step="any"></label>
          </div>
          <div v-if="rule.action === 'random'" class="random-settings">
            <div class="random-choices">
              <label v-for="action in randomActions" :key="action.value">
                <input type="checkbox" :checked="rule.random_ranges.some(range => range.action === action.value)"
                  @change="toggleRandomAction(rule, action.value, ($event.target as HTMLInputElement).checked)">{{
                action.label }}
              </label>
            </div>
            <div v-for="range in rule.random_ranges" :key="range.action" class="random-range">
              <span>{{randomActions.find(action => action.value === range.action)?.label}}（{{randomActions.find(action => action.value === range.action)?.unit}}）</span>
              <label>最小<input v-model.number="range.min" type="number"
                  :min="['add', 'subtract'].includes(range.action) ? 0 : 0.01"
                  :max="['add', 'subtract'].includes(range.action) ? 315360000 : 100"
                  :step="['add', 'subtract'].includes(range.action) ? 1 : 0.01"></label>
              <label>最大<input v-model.number="range.max" type="number"
                  :min="['add', 'subtract'].includes(range.action) ? 0 : 0.01"
                  :max="['add', 'subtract'].includes(range.action) ? 315360000 : 100"
                  :step="['add', 'subtract'].includes(range.action) ? 1 : 0.01"></label>
            </div>
          </div>
          <label v-if="!['set_time', 'set_rate', 'clear'].includes(rule.action)" class="count-mode"><input v-model="rule.per_gift" type="checkbox">按礼物个数执行</label>
        </article>
      </section>
      <div class="save-bar"><button class="ext-btn ext-btn--primary" :disabled="busy || !dirty" @click="save">{{ busy ?
        '处理中…' : '保存设置与规则' }}</button></div>
      <section v-if="state.notices.length">
        <SettingsSectionTitle class="section-heading">最近触发</SettingsSectionTitle>
        <div class="history-row" v-for="notice in [...state.notices].reverse().slice(0, 5)" :key="notice.id">
          <span>{{ notice.sender_name }} · {{ notice.gift_name }} ×{{ notice.num }}<span class="history-result"
              v-for="(result, index) in notice.results.filter(result => result.random)" :key="index">{{
                resultLabel(result) }}</span></span>
          <strong>{{ notice.delta_ms >= 0 ? '+' : '-' }}{{ Math.round(Math.abs(notice.delta_ms) / 1000) }} 秒</strong>
        </div>
      </section>
    </template>
    <p v-else>正在加载加班机…</p>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/extension-shared.scss';
@use '@/styles/settings-controls' as controls;

.overtime-panel {
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
  container-type: inline-size;
  padding: 16px;
  color: var(--text-primary);
  font-size: var(--font-size-sm);
  line-height: 1.5
}

section {
  margin-bottom: 24px
}

.section-heading {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px
}

.section-heading span {
  color: var(--text-muted);
  font-size: var(--font-size-sm);
  font-weight: normal
}

.section-heading .unsaved {
  color: #eeb854
}

p {
  @include controls.hint;
  margin: 6px 0 0
}

input:not([type=checkbox]) {
  @include controls.control;
}

input[type=checkbox] {
  accent-color: var(--accent-primary);
  flex-shrink: 0;
  margin: 0
}

label {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--text-secondary);
  font-size: var(--font-size-sm)
}

.button-row {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  align-items: center;
  margin-top: 10px
}

button:disabled {
  opacity: .45;
  cursor: not-allowed
}

.clock-panel {
  text-align: center;
  padding: 16px;
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius);
  background: var(--bg-secondary)
}

.clock-caption {
  display: flex;
  justify-content: space-between;
  text-align: left
}

.clock-caption span {
  font-size: var(--font-size-sm);
  color: var(--text-secondary)
}

.clock-value {
  font-size: clamp(32px, calc(var(--font-size-base) * 2.85), 72px);
  font-weight: 700;
  font-variant-numeric: tabular-nums;
  letter-spacing: 0.05em;
  padding: 12px 0
}

.clock-panel .button-row {
  justify-content: center
}

.manual-row {
  display: flex;
  gap: 8px;
  align-items: stretch;
  margin-top: 16px
}

.manual-row .manual-select {
  flex: 2
}

.manual-row input {
  flex: 1;
  width: 70px
}

.manual-row button {
  white-space: nowrap
}

.port-label input {
  width: max(90px, 5em)
}

.settings-row {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 220px), 1fr));
  align-items: center;
  gap: 16px;
  margin-top: 16px
}

.field-setting {
  display: flex;
  gap: 6px;
  color: var(--text-secondary);
  font-size: var(--font-size-sm);
  flex-direction: column;
  align-items: stretch;
  min-width: 0
}

.toggle-setting {
  justify-content: space-between;
  min-height: 34px
}

.help-details {
  @include controls.hint;
  margin-top: 12px
}

.help-details summary {
  color: var(--text-secondary);
  cursor: pointer;
  width: fit-content
}

.help-details summary:focus-visible {
  outline: 1px solid var(--accent-primary);
  outline-offset: 4px
}

.feedback {
  padding: 10px 12px;
  margin-bottom: 10px;
  border-radius: 6px;
  overflow-wrap: anywhere;
  font-size: var(--font-size-sm);
  line-height: 1.6
}

.error {
  color: #ff9292;
  background: #f5555515
}

.success {
  color: #8edda9;
  background: #40b97815
}

.gift-search {
  display: flex;
  flex-wrap: wrap;
  gap: 8px
}

.gift-search input {
  flex: 1 1 180px;
}

.gift-search button {
  white-space: nowrap
}

.gift-results {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 8px
}

.gift-results button {
  @include controls.button;
  border: 1px solid var(--border-color);
  border-radius: controls.$radius;
  color: var(--text-primary);
  background: var(--bg-primary);
  cursor: pointer
}

.gift-id {
  color: var(--text-muted)
}

.empty-rules {
  text-align: center;
  padding: 24px 0;
  color: var(--text-muted);
  font-size: var(--font-size-sm)
}

.rule-card {
  margin-top: 12px;
  padding: 16px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: var(--bg-primary)
}

.rule-heading {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px
}

.rule-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 220px), 1fr));
  gap: 16px
}

.rule-grid label {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  font-size: var(--font-size-sm);
  color: var(--text-secondary);
  gap: 6px
}

.count-mode {
  margin-top: 16px;
  font-size: var(--font-size-sm);
  line-height: 1.5
}

.save-bar {
  position: sticky;
  bottom: -16px;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px;
  margin: 0 -16px 16px;
  background: var(--bg-secondary);
  border-top: 1px solid var(--border-color);
  z-index: 1
}

.overlay-preview {
  display: block;
  width: 100%;
  height: 480px;
  margin-top: 12px;
  border: 0;
  background: repeating-conic-gradient(#292a2e 0% 25%, #202126 0% 50%) 50%/20px 20px
}

.appearance-settings { margin-top: 20px; }

.history-row {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 0;
  font-size: var(--font-size-sm)
}

.history-row > span {
  overflow: hidden;
  min-width: 0;
  overflow-wrap: anywhere
}

.history-row strong {
  white-space: nowrap;
  color: var(--accent-primary)
}

.random-settings {
  margin-top: 10px;
  padding: 10px;
  border: 1px solid var(--border-color);
  border-radius: 5px
}

.random-choices {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
  margin-bottom: 8px
}

.random-range {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 8px 0;
  font-size: var(--font-size-sm)
}

.random-range>span {
  flex-shrink: 0
}

.random-range label {
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  font-size: var(--font-size-sm)
}

.random-range input {
  width: 100%;
  min-width: 0
}

.history-result {
  display: block;
  margin-top: 3px;
  color: var(--text-secondary)
}

@container (max-width: 460px) {
  .settings-row,
  .rule-grid {
    grid-template-columns: 1fr
  }

  .clock-value {
    font-size: clamp(28px, calc(var(--font-size-base) * 2.3), 56px)
  }

  .manual-row {
    flex-wrap: wrap
  }

  .manual-row .manual-select {
    flex-basis: 100%
  }

  .random-range {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
    margin: 16px 0
  }

  .random-range > span:first-child {
    grid-column: 1 / -1
  }

  .random-range label {
    flex-direction: column;
    align-items: stretch
  }
}
</style>

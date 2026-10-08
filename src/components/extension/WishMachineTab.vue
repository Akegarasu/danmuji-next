<script setup lang="ts">
import { computed, onActivated, onBeforeUnmount, onDeactivated, ref } from 'vue'
import ConfirmDialog from '@/components/common/ConfirmDialog.vue'
import SettingsSectionTitle from '@/components/common/SettingsSectionTitle.vue'
import SettingsToggle from '@/components/common/SettingsToggle.vue'
import ExtSelect from '@/components/common/ExtSelect.vue'
import WishGoalEditor from './WishGoalEditor.vue'
import WishStyleSettings from './WishStyleSettings.vue'
import { createExtensionClient, getOverlayServer, startOverlayServer, type OverlayServerInfo } from '@/services/extensions'
import type { WishConfig, WishGoal, WishRequest, WishSnapshot } from '@/types/wish-machine'

const state = ref<WishSnapshot | null>(null)
const config = ref<WishConfig | null>(null)
const dirty = ref(false)
const busy = ref(false)
const error = ref('')
const message = ref('')
const editingId = ref<string | null>(null)
const adding = ref(false)
const removing = ref<WishGoal | null>(null)
const confirmRemove = ref(false)
const server = ref<OverlayServerInfo | null>(null)
const port = ref(17654)
let active = false
let revision = 0
let interval: ReturnType<typeof setInterval> | undefined
const modes: { value: WishConfig['blind_gift_mode']; label: string }[] = [
  { value: 'original', label: '统计盲盒本身（默认）' },
  { value: 'revealed', label: '统计盲盒爆出的礼物' },
]
const url = computed(() => server.value?.url ? new URL('/overlays/wish-machine/', server.value.url).href : '')
const completed = computed(() => state.value?.goals.filter(goal => goal.current >= goal.target).length ?? 0)
const client = createExtensionClient('wish-machine', snapshot => {
  state.value = snapshot
  if (!dirty.value) config.value = structuredClone(snapshot.config)
}, value => { error.value = value })

async function execute(request: WishRequest) {
  if (busy.value) return
  busy.value = true; error.value = ''; message.value = ''
  try {
    await client.request(request)
    if (request.type === 'configure') {
      dirty.value = false
      if (state.value) config.value = JSON.parse(JSON.stringify(state.value.config)) as WishConfig
      message.value = '设置已保存。样式修改请复制 CSS 到 OBS。'
    } else {
      editingId.value = null; adding.value = false; confirmRemove.value = false
      message.value = request.type === 'remove_goal' ? '心愿已删除' : request.type === 'move_goal' ? '顺序已更新' : '心愿已保存，OBS 已同步'
    }
  } catch (cause) { error.value = String(cause) }
  finally { busy.value = false }
}
function saveConfig() {
  if (config.value) void execute({ type: 'configure', config: JSON.parse(JSON.stringify(config.value)) as WishConfig })
}
function changeStyle(value: WishConfig) { config.value = value; dirty.value = true }
function remove(goal: WishGoal) { removing.value = goal; confirmRemove.value = true }
async function refreshServer() {
  const token = ++revision
  try {
    const info = await getOverlayServer()
    if (!active || token !== revision) return
    if (!server.value) port.value = info.port
    server.value = info
  } catch (cause) { if (active && token === revision) error.value = String(cause) }
}
async function restartServer() {
  if (busy.value) return
  busy.value = true; error.value = ''
  ++revision
  try { server.value = await startOverlayServer(Number(port.value)); message.value = 'OBS 服务已就绪，更换端口后请重新复制地址。' }
  catch (cause) { error.value = String(cause); server.value = await getOverlayServer().catch(() => server.value) }
  finally { busy.value = false }
}
async function copyUrl() {
  try { await navigator.clipboard.writeText(url.value); message.value = '心愿机 OBS 地址已复制' }
  catch { error.value = '复制失败，请选中地址手动复制。' }
}
function stop() { active = false; ++revision; clearInterval(interval); client.disconnect() }
onActivated(() => {
  active = true; void client.connect(); void refreshServer()
  clearInterval(interval)
  interval = setInterval(() => { if (!busy.value) void refreshServer() }, 1500)
})
onDeactivated(stop)
onBeforeUnmount(stop)
</script>

<template>
  <div class="wish-panel">
    <div v-if="error" class="feedback error" role="alert">{{ error }} <button class="ext-btn" :disabled="busy" @click="client.connect()">刷新状态</button></div>
    <div v-if="message" class="feedback success" role="status">{{ message }}</div>
    <div v-if="server?.persistence_error" class="feedback error">状态保存异常：{{ server.persistence_error }}</div>
    <template v-if="state && config">
      <section>
        <div class="heading"><div><h2>心愿机</h2><p>把直播里的每一份心意，变成看得见的小目标。</p></div><span class="status" :class="{ enabled: state.config.enabled }">{{ state.config.enabled ? '正在累计' : '累计已暂停' }}</span></div>
        <div class="goal-summary"><span>{{ state.goals.length }} 个心愿 · 已达成 {{ completed }} 个</span><button class="ext-btn ext-btn--primary" :disabled="busy || adding || editingId !== null || state.goals.length >= 20" @click="adding = true">+ 添加心愿</button></div>
        <WishGoalEditor v-if="adding" :busy="busy" @save="execute" @cancel="adding = false" />
        <p v-if="!state.goals.length && !adding" class="empty">还没有心愿，添加一个目标礼物开始吧。</p>
        <article v-for="(goal, index) in state.goals" :key="goal.id" class="goal-card">
          <div class="goal-row">
            <span class="goal-index">{{ String(index + 1).padStart(2, '0') }}</span>
            <div class="goal-title"><strong>{{ goal.gift_name }}</strong><small>{{ goal.gift_id ? `礼物 ID ${goal.gift_id}` : '按名称匹配' }}</small></div>
            <div class="goal-count" :class="{ complete: goal.current >= goal.target }"><strong>{{ goal.current }}</strong><span>/ {{ goal.target }}</span><small v-if="goal.current >= goal.target">已达成</small></div>
          </div>
          <progress :value="Math.min(goal.current, goal.target)" :max="goal.target" :aria-label="`${goal.gift_name}：${goal.current} / ${goal.target}`" />
          <div class="goal-actions">
            <button class="ext-btn" :disabled="busy || adding || (editingId !== null && editingId !== goal.id)" @click="editingId = editingId === goal.id ? null : goal.id">{{ editingId === goal.id ? '收起编辑' : '修改礼物 / 数量' }}</button>
            <div class="move-actions"><button class="ext-btn" :disabled="busy || index === 0 || editingId !== null || adding" :aria-label="`上移${goal.gift_name}`" @click="execute({ type: 'move_goal', id: goal.id, direction: -1 })">↑</button><button class="ext-btn" :disabled="busy || index === state.goals.length - 1 || editingId !== null || adding" :aria-label="`下移${goal.gift_name}`" @click="execute({ type: 'move_goal', id: goal.id, direction: 1 })">↓</button><button class="ext-btn ext-btn--danger" :disabled="busy || adding || editingId !== null" @click="remove(goal)">删除</button></div>
          </div>
          <WishGoalEditor v-if="editingId === goal.id" :goal="goal" :busy="busy" @save="execute" @cancel="editingId = null" />
        </article>
        <p>按本次收到的礼物数量累计，达标后继续计数。关闭面板、断线重连或重启会保留进度；切换直播间也不会自动清零。</p>
      </section>
      <section>
        <SettingsSectionTitle>累计设置 <span v-if="dirty" class="unsaved">未保存</span></SettingsSectionTitle>
        <fieldset class="settings-row" :disabled="busy"><SettingsToggle v-model="config.enabled" label="自动累计直播礼物" :disabled="busy" @change="dirty = true" /><label class="mode-field">盲盒处理方式<ExtSelect v-model="config.blind_gift_mode" :options="modes" aria-label="盲盒处理方式" @update:model-value="dirty = true" /></label></fieldset>
        <p>{{ config.blind_gift_mode === 'original' ? '按“心动盲盒”等盲盒本身累计，不同时累计爆出的礼物。' : '按盲盒爆出的礼物累计，不同时累计盲盒本身。' }}普通礼物和舰长等大航海不受影响。</p>
      </section>
      <section>
        <SettingsSectionTitle>OBS 浏览器源</SettingsSectionTitle>
        <p>在 OBS 中添加「浏览器源」并粘贴下面的地址。使用时保持弹幕姬运行并连接直播间。</p>
        <div v-if="server?.error" class="feedback error">{{ server.error }}</div>
        <input class="url" :value="url || '服务尚未启动'" readonly aria-label="心愿机 OBS 地址" @focus="($event.target as HTMLInputElement).select()">
        <div class="server-actions"><button class="ext-btn ext-btn--primary" :disabled="!url" @click="copyUrl">复制 OBS 地址</button><form @submit.prevent="restartServer"><label class="port-field">端口<input v-model.number="port" type="number" required min="1024" max="65535" step="1" aria-label="OBS 服务端口"></label><button class="ext-btn" :disabled="busy">应用端口 / 重试</button></form></div>
        <p>端口与加班机、点歌机共用，修改后需同步更新其他浏览器源地址。</p>
      </section>
      <section><SettingsSectionTitle>样式与主题</SettingsSectionTitle><WishStyleSettings :config="config" :goals="state.goals" :disabled="busy" @change="changeStyle" /></section>
      <div class="save-bar"><span>{{ dirty ? '累计设置或样式草稿尚未保存' : '设置已保存' }}</span><button class="ext-btn ext-btn--primary" :disabled="busy || !dirty" @click="saveConfig">保存设置与样式</button></div>
    </template>
    <p v-else>正在加载心愿机…</p>
    <ConfirmDialog v-model:visible="confirmRemove" title="删除心愿" :message="`删除「${removing?.gift_name ?? ''}」及其累计进度？`" danger :loading="busy" :close-on-confirm="false" @confirm="removing && execute({ type: 'remove_goal', id: removing.id })" />
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/extension-shared';
@use '@/styles/settings-controls' as controls;
.wish-panel { height: 100%; overflow-y: auto; padding: 16px; color: var(--text-primary); }
section { margin-bottom: 18px; padding: 18px; border: 1px solid var(--border-color); border-radius: 10px; background: var(--bg-secondary); }
h2 { font-size: var(--font-size-lg); margin: 0; }
p { @include controls.hint; margin: 10px 0; }
.heading, .goal-summary, .goal-actions, .save-bar { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 10px; }
.heading p { margin-bottom: 0; }
.status { color: var(--text-muted); font-size: var(--font-size-xs); border-radius: 20px; padding: 5px 10px; background: var(--bg-primary); }
.status.enabled { color: #8edda9; }
.goal-summary { margin: 18px 0 12px; font-size: var(--font-size-sm); color: var(--text-secondary); }
.goal-card { margin: 10px 0; padding: 14px; border: 1px solid var(--border-color); border-radius: 8px; background: var(--bg-primary); }
.goal-row { display: flex; align-items: center; gap: 12px; }
.goal-index { color: var(--text-muted); font-size: var(--font-size-sm); }
.goal-title { flex: 1; min-width: 0; overflow-wrap: anywhere; }
.goal-title strong { font-size: var(--font-size-base); }
.goal-title small, .goal-count small { display: block; margin-top: 3px; color: var(--text-muted); font-size: var(--font-size-xs); }
.goal-count { display: flex; align-items: baseline; justify-content: flex-end; flex-wrap: wrap; gap: 6px; font-variant-numeric: tabular-nums; }
.goal-count strong { font-size: var(--font-size-lg); }
.goal-count span { color: var(--text-muted); font-size: var(--font-size-sm); }
.goal-count.complete strong, .goal-count.complete small { color: #8edda9; }
progress { display: block; width: 100%; height: 4px; border: 0; border-radius: 4px; margin: 12px 0; overflow: hidden; }
progress::-webkit-progress-bar { background: var(--bg-hover); }
progress::-webkit-progress-value { background: var(--accent-primary); border-radius: 4px; }
.move-actions { display: flex; gap: 5px; }
.goal-card .goal-editor { margin-top: 12px; }
.settings-row { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 210px), 1fr)); gap: 16px; margin: 16px 0 0; padding: 0; border: 0; min-width: 0; }
.mode-field, .port-field { display: flex; flex-direction: column; gap: 8px; color: var(--text-secondary); font-size: var(--font-size-sm); }
input { @include controls.control; }
.url { font-family: Consolas, monospace; }
.server-actions, .server-actions form { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; margin-top: 8px; }
.server-actions form { margin: 0; }
.server-actions label { flex-direction: row; align-items: center; }
.server-actions input { width: 90px; }
.save-bar { position: sticky; bottom: -16px; margin: 0 -16px -16px; padding: 12px 16px; background: var(--bg-secondary); border-top: 1px solid var(--border-color); z-index: 1; }
.save-bar span, .unsaved { color: var(--text-muted); font-size: var(--font-size-xs); }
.unsaved { color: #edc67a; margin-left: 8px; }
.feedback { padding: 10px 12px; margin-bottom: 10px; border-radius: 6px; font-size: var(--font-size-sm); overflow-wrap: anywhere; }
.error { color: #ff9292; background: #f5555515; }
.success { color: #8edda9; background: #40b97815; }
.empty { text-align: center; padding: 24px 0; }
</style>

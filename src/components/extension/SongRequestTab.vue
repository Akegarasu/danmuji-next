<script setup lang="ts">
import { computed, onActivated, onBeforeUnmount, onDeactivated, ref } from 'vue'
import ConfirmDialog from '@/components/common/ConfirmDialog.vue'
import { createExtensionClient } from '@/services/extensions'
import { createExtensionSettingsWindow } from '@/services/window-manager'
import type { SongRequest, SongSnapshot } from '@/types/song-request'

const state = ref<SongSnapshot | null>(null)
const error = ref('')
const message = ref('')
const busy = ref(false)
const showHistory = ref(false)
const confirmClear = ref(false)
const confirmSession = ref(false)
const list = ref<HTMLElement | null>(null)
const dragging = ref<string | null>(null)
const dropTarget = ref<{ id: string; placement: 'before' | 'after' } | null>(null)
let scrollTimer: ReturnType<typeof setInterval> | undefined
let pointer: { x: number; y: number } | null = null
const pending = computed(() => state.value?.requests.filter(item => !item.sung) ?? [])
const sung = computed(() => state.value?.requests.filter(item => item.sung).reverse() ?? [])
const guardLabels: Record<number, string> = { 1: '总督', 2: '提督', 3: '舰长' }
const client = createExtensionClient('song-request', snapshot => {
  state.value = snapshot
  if (dragging.value && !snapshot.requests.some(item => item.id === dragging.value && !item.sung)) cancelDrag()
}, value => { error.value = value })

async function openSettings() {
  try { await createExtensionSettingsWindow('song-request') }
  catch (cause) { error.value = String(cause) }
}
async function copySongName(songName: string) {
  error.value = ''; message.value = ''
  try { await navigator.clipboard.writeText(songName); message.value = '歌名已复制' }
  catch { error.value = '复制歌名失败，请重试。' }
}
async function execute(request: SongRequest) {
  if (busy.value) return
  busy.value = true; message.value = ''
  try {
    await client.request(request)
    if (request.type === 'clear_all') confirmClear.value = false
    if (request.type === 'new_session') { confirmSession.value = false; message.value = '已开始新一场，次数、冷却和礼物累计已重置' }
  } catch (cause) { error.value = String(cause) }
  finally { busy.value = false }
}
// 使用指针拖动，兼容 Windows WebView 的原生文件拖放处理；键盘方向键同样可排序。
function cancelDrag() {
  dragging.value = null; dropTarget.value = null; pointer = null
  clearInterval(scrollTimer)
  scrollTimer = undefined
}
function updateDropTarget() {
  if (!pointer || !list.value) return
  const bounds = list.value.getBoundingClientRect()
  if (pointer.x < bounds.left || pointer.x > bounds.right || pointer.y < bounds.top || pointer.y > bounds.bottom) {
    dropTarget.value = null
    return
  }
  const delta = pointer.y < bounds.top + 36 ? -10 : pointer.y > bounds.bottom - 36 ? 10 : 0
  if (delta) list.value.scrollTop += delta
  const row = document.elementFromPoint(pointer.x, pointer.y)?.closest<HTMLElement>('[data-song-id]')
  const id = row?.dataset.songId
  if (!row || !id || id === dragging.value || !list.value.contains(row)) { dropTarget.value = null; return }
  const rect = row.getBoundingClientRect()
  dropTarget.value = { id, placement: pointer.y < rect.top + rect.height / 2 ? 'before' : 'after' }
}
function startDrag(event: PointerEvent, id: string) {
  if (event.button !== 0 || busy.value) return
  cancelDrag()
  dragging.value = id
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  pointer = { x: event.clientX, y: event.clientY }
  scrollTimer = setInterval(updateDropTarget, 30)
}
function moveDrag(event: PointerEvent) {
  if (!dragging.value) return
  pointer = { x: event.clientX, y: event.clientY }
  updateDropTarget()
}
function endDrag(event: PointerEvent) {
  if (!dragging.value) return
  moveDrag(event)
  const id = dragging.value
  const target = dropTarget.value
  cancelDrag()
  if (target) void execute({ type: 'move', request_id: id, target_id: target.id, placement: target.placement })
}
function moveByKeyboard(id: string, direction: -1 | 1) {
  const index = pending.value.findIndex(item => item.id === id)
  const target = pending.value[index + direction]
  if (target) void execute({ type: 'move', request_id: id, target_id: target.id, placement: direction < 0 ? 'before' : 'after' })
}
function stop() { client.disconnect(); cancelDrag() }
onActivated(() => { void client.connect() })
onDeactivated(stop)
onBeforeUnmount(stop)
</script>

<template>
  <div class="song-request-tab" @keydown.esc="cancelDrag">
    <div v-if="error" class="extension-error" role="alert">{{ error }} <button class="ext-btn" :disabled="busy" @click="client.connect()">刷新状态</button></div>
    <div v-if="message" class="notice" role="status">{{ message }}</div>
    <div class="ext-toolbar">
      <div class="queue-count">待唱 <span class="ext-badge ext-badge--primary">{{ pending.length }}</span><span v-if="state && !state.config.enabled" class="paused">接收已暂停</span></div>
      <div class="toolbar-actions">
        <button class="ext-btn" @click="openSettings">设置与 OBS ↗</button>
        <button class="ext-btn" :disabled="busy || !pending.length" title="按已保存的优先级重新排列待唱队列" @click="execute({ type: 'sort_priority' })">按优先级排序</button>
      </div>
    </div>

    <template v-if="state">
      <p class="queue-hint">拖动序号调整顺序，或聚焦序号后按 ↑ / ↓。发送“{{ state.config.command }} 歌名”点歌。</p>
      <div class="song-row headings"><span>序号</span><span>歌名</span><span>点歌人</span><span>操作</span></div>
      <div ref="list" class="queue-list">
        <div v-if="!pending.length" class="ext-empty"><div class="ext-empty__title">{{ state.config.enabled ? '等待观众点歌' : '点歌接收已暂停' }}</div><div class="ext-empty__hint">弹幕和 SC 点歌会出现在这里，可在设置中调整接收方式与优先级。</div></div>
        <div v-for="(item, index) in pending" :key="item.id" class="song-row" :data-song-id="item.id"
          :class="{ dragging: dragging === item.id, 'drop-before': dropTarget?.id === item.id && dropTarget.placement === 'before', 'drop-after': dropTarget?.id === item.id && dropTarget.placement === 'after' }">
          <button class="drag-handle" :disabled="busy" :aria-label="`调整第 ${index + 1} 首 ${item.song_name} 的顺序，按上下方向键移动`" title="拖动排序；也可按 ↑ / ↓"
            @pointerdown.prevent="startDrag($event, item.id)" @pointermove="moveDrag" @pointerup="endDrag" @pointercancel="cancelDrag" @lostpointercapture="cancelDrag"
            @keydown.up.prevent="moveByKeyboard(item.id, -1)" @keydown.down.prevent="moveByKeyboard(item.id, 1)">{{ index + 1 }}</button>
          <div class="song-name">{{ item.song_name }}<div class="badges"><span v-if="item.source === 'superchat'" class="ext-badge ext-badge--pink">SC ¥{{ (item.sc_price ?? 0) / 10 }}</span><span v-if="guardLabels[item.guard_level]" class="ext-badge ext-badge--blue">{{ guardLabels[item.guard_level] }}</span></div></div>
          <div class="username">{{ item.username }}</div>
          <div class="row-actions">
            <button class="ext-btn" :aria-label="`复制歌名：${item.song_name}`" @click="copySongName(item.song_name)">复制歌名</button>
            <button class="ext-btn" :disabled="busy" :aria-label="`标记 ${item.song_name} 为已唱`" @click="execute({ type: 'mark_sung', request_id: item.id, sung: true })">已唱</button>
            <button class="ext-icon-btn ext-icon-btn--danger" :disabled="busy" :aria-label="`删除 ${item.song_name}`" title="删除" @click="execute({ type: 'remove', request_id: item.id })">×</button>
          </div>
        </div>
        <button class="history-toggle" :aria-expanded="showHistory" @click="showHistory = !showHistory">{{ showHistory ? '▾' : '▸' }} 已唱记录（{{ sung.length }}）</button>
        <template v-if="showHistory">
          <p v-if="!sung.length" class="queue-hint">还没有已唱记录</p>
          <div v-for="(item, index) in sung" :key="item.id" class="song-row history-row"><span class="history-index">{{ index + 1 }}</span><div class="song-name">{{ item.song_name }}</div><div class="username">{{ item.username }}</div><button class="ext-btn" :disabled="busy" @click="execute({ type: 'mark_sung', request_id: item.id, sung: false })">恢复</button></div>
        </template>
      </div>
      <div class="ext-toolbar footer-toolbar"><button class="ext-btn" :disabled="busy" @click="confirmSession = true">新一场</button><div class="toolbar-actions"><button class="ext-btn" :disabled="busy || !sung.length" @click="execute({ type: 'clear_sung' })">清除已唱</button><button class="ext-btn ext-btn--danger" :disabled="busy || !state.requests.length" @click="confirmClear = true">清空全部</button></div></div>
      <p v-if="pending.length >= 1000" class="notice">待唱队列已满（1000 首），清理后继续接收。</p>
    </template>
    <p v-else class="queue-hint">正在加载点歌机…</p>
    <ConfirmDialog v-model:visible="confirmSession" title="开始新一场" message="重置所有观众的次数、冷却和礼物累计，保留当前点歌队列。" :loading="busy" :close-on-confirm="false" @confirm="execute({ type: 'new_session' })" />
    <ConfirmDialog v-model:visible="confirmClear" title="清空点歌队列" message="将清除全部待唱歌曲和已唱记录，设置会保留。" danger :loading="busy" :close-on-confirm="false" @confirm="execute({ type: 'clear_all' })" />
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/extension-shared';
.song-request-tab { display: flex; flex-direction: column; height: 100%; min-height: 0; color: var(--text-primary); }
.queue-count, .row-actions { display: flex; align-items: center; gap: 8px; }
.paused { color: var(--text-muted); font-size: var(--font-size-xs); }
.queue-count { flex-wrap: wrap; }
.notice { padding: 8px 16px; font-size: var(--font-size-xs); color: var(--accent-primary); background: var(--bg-secondary); }
.queue-hint { margin: 0; padding: 10px 16px; color: var(--text-muted); font-size: var(--font-size-xs); line-height: 1.5; }
.queue-list { flex: 1; min-height: 0; overflow-y: auto; overflow-x: hidden; }
.song-row { position: relative; display: grid; grid-template-columns: 32px minmax(0, 1fr) minmax(50px, .65fr) calc(6 * var(--font-size-sm) + 72px); gap: 8px; align-items: center; padding: 12px 16px; border-bottom: 1px solid var(--border-color); }
.headings { padding-top: 8px; padding-bottom: 8px; color: var(--text-muted); font-size: var(--font-size-xs); }
.song-name { min-width: 0; overflow-wrap: anywhere; line-height: 1.5; font-size: var(--font-size-sm); }
.username { overflow-wrap: anywhere; color: var(--text-secondary); font-size: var(--font-size-xs); }
.badges { display: flex; flex-wrap: wrap; gap: 4px; &:not(:empty) { margin-top: 4px; } }
.drag-handle { border: 1px solid var(--border-color); border-radius: 6px; background: var(--bg-hover); color: var(--text-secondary); height: 32px; padding: 0; cursor: grab; touch-action: none; user-select: none; font: inherit; font-size: var(--font-size-xs); &:focus-visible { outline: 2px solid var(--accent-primary); } &:disabled { cursor: default; opacity: .5; } }
.dragging { background: var(--bg-hover); opacity: .55; .drag-handle { cursor: grabbing; } }
.drop-before::before, .drop-after::after { content: ''; position: absolute; left: 16px; right: 16px; height: 2px; background: var(--accent-primary); z-index: 1; }
.drop-before::before { top: 0; } .drop-after::after { bottom: 0; }
.row-actions { gap: 4px; .ext-btn { padding: 0 8px; } }
.history-toggle { display: block; padding: 16px; width: 100%; text-align: left; border: 0; background: var(--bg-secondary); color: var(--text-secondary); font: inherit; font-size: var(--font-size-sm); cursor: pointer; }
.history-row { color: var(--text-muted); .song-name { text-decoration: line-through; } }
.history-index { text-align: center; font-size: var(--font-size-xs); }
.footer-toolbar { color: var(--text-muted); font-size: var(--font-size-xs); }
@media (max-width: 390px) { .song-row { padding-left: 10px; padding-right: 10px; gap: 6px; } }
</style>

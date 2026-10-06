<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from 'vue'
import SettingsToggle from '@/components/common/SettingsToggle.vue'
import ExtensionSettingsLayout from './ExtensionSettingsLayout.vue'
import OverlayStyleSettings from './OverlayStyleSettings.vue'
import giftCatalog from '@/assets/gift.json'
import { createExtensionClient, getOverlayServer, startOverlayServer, type OverlayServerInfo } from '@/services/extensions'
import type { SongConfig, SongGiftRule, SongPriorities } from '@/types/song-request'

const emit = defineEmits<{ close: [] }>()
const config = ref<SongConfig | null>(null)
const savedConfig = ref<SongConfig | null>(null)
const dirty = ref(false)
const busy = ref(false)
const error = ref('')
const message = ref('')
const server = ref<OverlayServerInfo | null>(null)
const port = ref(17654)
const portDirty = ref(false)
const preview = ref(false)
const giftSearch = ref('')
let active = true
let interval: ReturnType<typeof setInterval> | undefined
const roles: { key: keyof SongConfig['audience']; label: string }[] = [
  { key: 'normal', label: '普通观众' }, { key: 'fans', label: '本直播间粉丝团' },
  { key: 'captain', label: '舰长' }, { key: 'admiral', label: '提督' }, { key: 'governor', label: '总督' },
]
const priorityFields: { key: keyof SongPriorities; label: string }[] = [
  { key: 'superchat', label: 'SC' }, { key: 'governor', label: '总督' },
  { key: 'admiral', label: '提督' }, { key: 'captain', label: '舰长' },
]
const overlayUrl = computed(() => server.value?.url ? new URL('/overlays/song-request/', server.value.url).href : '')
const matchingGifts = computed(() => {
  const query = giftSearch.value.trim().toLowerCase()
  return query ? giftCatalog.filter(gift => gift.name.toLowerCase().includes(query) || String(gift.id).includes(query)).slice(0, 12) : []
})
const cloneConfig = (value: SongConfig) => JSON.parse(JSON.stringify(value)) as SongConfig
const client = createExtensionClient('song-request', snapshot => {
  savedConfig.value = snapshot.config
  if (!dirty.value) config.value = cloneConfig(snapshot.config)
}, value => { error.value = value })

async function save() {
  if (!config.value || busy.value) return
  busy.value = true; message.value = ''
  try {
    await client.request({ type: 'configure', config: cloneConfig(config.value) })
    dirty.value = false
    if (savedConfig.value) config.value = cloneConfig(savedConfig.value)
    message.value = '设置已保存，队列与 OBS 已同步'
  } catch (cause) { error.value = String(cause) }
  finally { busy.value = false }
}
function discard() {
  if (savedConfig.value) config.value = cloneConfig(savedConfig.value)
  dirty.value = false; message.value = ''
}
function resetAudience() {
  if (!config.value) return
  roles.forEach((role, index) => { config.value!.audience[role.key] = { enabled: true, limit: [10, 10, 20, 30, 40][index], cooldown_secs: 60 } })
  dirty.value = true
}
function addGift(gift?: { id: number; name: string }) {
  if (!config.value || config.value.gift_rules.length >= 10) return
  if (gift && config.value.gift_rules.some(rule => rule.gift_id === gift.id)) { message.value = '这份礼物已经添加'; return }
  config.value.gift_rules.push({ id: crypto.randomUUID(), gift_id: gift?.id ?? null, gift_name: gift?.name ?? '', gifts_required: 10, extra_requests: 1 })
  giftSearch.value = ''; dirty.value = true
}
function editGiftName(rule: SongGiftRule, event: Event) {
  rule.gift_name = (event.target as HTMLInputElement).value
  rule.gift_id = null
  dirty.value = true
}
async function refreshServer() {
  try {
    const info = await getOverlayServer()
    if (!active) return
    server.value = info
    if (!portDirty.value) port.value = info.port
  } catch (cause) { if (active) error.value = String(cause) }
}
async function startServer() {
  if (busy.value) return
  busy.value = true; error.value = ''; message.value = ''
  try {
    server.value = await startOverlayServer(Number(port.value))
    portDirty.value = false
    message.value = 'OBS 服务已就绪；更换端口后请更新浏览器源地址。'
  } catch (cause) { error.value = String(cause); await refreshServer() }
  finally { busy.value = false }
}
async function copyUrl() {
  try { await navigator.clipboard.writeText(overlayUrl.value); message.value = 'OBS 地址已复制' }
  catch { error.value = '复制失败，请选中地址手动复制。' }
}
onMounted(() => {
  void client.connect(); void refreshServer()
  interval = setInterval(() => { if (!busy.value) void refreshServer() }, 3000)
})
onBeforeUnmount(() => { active = false; clearInterval(interval); client.disconnect() })
</script>

<template>
  <ExtensionSettingsLayout :ready="!!config" :dirty="dirty" :busy="busy" :error="error" :message="message"
    @save="save" @discard="discard" @close="emit('close')" @refresh="client.connect()">
      <template v-if="config">
        <fieldset :disabled="busy" @input="dirty = true" @change="dirty = true">
          <section>
            <h2>点歌接收</h2>
            <div class="toggle-grid">
              <SettingsToggle v-model="config.enabled" label="启用点歌机" :disabled="busy" />
              <SettingsToggle v-model="config.accept_danmaku" label="接收弹幕点歌" :disabled="busy" />
              <SettingsToggle v-model="config.deduplicate" label="过滤同一用户的重复待唱歌曲" :disabled="busy" />
            </div>
            <label class="field command-field">点歌口令<input v-model="config.command" required maxlength="20" placeholder="点歌"></label>
            <p>弹幕发送“{{ config.command || '点歌' }} 歌名”或“{{ config.command || '点歌' }}：歌名”。口令须在开头，歌名最多 120 字。</p>
          </section>

          <section>
            <div class="section-heading"><h2>点歌次数</h2><button type="button" class="text-button" @click="resetAudience">恢复初始设置</button></div>
            <p>按当前身份计算，大航海优先于本直播间粉丝团。次数填 0 表示不能点；冷却填 0 表示不限。SC 不占普通次数，也不受冷却限制。</p>
            <div v-for="role in roles" :key="role.key" class="audience-row">
              <SettingsToggle v-model="config.audience[role.key].enabled" :label="role.label" :disabled="busy" />
              <div class="allowance-fields">
                <label>每人每场<input v-model.number="config.audience[role.key].limit" :aria-label="`${role.label}每场次数`" type="number" min="0" max="10000" step="1" required :disabled="!config.audience[role.key].enabled">次</label>
                <label>冷却<input v-model.number="config.audience[role.key].cooldown_secs" :aria-label="`${role.label}冷却秒数`" type="number" min="0" max="86400" step="1" required :disabled="!config.audience[role.key].enabled">秒</label>
              </div>
            </div>
            <p>收到新开播通知、切换房间时重置次数；同房间重连保留。也可在点歌页点击“新一场”。已唱或删除歌曲不会返还次数。</p>
          </section>

          <section>
            <h2>礼物点歌</h2>
            <SettingsToggle v-model="config.gift_bonus_enabled" label="送指定礼物，可以增加点歌次数" :disabled="busy" />
            <template v-if="config.gift_bonus_enabled">
              <div class="gift-search"><input v-model="giftSearch" placeholder="搜索 B 站礼物名称或 ID" aria-label="搜索礼物"><button class="ext-btn" type="button" :disabled="config.gift_rules.length >= 10" @click="addGift()">+ 自定义礼物</button></div>
              <div v-if="giftSearch" class="gift-results">
                <button v-for="gift in matchingGifts" :key="gift.id" type="button" :disabled="config.gift_rules.length >= 10" @click="addGift(gift)">{{ gift.name }} <span>#{{ gift.id }}</span></button>
                <p v-if="!matchingGifts.length">未找到礼物，可填写自定义名称。</p>
              </div>
              <article v-for="(rule, index) in config.gift_rules" :key="rule.id" class="gift-rule">
                <label class="field">礼物 {{ index + 1 }}<input :value="rule.gift_name" required maxlength="80" placeholder="礼物名称，如：玫瑰" @input="editGiftName(rule, $event)"><small>{{ rule.gift_id ? `按 ID ${rule.gift_id} 匹配` : '按完整名称匹配' }}</small></label>
                <label class="field">每送几个<input v-model.number="rule.gifts_required" type="number" min="1" max="10000" step="1" required></label>
                <label class="field">加几次点歌<input v-model.number="rule.extra_requests" type="number" min="1" max="10000" step="1" required></label>
                <button type="button" class="ext-icon-btn ext-icon-btn--danger" :aria-label="`删除礼物规则 ${index + 1}`" @click="config.gift_rules.splice(index, 1); dirty = true">×</button>
              </article>
              <p>已添加 {{ config.gift_rules.length }} / 10 种礼物。分次送礼会累计，本场有效。直接处理盲盒：按盲盒本身累计，不计爆出的礼物；同一礼物按 ID 优先匹配。</p>
              <p>增加的次数在基础次数用完后消耗，仍需符合身份开关和冷却要求。修改一条礼物规则会清除该规则未兑换的余数，已获次数保留。</p>
            </template>
          </section>

          <section>
            <h2>SC 点歌</h2>
            <SettingsToggle v-model="config.accept_superchat" label="发 SC 可以点歌" :disabled="busy" />
            <p>不占普通次数，不受身份次数与冷却限制。金额不足的 SC 仍在 SC 页正常显示，不进入点歌队列。</p>
            <label class="field command-field">SC 至少多少元才算点歌<input v-model.number="config.sc_min_price" type="number" min="0" max="1000000" step="0.1" required></label>
            <p>填 0 表示不限制金额。</p>
            <div class="toggle-grid">
              <SettingsToggle v-model="config.sc_content_as_song" label="SC 内容直接当歌名" :disabled="busy" />
              <SettingsToggle v-model="config.sc_sort_by_price" label="SC 金额越高越靠前" :disabled="busy" />
            </div>
            <p>直接当歌名开启后无需口令；关闭后按上方口令匹配。同优先级的 SC 按金额排序，金额相同先到先唱；SC 优先级为 0 时不按金额插队。</p>
          </section>

          <section>
            <h2>队列优先级</h2>
            <div class="priority-grid"><label v-for="field in priorityFields" :key="field.key" class="field">{{ field.label }}<input v-model.number="config.priorities[field.key]" type="number" min="0" max="1000" step="1" required></label></div>
            <p>数值越大越优先，0 表示不优先。SC 和大航海身份同时命中取较高值；普通弹幕为 0。改变排序规则并保存会重排待唱队列。</p>
            <p>拖动可手动调整；新点歌按规则插入，已有条目的相对顺序保留。</p>
          </section>

          <section>
            <h2>OBS 显示</h2>
            <div class="overlay-fields"><label class="field">最多显示几首<input v-model.number="config.overlay_max_rows" type="number" min="1" max="30" step="1" required></label><SettingsToggle v-model="config.overlay_show_username" label="显示点歌人" :disabled="busy" /></div>
            <p>透明背景，仅显示“01 歌名 点歌人”。点歌人使用更小、更浅的文字；空队列不显示内容。</p>
            <div class="appearance-settings">
              <OverlayStyleSettings v-model="config.overlay_style" preset="song-request" :disabled="busy" @update:model-value="dirty = true" />
            </div>
          </section>
        </fieldset>
      </template>
      <p v-else class="loading">正在加载设置…</p>
      <section class="obs-section">
        <h2>OBS 浏览器源</h2>
        <p>添加 OBS“浏览器”源并粘贴地址，建议宽度 600、高度 600。请保持弹幕姬运行。</p>
        <div v-if="server?.error || server?.persistence_error" class="extension-error" role="alert">{{ server.error || server.persistence_error }}</div>
        <div class="url-row"><input :value="overlayUrl" readonly aria-label="点歌机 OBS 地址" placeholder="本地服务尚未启动" @focus="($event.target as HTMLInputElement).select()"><button type="button" class="ext-btn" :disabled="!overlayUrl" @click="copyUrl">复制地址</button></div>
        <div class="server-row"><label>端口<input v-model.number="port" :disabled="busy" type="number" min="1024" max="65535" step="1" @input="portDirty = true"></label><button type="button" class="ext-btn" :disabled="busy" @click="startServer">启动 / 应用端口</button><button type="button" class="ext-btn" :disabled="!overlayUrl" @click="preview = !preview">{{ preview ? '收起预览' : '预览' }}</button></div>
        <p>端口与加班机共用；更换后需同步更新 OBS 地址。预览使用已保存的设置。</p>
        <iframe v-if="preview && overlayUrl" :src="overlayUrl" title="点歌队列 OBS 预览" />
      </section>
  </ExtensionSettingsLayout>
</template>

<style scoped lang="scss">
@use '@/styles/extension-shared';
@use '@/styles/settings-controls' as controls;
fieldset { margin: 0; padding: 0; border: 0; min-width: 0; }
section { padding: 24px 0; border-bottom: 1px solid var(--border-color); &:last-child { border-bottom: none; } }
h2 { font-size: var(--font-size-base); font-weight: 600; margin: 0 0 16px; }
p, small { @include controls.hint; } p { margin: 10px 0 0; }
.section-heading { display: flex; align-items: baseline; flex-wrap: wrap; gap: 8px; justify-content: space-between; }
.text-button { border: 0; background: none; color: var(--accent-primary); cursor: pointer; font: inherit; font-size: var(--font-size-xs); }
.toggle-grid { display: grid; gap: 16px; margin: 16px 0; }
input { @include controls.control; }
.field { display: flex; flex-direction: column; gap: 8px; min-width: 0; font-size: var(--font-size-sm); color: var(--text-secondary); }
.command-field { margin-top: 16px; }
.audience-row { display: grid; grid-template-columns: minmax(150px, 1fr) auto; align-items: center; gap: 24px; padding: 18px 0; border-bottom: 1px solid var(--border-color); }
.allowance-fields { display: flex; gap: 20px; label { display: flex; align-items: center; gap: 8px; font-size: var(--font-size-xs); color: var(--text-secondary); white-space: nowrap; } input { width: 72px; text-align: center; } }
.gift-search, .url-row, .server-row { display: flex; align-items: center; gap: 8px; }
.gift-search { margin-top: 20px; input { flex: 1; } }
.gift-results { display: flex; flex-wrap: wrap; gap: 6px; padding: 10px 0; button { border: 1px solid var(--border-color); border-radius: 6px; background: var(--bg-card); color: var(--text-primary); padding: 6px 10px; font: inherit; font-size: var(--font-size-xs); cursor: pointer; span { color: var(--text-muted); } &:disabled { opacity: .5; } } }
.gift-rule { display: grid; grid-template-columns: minmax(120px, 2fr) minmax(65px, 1fr) minmax(65px, 1fr) 24px; align-items: start; gap: 12px; margin: 20px 0; .ext-icon-btn { align-self: center; } }
.priority-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; }
.overlay-fields { display: grid; grid-template-columns: minmax(100px, 1fr) 1fr; gap: 24px; align-items: center; }
.server-row { flex-wrap: wrap; margin-top: 12px; label { display: flex; align-items: center; gap: 8px; font-size: var(--font-size-xs); } input { width: 88px; } }
.url-row input { flex: 1; min-width: 0; }
iframe { width: 100%; height: 320px; margin-top: 16px; border: 1px solid var(--border-color); border-radius: 8px; background: repeating-conic-gradient(#20242d 0% 25%, #2b303a 0% 50%) 50% / 20px 20px; }
.loading { padding-top: 24px; }
.appearance-settings { margin-top: 24px; }
@media (max-width: 620px) { .audience-row { grid-template-columns: 1fr; gap: 14px; } .allowance-fields { justify-content: space-between; gap: 10px; } }
@media (max-width: 460px) { .gift-rule { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) 24px; .field:first-child { grid-column: 1 / -1; } } .priority-grid { gap: 8px; } }
</style>

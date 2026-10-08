<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import giftCatalog from '@/assets/gift.json'
import type { WishGoal, WishRequest } from '@/types/wish-machine'

const props = defineProps<{ goal?: WishGoal; busy: boolean }>()
const emit = defineEmits<{ save: [request: WishRequest]; cancel: [] }>()
const name = ref(props.goal?.gift_name ?? '')
const giftId = ref(String(props.goal?.gift_id ?? ''))
const current = ref(props.goal?.current ?? 0)
const target = ref(props.goal?.target ?? 100)
const countEdited = ref(false)
const search = ref('')
const gifts = computed(() => {
  const query = search.value.trim().toLowerCase()
  if (!query) return []
  return giftCatalog.filter(gift => gift.name.toLowerCase().includes(query) || String(gift.id).includes(query)).slice(0, 12)
})
watch(() => props.goal?.current, value => { if (!countEdited.value && value !== undefined) current.value = value })
function selectGift(gift: { id: number | null; name: string }) {
  name.value = gift.name; giftId.value = String(gift.id ?? ''); search.value = ''
}
function save() {
  const fields = { gift_name: name.value.trim(), gift_id: giftId.value.trim() ? Number(giftId.value) : null, target: Number(target.value) }
  emit('save', props.goal
    ? { type: 'update_goal', id: props.goal.id, ...fields, ...(countEdited.value ? { current: Number(current.value) } : {}) }
    : { type: 'add_goal', ...fields, current: Number(current.value) })
}
</script>

<template>
  <form class="goal-editor" @submit.prevent="save">
    <fieldset :disabled="busy">
      <label class="search-field">从礼物库选择<input v-model="search" placeholder="搜索名称或 ID…" aria-label="搜索目标礼物"></label>
      <div v-if="search" class="gift-results">
        <button v-for="gift in gifts" :key="gift.id" type="button" class="ext-btn" @click="selectGift(gift)">{{ gift.name }} <small>#{{ gift.id }}</small></button>
        <p v-if="!gifts.length">未找到礼物，可直接填写下方名称。</p>
      </div>
      <div class="quick-gifts"><span>常用：</span><button v-for="gift in ['舰长', '提督', '总督', '心动盲盒', '粉丝团灯牌']" :key="gift" type="button" class="ext-btn" @click="selectGift({ id: null, name: gift })">{{ gift }}</button></div>
      <div class="fields">
        <label>目标礼物<input v-model="name" required maxlength="80" placeholder="例如：心动盲盒"></label>
        <label>礼物 ID（选填）<input v-model="giftId" type="number" min="1" max="9007199254740991" step="1" placeholder="留空按完整名称匹配"></label>
        <label>已获得数量<input v-model.number="current" type="number" required min="0" max="999999999" step="1" @input="countEdited = true"></label>
        <label>目标数量<input v-model.number="target" type="number" required min="1" max="999999999" step="1"></label>
      </div>
      <p v-if="goal && countEdited">保存后将已获得数量设为 {{ current }}，之后继续累计新礼物。当前实际累计：{{ goal.current }}。</p>
      <p v-else>填写 ID 时优先匹配 ID；留空时按完整名称匹配。更换礼物会保留已获得数量，可手动改为 0。</p>
      <div class="actions"><button type="submit" class="ext-btn ext-btn--primary">{{ busy ? '保存中…' : goal ? '保存心愿' : '添加心愿' }}</button><button type="button" class="ext-btn" @click="emit('cancel')">取消</button></div>
    </fieldset>
  </form>
</template>

<style scoped lang="scss">
@use '@/styles/extension-shared';
@use '@/styles/settings-controls' as controls;
.goal-editor { padding: 14px; background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: 8px; }
fieldset { border: 0; margin: 0; padding: 0; min-width: 0; }
label { display: flex; flex-direction: column; gap: 6px; min-width: 0; color: var(--text-secondary); font-size: var(--font-size-sm); }
input { @include controls.control; }
.fields { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 170px), 1fr)); gap: 12px; }
.quick-gifts, .gift-results, .actions { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; margin: 10px 0; }
.quick-gifts span, small { color: var(--text-muted); font-size: var(--font-size-xs); }
small { margin-left: 4px; }
p { @include controls.hint; margin: 10px 0; }
.actions { margin-bottom: 0; }
</style>

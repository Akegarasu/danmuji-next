<script setup lang="ts">
import { computed, nextTick, ref, useId, watch } from 'vue'
import { useControlPopover } from '@/composables/useControlPopover'
import { fontPreviewFamily, getSystemFonts, type SystemFont } from '@/services/system-fonts'

const props = withDefaults(defineProps<{
  modelValue: string
  label?: string
  placeholder?: string
  disabled?: boolean
}>(), { label: '字体名称', placeholder: '默认字体', disabled: false })
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
const { anchor, panel, open, panelStyle, show, close } = useControlPopover(300, 390)
const listId = `font-list-${useId()}`
const search = ref<HTMLInputElement>()
const query = ref('')
const fonts = ref<SystemFont[]>([])
const loading = ref(false)
const error = ref('')
const activeIndex = ref(0)
const selected = computed(() => fonts.value.find(font => font.family === props.modelValue)?.label || props.modelValue || props.placeholder)
const filtered = computed(() => {
  const term = query.value.trim().toLocaleLowerCase()
  return fonts.value.filter(font => `${font.family}\n${font.label}`.toLocaleLowerCase().includes(term))
})
const options = computed(() => {
  const entries = [{ family: '', label: props.placeholder, kind: 'default' }]
  entries.push(...filtered.value.map(font => ({ ...font, kind: 'installed' })))
  const custom = query.value.trim()
  if (custom && !fonts.value.some(font => font.family.toLocaleLowerCase() === custom.toLocaleLowerCase() || font.label === custom)) {
    entries.push({ family: custom, label: `使用“${custom}”`, kind: 'custom' })
  }
  return entries
})
watch(query, () => { activeIndex.value = query.value.trim() && options.value.length > 1 ? 1 : 0 })
watch(() => props.disabled, disabled => { if (disabled) close() })
async function load(refresh = false) {
  if (loading.value) return
  loading.value = true; error.value = ''
  try { fonts.value = await getSystemFonts(refresh) }
  catch { error.value = '无法读取本机字体，可手动输入名称或重试。' }
  finally {
    loading.value = false
    if (open.value) {
      activeIndex.value = Math.min(activeIndex.value, options.value.length - 1)
      await show()
    }
  }
}
async function toggle() {
  if (open.value) { close(); return }
  query.value = ''; activeIndex.value = 0
  await show()
  if (!open.value) return
  await nextTick()
  search.value?.focus({ preventScroll: true })
  void load()
}
function select(family: string) {
  emit('update:modelValue', family)
  close(true)
}
function keydown(event: KeyboardEvent) {
  if (event.isComposing) return
  if (event.key === 'Enter') {
    event.preventDefault()
    const option = options.value[activeIndex.value]
    if (option) select(option.family)
  } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault()
    activeIndex.value = (activeIndex.value + (event.key === 'ArrowDown' ? 1 : -1) + options.value.length) % options.value.length
    void nextTick(() => document.getElementById(`${listId}-${activeIndex.value}`)?.scrollIntoView({ block: 'nearest' }))
  }
}
</script>

<template>
  <div class="font-picker">
    <button ref="anchor" type="button" class="font-trigger" :disabled="disabled" :aria-label="label" aria-haspopup="dialog" :aria-expanded="open" @click="toggle">
      <span class="selected-name" :class="{ placeholder: !modelValue }">{{ selected }}</span>
      <span class="font-sample" :style="{ fontFamily: fontPreviewFamily(modelValue) }" aria-hidden="true">Aa</span>
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>
    </button>
    <Teleport to="body">
      <div v-if="open" ref="panel" class="font-popover" :style="panelStyle" role="dialog" aria-label="选择字体" @keydown.esc.stop.prevent="close(true)">
        <div class="font-search-row">
          <input ref="search" v-model="query" role="combobox" aria-label="搜索或输入字体名称" placeholder="搜索字体，也可手动输入名称" maxlength="100"
            autocomplete="off" :spellcheck="false" aria-autocomplete="list" :aria-controls="listId" :aria-expanded="true" :aria-activedescendant="`${listId}-${activeIndex}`" @keydown="keydown">
          <button type="button" class="refresh-fonts" aria-label="刷新本机字体" :disabled="loading" @click="load(true)">刷新</button>
        </div>
        <p v-if="loading" role="status">正在读取本机字体…</p>
        <p v-else-if="error" role="status">{{ error }}</p>
        <p v-else>{{ query.trim() ? `找到 ${filtered.length} 个匹配字体` : `本机已安装 ${fonts.length} 个字体` }}</p>
        <div :id="listId" class="font-options" role="listbox" :aria-label="label">
          <button v-for="(font, index) in options" :id="`${listId}-${index}`" :key="font.family" type="button" role="option" tabindex="-1"
            :aria-selected="modelValue === font.family" :class="{ highlighted: activeIndex === index, selected: modelValue === font.family }"
            @pointermove="activeIndex = index" @mousedown.prevent @click="select(font.family)">
            <span class="font-info"><span>{{ font.label }}</span><small v-if="font.kind === 'installed' && font.family !== font.label">{{ font.family }}</small><small v-else-if="font.kind === 'custom'">手动指定字体名称</small></span>
            <span v-if="font.family" class="font-preview" :style="{ fontFamily: fontPreviewFamily(font.family) }" aria-hidden="true">弹幕 Aa</span>
            <span v-if="modelValue === font.family" class="check" aria-hidden="true">✓</span>
          </button>
        </div>
        <p class="font-hint">安装新字体后可刷新列表；OBS 需安装同一字体。</p>
      </div>
    </Teleport>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/settings-controls' as controls;
.font-picker { min-width: 0; width: 100%; }
.font-trigger { @include controls.control; display: flex; align-items: center; gap: 10px; text-align: left; cursor: pointer; &:focus-visible { outline: 2px solid var(--accent-primary); outline-offset: 2px; } svg, .font-sample { flex-shrink: 0; color: var(--text-secondary); } }
.selected-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; &.placeholder { color: var(--text-secondary); } }
.font-popover { @include controls.popover; display: flex; flex-direction: column; gap: 10px; }
.font-search-row { display: flex; flex-shrink: 0; gap: 8px; input { @include controls.control; flex: 1; } }
.refresh-fonts { @include controls.icon-button; border-radius: 4px; padding: 0 6px; }
p { @include controls.hint; margin: 0; flex-shrink: 0; }
.font-options { flex: 1; min-height: 60px; max-height: 250px; overflow: auto; overscroll-behavior: contain; }
.font-options button { display: flex; align-items: center; gap: 10px; min-height: 44px; padding: 7px 8px; width: 100%; border: 0; border-radius: 4px; background: transparent; color: var(--text-primary); text-align: left; font: inherit; cursor: pointer; &.highlighted { background: var(--bg-hover); } &.selected { color: var(--accent-primary); } }
.font-info { flex: 1; min-width: 0; overflow-wrap: anywhere; small { display: block; margin-top: 3px; color: var(--text-secondary); font-size: var(--font-size-xs); } }
.font-preview { flex: 0 0 75px; font-size: 16px; overflow: hidden; white-space: nowrap; color: var(--text-secondary); }
.check { flex-shrink: 0; }
</style>

<script setup lang="ts">
import { useId } from 'vue'

withDefaults(defineProps<{
  ready: boolean
  dirty: boolean
  busy: boolean
  error?: string
  message?: string
}>(), { error: '', message: '' })
const emit = defineEmits<{ save: []; discard: []; close: []; refresh: [] }>()
const formId = `extension-settings-${useId()}`
</script>

<template>
  <div class="extension-settings-layout">
    <div v-if="error" class="extension-error" role="alert">{{ error }} <button class="ext-btn" :disabled="busy" @click="emit('refresh')">刷新状态</button></div>
    <div v-if="message" class="notice" role="status">{{ message }}</div>
    <form :id="formId" class="settings-scroll" @submit.prevent="emit('save')">
      <slot />
    </form>
    <footer class="save-bar">
      <span>{{ !ready ? '正在加载设置…' : dirty ? '有未保存的修改' : '设置已同步' }}</span>
      <div class="save-actions">
        <button type="button" class="ext-btn" :disabled="busy || !dirty" @click="emit('discard')">撤销修改</button>
        <button type="button" class="ext-btn" :disabled="busy" @click="emit('close')">关闭</button>
        <button :form="formId" type="submit" class="ext-btn ext-btn--primary" :disabled="busy || !ready || !dirty">{{ busy ? '保存中…' : '保存设置' }}</button>
      </div>
    </footer>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/extension-shared';
.extension-settings-layout { display: flex; flex-direction: column; flex: 1; min-height: 0; color: var(--text-primary); }
.settings-scroll { flex: 1; min-height: 0; overflow: auto; padding: 0 24px 24px; }
.save-bar { flex-shrink: 0; position: relative; z-index: 1; display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 8px; padding: 16px 24px; border-top: 1px solid var(--border-color); background: var(--bg-secondary); box-shadow: 0 -8px 24px rgba(0,0,0,.2); > span { font-size: var(--font-size-xs); color: var(--text-secondary); } }
.save-actions { display: flex; align-items: center; gap: 8px; }
.notice { padding: 10px 24px; color: var(--accent-primary); font-size: var(--font-size-sm); }
@media (max-width: 620px) { .settings-scroll { padding: 0 16px 16px; } .save-bar { padding: 12px 16px; } .save-actions { margin-left: auto; } }
</style>

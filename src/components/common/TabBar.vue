<script setup lang="ts">
import type { TabType } from '@/types'

defineProps<{
  tabs: { type: TabType; label: string; icon: string }[]
  active: TabType
}>()

const emit = defineEmits<{
  'update:active': [value: TabType]
  'pop-out': [tabType: TabType]
}>()

const setActive = (type: TabType) => {
  emit('update:active', type)
}

// 右键弹出为独立窗口
const handleContextMenu = (e: MouseEvent, type: TabType) => {
  e.preventDefault()
  emit('pop-out', type)
}
</script>

<template>
  <div class="tab-bar">
    <button
      v-for="tab in tabs"
      :key="tab.type"
      class="tab-item"
      :class="{ active: active === tab.type }"
      @click="setActive(tab.type)"
      @contextmenu="handleContextMenu($event, tab.type)"
      :title="`${tab.label} (右键拆分为独立窗口)`"
    >
      {{ tab.label }}
    </button>
  </div>
</template>

<style scoped lang="scss">
.tab-bar {
  display: flex;
  height: var(--tab-bar-height);
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  padding: 4px;
  gap: 4px;
}

.tab-item {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0 12px;
  background: transparent;
  border: none;
  border-radius: var(--border-radius-sm);
  color: var(--text-secondary);
  font-size: var(--font-size-sm);
  font-weight: 500;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
  
  &:hover {
    color: var(--text-primary);
    background: var(--bg-hover);
  }
  
  // Notion 风格：选中时填充色块
  &.active {
    color: var(--text-primary);
    background: var(--bg-active);
  }
}
</style>

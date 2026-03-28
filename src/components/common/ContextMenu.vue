<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick } from 'vue'

export interface MenuItem {
  label: string
  icon?: string
  action: () => void
  disabled?: boolean
  divider?: boolean
}

defineProps<{
  items: MenuItem[]
}>()

const visible = ref(false)
const position = ref({ x: 0, y: 0 })
const menuRef = ref<HTMLElement>()

const show = async (x: number, y: number) => {
  // 先显示菜单
  position.value = { x, y }
  visible.value = true
  
  // 等待 DOM 更新后调整位置，防止超出屏幕
  await nextTick()
  if (menuRef.value) {
    const rect = menuRef.value.getBoundingClientRect()
    const viewportWidth = window.innerWidth
    const viewportHeight = window.innerHeight
    
    if (x + rect.width > viewportWidth) {
      position.value.x = viewportWidth - rect.width - 8
    }
    if (y + rect.height > viewportHeight) {
      position.value.y = viewportHeight - rect.height - 8
    }
  }
}

const hide = () => {
  visible.value = false
}

const handleClick = (item: MenuItem) => {
  if (item.disabled) return
  item.action()
  hide()
}

// 点击外部关闭
const handleClickOutside = (e: MouseEvent) => {
  if (visible.value && menuRef.value && !menuRef.value.contains(e.target as Node)) {
    hide()
  }
}

// 右键其他地方时关闭当前菜单
const handleContextMenuOutside = (e: MouseEvent) => {
  if (visible.value && menuRef.value && !menuRef.value.contains(e.target as Node)) {
    hide()
  }
}

onMounted(() => {
  // 使用 capture 阶段，确保先处理
  document.addEventListener('mousedown', handleClickOutside)
  document.addEventListener('contextmenu', handleContextMenuOutside)
})

onUnmounted(() => {
  document.removeEventListener('mousedown', handleClickOutside)
  document.removeEventListener('contextmenu', handleContextMenuOutside)
})

defineExpose({ show, hide })
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div 
        v-if="visible" 
        ref="menuRef"
        class="context-menu"
        :style="{ left: position.x + 'px', top: position.y + 'px' }"
        @click.stop
        @contextmenu.stop.prevent
      >
        <template v-for="(item, index) in items" :key="index">
          <div v-if="item.divider" class="divider" />
          <button
            v-else
            class="menu-item"
            :class="{ disabled: item.disabled }"
            @click="handleClick(item)"
          >
            <span v-if="item.icon" class="icon">{{ item.icon }}</span>
            <span class="label">{{ item.label }}</span>
          </button>
        </template>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped lang="scss">
.context-menu {
  position: fixed;
  z-index: 9999;
  min-width: 150px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius);
  padding: 4px;
}

.menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px 12px;
  background: transparent;
  border: none;
  color: var(--text-primary);
  font-size: var(--font-size-sm);
  text-align: left;
  cursor: pointer;
  border-radius: var(--border-radius-sm);
  transition: background 0.15s;
  
  &:hover:not(.disabled) {
    background: var(--bg-hover);
  }
  
  &.disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  
  .icon {
    font-size: 14px;
  }
  
  .label {
    flex: 1;
  }
}

.divider {
  height: 1px;
  background: var(--border-color);
  margin: 4px 8px;
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.15s, transform 0.15s;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
  transform: scale(0.95);
}
</style>


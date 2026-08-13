<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick } from 'vue'
import type { ComponentPublicInstance } from 'vue'

export interface MenuItem {
  label: string
  icon?: string
  /** 传入 null 时显示文字占位头像，undefined 时不显示头像 */
  avatar?: string | null
  action?: () => void
  children?: MenuItem[]
  disabled?: boolean
  divider?: boolean
}

defineProps<{
  items: MenuItem[]
}>()

const VIEWPORT_MARGIN = 8
const SUBMENU_GAP = 4
const SUBMENU_CLOSE_DELAY = 180

const visible = ref(false)
const position = ref({ x: 0, y: 0 })
const menuRef = ref<HTMLElement>()
const submenuRef = ref<HTMLElement>()
const activeSubmenuIndex = ref<number | null>(null)
const submenuPosition = ref({ x: 0, y: 0 })
const submenuReady = ref(false)
let submenuCloseTimer: number | null = null

const cancelSubmenuClose = () => {
  if (submenuCloseTimer === null) return
  window.clearTimeout(submenuCloseTimer)
  submenuCloseTimer = null
}

const resetSubmenu = () => {
  cancelSubmenuClose()
  activeSubmenuIndex.value = null
  submenuReady.value = false
}

const scheduleSubmenuClose = (index: number) => {
  if (activeSubmenuIndex.value !== index) return
  cancelSubmenuClose()
  submenuCloseTimer = window.setTimeout(() => {
    submenuCloseTimer = null
    activeSubmenuIndex.value = null
    submenuReady.value = false
  }, SUBMENU_CLOSE_DELAY)
}

const show = async (x: number, y: number) => {
  position.value = { x, y }
  resetSubmenu()
  visible.value = true

  // 等待 DOM 更新后调整位置，防止主菜单超出屏幕
  await nextTick()
  if (!menuRef.value) return

  const rect = menuRef.value.getBoundingClientRect()
  const viewportWidth = window.innerWidth
  const viewportHeight = window.innerHeight

  const nextX = Math.min(
    Math.max(VIEWPORT_MARGIN, x),
    Math.max(VIEWPORT_MARGIN, viewportWidth - rect.width - VIEWPORT_MARGIN)
  )
  const nextY = Math.min(
    Math.max(VIEWPORT_MARGIN, y),
    Math.max(VIEWPORT_MARGIN, viewportHeight - rect.height - VIEWPORT_MARGIN)
  )

  position.value = { x: nextX, y: nextY }
}

const hide = () => {
  visible.value = false
  resetSubmenu()
}

const openSubmenu = async (index: number, event: Event) => {
  cancelSubmenuClose()
  if (activeSubmenuIndex.value === index && submenuReady.value) return

  const currentTarget = event.currentTarget as HTMLElement | null
  const anchor = currentTarget?.classList.contains('menu-entry')
    ? currentTarget
    : currentTarget?.closest<HTMLElement>('.menu-entry')
  if (!anchor) return

  const anchorRect = anchor.getBoundingClientRect()
  activeSubmenuIndex.value = index
  submenuReady.value = false

  await nextTick()
  if (activeSubmenuIndex.value !== index) return
  const menuRect = menuRef.value?.getBoundingClientRect()
  const submenu = submenuRef.value
  const submenuRect = submenu?.getBoundingClientRect()
  if (!menuRect || !submenu || !submenuRect) return

  const rightX = menuRect.right + SUBMENU_GAP
  const leftX = menuRect.left - submenuRect.width - SUBMENU_GAP
  const fitsRight = rightX + submenuRect.width <= window.innerWidth - VIEWPORT_MARGIN
  const fitsLeft = leftX >= VIEWPORT_MARGIN

  let x: number
  if (fitsRight) {
    x = rightX
  } else if (fitsLeft) {
    x = leftX
  } else {
    // 极窄窗口无法并排时允许覆盖部分一级菜单，但绝不超出视口。
    const preferredX = menuRect.left > window.innerWidth - menuRect.right ? leftX : rightX
    x = Math.min(
      Math.max(VIEWPORT_MARGIN, preferredX),
      Math.max(VIEWPORT_MARGIN, window.innerWidth - submenuRect.width - VIEWPORT_MARGIN)
    )
  }

  // 二级菜单自身有 border + padding。用首个菜单项的实际内缩进行补偿，
  // 使二级菜单首项与触发项对齐；当触发项是一级首项时，两级外框顶部也会对齐。
  const firstSubmenuItemRect = submenu.querySelector<HTMLElement>('.menu-item')?.getBoundingClientRect()
  const submenuTopInset = firstSubmenuItemRect
    ? Math.max(0, firstSubmenuItemRect.top - submenuRect.top)
    : 0
  const preferredY = anchorRect.top - submenuTopInset
  const maxY = Math.max(
    VIEWPORT_MARGIN,
    window.innerHeight - VIEWPORT_MARGIN - submenuRect.height
  )
  const y = Math.min(Math.max(VIEWPORT_MARGIN, preferredY), maxY)

  submenuPosition.value = { x, y }
  submenuReady.value = true
}

const handleClick = (item: MenuItem, index: number, event: Event) => {
  if (item.disabled) return
  if (item.children?.length) {
    void openSubmenu(index, event)
    return
  }
  item.action?.()
  hide()
}

const handleSubmenuClick = (item: MenuItem) => {
  if (item.disabled || item.children?.length) return
  item.action?.()
  hide()
}

const handleAvatarError = (event: Event) => {
  const image = event.currentTarget as HTMLImageElement
  image.style.display = 'none'
}

const setSubmenuRef = (element: Element | ComponentPublicInstance | null) => {
  submenuRef.value = element instanceof HTMLElement ? element : undefined
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

const handleKeydown = (e: KeyboardEvent) => {
  if (visible.value && e.key === 'Escape') hide()
}

onMounted(() => {
  // 使用 capture 阶段，确保先处理
  document.addEventListener('mousedown', handleClickOutside)
  document.addEventListener('contextmenu', handleContextMenuOutside)
  document.addEventListener('keydown', handleKeydown)
})

onUnmounted(() => {
  cancelSubmenuClose()
  document.removeEventListener('mousedown', handleClickOutside)
  document.removeEventListener('contextmenu', handleContextMenuOutside)
  document.removeEventListener('keydown', handleKeydown)
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
          <div
            v-else
            class="menu-entry"
            @mouseenter="item.children?.length && openSubmenu(index, $event)"
            @mouseleave="item.children?.length && scheduleSubmenuClose(index)"
          >
            <button
              class="menu-item"
              :class="{ disabled: item.disabled, active: activeSubmenuIndex === index }"
              @click="handleClick(item, index, $event)"
            >
              <span v-if="item.avatar !== undefined" class="avatar">
                <span class="avatar-placeholder">{{ item.label.slice(0, 1) }}</span>
                <img
                  v-if="item.avatar"
                  :src="item.avatar"
                  alt=""
                  referrerpolicy="no-referrer"
                  crossorigin="anonymous"
                  @error="handleAvatarError"
                />
              </span>
              <span v-else-if="item.icon" class="icon">{{ item.icon }}</span>
              <span class="label">{{ item.label }}</span>
              <span v-if="item.children?.length" class="submenu-arrow">›</span>
            </button>

            <div
              v-if="item.children?.length && activeSubmenuIndex === index"
              :ref="setSubmenuRef"
              class="context-menu submenu"
              :class="{ ready: submenuReady }"
              :style="{
                left: submenuPosition.x + 'px',
                top: submenuPosition.y + 'px'
              }"
              @mouseenter="cancelSubmenuClose"
              @mouseleave="scheduleSubmenuClose(index)"
            >
              <template v-for="(child, childIndex) in item.children" :key="childIndex">
                <div v-if="child.divider" class="divider" />
                <button
                  v-else
                  class="menu-item"
                  :class="{ disabled: child.disabled }"
                  @click.stop="handleSubmenuClick(child)"
                >
                  <span v-if="child.icon" class="icon">{{ child.icon }}</span>
                  <span class="label">{{ child.label }}</span>
                </button>
              </template>
            </div>
          </div>
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
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.28);
}

.menu-entry {
  position: relative;
}

.menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: 30px;
  padding: 6px 8px;
  background: transparent;
  border: none;
  color: var(--text-primary);
  font-size: var(--font-size-sm);
  text-align: left;
  cursor: pointer;
  border-radius: var(--border-radius-sm);
  transition: background 0.15s;

  &:hover:not(.disabled),
  &.active:not(.disabled) {
    background: var(--bg-hover);
  }

  &.disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .icon {
    width: 18px;
    font-size: 14px;
    text-align: center;
    flex-shrink: 0;
  }

  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
}

.avatar {
  position: relative;
  width: 22px;
  height: 22px;
  overflow: hidden;
  border-radius: 50%;
  background: var(--bg-active);
  flex-shrink: 0;

  img,
  .avatar-placeholder {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }

  img {
    object-fit: cover;
  }

  .avatar-placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary);
    font-size: 11px;
  }
}

.submenu-arrow {
  margin-left: 8px;
  color: var(--text-muted);
  font-size: 18px;
  line-height: 1;
  flex-shrink: 0;
}

.submenu {
  position: fixed;
  min-width: 170px;
  max-width: calc(100vw - 16px);
  max-height: calc(100vh - 16px);
  overflow-y: auto;
  visibility: hidden;

  &.ready {
    visibility: visible;
  }
}

.divider {
  height: 1px;
  background: var(--border-color);
  margin: 4px 8px;
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.15s;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>

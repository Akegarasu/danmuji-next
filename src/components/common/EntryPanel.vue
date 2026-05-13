<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { useDanmakuStore } from '@/stores/danmaku'
import { useSettingsStore } from '@/stores/settings'
import { formatEventTime, getMedalGradient } from '@/types'

const danmakuStore = useDanmakuStore()
const settingsStore = useSettingsStore()

// ==================== 面板高度与拖拽 ====================

const panelHeight = ref(settingsStore.entryPanelHeight)
const isDragging = ref(false)
let startY = 0
let startHeight = 0

const startResize = (e: MouseEvent) => {
  e.preventDefault()
  isDragging.value = true
  startY = e.clientY
  startHeight = panelHeight.value
  document.addEventListener('mousemove', onMouseMove)
  document.addEventListener('mouseup', onMouseUp)
}

const onMouseMove = (e: MouseEvent) => {
  const delta = startY - e.clientY
  panelHeight.value = Math.min(400, Math.max(60, startHeight + delta))
}

const onMouseUp = () => {
  isDragging.value = false
  document.removeEventListener('mousemove', onMouseMove)
  document.removeEventListener('mouseup', onMouseUp)
  settingsStore.updateDisplaySettings({ entryPanelHeight: panelHeight.value })
}

watch(() => settingsStore.entryPanelHeight, (v) => {
  if (!isDragging.value) panelHeight.value = v
})

// ==================== 过滤 ====================

const filteredEntries = computed(() => {
  const list = danmakuStore.interactWordList
  if (settingsStore.entryFilterAll) return list

  return list.filter(entry => {
    if (settingsStore.entryFilterGovernor && entry.user.guard_level === 1) return true
    if (settingsStore.entryFilterAdmiral && entry.user.guard_level === 2) return true
    if (settingsStore.entryFilterCaptain && entry.user.guard_level === 3) return true
    if (settingsStore.entryFilterSpecialFollow && settingsStore.isSpecialFollow(entry.user.uid)) return true
    return false
  })
})

// ==================== 自动滚动 ====================

const listRef = ref<HTMLElement>()
const autoScroll = ref(true)
const isScrolling = ref(false)
let resizeObserver: ResizeObserver | null = null

const doScrollToBottom = () => {
  if (!listRef.value) return
  isScrolling.value = true
  listRef.value.scrollTo({ top: listRef.value.scrollHeight, behavior: 'instant' })
  requestAnimationFrame(() => {
    requestAnimationFrame(() => { isScrolling.value = false })
  })
}

watch(() => filteredEntries.value.length, () => {
  if (autoScroll.value && listRef.value) {
    requestAnimationFrame(() => doScrollToBottom())
  }
})

const onScroll = () => {
  if (isScrolling.value || !listRef.value) return
  const el = listRef.value
  autoScroll.value = el.scrollHeight - el.scrollTop - el.clientHeight < 50
}

// ==================== 工具函数 ====================

const getGuardName = (level: number) => {
  switch (level) {
    case 1: return '总督'
    case 2: return '提督'
    case 3: return '舰长'
    default: return ''
  }
}

// ==================== 生命周期 ====================

onMounted(() => {
  if (listRef.value) {
    resizeObserver = new ResizeObserver(() => {
      if (autoScroll.value) {
        requestAnimationFrame(() => doScrollToBottom())
      }
    })
    resizeObserver.observe(listRef.value)
  }
})

onUnmounted(() => {
  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }
  document.removeEventListener('mousemove', onMouseMove)
  document.removeEventListener('mouseup', onMouseUp)
})
</script>

<template>
  <div
    v-if="settingsStore.entryShowEnabled"
    class="entry-panel"
    :style="{ height: panelHeight + 'px' }"
    :class="{ dragging: isDragging }"
  >
    <div class="resize-handle" @mousedown="startResize">
      <div class="handle-line" />
    </div>

    <div ref="listRef" class="entry-list" @scroll="onScroll">
      <div
        v-for="entry in filteredEntries"
        :key="entry.id"
        class="entry-item"
        :class="{
          'has-guard': entry.user.guard_level > 0,
          'is-special-follow': settingsStore.isSpecialFollow(entry.user.uid)
        }"
      >
        

        <span
          v-if="settingsStore.entryShowGuard && entry.user.guard_level"
          class="guard-badge"
          :class="`guard-${entry.user.guard_level}`"
        >
          {{ getGuardName(entry.user.guard_level) }}
        </span>

        <span
          v-if="settingsStore.entryShowMedal && entry.user.medal"
          class="medal-badge"
          :style="{ backgroundImage: getMedalGradient(entry.user.medal.level) }"
        >
          {{ entry.user.medal.name }} {{ entry.user.medal.level }}
        </span>

        <span
          class="entry-name"
          :class="{
            'guard-1': entry.user.guard_level === 1,
            'guard-2': entry.user.guard_level === 2,
            'guard-3': entry.user.guard_level === 3,
            'special-follow': settingsStore.isSpecialFollow(entry.user.uid)
          }"
        >
          {{ entry.user.name }}
        </span>

        <span class="entry-time">{{ formatEventTime(entry.timestamp) }} 进入</span>

      </div>

      <div v-if="filteredEntries.length === 0" class="empty-state">
        <span class="text">等待观众进入...</span>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.entry-panel {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  border-top: 1px solid var(--border-color);
  background: var(--bg-primary);
  min-height: 60px;
  max-height: 400px;

  &.dragging {
    user-select: none;
  }
}

.resize-handle {
  flex-shrink: 0;
  height: 8px;
  cursor: row-resize;
  display: flex;
  align-items: center;
  justify-content: center;

  &:hover .handle-line {
    background: var(--accent-secondary);
    opacity: 0.6;
    width: 48px;
  }

  &:active .handle-line {
    background: var(--accent-secondary);
    opacity: 1;
    width: 48px;
  }
}

.handle-line {
  width: 32px;
  height: 3px;
  background: transparent;
  border-radius: 2px;
  transition: all 0.15s;
}

.entry-list {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 2px 8px;
  min-height: 0;
}

.entry-item {
  display: flex;
  align-items: center;
  gap: 0.4em;
  padding: 2px 4px;
  font-size: var(--content-font-size-xs);
  line-height: 1.6;
  animation: fadeIn 0.2s ease;

  &.has-guard {
    background: rgba(91, 142, 201, 0.06);
  }

  &.is-special-follow {
    background: rgba(245, 200, 66, 0.06);
  }
}

.entry-action {
  color: var(--text-muted);
  flex-shrink: 0;
  font-size: 0.9em;
}

.guard-badge {
  padding: 0 0.4em;
  border-radius: 0.2em;
  font-size: 0.8em;
  color: white;
  font-weight: 500;
  flex-shrink: 0;
  line-height: 1.4;

  &.guard-1 { background: var(--guard-governor); color: #333; }
  &.guard-2 { background: var(--guard-admiral); }
  &.guard-3 { background: var(--guard-captain); }
}

.medal-badge {
  padding: 0 0.35em;
  border-radius: 0.2em;
  font-size: 0.7em;
  color: white;
  font-weight: 500;
  flex-shrink: 0;
  line-height: 1.4;
  opacity: 0.9;
}

.entry-name {
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  min-width: 0;

  &.guard-1 { color: var(--guard-governor); }
  &.guard-2 { color: var(--guard-admiral); }
  &.guard-3 { color: var(--guard-captain); }
  &.special-follow { color: var(--accent-gold); }
}

.entry-time {
  color: var(--text-muted);
  font-size: 0.85em;
  flex-shrink: 0;
  margin-left: auto;
}

.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  min-height: 40px;
  color: var(--text-muted);

  .text {
    font-size: var(--font-size-xs);
  }
}

@keyframes fadeIn {
  from { opacity: 0; }
  to { opacity: 1; }
}
</style>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, computed } from 'vue'
import TitleBar from '@/components/common/TitleBar.vue'
import ArchiveDanmakuItem from '@/components/archive/ArchiveDanmakuItem.vue'
import ArchiveGiftItem from '@/components/archive/ArchiveGiftItem.vue'
import ArchiveSuperChatItem from '@/components/archive/ArchiveSuperChatItem.vue'
import { useArchiveStore } from '@/stores/archive'
import { initWindowManager, cleanupWindowManager } from '@/services/window-manager'
import { formatPrice } from '@/types'
import type { ArchiveContentType, ArchiveSession } from '@/types'

const archiveStore = useArchiveStore()

// 搜索防抖
let searchTimer: ReturnType<typeof setTimeout> | null = null
const searchInput = ref('')

const onSearchInput = () => {
  if (searchTimer) clearTimeout(searchTimer)
  searchTimer = setTimeout(() => {
    archiveStore.setSearchQuery(searchInput.value)
  }, 300)
}

// 格式化日期时间
const formatDateTime = (timestamp: number): string => {
  if (!timestamp) return ''
  const date = new Date(timestamp * 1000)
  const pad = (n: number) => n.toString().padStart(2, '0')
  return `${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`
}

// 格式化时长
const formatDuration = (session: ArchiveSession): string => {
  if (!session.end_time) return '进行中'
  const seconds = session.end_time - session.start_time
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  if (hours > 0) return `${hours}h${minutes}m`
  return `${minutes}m`
}

// 总结果数
const totalResults = computed(() => {
  return archiveStore.danmakuResult.total + archiveStore.giftResult.total + archiveStore.scResult.total
})

// 总页数（取最大值）
const totalPages = computed(() => {
  const maxTotal = Math.max(
    archiveStore.danmakuResult.total,
    archiveStore.giftResult.total,
    archiveStore.scResult.total
  )
  return Math.ceil(maxTotal / archiveStore.pageSize)
})

// 价格筛选
const minPriceInput = ref('')
const maxPriceInput = ref('')

const applyPriceFilter = () => {
  const min = minPriceInput.value ? Number(minPriceInput.value) * 10 : undefined // 元转电池
  const max = maxPriceInput.value ? Number(maxPriceInput.value) * 10 : undefined
  archiveStore.setPriceRange(min, max)
}

// 删除确认
const confirmDelete = ref<number | null>(null)
const handleDelete = (id: number) => {
  if (confirmDelete.value === id) {
    archiveStore.removeSession(id)
    confirmDelete.value = null
  } else {
    confirmDelete.value = id
    // 3秒后取消确认状态
    setTimeout(() => {
      if (confirmDelete.value === id) confirmDelete.value = null
    }, 3000)
  }
}

onMounted(async () => {
  await initWindowManager('archive')
  await archiveStore.loadSessions()
})

onUnmounted(async () => {
  await cleanupWindowManager('archive')
})
</script>

<template>
  <div class="archive-window">
    <TitleBar title="存档" :is-sub-window="true" window-label="archive" />

    <div class="archive-body">
      <!-- 侧边栏：会话列表 -->
      <div class="session-sidebar">
        <div class="sidebar-header">会话列表</div>
        <div class="session-list">
          <div v-if="archiveStore.sessions.length === 0" class="empty-state">
            暂无存档
          </div>
          <div
            v-for="session in archiveStore.sessions"
            :key="session.id"
            class="session-item"
            :class="{ active: archiveStore.selectedSessionId === session.id }"
            @click="archiveStore.selectSession(session.id)"
          >
            <div class="session-title">{{ session.room_title || `房间 ${session.room_id}` }}</div>
            <div class="session-meta">
              <span>{{ formatDateTime(session.start_time) }}</span>
              <span>{{ formatDuration(session) }}</span>
            </div>
            <div class="session-stats">
              <span title="弹幕">💬 {{ session.danmaku_count }}</span>
              <span title="礼物">🎁 {{ session.gift_count }}</span>
              <span title="SC">💰 {{ session.sc_count }}</span>
            </div>
            <button
              class="delete-btn"
              :class="{ confirming: confirmDelete === session.id }"
              @click.stop="handleDelete(session.id)"
              :title="confirmDelete === session.id ? '再次点击确认删除' : '删除'"
            >
              {{ confirmDelete === session.id ? '确认?' : '✕' }}
            </button>
          </div>
        </div>
      </div>

      <!-- 内容区 -->
      <div class="content-area">
        <!-- 未选中状态 -->
        <div v-if="!archiveStore.selectedSession" class="empty-content">
          <div class="empty-icon">📂</div>
          <div>选择一个会话查看存档</div>
        </div>

        <template v-else>
          <!-- 会话头部 -->
          <div class="session-header">
            <div class="header-title">
              <span class="room-name">{{ archiveStore.selectedSession.room_title || `房间 ${archiveStore.selectedSession.room_id}` }}</span>
              <span class="room-id">#{{ archiveStore.selectedSession.room_id }}</span>
            </div>
            <div class="header-stats">
              <span v-if="archiveStore.selectedSession.total_revenue > 0" class="revenue">
                收入 {{ formatPrice(archiveStore.selectedSession.total_revenue) }}
              </span>
              <span class="date-range">
                {{ formatDateTime(archiveStore.selectedSession.start_time) }}
                <template v-if="archiveStore.selectedSession.end_time">
                  ~ {{ formatDateTime(archiveStore.selectedSession.end_time) }}
                </template>
              </span>
            </div>
          </div>

          <!-- 筛选栏 -->
          <div class="filter-bar">
            <input
              v-model="searchInput"
              @input="onSearchInput"
              type="text"
              class="search-input"
              placeholder="搜索内容/用户名..."
            />
            <div class="type-tabs">
              <button
                v-for="t in (['all', 'danmaku', 'gift', 'superchat'] as ArchiveContentType[])"
                :key="t"
                class="type-tab"
                :class="{ active: archiveStore.contentType === t }"
                @click="archiveStore.setContentType(t)"
              >
                {{ { all: '全部', danmaku: '弹幕', gift: '礼物', superchat: 'SC' }[t] }}
              </button>
            </div>
            <div v-if="archiveStore.contentType === 'gift' || archiveStore.contentType === 'superchat'" class="price-filter">
              <input
                v-model="minPriceInput"
                @change="applyPriceFilter"
                type="number"
                class="price-input"
                placeholder="最低¥"
              />
              <span class="price-separator">-</span>
              <input
                v-model="maxPriceInput"
                @change="applyPriceFilter"
                type="number"
                class="price-input"
                placeholder="最高¥"
              />
            </div>
          </div>

          <!-- 内容列表 -->
          <div class="content-list" v-if="!archiveStore.loading">
            <template v-if="totalResults === 0">
              <div class="empty-results">无匹配结果</div>
            </template>
            <template v-else>
              <!-- 弹幕 -->
              <template v-if="archiveStore.danmakuResult.items.length > 0">
                <div v-if="archiveStore.contentType === 'all'" class="section-label">弹幕 ({{ archiveStore.danmakuResult.total }})</div>
                <ArchiveDanmakuItem
                  v-for="item in archiveStore.danmakuResult.items"
                  :key="'d' + item.id"
                  :item="item"
                />
              </template>

              <!-- 礼物 -->
              <template v-if="archiveStore.giftResult.items.length > 0">
                <div v-if="archiveStore.contentType === 'all'" class="section-label">礼物 ({{ archiveStore.giftResult.total }})</div>
                <ArchiveGiftItem
                  v-for="item in archiveStore.giftResult.items"
                  :key="'g' + item.id"
                  :item="item"
                />
              </template>

              <!-- SC -->
              <template v-if="archiveStore.scResult.items.length > 0">
                <div v-if="archiveStore.contentType === 'all'" class="section-label">SC ({{ archiveStore.scResult.total }})</div>
                <ArchiveSuperChatItem
                  v-for="item in archiveStore.scResult.items"
                  :key="'sc' + item.id"
                  :item="item"
                />
              </template>
            </template>
          </div>
          <div v-else class="loading">加载中...</div>

          <!-- 分页 -->
          <div v-if="totalPages > 1" class="pagination">
            <button
              class="page-btn"
              :disabled="archiveStore.currentPage <= 1"
              @click="archiveStore.setPage(archiveStore.currentPage - 1)"
            >
              ‹
            </button>
            <span class="page-info">{{ archiveStore.currentPage }} / {{ totalPages }}</span>
            <button
              class="page-btn"
              :disabled="archiveStore.currentPage >= totalPages"
              @click="archiveStore.setPage(archiveStore.currentPage + 1)"
            >
              ›
            </button>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.archive-window {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--bg-primary);
  color: var(--text-primary);
  border-radius: var(--border-radius);
  overflow: hidden;
}

.archive-body {
  display: flex;
  flex: 1;
  overflow: hidden;
}

// ==================== 侧边栏 ====================

.session-sidebar {
  width: 200px;
  flex-shrink: 0;
  border-right: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  background: var(--bg-secondary);
}

.sidebar-header {
  padding: 10px 12px;
  font-size: var(--font-size-sm, 13px);
  font-weight: 500;
  color: var(--text-secondary);
  border-bottom: 1px solid var(--border-color);
}

.session-list {
  flex: 1;
  overflow-y: auto;
}

.session-item {
  padding: 8px 12px;
  cursor: pointer;
  border-bottom: 1px solid var(--border-subtle, rgba(255,255,255,0.05));
  position: relative;
  transition: background 0.15s;

  &:hover {
    background: var(--bg-hover);
  }

  &.active {
    background: var(--bg-active);
    border-left: 2px solid var(--accent-primary);
  }
}

.session-title {
  font-size: var(--font-size-sm, 13px);
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  padding-right: 20px;
}

.session-meta {
  display: flex;
  gap: 8px;
  font-size: var(--font-size-xs, 11px);
  color: var(--text-muted);
  margin-top: 3px;
}

.session-stats {
  display: flex;
  gap: 8px;
  font-size: var(--font-size-xs, 11px);
  color: var(--text-secondary);
  margin-top: 3px;
}

.delete-btn {
  position: absolute;
  top: 6px;
  right: 6px;
  width: auto;
  min-width: 18px;
  height: 18px;
  border: none;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  border-radius: 3px;
  font-size: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
  transition: opacity 0.15s, background 0.15s, color 0.15s;
  padding: 0 4px;

  .session-item:hover & {
    opacity: 1;
  }

  &:hover {
    background: rgba(220, 60, 60, 0.2);
    color: #dc3c3c;
  }

  &.confirming {
    opacity: 1;
    background: rgba(220, 60, 60, 0.3);
    color: #dc3c3c;
    font-size: 10px;
  }
}

// ==================== 内容区 ====================

.content-area {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.empty-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
  gap: 8px;
}

.empty-icon {
  font-size: 32px;
  opacity: 0.5;
}

.empty-state {
  padding: 20px;
  text-align: center;
  color: var(--text-muted);
  font-size: var(--font-size-sm, 13px);
}

// 会话头部
.session-header {
  padding: 10px 14px;
  border-bottom: 1px solid var(--border-color);
  background: var(--bg-secondary);
}

.header-title {
  display: flex;
  align-items: baseline;
  gap: 8px;
}

.room-name {
  font-size: var(--font-size-base, 14px);
  font-weight: 500;
}

.room-id {
  font-size: var(--font-size-xs, 11px);
  color: var(--text-muted);
}

.header-stats {
  display: flex;
  gap: 12px;
  margin-top: 4px;
  font-size: var(--font-size-xs, 11px);
  color: var(--text-secondary);
}

.revenue {
  color: var(--accent-gold);
}

.date-range {
  color: var(--text-muted);
}

// 筛选栏
.filter-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 14px;
  border-bottom: 1px solid var(--border-color);
  flex-wrap: wrap;
}

.search-input {
  flex: 1;
  min-width: 120px;
  height: 28px;
  padding: 0 8px;
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm, 4px);
  background: var(--bg-card);
  color: var(--text-primary);
  font-size: var(--font-size-sm, 13px);
  outline: none;

  &:focus {
    border-color: var(--accent-primary);
  }

  &::placeholder {
    color: var(--text-muted);
  }
}

.type-tabs {
  display: flex;
  gap: 2px;
  background: var(--bg-card);
  border-radius: var(--border-radius-sm, 4px);
  padding: 2px;
}

.type-tab {
  padding: 3px 10px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  border-radius: 3px;
  font-size: var(--font-size-xs, 11px);
  transition: background 0.15s, color 0.15s;

  &:hover {
    color: var(--text-primary);
  }

  &.active {
    background: var(--accent-primary);
    color: #fff;
  }
}

.price-filter {
  display: flex;
  align-items: center;
  gap: 4px;
}

.price-input {
  width: 60px;
  height: 28px;
  padding: 0 6px;
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm, 4px);
  background: var(--bg-card);
  color: var(--text-primary);
  font-size: var(--font-size-xs, 11px);
  outline: none;

  &:focus {
    border-color: var(--accent-primary);
  }

  &::placeholder {
    color: var(--text-muted);
  }
}

.price-separator {
  color: var(--text-muted);
}

// 内容列表
.content-list {
  flex: 1;
  overflow-y: auto;
  padding: 0 4px 4px 0;
}

.section-label {
  padding: 6px 14px 4px;
  font-size: var(--font-size-xs, 11px);
  color: var(--text-muted);
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  background: rgb(25, 25, 25);
  position: sticky;
  top: 0;
  z-index: 1;
  border-bottom: 1px solid var(--border-color);
}

.empty-results {
  padding: 40px 20px;
  text-align: center;
  color: var(--text-muted);
  font-size: var(--font-size-sm, 13px);
}

.loading {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
}

// 分页
.pagination {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 8px;
  border-top: 1px solid var(--border-color);
}

.page-btn {
  width: 28px;
  height: 28px;
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm, 4px);
  background: var(--bg-card);
  color: var(--text-primary);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 14px;

  &:hover:not(:disabled) {
    background: var(--bg-hover);
  }

  &:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }
}

.page-info {
  font-size: var(--font-size-sm, 13px);
  color: var(--text-secondary);
}
</style>

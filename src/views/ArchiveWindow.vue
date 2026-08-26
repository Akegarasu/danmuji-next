<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import TitleBar from '@/components/common/TitleBar.vue'
import ConfirmDialog from '@/components/common/ConfirmDialog.vue'
import ArchiveSearchResults from '@/components/archive/ArchiveSearchResults.vue'
import ArchiveStatsPanel from '@/components/archive/ArchiveStatsPanel.vue'
import { useArchiveStore } from '@/stores/archive'
import { initWindowManager, cleanupWindowManager } from '@/services/window-manager'
import { formatPrice } from '@/types'
import type { ArchiveContentType, ArchiveSession } from '@/types'

const archiveStore = useArchiveStore()
const activityInput = ref('')
const roomFilterInput = ref('')
const startDate = ref('')
const endDate = ref('')
const dateError = ref('')
type DateRangePreset = '7' | '30' | 'all' | 'custom'
type RoomSort = 'recent' | 'revenue' | 'sessions' | 'danmaku'
type NoticeType = 'success' | 'error'

const dateDirty = ref(false)
const activeRange = ref<DateRangePreset>('all')
const roomSort = ref<RoomSort>('recent')
const deleteTarget = ref<ArchiveSession | null>(null)
const deleteDialogVisible = ref(false)
const pruneDialogVisible = ref(false)
const operationNotice = ref<{ message: string; type: NoticeType } | null>(null)
let activityTimer: ReturnType<typeof setTimeout> | undefined
let roomTimer: ReturnType<typeof setTimeout> | undefined
let noticeTimer: ReturnType<typeof setTimeout> | undefined

const contentTypes: { value: ArchiveContentType; label: string }[] = [
  { value: 'all', label: '全部' },
  { value: 'danmaku', label: '弹幕' },
  { value: 'gift', label: '礼物' },
  { value: 'superchat', label: '醒目留言' },
]

const pageTitle = computed(() => {
  if (archiveStore.view === 'session') return '场次详情'
  if (archiveStore.view === 'room') return archiveStore.selectedRoom?.room_title || '直播间归档'
  return '归档总览'
})

const sortedRooms = computed(() => {
  const rooms = [...archiveStore.overview.rooms]
  switch (roomSort.value) {
    case 'revenue':
      return rooms.sort((a, b) => b.total_revenue - a.total_revenue)
    case 'sessions':
      return rooms.sort((a, b) => b.session_count - a.session_count)
    case 'danmaku':
      return rooms.sort((a, b) => b.danmaku_count - a.danmaku_count)
    default:
      return rooms.sort((a, b) => b.last_live_time - a.last_live_time)
  }
})

const roomCountText = computed(() => {
  const shown = archiveStore.overview.rooms.length
  const total = archiveStore.overview.summary.room_count
  return roomFilterInput.value.trim() ? `显示 ${shown} / ${total} 个房间` : `${total} 个房间`
})

const hasDateFilter = computed(() => Boolean(startDate.value || endDate.value))
const rangeLabel = computed(() => {
  if (!startDate.value && !endDate.value) return '全部时间'
  if (startDate.value && endDate.value) return `${startDate.value} 至 ${endDate.value}`
  if (startDate.value) return `${startDate.value} 起`
  return `截至 ${endDate.value}`
})

const searchEmptyText = computed(() => {
  if (archiveStore.searchQuery) return '没有匹配当前关键词的归档记录'
  if (hasDateFilter.value) return '所选时间范围内没有归档记录'
  if (archiveStore.view === 'session') return '本场直播没有可展示的互动记录'
  return '该直播间还没有互动记录'
})

const roomsEmptyText = computed(() => {
  if (roomFilterInput.value) return '没有匹配的直播间'
  if (hasDateFilter.value) return '所选时间内没有直播间归档'
  return '还没有直播间归档'
})

const deleteMessage = computed(() => {
  if (!deleteTarget.value) return ''
  const title = deleteTarget.value.room_title || archiveStore.selectedRoom?.room_title || '当前直播间'
  return `${title}\n${formatDateTime(deleteTarget.value.start_time)}\n\n将永久删除本场弹幕、礼物和醒目留言，且无法恢复。`
})
const pruneMessage = '将删除没有弹幕、礼物或醒目留言的历史场次。\n\n该操作不会影响包含任何互动记录的存档。'

const formatDateTime = (timestamp: number) => {
  const date = new Date(timestamp * 1000)
  return new Intl.DateTimeFormat('zh-CN', {
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', hour12: false,
  }).format(date)
}

const formatShortDate = (timestamp: number) =>
  new Intl.DateTimeFormat('zh-CN', { year: 'numeric', month: '2-digit', day: '2-digit' })
    .format(new Date(timestamp * 1000))

const monthDayFormatter = new Intl.DateTimeFormat('zh-CN', { month: '2-digit', day: '2-digit' })
const formatMonthDay = (timestamp: number) => monthDayFormatter.format(new Date(timestamp * 1000))

const MINUTES_PER_HOUR = 60
const MINUTES_PER_DAY = 1440
const formatSecondsDuration = (seconds: number) => {
  const minutes = Math.max(0, Math.floor(seconds / 60))
  if (minutes >= MINUTES_PER_DAY) {
    return `${Math.floor(minutes / MINUTES_PER_DAY)}天 ${Math.floor((minutes % MINUTES_PER_DAY) / MINUTES_PER_HOUR)}小时`
  }
  if (minutes >= MINUTES_PER_HOUR) {
    return `${Math.floor(minutes / MINUTES_PER_HOUR)}小时 ${minutes % MINUTES_PER_HOUR}分`
  }
  return `${minutes}分钟`
}

const formatDuration = (session: ArchiveSession) => {
  if (!session.end_time) return '进行中'
  return formatSecondsDuration(session.end_time - session.start_time)
}

const localDateValue = (date: Date) => {
  const offset = date.getTimezoneOffset() * 60_000
  return new Date(date.getTime() - offset).toISOString().slice(0, 10)
}

const toTimestamp = (value: string, endExclusive = false) => {
  if (!value) return undefined
  const date = new Date(`${value}T00:00:00`)
  if (endExclusive) date.setDate(date.getDate() + 1)
  return Math.floor(date.getTime() / 1000)
}

const markDateDirty = () => {
  activeRange.value = 'custom'
  dateDirty.value = true
  dateError.value = ''
}

const applyDateRange = async (range: DateRangePreset = 'custom') => {
  const fromTime = toTimestamp(startDate.value)
  const toTime = toTimestamp(endDate.value, true)
  if (fromTime !== undefined && toTime !== undefined && fromTime >= toTime) {
    dateError.value = '开始日期不能晚于结束日期'
    return
  }
  dateError.value = ''
  activeRange.value = range
  dateDirty.value = false
  await archiveStore.applyDateFilter({ fromTime, toTime })
}

const setRecentDays = async (days?: 7 | 30) => {
  if (!days) {
    startDate.value = ''
    endDate.value = ''
    await applyDateRange('all')
    return
  }
  const end = new Date()
  const start = new Date()
  start.setDate(start.getDate() - days + 1)
  startDate.value = localDateValue(start)
  endDate.value = localDateValue(end)
  await applyDateRange(days === 7 ? '7' : '30')
}

const onActivityInput = () => {
  if (activityTimer) clearTimeout(activityTimer)
  activityTimer = setTimeout(() => archiveStore.runSearch(activityInput.value, 1), 300)
}

const submitActivitySearch = async () => {
  if (activityTimer) clearTimeout(activityTimer)
  await archiveStore.runSearch(activityInput.value, 1)
}

const clearActivitySearch = async () => {
  if (activityTimer) clearTimeout(activityTimer)
  activityInput.value = ''
  await archiveStore.runSearch('', 1)
}

const retrySearch = () => archiveStore.runSearch(activityInput.value, archiveStore.searchResult.page)

const setContentType = async (type: ArchiveContentType) => {
  if (activityTimer) clearTimeout(activityTimer)
  await archiveStore.setContentType(type, activityInput.value)
}

const onRoomFilterInput = () => {
  if (roomTimer) clearTimeout(roomTimer)
  roomTimer = setTimeout(() => archiveStore.loadOverview(roomFilterInput.value), 250)
}

const clearRoomFilter = async () => {
  if (roomTimer) clearTimeout(roomTimer)
  roomFilterInput.value = ''
  await archiveStore.loadOverview('')
}

const clearPendingTimers = () => {
  if (activityTimer) clearTimeout(activityTimer)
  if (roomTimer) clearTimeout(roomTimer)
}

const openRoom = async (room: Parameters<typeof archiveStore.openRoom>[0]) => {
  clearPendingTimers()
  activityInput.value = ''
  await archiveStore.openRoom(room)
}

const openSession = async (session: ArchiveSession) => {
  clearPendingTimers()
  activityInput.value = ''
  await archiveStore.openSession(session)
}

const goOverview = async () => {
  clearPendingTimers()
  activityInput.value = ''
  roomFilterInput.value = ''
  await archiveStore.goOverview()
}

const goRoom = async () => {
  clearPendingTimers()
  activityInput.value = ''
  await archiveStore.goRoom()
}

const showOperationNotice = (message: string, type: NoticeType = 'success') => {
  operationNotice.value = { message, type }
  if (noticeTimer) clearTimeout(noticeTimer)
  noticeTimer = setTimeout(() => {
    operationNotice.value = null
  }, 4000)
}

const requestDelete = (session: ArchiveSession) => {
  deleteTarget.value = session
  deleteDialogVisible.value = true
}

const confirmDeleteSession = async () => {
  const target = deleteTarget.value
  if (!target) return
  try {
    await archiveStore.removeSession(target.id)
    deleteTarget.value = null
    showOperationNotice('场次归档已删除')
  } catch {
    showOperationNotice('删除失败，请根据错误提示重试', 'error')
  } finally {
    deleteDialogVisible.value = false
  }
}

const confirmPrune = async () => {
  try {
    const deleted = await archiveStore.pruneEmptySessions()
    showOperationNotice(deleted > 0 ? `已清理 ${deleted} 个空场次` : '没有需要清理的空场次')
  } catch {
    showOperationNotice('清理失败，请根据错误提示重试', 'error')
  } finally {
    pruneDialogVisible.value = false
  }
}

onMounted(async () => {
  await initWindowManager('archive')
  await archiveStore.loadOverview()
})

onUnmounted(async () => {
  if (activityTimer) clearTimeout(activityTimer)
  if (roomTimer) clearTimeout(roomTimer)
  if (noticeTimer) clearTimeout(noticeTimer)
  await cleanupWindowManager('archive')
})
</script>

<template>
  <div class="archive-window">
    <TitleBar title="数据存档" :is-sub-window="true" window-label="archive" />

    <main class="archive-body">
      <header class="page-toolbar">
        <div class="breadcrumb" aria-label="当前位置">
          <button v-if="archiveStore.view !== 'overview'" @click="goOverview">总览</button>
          <span v-if="archiveStore.view !== 'overview'">/</span>
          <button v-if="archiveStore.view === 'session'" @click="goRoom">
            {{ archiveStore.selectedRoom?.room_title || '直播间' }}
          </button>
          <span v-if="archiveStore.view === 'session'">/</span>
          <strong>{{ pageTitle }}</strong>
        </div>

        <div class="filter-toolbar">
          <div class="quick-ranges" aria-label="快捷时间范围">
            <button :class="{ active: activeRange === '7' }" @click="setRecentDays(7)">近 7 天</button>
            <button :class="{ active: activeRange === '30' }" @click="setRecentDays(30)">近 30 天</button>
            <button :class="{ active: activeRange === 'all' }" @click="setRecentDays()">全部</button>
          </div>
          <div class="date-filter" :title="`当前范围：${rangeLabel}`">
            <label><span>从</span><input v-model="startDate" type="date" @input="markDateDirty" /></label>
            <label><span>至</span><input v-model="endDate" type="date" @input="markDateDirty" /></label>
            <button class="apply-date" :disabled="!dateDirty" @click="applyDateRange('custom')">应用</button>
          </div>
          <button
            class="refresh-button"
            :disabled="archiveStore.loadingPage || archiveStore.loadingSessions"
            title="刷新当前归档"
            aria-label="刷新当前归档"
            @click="archiveStore.refreshCurrentView"
          >
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20 6v5h-5M4 18v-5h5M18.5 9A7 7 0 0 0 6.2 6.5L4 11M5.5 15A7 7 0 0 0 17.8 17.5L20 13" /></svg>
          </button>
        </div>
      </header>

      <div v-if="archiveStore.loadingPage || archiveStore.loadingSessions" class="top-progress" aria-label="正在更新归档" />

      <div v-if="dateError || archiveStore.error" class="error-banner" role="alert">
        <span>{{ dateError || archiveStore.error }}</span>
        <div>
          <button v-if="archiveStore.error" @click="archiveStore.refreshCurrentView">重试</button>
          <button aria-label="关闭错误提示" @click="dateError = ''; archiveStore.dismissError()">×</button>
        </div>
      </div>

      <Transition name="notice">
        <div v-if="operationNotice" class="operation-notice" :class="operationNotice.type" role="status">
          {{ operationNotice.message }}
        </div>
      </Transition>

      <div v-if="archiveStore.loadingPage && !archiveStore.initialized" class="page-loading">
        <i aria-hidden="true" />
        <span>正在整理归档数据…</span>
      </div>

      <template v-else-if="archiveStore.view === 'overview'">
        <ArchiveStatsPanel
          :summary="archiveStore.overview.summary"
          :daily="archiveStore.statistics.daily"
          :loading="archiveStore.loadingPage"
        />

        <section class="panel global-search">
          <div class="section-heading">
            <div>
              <h2>全局搜索</h2>
              <p>可搜索弹幕内容、礼物、用户名或 UID</p>
            </div>
            <span v-if="archiveStore.searchQuery" class="result-count">{{ archiveStore.searchResult.total }} 条结果</span>
          </div>
          <div class="search-row">
            <div class="search-box">
              <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7" /><path d="m20 20-4-4" /></svg>
              <input
                v-model="activityInput"
                type="search"
                placeholder="输入弹幕内容、用户名或 UID…"
                @input="onActivityInput"
                @keyup.enter="submitActivitySearch"
              />
              <button v-if="activityInput" aria-label="清空搜索" @click="clearActivitySearch">×</button>
            </div>
            <div class="type-tabs" aria-label="记录类型">
              <button
                v-for="type in contentTypes"
                :key="type.value"
                :class="{ active: archiveStore.contentType === type.value }"
                :aria-pressed="archiveStore.contentType === type.value"
                @click="setContentType(type.value)"
              >{{ type.label }}</button>
            </div>
          </div>
          <ArchiveSearchResults
            v-if="archiveStore.searchQuery || archiveStore.loadingSearch || archiveStore.searchError"
            :result="archiveStore.searchResult"
            :loading="archiveStore.loadingSearch"
            :error="archiveStore.searchError"
            empty-text="没有匹配当前关键词的归档记录"
            show-room
            @page="page => archiveStore.runSearch(activityInput, page)"
            @retry="retrySearch"
          />
        </section>

        <section class="rooms-section">
          <div class="section-heading rooms-heading">
            <div>
              <h2>直播间</h2>
              <p>{{ roomCountText }}，按所选方式排序</p>
            </div>
            <div class="rooms-actions">
              <button
                class="prune-button"
                title="删除没有弹幕、礼物或醒目留言的历史场次"
                :disabled="archiveStore.pruning"
                @click="pruneDialogVisible = true"
              >清理空场次</button>
              <label class="sort-select">
                <span>排序</span>
                <select v-model="roomSort">
                  <option value="recent">最近直播</option>
                  <option value="revenue">累计收益</option>
                  <option value="sessions">直播场次</option>
                  <option value="danmaku">弹幕数量</option>
                </select>
              </label>
              <div class="room-filter-wrap">
                <input
                  v-model="roomFilterInput"
                  class="room-filter"
                  type="search"
                  placeholder="筛选标题 / 房间 ID / UID"
                  @input="onRoomFilterInput"
                />
                <button v-if="roomFilterInput" aria-label="清空房间筛选" @click="clearRoomFilter">×</button>
              </div>
            </div>
          </div>

          <div v-if="sortedRooms.length" class="room-grid" :class="{ refreshing: archiveStore.loadingPage }">
            <button
              v-for="room in sortedRooms"
              :key="room.room_id"
              class="room-card"
              @click="openRoom(room)"
            >
              <div class="room-avatar" aria-hidden="true">{{ (room.room_title || '房').slice(0, 1) }}</div>
              <div class="room-info">
                <div class="room-title-line">
                  <strong>{{ room.room_title }}</strong>
                  <span>#{{ room.room_id }}</span>
                </div>
                <p>最近直播 {{ formatShortDate(room.last_live_time) }} · {{ formatSecondsDuration(room.live_duration) }}</p>
                <div class="room-metrics">
                  <span><b>{{ room.session_count }}</b> 场</span>
                  <span><b>{{ room.danmaku_count }}</b> 弹幕</span>
                  <span><b>{{ formatPrice(room.total_revenue) || '¥0' }}</b> 收益</span>
                </div>
              </div>
              <span class="room-arrow" aria-hidden="true">›</span>
            </button>
          </div>
          <div v-else class="empty-panel">
            <strong>{{ roomsEmptyText }}</strong>
            <span v-if="roomFilterInput">请尝试其他标题、房间 ID 或主播 UID</span>
            <button v-if="roomFilterInput" @click="clearRoomFilter">清空筛选</button>
          </div>
        </section>
      </template>

      <template v-else-if="archiveStore.view === 'room' && archiveStore.selectedRoom">
        <section class="room-hero">
          <div class="room-avatar large" aria-hidden="true">{{ archiveStore.selectedRoom.room_title.slice(0, 1) }}</div>
          <div>
            <h1>{{ archiveStore.selectedRoom.room_title }}</h1>
            <p>房间 {{ archiveStore.selectedRoom.room_id }} · 主播 UID {{ archiveStore.selectedRoom.streamer_uid }}</p>
          </div>
        </section>

        <ArchiveStatsPanel
          :summary="archiveStore.statistics.summary"
          :daily="archiveStore.statistics.daily"
          :loading="archiveStore.loadingPage"
        />

        <div class="room-content-grid">
          <section class="panel session-panel">
            <div class="section-heading compact">
              <div>
                <h2>直播场次</h2>
                <p>共 {{ archiveStore.roomSessions.total }} 场 · {{ rangeLabel }}</p>
              </div>
            </div>

            <div v-if="archiveStore.loadingSessions && archiveStore.roomSessions.items.length === 0" class="session-skeleton" aria-label="正在加载直播场次">
              <i v-for="index in 4" :key="index" />
            </div>
            <div v-else-if="archiveStore.roomSessions.items.length" class="session-list" :class="{ refreshing: archiveStore.loadingSessions }">
              <article
                v-for="session in archiveStore.roomSessions.items"
                :key="session.id"
                class="session-card"
              >
                <button class="session-open" @click="openSession(session)">
                  <div class="session-date">
                    <strong>{{ formatMonthDay(session.start_time) }}</strong>
                    <span>{{ new Date(session.start_time * 1000).getFullYear() }}</span>
                  </div>
                  <div class="session-info">
                    <div class="session-title-line">
                      <strong>{{ session.room_title || archiveStore.selectedRoom.room_title }}</strong>
                      <span v-if="!session.end_time" class="live-badge">进行中</span>
                    </div>
                    <p>{{ formatDateTime(session.start_time) }} · {{ formatDuration(session) }}</p>
                    <div><span>{{ session.danmaku_count }} 弹幕</span><span>{{ session.gift_count }} 礼物</span><b>{{ formatPrice(session.total_revenue) || '¥0' }}</b></div>
                  </div>
                </button>
                <button
                  v-if="session.end_time"
                  class="delete-button"
                  title="删除本场归档"
                  aria-label="删除本场归档"
                  @click="requestDelete(session)"
                >
                  <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16M9 7V4h6v3M7 7l1 13h8l1-13M10 11v5M14 11v5" /></svg>
                </button>
              </article>
            </div>
            <div v-else class="empty-panel small">
              <strong>所选时间内没有直播场次</strong>
              <span>可调整顶部日期范围后重试</span>
            </div>
            <div v-if="archiveStore.roomPages > 1" class="pagination">
              <button :disabled="archiveStore.loadingSessions || archiveStore.roomSessions.page <= 1" @click="archiveStore.loadRoomSessions(archiveStore.roomSessions.page - 1)">上一页</button>
              <span>{{ archiveStore.roomSessions.page }} / {{ archiveStore.roomPages }}</span>
              <button :disabled="archiveStore.loadingSessions || archiveStore.roomSessions.page >= archiveStore.roomPages" @click="archiveStore.loadRoomSessions(archiveStore.roomSessions.page + 1)">下一页</button>
            </div>
          </section>

          <section class="panel activity-panel">
            <div class="section-heading compact">
              <div>
                <h2>房间记录</h2>
                <p>在该直播间的全部场次中搜索</p>
              </div>
              <span class="result-count">{{ archiveStore.searchResult.total }} 条</span>
            </div>
            <div class="stacked-search">
              <div class="search-box">
                <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7" /><path d="m20 20-4-4" /></svg>
                <input v-model="activityInput" type="search" placeholder="内容、用户名或 UID" @input="onActivityInput" @keyup.enter="submitActivitySearch" />
                <button v-if="activityInput" aria-label="清空搜索" @click="clearActivitySearch">×</button>
              </div>
              <div class="type-tabs" aria-label="记录类型">
                <button v-for="type in contentTypes" :key="type.value" :class="{ active: archiveStore.contentType === type.value }" :aria-pressed="archiveStore.contentType === type.value" @click="setContentType(type.value)">{{ type.label }}</button>
              </div>
            </div>
            <ArchiveSearchResults
              :result="archiveStore.searchResult"
              :loading="archiveStore.loadingSearch"
              :error="archiveStore.searchError"
              :empty-text="searchEmptyText"
              @page="page => archiveStore.runSearch(activityInput, page)"
              @retry="retrySearch"
            />
          </section>
        </div>
      </template>

      <template v-else-if="archiveStore.view === 'session' && archiveStore.selectedSession">
        <section class="session-hero">
          <div>
            <span class="eyebrow">单场直播归档</span>
            <h1>{{ archiveStore.selectedSession.room_title || archiveStore.selectedRoom?.room_title }}</h1>
            <p>{{ formatDateTime(archiveStore.selectedSession.start_time) }} — {{ archiveStore.selectedSession.end_time ? formatDateTime(archiveStore.selectedSession.end_time) : '进行中' }}</p>
          </div>
          <div class="session-summary">
            <span><b>{{ formatDuration(archiveStore.selectedSession) }}</b>直播时长</span>
            <span><b>{{ archiveStore.selectedSession.danmaku_count }}</b>弹幕</span>
            <span><b>{{ archiveStore.selectedSession.gift_count + archiveStore.selectedSession.sc_count }}</b>付费互动</span>
            <span><b class="gold">{{ formatPrice(archiveStore.selectedSession.total_revenue) || '¥0' }}</b>本场收益</span>
          </div>
        </section>

        <section class="panel session-events">
          <div class="section-heading">
            <div>
              <h2>本场记录</h2>
              <p>搜索本场弹幕内容、用户名或 UID<span v-if="hasDateFilter"> · 已应用 {{ rangeLabel }}</span></p>
            </div>
            <span class="result-count">{{ archiveStore.searchResult.total }} 条</span>
          </div>
          <div class="search-row">
            <div class="search-box">
              <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7" /><path d="m20 20-4-4" /></svg>
              <input v-model="activityInput" type="search" placeholder="搜索本场记录…" @input="onActivityInput" @keyup.enter="submitActivitySearch" />
              <button v-if="activityInput" aria-label="清空搜索" @click="clearActivitySearch">×</button>
            </div>
            <div class="type-tabs" aria-label="记录类型">
              <button v-for="type in contentTypes" :key="type.value" :class="{ active: archiveStore.contentType === type.value }" :aria-pressed="archiveStore.contentType === type.value" @click="setContentType(type.value)">{{ type.label }}</button>
            </div>
          </div>
          <ArchiveSearchResults
            :result="archiveStore.searchResult"
            :loading="archiveStore.loadingSearch"
            :error="archiveStore.searchError"
            :empty-text="searchEmptyText"
            @page="page => archiveStore.runSearch(activityInput, page)"
            @retry="retrySearch"
          />
        </section>
      </template>
    </main>

    <ConfirmDialog
      v-model:visible="deleteDialogVisible"
      title="删除场次归档"
      :message="deleteMessage"
      confirm-text="永久删除"
      loading-text="正在删除…"
      danger
      :loading="archiveStore.deletingSessionId !== null"
      :close-on-confirm="false"
      @confirm="confirmDeleteSession"
      @cancel="deleteTarget = null"
    />
    <ConfirmDialog
      v-model:visible="pruneDialogVisible"
      title="清理空场次"
      :message="pruneMessage"
      confirm-text="开始清理"
      loading-text="正在清理…"
      danger
      :loading="archiveStore.pruning"
      :close-on-confirm="false"
      @confirm="confirmPrune"
    />
  </div>
</template>

<style scoped lang="scss">
.archive-window {
  display: flex;
  height: 100vh;
  flex-direction: column;
  overflow: hidden;
  border-radius: var(--border-radius);
  background: var(--bg-primary);
  color: var(--text-primary);
}
.archive-body { position: relative; flex: 1; overflow-y: auto; padding: 14px 16px 22px; }
button, input, select { font: inherit; }
button { color: inherit; }
button:focus-visible, input:focus-visible, select:focus-visible { outline: 2px solid var(--accent-primary); outline-offset: 1px; }
button:disabled { cursor: default; opacity: 0.45; }

.page-toolbar {
  position: sticky;
  z-index: 5;
  top: -14px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin: -14px -16px 14px;
  padding: 10px 16px;
  border-bottom: 1px solid var(--border-color);
  background: rgb(25, 25, 25);
}
.breadcrumb { display: flex; min-width: 90px; align-items: center; gap: 7px; font-size: var(--font-size-sm); }
.breadcrumb button { overflow: hidden; border: 0; background: transparent; color: var(--accent-primary); cursor: pointer; text-overflow: ellipsis; white-space: nowrap; }
.breadcrumb strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.breadcrumb span { color: var(--text-muted); }
.filter-toolbar, .date-filter, .quick-ranges, .date-filter label { display: flex; align-items: center; gap: 5px; }
.filter-toolbar { min-width: 0; justify-content: flex-end; }
.quick-ranges { margin-right: 3px; }
.quick-ranges button, .apply-date, .pagination button, .refresh-button {
  padding: 5px 8px;
  border: 1px solid var(--border-color);
  border-radius: 4px;
  background: var(--bg-card);
  cursor: pointer;
}
.quick-ranges button:hover, .apply-date:hover:not(:disabled), .pagination button:hover:not(:disabled), .refresh-button:hover:not(:disabled) { background: var(--bg-hover); }
.quick-ranges button.active { border-color: rgba(92, 158, 255, 0.55); background: rgba(92, 158, 255, 0.16); color: #a9cbff; }
.date-filter label span { color: var(--text-muted); font-size: var(--font-size-xs); }
.date-filter input {
  width: 116px;
  padding: 4px 6px;
  border: 1px solid var(--border-color);
  border-radius: 4px;
  outline: none;
  background: var(--bg-card);
  color: var(--text-primary);
  color-scheme: dark;
  font-size: var(--font-size-xs);
}
.date-filter input:focus { border-color: var(--accent-primary); }
.apply-date { color: var(--accent-primary); font-size: var(--font-size-xs); }
.refresh-button { display: grid; width: 28px; height: 28px; padding: 5px; place-items: center; }
.refresh-button svg { width: 15px; height: 15px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }

.top-progress { position: sticky; z-index: 6; top: 37px; height: 2px; margin: -14px -16px 12px; overflow: hidden; background: rgba(92, 158, 255, 0.12); }
.top-progress::after { display: block; width: 40%; height: 100%; background: var(--accent-primary); content: ''; animation: progressMove 1s ease-in-out infinite; }
.error-banner { display: flex; align-items: center; justify-content: space-between; gap: 10px; margin-bottom: 12px; padding: 8px 10px; border: 1px solid rgba(220, 60, 60, 0.4); border-radius: 6px; background: rgba(220, 60, 60, 0.12); color: #ef8a8a; font-size: var(--font-size-sm); }
.error-banner > span { min-width: 0; word-break: break-word; }
.error-banner > div { display: flex; flex: 0 0 auto; gap: 5px; }
.error-banner button { padding: 2px 6px; border: 0; border-radius: 3px; background: transparent; color: inherit; cursor: pointer; }
.error-banner button:hover { background: rgba(255, 255, 255, 0.08); }
.operation-notice { position: fixed; z-index: 20; top: 50px; left: 50%; padding: 7px 12px; transform: translateX(-50%); border: 1px solid rgba(74, 180, 115, 0.4); border-radius: 6px; background: rgb(31, 54, 40); color: #8de0ad; box-shadow: 0 5px 18px rgba(0, 0, 0, 0.25); font-size: var(--font-size-xs); }
.operation-notice.error { border-color: rgba(220, 60, 60, 0.45); background: rgb(63, 34, 34); color: #ef8a8a; }
.page-loading { display: flex; height: 240px; align-items: center; justify-content: center; gap: 9px; color: var(--text-muted); }
.page-loading i { width: 16px; height: 16px; border: 2px solid var(--border-color); border-top-color: var(--accent-primary); border-radius: 50%; animation: spin 0.7s linear infinite; }

.panel, .rooms-section { margin-top: 14px; border: 1px solid var(--border-color); border-radius: var(--border-radius); background: var(--bg-secondary); }
.panel, .rooms-section { padding: 14px; }
.section-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 11px; }
.section-heading.compact { margin-bottom: 9px; }
.section-heading h2 { font-size: var(--font-size-base); font-weight: 600; }
.section-heading p { margin-top: 2px; color: var(--text-muted); font-size: var(--font-size-xs); }
.result-count { flex: 0 0 auto; color: var(--text-secondary); font-size: var(--font-size-xs); }

.search-row, .stacked-search { display: flex; align-items: center; gap: 8px; margin-bottom: 11px; }
.stacked-search { align-items: stretch; flex-direction: column; }
.search-box { display: flex; min-width: 160px; flex: 1; align-items: center; gap: 7px; height: 32px; padding: 0 7px 0 9px; border: 1px solid var(--border-color); border-radius: 6px; background: var(--bg-card); }
.search-box:focus-within { border-color: var(--accent-primary); }
.search-box > svg { width: 15px; height: 15px; flex: 0 0 auto; fill: none; stroke: var(--text-muted); stroke-width: 1.7; stroke-linecap: round; }
.search-box input { width: 100%; border: 0; outline: 0; background: transparent; color: var(--text-primary); font-size: var(--font-size-sm); }
.search-box button, .room-filter-wrap button { display: grid; width: 20px; height: 20px; flex: 0 0 20px; place-items: center; border: 0; border-radius: 4px; background: transparent; color: var(--text-muted); cursor: pointer; }
.search-box button:hover, .room-filter-wrap button:hover { background: var(--bg-hover); color: var(--text-primary); }
.search-box input::placeholder, .room-filter::placeholder { color: var(--text-muted); }
.search-box input::-webkit-search-cancel-button, .room-filter::-webkit-search-cancel-button { display: none; }
.type-tabs { display: flex; align-items: center; gap: 2px; padding: 2px; border-radius: 6px; background: var(--bg-card); }
.type-tabs button { padding: 5px 9px; border: 0; border-radius: 4px; background: transparent; color: var(--text-secondary); cursor: pointer; font-size: var(--font-size-xs); }
.type-tabs button:hover { color: var(--text-primary); }
.type-tabs button.active { background: var(--accent-primary); color: white; }

.rooms-actions { display: flex; align-items: center; justify-content: flex-end; gap: 7px; }
.prune-button { padding: 6px 9px; border: 1px solid var(--border-color); border-radius: var(--border-radius-sm); background: var(--bg-card); color: var(--text-secondary); cursor: pointer; font-size: var(--font-size-xs); white-space: nowrap; }
.prune-button:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
.sort-select { display: flex; align-items: center; gap: 4px; color: var(--text-muted); font-size: var(--font-size-xs); }
.sort-select select { height: 29px; padding: 0 6px; border: 1px solid var(--border-color); border-radius: 5px; outline: 0; background: var(--bg-card); color: var(--text-primary); font-size: var(--font-size-xs); }
.room-filter-wrap { display: flex; width: 205px; height: 30px; align-items: center; padding-right: 4px; border: 1px solid var(--border-color); border-radius: 5px; background: var(--bg-card); }
.room-filter-wrap:focus-within { border-color: var(--accent-primary); }
.room-filter { width: 100%; min-width: 0; padding: 0 8px; border: 0; outline: 0; background: transparent; color: var(--text-primary); font-size: var(--font-size-xs); }
.room-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; transition: opacity 0.15s; }
.room-grid.refreshing, .session-list.refreshing { opacity: 0.58; pointer-events: none; }
.room-card { display: flex; min-width: 0; align-items: center; gap: 11px; padding: 12px; border: 1px solid var(--border-color); border-radius: var(--border-radius); background: var(--bg-card); cursor: pointer; text-align: left; transition: border-color 0.15s, background 0.15s, transform 0.15s; }
.room-card:hover { transform: translateY(-1px); border-color: var(--accent-primary); background: var(--bg-hover); }
.room-avatar { display: grid; width: 38px; height: 38px; flex: 0 0 38px; place-items: center; border: 1px solid rgba(92, 158, 255, 0.22); border-radius: var(--border-radius); background: rgba(92, 158, 255, 0.1); color: var(--accent-primary); font-weight: 600; }
.room-avatar.large { width: 48px; height: 48px; flex-basis: 48px; font-size: 18px; }
.room-info { min-width: 0; flex: 1; }
.room-title-line { display: flex; align-items: baseline; gap: 7px; }
.room-title-line strong { overflow: hidden; font-size: var(--font-size-sm); text-overflow: ellipsis; white-space: nowrap; }
.room-title-line span, .room-info > p { color: var(--text-muted); font-size: var(--font-size-xs); }
.room-info > p { margin-top: 3px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.room-metrics { display: flex; gap: 10px; margin-top: 7px; color: var(--text-secondary); font-size: 10px; }
.room-metrics b { color: var(--text-primary); font-weight: 500; }
.room-arrow { color: var(--text-muted); font-size: 22px; }

.room-hero, .session-hero { display: flex; align-items: center; gap: 12px; margin-bottom: 12px; }
.room-hero h1, .session-hero h1 { font-size: 19px; }
.room-hero p, .session-hero p { margin-top: 3px; color: var(--text-muted); font-size: var(--font-size-xs); }
.room-content-grid { display: grid; grid-template-columns: minmax(280px, 0.8fr) minmax(360px, 1.2fr); gap: 12px; align-items: start; }
.session-panel, .activity-panel { min-width: 0; }
.session-list { display: grid; gap: 6px; transition: opacity 0.15s; }
.session-card { position: relative; display: flex; min-width: 0; align-items: stretch; border: 1px solid transparent; border-radius: 7px; background: var(--bg-card); transition: border-color 0.15s, background 0.15s; }
.session-card:hover, .session-card:focus-within { border-color: var(--border-color); background: var(--bg-hover); }
.session-open { display: flex; min-width: 0; flex: 1; align-items: center; gap: 10px; padding: 9px 35px 9px 9px; border: 0; border-radius: inherit; background: transparent; cursor: pointer; text-align: left; }
.session-date { width: 58px; flex: 0 0 58px; padding-right: 9px; border-right: 1px solid var(--border-color); text-align: center; }
.session-date strong { display: block; font-size: var(--font-size-sm); }
.session-date span { color: var(--text-muted); font-size: 10px; }
.session-info { min-width: 0; flex: 1; }
.session-title-line { display: flex; min-width: 0; align-items: center; gap: 6px; }
.session-title-line > strong { overflow: hidden; font-size: var(--font-size-sm); text-overflow: ellipsis; white-space: nowrap; }
.live-badge { flex: 0 0 auto; padding: 1px 5px; border-radius: 8px; background: rgba(53, 190, 110, 0.15); color: #65d795; font-size: 9px; }
.session-info p { margin: 2px 0 5px; color: var(--text-muted); font-size: 10px; }
.session-info > div:last-child { display: flex; gap: 8px; color: var(--text-secondary); font-size: 10px; }
.session-info > div:last-child b { color: var(--accent-gold); }
.delete-button { position: absolute; z-index: 1; top: 7px; right: 7px; display: grid; width: 23px; height: 23px; padding: 4px; place-items: center; border: 0; border-radius: 4px; background: transparent; color: var(--text-muted); cursor: pointer; opacity: 0; }
.delete-button svg { width: 14px; height: 14px; fill: none; stroke: currentColor; stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round; }
.session-card:hover .delete-button, .delete-button:focus-visible { opacity: 1; }
.delete-button:hover { background: rgba(220, 60, 60, 0.2); color: #ef7373; }
.session-skeleton { display: grid; gap: 6px; }
.session-skeleton i { height: 68px; border-radius: 7px; background: var(--bg-card); animation: skeletonPulse 1.3s ease-in-out infinite; }
.pagination { display: flex; align-items: center; justify-content: center; gap: 10px; padding-top: 11px; color: var(--text-secondary); font-size: var(--font-size-xs); }
.empty-panel { display: flex; min-height: 118px; align-items: center; justify-content: center; flex-direction: column; gap: 5px; color: var(--text-muted); text-align: center; }
.empty-panel.small { min-height: 100px; }
.empty-panel strong { color: var(--text-secondary); font-size: var(--font-size-sm); font-weight: 500; }
.empty-panel span { font-size: var(--font-size-xs); }
.empty-panel button { margin-top: 4px; padding: 5px 9px; border: 1px solid var(--border-color); border-radius: 4px; background: var(--bg-card); color: var(--accent-primary); cursor: pointer; }

.session-hero { align-items: flex-end; justify-content: space-between; padding: 6px 2px; }
.eyebrow { color: var(--accent-primary); font-size: var(--font-size-xs); }
.session-summary { display: flex; align-items: stretch; gap: 6px; }
.session-summary span { display: flex; min-width: 72px; flex-direction: column; padding: 7px 9px; border-left: 1px solid var(--border-color); color: var(--text-muted); font-size: 10px; }
.session-summary b { color: var(--text-primary); font-size: var(--font-size-sm); }
.session-summary b.gold { color: var(--accent-gold); }
.session-events { margin-top: 4px; }

.notice-enter-active, .notice-leave-active { transition: opacity 0.15s, transform 0.15s; }
.notice-enter-from, .notice-leave-to { opacity: 0; transform: translate(-50%, -5px); }
@keyframes spin { to { transform: rotate(360deg); } }
@keyframes progressMove { from { transform: translateX(-110%); } to { transform: translateX(360%); } }
@keyframes skeletonPulse { 0%, 100% { opacity: 0.45; } 50% { opacity: 0.9; } }

@media (max-width: 820px) {
  .page-toolbar { align-items: flex-start; flex-direction: column; }
  .filter-toolbar { width: 100%; justify-content: flex-start; flex-wrap: wrap; }
  .top-progress { top: 72px; }
  .room-content-grid { grid-template-columns: 1fr; }
  .rooms-heading { align-items: flex-start; }
  .rooms-actions { align-items: flex-end; flex-wrap: wrap; }
}

@media (max-width: 680px) {
  .room-grid { grid-template-columns: 1fr; }
  .session-hero { align-items: flex-start; flex-direction: column; }
  .session-summary { width: 100%; flex-wrap: wrap; }
  .session-summary span { flex: 1; }
  .delete-button { opacity: 1; }
}

@media (max-width: 590px) {
  .archive-body { padding-right: 10px; padding-left: 10px; }
  .page-toolbar { margin-right: -10px; margin-left: -10px; padding-right: 10px; padding-left: 10px; }
  .top-progress { margin-right: -10px; margin-left: -10px; }
  .quick-ranges { order: 1; }
  .refresh-button { order: 1; margin-left: auto; }
  .date-filter { order: 2; width: 100%; flex-wrap: wrap; }
  .date-filter label { min-width: 0; flex: 1; }
  .date-filter input { width: auto; min-width: 0; flex: 1; }
  .search-row { align-items: stretch; flex-direction: column; }
  .type-tabs { align-self: flex-start; }
  .section-heading { align-items: flex-start; }
  .rooms-heading { flex-direction: column; }
  .rooms-actions { width: 100%; align-items: center; justify-content: flex-start; }
  .room-filter-wrap { min-width: 150px; flex: 1; }
  .sort-select > span { display: none; }
}
</style>

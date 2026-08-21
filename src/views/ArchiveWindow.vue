<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import TitleBar from '@/components/common/TitleBar.vue'
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
const confirmDelete = ref<number | null>(null)
const confirmPrune = ref(false)
const pruneNotice = ref('')
let activityTimer: ReturnType<typeof setTimeout> | undefined
let roomTimer: ReturnType<typeof setTimeout> | undefined
let deleteTimer: ReturnType<typeof setTimeout> | undefined
let pruneTimer: ReturnType<typeof setTimeout> | undefined
let pruneNoticeTimer: ReturnType<typeof setTimeout> | undefined

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

const formatDuration = (session: ArchiveSession) => {
  if (!session.end_time) return '进行中'
  const minutes = Math.max(0, Math.floor((session.end_time - session.start_time) / 60))
  return minutes >= 60 ? `${Math.floor(minutes / 60)}小时 ${minutes % 60}分` : `${minutes}分钟`
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

const applyDateRange = async () => {
  const fromTime = toTimestamp(startDate.value)
  const toTime = toTimestamp(endDate.value, true)
  if (fromTime !== undefined && toTime !== undefined && fromTime >= toTime) {
    dateError.value = '开始日期不能晚于结束日期'
    return
  }
  dateError.value = ''
  await archiveStore.applyDateFilter({ fromTime, toTime })
}

const setRecentDays = async (days?: number) => {
  if (!days) {
    startDate.value = ''
    endDate.value = ''
  } else {
    const end = new Date()
    const start = new Date()
    start.setDate(start.getDate() - days + 1)
    startDate.value = localDateValue(start)
    endDate.value = localDateValue(end)
  }
  await applyDateRange()
}

const onActivityInput = () => {
  if (activityTimer) clearTimeout(activityTimer)
  activityTimer = setTimeout(() => archiveStore.runSearch(activityInput.value, 1), 300)
}

const onRoomFilterInput = () => {
  if (roomTimer) clearTimeout(roomTimer)
  roomTimer = setTimeout(() => archiveStore.loadOverview(roomFilterInput.value), 250)
}

const openRoom = async (room: Parameters<typeof archiveStore.openRoom>[0]) => {
  activityInput.value = ''
  await archiveStore.openRoom(room)
}

const openSession = async (session: ArchiveSession) => {
  activityInput.value = ''
  await archiveStore.openSession(session)
}

const goOverview = async () => {
  activityInput.value = ''
  roomFilterInput.value = ''
  await archiveStore.goOverview()
}

const goRoom = async () => {
  activityInput.value = ''
  await archiveStore.goRoom()
}

const handleDelete = async (sessionId: number) => {
  if (confirmDelete.value !== sessionId) {
    confirmDelete.value = sessionId
    if (deleteTimer) clearTimeout(deleteTimer)
    deleteTimer = setTimeout(() => { confirmDelete.value = null }, 3000)
    return
  }
  await archiveStore.removeSession(sessionId)
  confirmDelete.value = null
}

const handlePrune = async () => {
  if (!confirmPrune.value) {
    confirmPrune.value = true
    pruneNotice.value = ''
    if (pruneTimer) clearTimeout(pruneTimer)
    pruneTimer = setTimeout(() => { confirmPrune.value = false }, 3000)
    return
  }
  const deleted = await archiveStore.pruneEmptySessions()
  confirmPrune.value = false
  pruneNotice.value = deleted > 0 ? `已清理 ${deleted} 个空场次` : '没有需要清理的空场次'
  if (pruneNoticeTimer) clearTimeout(pruneNoticeTimer)
  pruneNoticeTimer = setTimeout(() => { pruneNotice.value = '' }, 4000)
}

onMounted(async () => {
  await initWindowManager('archive')
  await archiveStore.loadOverview()
})

onUnmounted(async () => {
  if (activityTimer) clearTimeout(activityTimer)
  if (roomTimer) clearTimeout(roomTimer)
  if (deleteTimer) clearTimeout(deleteTimer)
  if (pruneTimer) clearTimeout(pruneTimer)
  if (pruneNoticeTimer) clearTimeout(pruneNoticeTimer)
  await cleanupWindowManager('archive')
})
</script>

<template>
  <div class="archive-window">
    <TitleBar title="数据存档" :is-sub-window="true" window-label="archive" />

    <main class="archive-body">
      <header class="page-toolbar">
        <div class="breadcrumb">
          <button v-if="archiveStore.view !== 'overview'" @click="goOverview">总览</button>
          <span v-if="archiveStore.view !== 'overview'">/</span>
          <button v-if="archiveStore.view === 'session'" @click="goRoom">
            {{ archiveStore.selectedRoom?.room_title || '直播间' }}
          </button>
          <span v-if="archiveStore.view === 'session'">/</span>
          <strong>{{ pageTitle }}</strong>
        </div>

        <div class="date-filter">
          <div class="quick-ranges">
            <button @click="setRecentDays(7)">近 7 天</button>
            <button @click="setRecentDays(30)">近 30 天</button>
            <button @click="setRecentDays()">全部</button>
          </div>
          <label><span>从</span><input v-model="startDate" type="date" @change="applyDateRange" /></label>
          <label><span>至</span><input v-model="endDate" type="date" @change="applyDateRange" /></label>
        </div>
      </header>

      <div v-if="dateError || archiveStore.error" class="error-banner">
        {{ dateError || archiveStore.error }}
      </div>

      <div v-if="archiveStore.loadingPage && archiveStore.overview.summary.session_count === 0" class="page-loading">
        正在整理归档数据…
      </div>

      <template v-else-if="archiveStore.view === 'overview'">
        <ArchiveStatsPanel :summary="archiveStore.overview.summary" :daily="archiveStore.statistics.daily" />

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
              <span>⌕</span>
              <input
                v-model="activityInput"
                type="search"
                placeholder="输入弹幕内容、用户名或 UID…"
                @input="onActivityInput"
                @keyup.enter="archiveStore.runSearch(activityInput, 1)"
              />
            </div>
            <div class="type-tabs">
              <button
                v-for="type in contentTypes"
                :key="type.value"
                :class="{ active: archiveStore.contentType === type.value }"
                @click="archiveStore.setContentType(type.value)"
              >{{ type.label }}</button>
            </div>
          </div>
          <ArchiveSearchResults
            v-if="archiveStore.searchQuery"
            :result="archiveStore.searchResult"
            :loading="archiveStore.loadingSearch"
            show-room
            @page="page => archiveStore.runSearch(activityInput, page)"
          />
        </section>

        <section class="rooms-section">
          <div class="section-heading">
            <div>
              <h2>直播间</h2>
              <p>{{ archiveStore.overview.summary.room_count }} 个房间，按最近直播排序</p>
            </div>
            <div class="rooms-actions">
              <span v-if="pruneNotice" class="prune-notice">{{ pruneNotice }}</span>
              <button
                class="prune-button"
                :class="{ confirming: confirmPrune }"
                :title="confirmPrune ? '再次点击确认清理' : '删除没有弹幕、礼物或醒目留言的历史场次'"
                @click="handlePrune"
              >{{ confirmPrune ? '确认清理' : '清理空场次' }}</button>
              <input
                v-model="roomFilterInput"
                class="room-filter"
                type="search"
                placeholder="筛选房间标题 / ID"
                @input="onRoomFilterInput"
              />
            </div>
          </div>

          <div v-if="archiveStore.overview.rooms.length" class="room-grid">
            <button
              v-for="room in archiveStore.overview.rooms"
              :key="room.room_id"
              class="room-card"
              @click="openRoom(room)"
            >
              <div class="room-avatar">{{ (room.room_title || '房').slice(0, 1) }}</div>
              <div class="room-info">
                <div class="room-title-line">
                  <strong>{{ room.room_title }}</strong>
                  <span>#{{ room.room_id }}</span>
                </div>
                <p>最近直播 {{ formatShortDate(room.last_live_time) }}</p>
                <div class="room-metrics">
                  <span><b>{{ room.session_count }}</b> 场</span>
                  <span><b>{{ room.danmaku_count }}</b> 弹幕</span>
                  <span><b>{{ formatPrice(room.total_revenue) || '¥0' }}</b> 收益</span>
                </div>
              </div>
              <span class="room-arrow">›</span>
            </button>
          </div>
          <div v-else class="empty-panel">所选条件下没有直播间归档</div>
        </section>
      </template>

      <template v-else-if="archiveStore.view === 'room' && archiveStore.selectedRoom">
        <section class="room-hero">
          <div class="room-avatar large">{{ archiveStore.selectedRoom.room_title.slice(0, 1) }}</div>
          <div>
            <h1>{{ archiveStore.selectedRoom.room_title }}</h1>
            <p>房间 {{ archiveStore.selectedRoom.room_id }} · 主播 UID {{ archiveStore.selectedRoom.streamer_uid }}</p>
          </div>
        </section>

        <ArchiveStatsPanel :summary="archiveStore.statistics.summary" :daily="archiveStore.statistics.daily" />

        <div class="room-content-grid">
          <section class="panel session-panel">
            <div class="section-heading compact">
              <div>
                <h2>直播场次</h2>
                <p>共 {{ archiveStore.roomSessions.total }} 场</p>
              </div>
            </div>
            <div v-if="archiveStore.roomSessions.items.length" class="session-list">
              <article
                v-for="session in archiveStore.roomSessions.items"
                :key="session.id"
                class="session-card"
                @click="openSession(session)"
              >
                <div class="session-date">
                  <strong>{{ formatShortDate(session.start_time).slice(5) }}</strong>
                  <span>{{ new Date(session.start_time * 1000).getFullYear() }}</span>
                </div>
                <div class="session-info">
                  <strong>{{ session.room_title || archiveStore.selectedRoom.room_title }}</strong>
                  <p>{{ formatDateTime(session.start_time) }} · {{ formatDuration(session) }}</p>
                  <div><span>{{ session.danmaku_count }} 弹幕</span><span>{{ session.gift_count }} 礼物</span><b>{{ formatPrice(session.total_revenue) || '¥0' }}</b></div>
                </div>
                <button
                  class="delete-button"
                  :class="{ confirming: confirmDelete === session.id }"
                  :title="confirmDelete === session.id ? '再次点击确认删除' : '删除本场归档'"
                  @click.stop="handleDelete(session.id)"
                >{{ confirmDelete === session.id ? '确认' : '×' }}</button>
              </article>
            </div>
            <div v-else class="empty-panel">所选时间内没有直播场次</div>
            <div v-if="archiveStore.roomPages > 1" class="pagination">
              <button :disabled="archiveStore.roomSessions.page <= 1" @click="archiveStore.loadRoomSessions(archiveStore.roomSessions.page - 1)">上一页</button>
              <span>{{ archiveStore.roomSessions.page }} / {{ archiveStore.roomPages }}</span>
              <button :disabled="archiveStore.roomSessions.page >= archiveStore.roomPages" @click="archiveStore.loadRoomSessions(archiveStore.roomSessions.page + 1)">下一页</button>
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
                <span>⌕</span>
                <input v-model="activityInput" type="search" placeholder="内容、用户名或 UID" @input="onActivityInput" />
              </div>
              <div class="type-tabs">
                <button v-for="type in contentTypes" :key="type.value" :class="{ active: archiveStore.contentType === type.value }" @click="archiveStore.setContentType(type.value)">{{ type.label }}</button>
              </div>
            </div>
            <ArchiveSearchResults
              :result="archiveStore.searchResult"
              :loading="archiveStore.loadingSearch"
              @page="page => archiveStore.runSearch(activityInput, page)"
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
              <p>搜索本场弹幕内容、用户名或 UID</p>
            </div>
            <span class="result-count">{{ archiveStore.searchResult.total }} 条</span>
          </div>
          <div class="search-row">
            <div class="search-box">
              <span>⌕</span>
              <input v-model="activityInput" type="search" placeholder="搜索本场记录…" @input="onActivityInput" />
            </div>
            <div class="type-tabs">
              <button v-for="type in contentTypes" :key="type.value" :class="{ active: archiveStore.contentType === type.value }" @click="archiveStore.setContentType(type.value)">{{ type.label }}</button>
            </div>
          </div>
          <ArchiveSearchResults
            :result="archiveStore.searchResult"
            :loading="archiveStore.loadingSearch"
            @page="page => archiveStore.runSearch(activityInput, page)"
          />
        </section>
      </template>
    </main>
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

.archive-body { flex: 1; overflow-y: auto; padding: 14px 16px 22px; }

button, input { font: inherit; }
button { color: inherit; }

.page-toolbar {
  position: sticky;
  z-index: 5;
  top: -14px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin: -14px -16px 14px;
  padding: 11px 16px;
  border-bottom: 1px solid var(--border-color);
  background: rgb(25, 25, 25);
}

.breadcrumb { display: flex; align-items: center; gap: 7px; min-width: 0; font-size: var(--font-size-sm); }
.breadcrumb button { border: 0; background: transparent; color: var(--accent-primary); cursor: pointer; }
.breadcrumb strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.breadcrumb span { color: var(--text-muted); }

.date-filter, .quick-ranges, .date-filter label { display: flex; align-items: center; gap: 5px; }
.quick-ranges { margin-right: 4px; }
.quick-ranges button, .pagination button {
  padding: 5px 8px;
  border: 1px solid var(--border-color);
  border-radius: 4px;
  background: var(--bg-card);
  cursor: pointer;
}
.quick-ranges button:hover, .pagination button:hover:not(:disabled) { background: var(--bg-hover); }
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

.error-banner { margin-bottom: 12px; padding: 8px 10px; border: 1px solid rgba(220, 60, 60, 0.4); border-radius: 6px; background: rgba(220, 60, 60, 0.12); color: #ef8a8a; font-size: var(--font-size-sm); }
.page-loading { display: grid; height: 240px; place-items: center; color: var(--text-muted); }

.panel, .rooms-section { margin-top: 14px; border: 1px solid var(--border-color); border-radius: var(--border-radius); background: var(--bg-secondary); }
.panel { padding: 14px; }
.rooms-section { padding: 14px; }
.section-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 11px; }
.section-heading.compact { margin-bottom: 9px; }
.section-heading h2 { font-size: var(--font-size-base); font-weight: 600; }
.section-heading p { margin-top: 2px; color: var(--text-muted); font-size: var(--font-size-xs); }
.result-count { flex: 0 0 auto; color: var(--text-secondary); font-size: var(--font-size-xs); }
.rooms-actions { display: flex; align-items: center; justify-content: flex-end; gap: 7px; }
.prune-notice { color: var(--text-secondary); font-size: var(--font-size-xs); white-space: nowrap; }
.prune-button { padding: 6px 9px; border: 1px solid var(--border-color); border-radius: var(--border-radius-sm); background: var(--bg-card); color: var(--text-secondary); cursor: pointer; font-size: var(--font-size-xs); white-space: nowrap; }
.prune-button:hover { background: var(--bg-hover); color: var(--text-primary); }
.prune-button.confirming { border-color: rgba(220, 60, 60, 0.5); background: rgba(220, 60, 60, 0.14); color: #ef7373; }

.search-row, .stacked-search { display: flex; align-items: center; gap: 8px; margin-bottom: 11px; }
.stacked-search { align-items: stretch; flex-direction: column; }
.search-box { display: flex; min-width: 160px; flex: 1; align-items: center; gap: 7px; height: 32px; padding: 0 9px; border: 1px solid var(--border-color); border-radius: 6px; background: var(--bg-card); }
.search-box:focus-within { border-color: var(--accent-primary); }
.search-box > span { color: var(--text-muted); font-size: 17px; }
.search-box input { width: 100%; border: 0; outline: 0; background: transparent; color: var(--text-primary); font-size: var(--font-size-sm); }
.search-box input::placeholder, .room-filter::placeholder { color: var(--text-muted); }

.type-tabs { display: flex; align-items: center; gap: 2px; padding: 2px; border-radius: 6px; background: var(--bg-card); }
.type-tabs button { padding: 5px 9px; border: 0; border-radius: 4px; background: transparent; color: var(--text-secondary); cursor: pointer; font-size: var(--font-size-xs); }
.type-tabs button:hover { color: var(--text-primary); }
.type-tabs button.active { background: var(--accent-primary); color: white; }

.room-filter { width: 190px; padding: 6px 8px; border: 1px solid var(--border-color); border-radius: 5px; outline: 0; background: var(--bg-card); color: var(--text-primary); font-size: var(--font-size-xs); }
.room-filter:focus { border-color: var(--accent-primary); }
.room-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
.room-card { display: flex; align-items: center; gap: 11px; min-width: 0; padding: 12px; border: 1px solid var(--border-color); border-radius: var(--border-radius); background: var(--bg-card); cursor: pointer; text-align: left; transition: border-color 0.15s, background 0.15s; }
.room-card:hover { border-color: var(--accent-primary); background: var(--bg-hover); }
.room-avatar { display: grid; width: 38px; height: 38px; flex: 0 0 38px; place-items: center; border: 1px solid var(--border-color); border-radius: var(--border-radius); background: var(--bg-active); color: var(--accent-primary); font-weight: 600; }
.room-avatar.large { width: 48px; height: 48px; flex-basis: 48px; font-size: 18px; }
.room-info { min-width: 0; flex: 1; }
.room-title-line { display: flex; align-items: baseline; gap: 7px; }
.room-title-line strong { overflow: hidden; font-size: var(--font-size-sm); text-overflow: ellipsis; white-space: nowrap; }
.room-title-line span, .room-info > p { color: var(--text-muted); font-size: var(--font-size-xs); }
.room-info > p { margin-top: 3px; }
.room-metrics { display: flex; gap: 10px; margin-top: 7px; color: var(--text-secondary); font-size: 10px; }
.room-metrics b { color: var(--text-primary); font-weight: 500; }
.room-arrow { color: var(--text-muted); font-size: 22px; }

.room-hero, .session-hero { display: flex; align-items: center; gap: 12px; margin-bottom: 12px; }
.room-hero h1, .session-hero h1 { font-size: 19px; }
.room-hero p, .session-hero p { margin-top: 3px; color: var(--text-muted); font-size: var(--font-size-xs); }
.room-content-grid { display: grid; grid-template-columns: minmax(280px, 0.8fr) minmax(360px, 1.2fr); gap: 12px; align-items: start; }
.session-panel, .activity-panel { min-width: 0; }
.session-list { display: grid; gap: 6px; }
.session-card { position: relative; display: flex; align-items: center; gap: 10px; min-width: 0; padding: 9px; border: 1px solid transparent; border-radius: 7px; background: var(--bg-card); cursor: pointer; }
.session-card:hover { border-color: var(--border-color); background: var(--bg-hover); }
.session-date { width: 58px; flex: 0 0 58px; padding-right: 9px; border-right: 1px solid var(--border-color); text-align: center; }
.session-date strong { display: block; font-size: var(--font-size-sm); }
.session-date span { color: var(--text-muted); font-size: 10px; }
.session-info { min-width: 0; flex: 1; padding-right: 22px; }
.session-info > strong { display: block; overflow: hidden; font-size: var(--font-size-sm); text-overflow: ellipsis; white-space: nowrap; }
.session-info p { margin: 2px 0 5px; color: var(--text-muted); font-size: 10px; }
.session-info div { display: flex; gap: 8px; color: var(--text-secondary); font-size: 10px; }
.session-info div b { color: var(--accent-gold); }
.delete-button { position: absolute; top: 7px; right: 7px; min-width: 20px; height: 20px; padding: 0 4px; border: 0; border-radius: 4px; background: transparent; color: var(--text-muted); cursor: pointer; opacity: 0; }
.session-card:hover .delete-button, .delete-button.confirming { opacity: 1; }
.delete-button:hover, .delete-button.confirming { background: rgba(220, 60, 60, 0.2); color: #ef7373; }

.pagination { display: flex; align-items: center; justify-content: center; gap: 10px; padding-top: 11px; color: var(--text-secondary); font-size: var(--font-size-xs); }
.pagination button:disabled { opacity: 0.35; cursor: default; }
.empty-panel { display: grid; min-height: 100px; place-items: center; color: var(--text-muted); font-size: var(--font-size-sm); }

.session-hero { align-items: flex-end; justify-content: space-between; padding: 6px 2px; }
.eyebrow { color: var(--accent-primary); font-size: var(--font-size-xs); }
.session-summary { display: flex; align-items: stretch; gap: 6px; }
.session-summary span { display: flex; min-width: 72px; flex-direction: column; padding: 7px 9px; border-left: 1px solid var(--border-color); color: var(--text-muted); font-size: 10px; }
.session-summary b { color: var(--text-primary); font-size: var(--font-size-sm); }
.session-summary b.gold { color: var(--accent-gold); }
.session-events { margin-top: 4px; }

@media (max-width: 760px) {
  .page-toolbar { align-items: flex-start; flex-direction: column; }
  .date-filter { width: 100%; flex-wrap: wrap; }
  .room-grid { grid-template-columns: 1fr; }
  .room-content-grid { grid-template-columns: 1fr; }
  .session-hero { align-items: flex-start; flex-direction: column; }
  .session-summary { width: 100%; flex-wrap: wrap; }
  .session-summary span { flex: 1; }
}

@media (max-width: 590px) {
  .archive-body { padding-right: 10px; padding-left: 10px; }
  .page-toolbar { margin-right: -10px; margin-left: -10px; padding-right: 10px; padding-left: 10px; }
  .quick-ranges { order: 3; }
  .search-row { align-items: stretch; flex-direction: column; }
  .type-tabs { align-self: flex-start; }
  .section-heading { align-items: flex-start; }
  .rooms-actions { align-items: flex-end; flex-direction: column-reverse; }
  .prune-notice { display: none; }
  .room-filter { width: 150px; }
}
</style>

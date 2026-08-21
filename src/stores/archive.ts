/**
 * 存档页面状态：总览 → 直播间 → 场次。
 */

import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import type {
  ArchiveContentType,
  ArchiveOverview,
  ArchiveRoomSummary,
  ArchiveSearchItem,
  ArchiveSession,
  ArchiveStatistics,
  ArchiveSummary,
  PagedResult,
} from '@/types'
import {
  deleteArchiveSession,
  getArchiveOverview,
  getArchiveRoomSessions,
  getArchiveStatistics,
  pruneEmptyArchiveSessions,
  searchArchive,
  type ArchiveDateFilter,
} from '@/services/archive'
import { createLogger } from '@/services/logger'

const logger = createLogger('ArchiveStore')

type ArchiveView = 'overview' | 'room' | 'session'

const emptySummary = (): ArchiveSummary => ({
  room_count: 0,
  session_count: 0,
  live_duration: 0,
  total_revenue: 0,
  gift_revenue: 0,
  sc_revenue: 0,
  guard_revenue: 0,
  danmaku_count: 0,
  gift_count: 0,
  sc_count: 0,
})

const emptySearch = (page = 1, pageSize = 40): PagedResult<ArchiveSearchItem> => ({
  items: [],
  total: 0,
  page,
  page_size: pageSize,
})

export const useArchiveStore = defineStore('archive', () => {
  const view = ref<ArchiveView>('overview')
  const overview = ref<ArchiveOverview>({ summary: emptySummary(), rooms: [] })
  const statistics = ref<ArchiveStatistics>({ summary: emptySummary(), daily: [] })
  const selectedRoom = ref<ArchiveRoomSummary | null>(null)
  const selectedSession = ref<ArchiveSession | null>(null)
  const roomSessions = ref<PagedResult<ArchiveSession>>({
    items: [],
    total: 0,
    page: 1,
    page_size: 20,
  })
  const searchResult = ref<PagedResult<ArchiveSearchItem>>(emptySearch())

  const fromTime = ref<number | undefined>()
  const toTime = ref<number | undefined>()
  const roomQuery = ref('')
  const searchQuery = ref('')
  const contentType = ref<ArchiveContentType>('all')
  const loadingPage = ref(false)
  const loadingSearch = ref(false)
  const error = ref('')
  let searchGeneration = 0

  const dateFilter = computed<ArchiveDateFilter>(() => ({
    fromTime: fromTime.value,
    toTime: toTime.value,
  }))
  const currentSummary = computed(() =>
    view.value === 'overview' ? overview.value.summary : statistics.value.summary
  )
  const searchPages = computed(() =>
    Math.max(1, Math.ceil(searchResult.value.total / searchResult.value.page_size))
  )
  const roomPages = computed(() =>
    Math.max(1, Math.ceil(roomSessions.value.total / roomSessions.value.page_size))
  )

  function captureError(context: string, cause: unknown) {
    error.value = cause instanceof Error ? cause.message : String(cause)
    logger.error(context, cause)
  }

  async function loadOverview(query = roomQuery.value) {
    roomQuery.value = query
    loadingPage.value = true
    error.value = ''
    try {
      const [nextOverview, nextStatistics] = await Promise.all([
        getArchiveOverview(dateFilter.value, query),
        getArchiveStatistics(undefined, dateFilter.value),
      ])
      overview.value = nextOverview
      statistics.value = nextStatistics
    } catch (cause) {
      captureError('加载存档总览失败', cause)
    } finally {
      loadingPage.value = false
    }
  }

  async function loadRoomSessions(page = 1) {
    if (!selectedRoom.value) return
    try {
      roomSessions.value = await getArchiveRoomSessions(
        selectedRoom.value.room_id,
        dateFilter.value,
        page,
        roomSessions.value.page_size
      )
    } catch (cause) {
      captureError('加载直播场次失败', cause)
    }
  }

  async function openRoom(room: ArchiveRoomSummary) {
    view.value = 'room'
    selectedRoom.value = room
    selectedSession.value = null
    searchQuery.value = ''
    contentType.value = 'all'
    searchResult.value = emptySearch()
    loadingPage.value = true
    error.value = ''
    try {
      const [sessions, nextStatistics] = await Promise.all([
        getArchiveRoomSessions(room.room_id, dateFilter.value, 1, roomSessions.value.page_size),
        getArchiveStatistics(room.room_id, dateFilter.value),
      ])
      roomSessions.value = sessions
      statistics.value = nextStatistics
      await runSearch('', 1)
    } catch (cause) {
      captureError('加载直播间归档失败', cause)
    } finally {
      loadingPage.value = false
    }
  }

  async function openSession(session: ArchiveSession) {
    view.value = 'session'
    selectedSession.value = session
    searchQuery.value = ''
    contentType.value = 'all'
    searchResult.value = emptySearch()
    await runSearch('', 1)
  }

  async function goOverview() {
    view.value = 'overview'
    selectedRoom.value = null
    selectedSession.value = null
    roomQuery.value = ''
    searchQuery.value = ''
    searchResult.value = emptySearch()
    await loadOverview('')
  }

  async function goRoom() {
    if (!selectedRoom.value) {
      await goOverview()
      return
    }
    view.value = 'room'
    selectedSession.value = null
    searchQuery.value = ''
    contentType.value = 'all'
    await runSearch('', 1)
  }

  async function applyDateFilter(filter: ArchiveDateFilter) {
    fromTime.value = filter.fromTime
    toTime.value = filter.toTime
    if (view.value === 'overview') {
      await loadOverview()
      if (searchQuery.value) await runSearch(searchQuery.value, 1)
      return
    }

    const roomId = selectedRoom.value?.room_id
    if (!roomId) return
    loadingPage.value = true
    error.value = ''
    try {
      const jobs: Promise<unknown>[] = [
        getArchiveStatistics(roomId, dateFilter.value).then(value => {
          statistics.value = value
        }),
        runSearch(searchQuery.value, 1),
      ]
      if (view.value === 'room') jobs.push(loadRoomSessions(1))
      await Promise.all(jobs)
    } catch (cause) {
      captureError('应用归档时间筛选失败', cause)
    } finally {
      loadingPage.value = false
    }
  }

  async function runSearch(query = searchQuery.value, page = 1) {
    searchQuery.value = query.trim()
    if (view.value === 'overview' && !searchQuery.value) {
      searchResult.value = emptySearch()
      return
    }

    const generation = ++searchGeneration
    loadingSearch.value = true
    error.value = ''
    try {
      const result = await searchArchive({
        roomId: view.value === 'overview' ? undefined : selectedRoom.value?.room_id,
        sessionId: view.value === 'session' ? selectedSession.value?.id : undefined,
        query: searchQuery.value,
        eventType: contentType.value,
        fromTime: fromTime.value,
        toTime: toTime.value,
        page,
        pageSize: searchResult.value.page_size,
      })
      if (generation === searchGeneration) searchResult.value = result
    } catch (cause) {
      if (generation === searchGeneration) captureError('搜索归档失败', cause)
    } finally {
      if (generation === searchGeneration) loadingSearch.value = false
    }
  }

  async function setContentType(type: ArchiveContentType) {
    contentType.value = type
    await runSearch(searchQuery.value, 1)
  }

  async function removeSession(id: number) {
    try {
      await deleteArchiveSession(id)
      if (selectedSession.value?.id === id) {
        selectedSession.value = null
        view.value = 'room'
      }
      await Promise.all([
        loadRoomSessions(1),
        selectedRoom.value
          ? getArchiveStatistics(selectedRoom.value.room_id, dateFilter.value).then(value => {
              statistics.value = value
            })
          : Promise.resolve(),
        runSearch(searchQuery.value, 1),
      ])
    } catch (cause) {
      captureError('删除归档场次失败', cause)
      throw cause
    }
  }

  async function pruneEmptySessions() {
    error.value = ''
    try {
      const deleted = await pruneEmptyArchiveSessions()
      await loadOverview(roomQuery.value)
      return deleted
    } catch (cause) {
      captureError('清理空归档场次失败', cause)
      throw cause
    }
  }

  return {
    view,
    overview,
    statistics,
    selectedRoom,
    selectedSession,
    roomSessions,
    searchResult,
    roomQuery,
    searchQuery,
    contentType,
    loadingPage,
    loadingSearch,
    error,
    dateFilter,
    currentSummary,
    searchPages,
    roomPages,
    loadOverview,
    loadRoomSessions,
    openRoom,
    openSession,
    goOverview,
    goRoom,
    applyDateFilter,
    runSearch,
    setContentType,
    removeSession,
    pruneEmptySessions,
  }
})

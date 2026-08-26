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

const emptySessions = (page = 1, pageSize = 20): PagedResult<ArchiveSession> => ({
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
  const roomSessions = ref<PagedResult<ArchiveSession>>(emptySessions())
  const searchResult = ref<PagedResult<ArchiveSearchItem>>(emptySearch())

  const fromTime = ref<number | undefined>()
  const toTime = ref<number | undefined>()
  const roomQuery = ref('')
  const searchQuery = ref('')
  const contentType = ref<ArchiveContentType>('all')

  const initialized = ref(false)
  const loadingPage = ref(false)
  const loadingSessions = ref(false)
  const loadingSearch = ref(false)
  const deletingSessionId = ref<number | null>(null)
  const pruning = ref(false)
  const error = ref('')
  const searchError = ref('')

  let pageGeneration = 0
  let sessionsGeneration = 0
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

  function captureError(context: string, cause: unknown, target: 'page' | 'search' = 'page') {
    const message = cause instanceof Error ? cause.message : String(cause)
    if (target === 'search') searchError.value = message
    else error.value = message
    logger.error(context, cause)
  }

  function dismissError() {
    error.value = ''
  }

  function clearSearchError() {
    searchError.value = ''
  }

  function isCurrentRoom(roomId: number) {
    return view.value === 'room' && selectedRoom.value?.room_id === roomId
  }

  function invalidateSearch(clearResult = false) {
    searchGeneration += 1
    loadingSearch.value = false
    searchError.value = ''
    if (clearResult) searchResult.value = emptySearch(1, searchResult.value.page_size)
  }

  async function loadOverview(query = roomQuery.value) {
    if (view.value !== 'overview') return
    roomQuery.value = query.trim()
    const generation = ++pageGeneration
    loadingPage.value = true
    error.value = ''
    try {
      const [nextOverview, nextStatistics] = await Promise.all([
        getArchiveOverview(dateFilter.value, roomQuery.value),
        getArchiveStatistics(undefined, dateFilter.value),
      ])
      if (generation !== pageGeneration || view.value !== 'overview') return
      overview.value = nextOverview
      statistics.value = nextStatistics
      initialized.value = true
    } catch (cause) {
      if (generation === pageGeneration) captureError('加载存档总览失败', cause)
    } finally {
      if (generation === pageGeneration) loadingPage.value = false
    }
  }

  async function refreshRoom(page = roomSessions.value.page) {
    const room = selectedRoom.value
    if (!room) return

    const generation = ++pageGeneration
    sessionsGeneration += 1
    loadingPage.value = true
    loadingSessions.value = true
    error.value = ''
    try {
      const [sessions, nextStatistics] = await Promise.all([
        getArchiveRoomSessions(room.room_id, dateFilter.value, page, roomSessions.value.page_size),
        getArchiveStatistics(room.room_id, dateFilter.value),
      ])
      if (generation !== pageGeneration || !isCurrentRoom(room.room_id)) return
      roomSessions.value = sessions
      statistics.value = nextStatistics
    } catch (cause) {
      if (generation === pageGeneration) captureError('加载直播间归档失败', cause)
    } finally {
      if (generation === pageGeneration) {
        loadingPage.value = false
        loadingSessions.value = false
      }
    }
  }

  async function loadRoomSessions(page = 1) {
    const room = selectedRoom.value
    if (!room) return

    const generation = ++sessionsGeneration
    loadingSessions.value = true
    error.value = ''
    try {
      const sessions = await getArchiveRoomSessions(
        room.room_id,
        dateFilter.value,
        page,
        roomSessions.value.page_size
      )
      if (generation !== sessionsGeneration || !isCurrentRoom(room.room_id)) return
      roomSessions.value = sessions
    } catch (cause) {
      if (generation === sessionsGeneration) captureError('加载直播场次失败', cause)
    } finally {
      if (generation === sessionsGeneration) loadingSessions.value = false
    }
  }

  async function openRoom(room: ArchiveRoomSummary) {
    view.value = 'room'
    selectedRoom.value = room
    selectedSession.value = null
    roomSessions.value = emptySessions(1, roomSessions.value.page_size)
    statistics.value = { summary: emptySummary(), daily: [] }
    searchQuery.value = ''
    contentType.value = 'all'
    invalidateSearch(true)
    await Promise.all([refreshRoom(1), runSearch('', 1)])
  }

  async function openSession(session: ArchiveSession) {
    pageGeneration += 1
    sessionsGeneration += 1
    loadingPage.value = false
    loadingSessions.value = false
    view.value = 'session'
    selectedSession.value = session
    searchQuery.value = ''
    contentType.value = 'all'
    invalidateSearch(true)
    await runSearch('', 1)
  }

  async function goOverview() {
    sessionsGeneration += 1
    loadingSessions.value = false
    view.value = 'overview'
    selectedRoom.value = null
    selectedSession.value = null
    roomQuery.value = ''
    searchQuery.value = ''
    contentType.value = 'all'
    invalidateSearch(true)
    await loadOverview('')
  }

  async function goRoom() {
    if (!selectedRoom.value) {
      await goOverview()
      return
    }
    pageGeneration += 1
    view.value = 'room'
    selectedSession.value = null
    searchQuery.value = ''
    contentType.value = 'all'
    invalidateSearch(true)
    await runSearch('', 1)
  }

  async function applyDateFilter(filter: ArchiveDateFilter) {
    fromTime.value = filter.fromTime
    toTime.value = filter.toTime

    if (view.value === 'overview') {
      await Promise.all([
        loadOverview(roomQuery.value),
        searchQuery.value ? runSearch(searchQuery.value, 1) : invalidateSearch(true),
      ])
      return
    }

    if (view.value === 'room') {
      await Promise.all([refreshRoom(1), runSearch(searchQuery.value, 1)])
      return
    }

    await runSearch(searchQuery.value, 1)
  }

  async function refreshCurrentView() {
    if (view.value === 'overview') {
      await Promise.all([
        loadOverview(roomQuery.value),
        searchQuery.value ? runSearch(searchQuery.value, searchResult.value.page) : undefined,
      ])
      return
    }
    if (view.value === 'room') {
      await Promise.all([
        refreshRoom(roomSessions.value.page),
        runSearch(searchQuery.value, searchResult.value.page),
      ])
      return
    }
    await runSearch(searchQuery.value, searchResult.value.page)
  }

  async function runSearch(query = searchQuery.value, page = 1) {
    searchQuery.value = query.trim()
    const generation = ++searchGeneration
    searchError.value = ''

    if (view.value === 'overview' && !searchQuery.value) {
      loadingSearch.value = false
      searchResult.value = emptySearch(1, searchResult.value.page_size)
      return
    }

    loadingSearch.value = true
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
      if (generation === searchGeneration) captureError('搜索归档失败', cause, 'search')
    } finally {
      if (generation === searchGeneration) loadingSearch.value = false
    }
  }

  async function setContentType(type: ArchiveContentType, query = searchQuery.value) {
    contentType.value = type
    await runSearch(query, 1)
  }

  async function removeSession(id: number) {
    if (deletingSessionId.value !== null) return
    deletingSessionId.value = id
    error.value = ''
    try {
      await deleteArchiveSession(id)
      if (selectedSession.value?.id === id) {
        selectedSession.value = null
        view.value = 'room'
      }

      if (!selectedRoom.value) {
        await loadOverview(roomQuery.value)
        return
      }

      await Promise.all([
        refreshRoom(roomSessions.value.page),
        runSearch(searchQuery.value, 1),
      ])

      if (roomSessions.value.total === 0) {
        await goOverview()
      } else if (roomSessions.value.items.length === 0 && roomSessions.value.page > 1) {
        await loadRoomSessions(roomSessions.value.page - 1)
      }
    } catch (cause) {
      captureError('删除归档场次失败', cause)
      throw cause
    } finally {
      deletingSessionId.value = null
    }
  }

  async function pruneEmptySessions() {
    if (pruning.value) return 0
    pruning.value = true
    error.value = ''
    try {
      const deleted = await pruneEmptyArchiveSessions()
      if (view.value === 'overview') await loadOverview(roomQuery.value)
      return deleted
    } catch (cause) {
      captureError('清理空归档场次失败', cause)
      throw cause
    } finally {
      pruning.value = false
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
    initialized,
    loadingPage,
    loadingSessions,
    loadingSearch,
    deletingSessionId,
    pruning,
    error,
    searchError,
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
    refreshCurrentView,
    runSearch,
    setContentType,
    removeSession,
    pruneEmptySessions,
    dismissError,
    clearSearchError,
  }
})

/**
 * 存档状态管理
 */

import { ref, computed } from 'vue'
import { defineStore } from 'pinia'
import type {
  ArchiveSession,
  ArchiveContentType,
  PagedResult,
  ArchivedDanmaku,
  ArchivedGift,
  ArchivedSuperChat,
} from '@/types'
import {
  getArchiveSessions,
  searchArchiveDanmaku,
  searchArchiveGifts,
  searchArchiveSuperChat,
  deleteArchiveSession,
} from '@/services/archive'

export const useArchiveStore = defineStore('archive', () => {
  const sessions = ref<ArchiveSession[]>([])
  const selectedSessionId = ref<number | null>(null)
  const contentType = ref<ArchiveContentType>('all')
  const searchQuery = ref('')
  const minPrice = ref<number | undefined>()
  const maxPrice = ref<number | undefined>()
  const currentPage = ref(1)
  const pageSize = ref(50)
  const loading = ref(false)

  const danmakuResult = ref<PagedResult<ArchivedDanmaku>>({ items: [], total: 0, page: 1, page_size: 50 })
  const giftResult = ref<PagedResult<ArchivedGift>>({ items: [], total: 0, page: 1, page_size: 50 })
  const scResult = ref<PagedResult<ArchivedSuperChat>>({ items: [], total: 0, page: 1, page_size: 50 })

  const selectedSession = computed(() =>
    sessions.value.find(s => s.id === selectedSessionId.value) ?? null
  )

  async function loadSessions() {
    try {
      sessions.value = await getArchiveSessions()
    } catch (e) {
      console.error('Failed to load archive sessions:', e)
    }
  }

  async function selectSession(id: number) {
    selectedSessionId.value = id
    currentPage.value = 1
    await search()
  }

  async function search() {
    const sid = selectedSessionId.value
    if (sid === null) return

    loading.value = true
    try {
      const q = searchQuery.value
      const p = currentPage.value
      const ps = pageSize.value

      if (contentType.value === 'all' || contentType.value === 'danmaku') {
        danmakuResult.value = await searchArchiveDanmaku(sid, q, p, ps)
      } else {
        danmakuResult.value = { items: [], total: 0, page: p, page_size: ps }
      }

      if (contentType.value === 'all' || contentType.value === 'gift') {
        giftResult.value = await searchArchiveGifts(sid, q, minPrice.value, maxPrice.value, p, ps)
      } else {
        giftResult.value = { items: [], total: 0, page: p, page_size: ps }
      }

      if (contentType.value === 'all' || contentType.value === 'superchat') {
        scResult.value = await searchArchiveSuperChat(sid, q, minPrice.value, maxPrice.value, p, ps)
      } else {
        scResult.value = { items: [], total: 0, page: p, page_size: ps }
      }
    } catch (e) {
      console.error('Archive search failed:', e)
    } finally {
      loading.value = false
    }
  }

  async function removeSession(id: number) {
    try {
      await deleteArchiveSession(id)
      sessions.value = sessions.value.filter(s => s.id !== id)
      if (selectedSessionId.value === id) {
        selectedSessionId.value = null
        danmakuResult.value = { items: [], total: 0, page: 1, page_size: 50 }
        giftResult.value = { items: [], total: 0, page: 1, page_size: 50 }
        scResult.value = { items: [], total: 0, page: 1, page_size: 50 }
      }
    } catch (e) {
      console.error('Failed to delete archive session:', e)
    }
  }

  async function setContentType(type: ArchiveContentType) {
    contentType.value = type
    currentPage.value = 1
    await search()
  }

  async function setPage(page: number) {
    currentPage.value = page
    await search()
  }

  async function setSearchQuery(q: string) {
    searchQuery.value = q
    currentPage.value = 1
    await search()
  }

  async function setPriceRange(min?: number, max?: number) {
    minPrice.value = min
    maxPrice.value = max
    currentPage.value = 1
    await search()
  }

  return {
    sessions,
    selectedSessionId,
    selectedSession,
    contentType,
    searchQuery,
    minPrice,
    maxPrice,
    currentPage,
    pageSize,
    loading,
    danmakuResult,
    giftResult,
    scResult,
    loadSessions,
    selectSession,
    search,
    removeSession,
    setContentType,
    setPage,
    setSearchQuery,
    setPriceRange,
  }
})

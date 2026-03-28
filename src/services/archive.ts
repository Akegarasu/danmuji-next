/**
 * 存档服务
 * 封装 Tauri invoke 调用
 */

import { invoke } from '@tauri-apps/api/core'
import type {
  ArchiveSession,
  PagedResult,
  ArchivedDanmaku,
  ArchivedGift,
  ArchivedSuperChat,
} from '@/types'

export const getArchiveSessions = async (): Promise<ArchiveSession[]> => {
  return await invoke<ArchiveSession[]>('get_archive_sessions')
}

export const getArchiveSessionDetail = async (sessionId: number): Promise<ArchiveSession> => {
  return await invoke<ArchiveSession>('get_archive_session_detail', { sessionId })
}

export const searchArchiveDanmaku = async (
  sessionId: number,
  query: string,
  page: number,
  pageSize: number
): Promise<PagedResult<ArchivedDanmaku>> => {
  return await invoke<PagedResult<ArchivedDanmaku>>('search_archive_danmaku', {
    sessionId,
    query,
    page,
    pageSize,
  })
}

export const searchArchiveGifts = async (
  sessionId: number,
  query: string,
  minPrice: number | undefined,
  maxPrice: number | undefined,
  page: number,
  pageSize: number
): Promise<PagedResult<ArchivedGift>> => {
  return await invoke<PagedResult<ArchivedGift>>('search_archive_gifts', {
    sessionId,
    query,
    minPrice: minPrice ?? null,
    maxPrice: maxPrice ?? null,
    page,
    pageSize,
  })
}

export const searchArchiveSuperChat = async (
  sessionId: number,
  query: string,
  minPrice: number | undefined,
  maxPrice: number | undefined,
  page: number,
  pageSize: number
): Promise<PagedResult<ArchivedSuperChat>> => {
  return await invoke<PagedResult<ArchivedSuperChat>>('search_archive_superchat', {
    sessionId,
    query,
    minPrice: minPrice ?? null,
    maxPrice: maxPrice ?? null,
    page,
    pageSize,
  })
}

export const deleteArchiveSession = async (sessionId: number): Promise<void> => {
  await invoke('delete_archive_session', { sessionId })
}

/**
 * 存档服务
 * 封装 Tauri invoke 调用
 */

import { invoke } from '@tauri-apps/api/core'
import type {
  ArchiveContentType,
  ArchiveOverview,
  ArchiveSearchItem,
  ArchiveSession,
  ArchiveStatistics,
  PagedResult,
  ArchivedDanmaku,
  ArchivedUserName,
  ArchivedGift,
  ArchivedSuperChat,
} from '@/types'

export interface ArchiveDateFilter {
  fromTime?: number
  toTime?: number
}

export interface ArchiveSearchParams extends ArchiveDateFilter {
  roomId?: number
  sessionId?: number
  query: string
  eventType: ArchiveContentType
  page: number
  pageSize: number
}

export const getArchiveOverview = async (
  filter: ArchiveDateFilter = {},
  query = ''
): Promise<ArchiveOverview> => {
  return await invoke<ArchiveOverview>('get_archive_overview', {
    fromTime: filter.fromTime ?? null,
    toTime: filter.toTime ?? null,
    query,
  })
}

export const getArchiveRoomSessions = async (
  roomId: number,
  filter: ArchiveDateFilter,
  page: number,
  pageSize: number
): Promise<PagedResult<ArchiveSession>> => {
  return await invoke<PagedResult<ArchiveSession>>('get_archive_room_sessions', {
    roomId,
    fromTime: filter.fromTime ?? null,
    toTime: filter.toTime ?? null,
    page,
    pageSize,
  })
}

export const getArchiveStatistics = async (
  roomId: number | undefined,
  filter: ArchiveDateFilter
): Promise<ArchiveStatistics> => {
  return await invoke<ArchiveStatistics>('get_archive_statistics', {
    roomId: roomId ?? null,
    fromTime: filter.fromTime ?? null,
    toTime: filter.toTime ?? null,
  })
}

export const searchArchive = async (
  params: ArchiveSearchParams
): Promise<PagedResult<ArchiveSearchItem>> => {
  return await invoke<PagedResult<ArchiveSearchItem>>('search_archive', {
    roomId: params.roomId ?? null,
    sessionId: params.sessionId ?? null,
    query: params.query,
    eventType: params.eventType,
    fromTime: params.fromTime ?? null,
    toTime: params.toTime ?? null,
    page: params.page,
    pageSize: params.pageSize,
  })
}

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

export const lookupArchiveUserNames = async (uids: number[]): Promise<ArchivedUserName[]> => {
  return await invoke<ArchivedUserName[]>('lookup_archive_user_names', { uids })
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

export const pruneEmptyArchiveSessions = async (): Promise<number> => {
  return await invoke<number>('prune_empty_archive_sessions')
}

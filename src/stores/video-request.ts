/**
 * 点播 Store
 * 被动接收后端推送的点播数据，通过 invoke 调用后端命令进行操作
 */

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { VideoRequestItem } from '@/types'

export const useVideoRequestStore = defineStore('videoRequest', () => {
    /** 点播列表（由后端推送更新） */
    const requests = ref<VideoRequestItem[]>([])

    /** 未看的点播 */
    const unwatchedRequests = computed(() =>
        requests.value.filter(r => !r.watched)
    )

    /** 已看的点播 */
    const watchedRequests = computed(() =>
        requests.value.filter(r => r.watched)
    )

    /** 未看数量 */
    const unwatchedCount = computed(() => unwatchedRequests.value.length)

    // ==================== 后端数据同步 ====================

    /** 追加新点播（由 blive-client 调用） */
    const appendRequest = (item: VideoRequestItem) => {
        requests.value.unshift(item)
    }

    /** 更新点播信息（视频加载完成，由 blive-client 调用） */
    const updateRequest = (item: VideoRequestItem) => {
        const idx = requests.value.findIndex(r => r.id === item.id)
        if (idx !== -1) {
            requests.value[idx] = item
        }
    }

    /** 全量同步（由 blive-client 调用） */
    const syncRequests = (items: VideoRequestItem[]) => {
        requests.value = items
    }

    // ==================== 用户操作（调用后端命令） ====================

    /** 标记为已看/未看 */
    const markWatched = async (id: string, watched = true) => {
        await invoke('mark_video_watched', { requestId: id, watched })
    }

    /** 删除请求 */
    const removeRequest = async (id: string) => {
        await invoke('remove_video_request', { requestId: id })
    }

    /** 清空已看 */
    const clearWatched = async () => {
        await invoke('clear_watched_videos')
    }

    /** 清空所有 */
    const clearAll = async () => {
        await invoke('clear_all_videos')
    }

    /** 重置状态 */
    const $reset = () => {
        requests.value = []
    }

    return {
        requests,
        unwatchedRequests,
        watchedRequests,
        unwatchedCount,
        appendRequest,
        updateRequest,
        syncRequests,
        markWatched,
        removeRequest,
        clearAll,
        clearWatched,
        $reset,
    }
})

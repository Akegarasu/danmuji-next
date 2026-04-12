/**
 * 投票 Store
 * 管理投票状态，接收后端推送的投票数据，通过 invoke 调用后端命令
 */

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { Poll, Voter, VoteKeyType } from '@/types'

export const useVotingStore = defineStore('voting', () => {
    /** 投票列表（由后端推送更新） */
    const polls = ref<Poll[]>([])

    /** 进行中的投票 */
    const activePolls = computed(() =>
        polls.value.filter(p => p.status === 'active')
    )

    /** 已结束的投票 */
    const endedPolls = computed(() =>
        polls.value.filter(p => p.status === 'ended')
    )

    /** 是否有进行中的投票 */
    const hasActivePolls = computed(() => activePolls.value.length > 0)

    // ==================== 后端数据同步 ====================

    /** 更新单个投票（创建/投票/结束） */
    const updatePoll = (poll: Poll) => {
        const idx = polls.value.findIndex(p => p.id === poll.id)
        if (idx !== -1) {
            polls.value[idx] = poll
        } else {
            polls.value.unshift(poll)
        }
    }

    /** 全量同步 */
    const syncPolls = (items: Poll[]) => {
        polls.value = items
    }

    // ==================== 用户操作（调用后端命令） ====================

    /** 创建投票 */
    const createPoll = async (
        title: string,
        options: [string, string][],
        keyType: VoteKeyType,
        durationSecs?: number,
    ): Promise<Poll> => {
        return await invoke<Poll>('create_poll', {
            title,
            options,
            keyType: keyType,
            durationSecs: durationSecs ?? null,
        })
    }

    /** 结束投票 */
    const endPoll = async (pollId: string) => {
        await invoke('end_poll', { pollId })
    }

    /** 删除投票 */
    const deletePoll = async (pollId: string) => {
        await invoke('delete_poll', { pollId })
    }

    /** 获取投票者列表 */
    const getVoters = async (pollId: string, optionKey: string): Promise<Voter[]> => {
        return await invoke<Voter[]>('get_poll_voters', { pollId, optionKey })
    }

    /** 重置状态 */
    const $reset = () => {
        polls.value = []
    }

    return {
        polls,
        activePolls,
        endedPolls,
        hasActivePolls,
        updatePoll,
        syncPolls,
        createPoll,
        endPoll,
        deletePoll,
        getVoters,
        $reset,
    }
})

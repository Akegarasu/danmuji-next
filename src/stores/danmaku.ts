/**
 * 弹幕数据 Store
 * 作为视图层缓存，数据主要由后端管理
 */

import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { 
  ProcessedDanmaku,
  ProcessedGift,
  ProcessedSuperChat,
  ProcessedOnlineRankUser,
  UserContribution,
  LiveStats,
  GiftUpsert,
  ContributionRankUser
} from '@/types'

// 前端缓存上限（与后端保持一致或更小）
const MAX_DANMAKU = 10000
const MAX_GIFTS = 5000
const MAX_SUPERCHAT = 2000

export const useDanmakuStore = defineStore('danmaku', () => {
  // ==================== 数据列表 ====================
  
  /** 弹幕列表 */
  const danmakuList = ref<ProcessedDanmaku[]>([])
  
  /** 礼物列表 */
  const giftList = ref<ProcessedGift[]>([])
  
  /** 礼物合并索引: merge_key -> list index */
  const giftMergeIndex = ref<Map<string, number>>(new Map())
  
  /** SC 列表 */
  const superChatList = ref<ProcessedSuperChat[]>([])
  
  /** 贡献排行实时更新（ONLINE_RANK_V2，前几名） */
  const contributionRankLive = ref<ProcessedOnlineRankUser[]>([])
  
  /** 贡献排行完整列表（API 获取，最多 100 人） */
  const contributionRankFull = ref<ContributionRankUser[]>([])
  
  /** 用户贡献排行（本场礼物/SC 贡献前 50） */
  const contributions = ref<UserContribution[]>([])
  
  /** 直播统计 */
  const stats = ref<LiveStats>({
    total_revenue: 0,
    gift_revenue: 0,
    sc_revenue: 0,
    guard_revenue: 0,
    online_count: 0
  })
  
  // ==================== 连接状态 ====================
  
  /** 是否已连接 */
  const isConnected = ref(false)
  
  /** 房间信息 */
  const roomInfo = ref({
    roomId: '',
    title: '',
    liveStatus: 0
  })

  // ==================== 数据设置方法（用于快照同步）====================
  
  /** 设置弹幕列表（快照同步） */
  const setDanmakuList = (items: ProcessedDanmaku[]) => {
    danmakuList.value = items.slice(-MAX_DANMAKU)
  }
  
  /** 设置礼物列表（快照同步） */
  const setGiftList = (items: ProcessedGift[]) => {
    giftList.value = items.slice(-MAX_GIFTS)
    rebuildGiftIndex()
  }
  
  /** 设置 SC 列表（快照同步） */
  const setSuperChatList = (items: ProcessedSuperChat[]) => {
    superChatList.value = items.slice(0, MAX_SUPERCHAT)
  }

  // ==================== 数据更新方法（供 blive-client 调用）====================
  
  /** 追加弹幕 */
  const appendDanmaku = (items: ProcessedDanmaku[]) => {
    danmakuList.value.push(...items)
    // 限制长度
    if (danmakuList.value.length > MAX_DANMAKU) {
      danmakuList.value.splice(0, danmakuList.value.length - MAX_DANMAKU)
    }
  }

  /** 更新礼物（新增或合并） */
  const upsertGifts = (upserts: GiftUpsert[]) => {
    for (const { merge_key, gift, action } of upserts) {
      if (action === 'insert') {
        const index = giftList.value.length
        giftList.value.push(gift)
        giftMergeIndex.value.set(merge_key, index)
        
        // 限制长度
        if (giftList.value.length > MAX_GIFTS) {
          giftList.value.shift()
          rebuildGiftIndex()
        }
      } else {
        // update
        const index = giftMergeIndex.value.get(merge_key)
        if (index !== undefined && giftList.value[index]) {
          giftList.value[index] = gift
        }
      }
    }
  }
  
  /** 重建礼物索引 */
  const rebuildGiftIndex = () => {
    giftMergeIndex.value.clear()
    giftList.value.forEach((gift, index) => {
      giftMergeIndex.value.set(gift.merge_key, index)
    })
  }

  /** 追加 SC */
  const appendSuperChat = (sc: ProcessedSuperChat) => {
    // 检查是否已存在相同 ID 的 SC，避免重复添加
    const exists = superChatList.value.some(item => item.id === sc.id)
    if (exists) {
      return
    }
    
    superChatList.value.unshift(sc)
    if (superChatList.value.length > MAX_SUPERCHAT) {
      superChatList.value.pop()
    }
  }

  /** 更新贡献排行实时数据 */
  const updateContributionRankLive = (rank: ProcessedOnlineRankUser[]) => {
    contributionRankLive.value = rank
  }
  
  /** 更新贡献排行完整列表 */
  const updateContributionRankFull = (rank: ContributionRankUser[]) => {
    contributionRankFull.value = rank
  }

  /** 更新统计 */
  const updateStats = (newStats: LiveStats) => {
    stats.value = newStats
  }

  /** 更新贡献排行 */
  const updateContributions = (newContributions: UserContribution[]) => {
    contributions.value = newContributions
  }

  // ==================== 状态管理 ====================

  /** 设置连接状态 */
  const setConnected = (connected: boolean) => {
    isConnected.value = connected
  }

  /** 更新房间信息 */
  const updateRoomInfo = (info: Partial<typeof roomInfo.value>) => {
    Object.assign(roomInfo.value, info)
  }

  /** 清空所有数据 */
  const clearAll = () => {
    danmakuList.value = []
    giftList.value = []
    giftMergeIndex.value.clear()
    superChatList.value = []
    contributionRankLive.value = []
    contributionRankFull.value = []
    contributions.value = []
    stats.value = {
      total_revenue: 0,
      gift_revenue: 0,
      sc_revenue: 0,
      guard_revenue: 0,
      online_count: 0
    }
  }

  return {
    // 数据列表
    danmakuList,
    giftList,
    superChatList,
    contributionRankLive,
    contributionRankFull,
    contributions,
    stats,
    
    // 状态
    isConnected,
    roomInfo,
    
    // 数据设置方法（快照同步）
    setDanmakuList,
    setGiftList,
    setSuperChatList,
    
    // 数据更新方法
    appendDanmaku,
    upsertGifts,
    appendSuperChat,
    updateContributionRankLive,
    updateContributionRankFull,
    updateStats,
    updateContributions,
    
    // 状态管理
    setConnected,
    updateRoomInfo,
    clearAll
  }
})

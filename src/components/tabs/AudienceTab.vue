<script setup lang="ts">
import { computed, ref } from 'vue'
import { useDanmakuStore } from '@/stores/danmaku'
import { useSettingsStore } from '@/stores/settings'
import { formatPrice, getMedalGradient } from '@/types'
import { refreshContributionRank } from '@/services/blive-client'
import ContextMenu from '@/components/common/ContextMenu.vue'
import type { MenuItem } from '@/components/common/ContextMenu.vue'
import { invoke } from '@tauri-apps/api/core'

const danmakuStore = useDanmakuStore()
const settingsStore = useSettingsStore()

// 刷新状态
const isRefreshing = ref(false)

// 右键菜单
const contextMenuRef = ref<InstanceType<typeof ContextMenu>>()
const currentUser = ref<DisplayUser | null>(null)

// 用于显示的用户项类型
interface DisplayUser {
  uid: number
  name: string
  face?: string
  guardLevel: number
  score: string
  rank: number
  medalName?: string
  medalLevel?: number
  medalColor?: string
}

// 根据排序类型选择数据源
const displayList = computed((): DisplayUser[] => {
  const sortType = settingsStore.audienceSortType
  
  if (sortType === 'giftValue') {
    // 使用本场贡献排行（礼物/SC 贡献）
    return danmakuStore.contributions.map((c, index) => ({
      uid: c.uid,
      name: c.name,
      face: c.face,
      guardLevel: c.guard_level,
      score: formatPrice(c.total_value) || '',
      rank: index + 1
    }))
  } else {
    // 使用贡献排行榜（API 获取的完整列表）
    return danmakuStore.contributionRankFull.map(u => ({
      uid: u.uid,
      name: u.name,
      face: u.face,
      guardLevel: u.guard_level,
      score: u.score.toString(),
      rank: u.rank,
      medalName: u.medal_name,
      medalLevel: u.medal_level,
      medalColor: u.medal_color
    }))
  }
})

// 大航海等级名称
const getGuardName = (level: number) => {
  switch (level) {
    case 1: return '总督'
    case 2: return '提督'
    case 3: return '舰长'
    default: return ''
  }
}

// 排序方式标签
const sortLabel = computed(() => {
  switch (settingsStore.audienceSortType) {
    case 'giftValue': return '本场贡献'
    default: return '贡献榜'
  }
})

// 贡献榜总人数
const totalCount = computed(() => danmakuStore.contributionRankFull.length)

// 刷新贡献排行榜
const handleRefresh = async () => {
  const cookie = settingsStore.settings.cookie
  if (!cookie) {
    console.warn('[AudienceTab] 无法刷新：未设置 Cookie')
    return
  }

  isRefreshing.value = true
  try {
    await refreshContributionRank(cookie)
    console.log('[AudienceTab] 贡献排行榜已刷新')
  } catch (error) {
    console.error('[AudienceTab] 刷新贡献排行榜失败:', error)
  } finally {
    isRefreshing.value = false
  }
}

// ==================== 右键菜单 ====================

const menuItems = computed<MenuItem[]>(() => ([
  {
    label: '打开用户主页',
    icon: '🔗',
    action: () => openUserPage()
  },
  {
    label: '复制用户名',
    icon: '📋',
    action: () => copyUsername()
  }
]))

const handleContextMenu = (e: MouseEvent, user: DisplayUser) => {
  e.preventDefault()
  e.stopPropagation()
  currentUser.value = user
  contextMenuRef.value?.show(e.clientX, e.clientY)
}

const openUserPage = async () => {
  if (!currentUser.value) return
  const url = `https://space.bilibili.com/${currentUser.value.uid}`
  try {
    await invoke('open_url', { url })
  } catch (e) {
    window.open(url, '_blank')
  }
}

const copyUsername = () => {
  if (!currentUser.value) return
  navigator.clipboard.writeText(currentUser.value.name)
}
</script>

<template>
  <div class="audience-tab">
    <div class="stats-bar">
      <span class="stat">
        在线: <strong>{{ danmakuStore.stats.online_count }}</strong>
      </span>
      <span class="stat" v-if="totalCount > 0">
        贡献榜 : <strong>{{ totalCount }}</strong>
      </span>
      <span class="sort-mode">{{ sortLabel }}</span>
      <button 
        v-if="settingsStore.audienceSortType !== 'giftValue'"
        class="refresh-btn" 
        @click="handleRefresh"
        :disabled="isRefreshing"
        :title="isRefreshing ? '刷新中...' : '刷新贡献榜'"
      >
        <span>↻</span>
      </button>
    </div>
    
    <div class="audience-list">
      <div 
        v-for="(user, index) in displayList"
        :key="user.uid"
        class="audience-item"
        :class="{ 'has-guard': user.guardLevel > 0 }"
        @contextmenu="handleContextMenu($event, user)"
      >
        <div class="rank-badge" v-if="index < 3">
          {{ ['🥇', '🥈', '🥉'][index] }}
        </div>
        <div class="rank-num" v-else>
          {{ index + 1 }}
        </div>
        
        <div class="user-info">
          <span 
            v-if="user.guardLevel" 
            class="guard-badge"
            :class="`guard-${user.guardLevel}`"
          >
            {{ getGuardName(user.guardLevel) }}
          </span>
          <span 
            v-if="settingsStore.audienceShowMedal && user.medalName" 
            class="medal-badge"
            :style="{ backgroundImage: getMedalGradient(user.medalLevel ?? 0) }"
          >
            {{ user.medalName }} {{ user.medalLevel }}
          </span>
          <span class="name">{{ user.name }}</span>
        </div>
        
        <div class="user-score" v-if="user.score">
          {{ user.score }}
        </div>
      </div>
      
      <div v-if="displayList.length === 0" class="empty-state">
        <span class="icon">👥</span>
        <span class="text">等待观众加入...</span>
      </div>
    </div>

    <ContextMenu ref="contextMenuRef" :items="menuItems" />
  </div>
</template>

<style scoped lang="scss">
.audience-tab {
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.stats-bar {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 12px;
  background: var(--bg-card);
  border-bottom: 1px solid var(--border-color);
  font-size: var(--font-size-xs);
  color: var(--text-secondary);
  flex-shrink: 0;
  
  strong {
    color: var(--accent-primary);
  }
  
  .sort-mode {
    margin-left: auto;
    color: var(--text-muted);
  }
  
  .refresh-btn {
    margin-left: 1px;
    border: none;
    background: transparent;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-sm);
    color: var(--text-primary);
    cursor: pointer;
    font-size: var(--font-size-sm);
    transition: all 0.15s;
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 28px;
    height: 24px;
    
    &:hover:not(:disabled) {
      background: var(--bg-hover);
      color: var(--text-primary);
    }
    
    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
    
    span {
      display: inline-block;
      line-height: 1;
    }
  }
}

.audience-list {
  flex: 1;
  overflow-y: auto;
  padding: 4px 0;
  min-height: 0;
}

.audience-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  transition: background 0.15s;
  cursor: default;
  
  &:hover {
    background: var(--bg-hover);
  }
  
  &.has-guard {
    background: rgba(91, 142, 201, 0.1);
  }
}

.rank-badge {
  width: 24px;
  text-align: center;
  font-size: 14px;
}

.rank-num {
  width: 24px;
  text-align: center;
  font-size: var(--content-font-size-xs);
  color: var(--text-muted);
}

.user-info {
  display: flex;
  align-items: center;
  gap: 0.5em;
  overflow: hidden;
  flex: 1;
  min-width: 0;
  font-size: var(--content-font-size-sm);
}

.guard-badge {
  padding: 0.1em 0.5em;
  border-radius: 0.2em;
  font-size: 0.85em;
  color: white;
  font-weight: 500;
  flex-shrink: 0;
  line-height: 1.3;
  
  &.guard-1 { background: var(--guard-governor); color: #333; }
  &.guard-2 { background: var(--guard-admiral); }
  &.guard-3 { background: var(--guard-captain); }
}

.medal-badge {
  padding: 0.1em 0.4em;
  border-radius: 0.2em;
  font-size: 0.75em;
  color: white;
  font-weight: 500;
  flex-shrink: 0;
  line-height: 1.3;
  opacity: 0.9;
}

.name {
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.user-score {
  font-size: var(--content-font-size-xs);
  color: var(--accent-gold);
  font-weight: 500;
  flex-shrink: 0;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  min-height: 200px;
  color: var(--text-muted);
  gap: 8px;
  
  .icon {
    font-size: 32px;
    opacity: 0.5;
  }
  
  .text {
    font-size: var(--font-size-sm);
  }
}
</style>

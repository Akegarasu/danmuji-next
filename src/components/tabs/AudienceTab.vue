<script setup lang="ts">
import { computed, onActivated, onDeactivated, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useDanmakuStore } from '@/stores/danmaku'
import { useSettingsStore } from '@/stores/settings'
import { getMedalGradient } from '@/types'
import type {
  ContributionRankType,
  ContributionRankUser,
  GuardTopListUser
} from '@/types'
import {
  refreshContributionRank,
  refreshGuardTopList
} from '@/services/blive-client'
import { createLogger } from '@/services/logger'
import EntryPanel from '@/components/common/EntryPanel.vue'
import ContextMenu from '@/components/common/ContextMenu.vue'
import type { MenuItem } from '@/components/common/ContextMenu.vue'

type AudienceMode = 'audience' | 'guard'

interface DisplayUser {
  uid: number
  name: string
  face?: string
  guardLevel: number
  score?: string
  rank: number
  medalName?: string
  medalLevel?: number
  medalColor?: string
}

const rankTabs: Array<{ type: ContributionRankType; label: string }> = [
  { type: 'online', label: '在线榜' },
  { type: 'daily', label: '日榜' },
  { type: 'weekly', label: '周榜' },
  { type: 'monthly', label: '月榜' }
]

const danmakuStore = useDanmakuStore()
const settingsStore = useSettingsStore()
const logger = createLogger('AudienceTab')

const activeMode = ref<AudienceMode>('audience')
const activeRankType = ref<ContributionRankType>('online')
const rankCache = ref<Record<ContributionRankType, ContributionRankUser[]>>({
  online: [],
  daily: [],
  weekly: [],
  monthly: []
})
const rankLoaded = ref<Record<ContributionRankType, boolean>>({
  online: false,
  daily: false,
  weekly: false,
  monthly: false
})
const guardUsers = ref<GuardTopListUser[]>([])
const guardCount = ref(0)
const guardLoaded = ref(false)
const isRefreshing = ref(false)
const needsActiveRefresh = ref(false)
const loadError = ref('')
const isTabActive = ref(true)
let autoRefreshTimer: number | null = null

const contextMenuRef = ref<InstanceType<typeof ContextMenu>>()
const currentUser = ref<DisplayUser | null>(null)

watch(
  () => danmakuStore.contributionRankFull,
  (rank) => {
    rankCache.value.online = rank
    if (rank.length > 0) rankLoaded.value.online = true
  },
  { immediate: true }
)

watch(
  () => danmakuStore.isConnected,
  (connected) => {
    if (connected) return
    rankCache.value = { online: [], daily: [], weekly: [], monthly: [] }
    rankLoaded.value = { online: false, daily: false, weekly: false, monthly: false }
    guardUsers.value = []
    guardCount.value = 0
    guardLoaded.value = false
  }
)

const normalizeRefreshInterval = (seconds: number) =>
  Math.min(300, Math.max(10, Number.isFinite(seconds) ? Math.round(seconds) : 120))

const autoRefreshIntervalSeconds = computed(() =>
  normalizeRefreshInterval(settingsStore.audienceAutoRefreshIntervalSeconds)
)

const activeContributionList = computed(() => rankCache.value[activeRankType.value])

const pinSpecialFollowUsers = (list: DisplayUser[]): DisplayUser[] => list
  .map((user, index) => ({
    user,
    index,
    isSpecialFollow: settingsStore.isSpecialFollow(user.uid)
  }))
  .sort((a, b) => {
    if (a.isSpecialFollow !== b.isSpecialFollow) return a.isSpecialFollow ? -1 : 1
    return a.index - b.index
  })
  .map(({ user }) => user)

const mapContributionUser = (user: ContributionRankUser, showScore: boolean): DisplayUser => ({
  uid: user.uid,
  name: user.name,
  face: user.face,
  guardLevel: user.guard_level,
  score: showScore ? user.score.toString() : undefined,
  rank: user.rank,
  medalName: user.medal_name,
  medalLevel: user.medal_level,
  medalColor: user.medal_color
})

const mapGuardUser = (user: GuardTopListUser): DisplayUser => ({
  uid: user.uid,
  name: user.name,
  face: user.face,
  guardLevel: user.guard_level,
  score: (user.accompany || user.score).toString(),
  rank: user.rank,
  medalName: user.medal_name,
  medalLevel: user.medal_level,
  medalColor: user.medal_color
})

const displayList = computed(() => pinSpecialFollowUsers(
  activeMode.value === 'guard'
    ? guardUsers.value.map(mapGuardUser)
    : activeContributionList.value.map(user =>
        mapContributionUser(user, activeRankType.value === 'online')
      )
))

const audienceCount = computed(() => Math.max(
  danmakuStore.stats.online_count,
  rankCache.value.online.length
))

const activeRankLabel = computed(() =>
  rankTabs.find((tab) => tab.type === activeRankType.value)?.label ?? '贡献榜'
)

const summaryText = computed(() => activeMode.value === 'guard'
  ? '榜单每月更新，统计大航海亲密度'
  : `${activeRankLabel.value} · ${displayList.value.length} 人`
)

const showScore = computed(() => activeMode.value === 'guard' || activeRankType.value === 'online')
const scoreLabel = computed(() => activeMode.value === 'guard' ? '陪伴值' : '贡献值')

const audienceStyle = computed(() => ({
  '--audience-font-family': settingsStore.contentFontFamily,
  '--audience-font-weight': String(settingsStore.contentFontWeight),
  '--audience-font-color': settingsStore.audienceFontColor,
  '--audience-score-color': settingsStore.audienceScoreColor
}))

const getGuardName = (level: number) => {
  switch (level) {
    case 1: return '总督'
    case 2: return '提督'
    case 3: return '舰长'
    default: return ''
  }
}

const refreshRank = async (silent = false, rankType = activeRankType.value) => {
  const cookie = settingsStore.settings.cookie
  if (!cookie || isRefreshing.value) return

  isRefreshing.value = true
  loadError.value = ''
  try {
    const response = await refreshContributionRank(cookie, rankType)
    rankCache.value[rankType] = response.list
    rankLoaded.value[rankType] = true
  } catch (error) {
    loadError.value = '贡献榜加载失败'
    logger.error(`${silent ? 'auto' : 'manual'} contribution rank refresh failed:`, error)
  } finally {
    isRefreshing.value = false
    if (needsActiveRefresh.value) {
      needsActiveRefresh.value = false
      queueMicrotask(ensureActiveData)
    } else if (
      rankLoaded.value[rankType] &&
      activeMode.value === 'audience' &&
      !guardLoaded.value
    ) {
      queueMicrotask(ensureActiveData)
    }
  }
}

const refreshGuards = async (silent = false) => {
  const cookie = settingsStore.settings.cookie
  if (!cookie || isRefreshing.value) return

  isRefreshing.value = true
  loadError.value = ''
  try {
    const response = await refreshGuardTopList(cookie)
    guardUsers.value = response.list
    guardCount.value = response.count
    guardLoaded.value = true
  } catch (error) {
    if (!silent || activeMode.value === 'guard') loadError.value = '大航海榜加载失败'
    logger.error(`${silent ? 'auto' : 'manual'} guard rank refresh failed:`, error)
  } finally {
    isRefreshing.value = false
    if (needsActiveRefresh.value) {
      needsActiveRefresh.value = false
      queueMicrotask(ensureActiveData)
    }
  }
}

const ensureActiveData = () => {
  if (!danmakuStore.isConnected || !settingsStore.settings.cookie) return
  if (isRefreshing.value) {
    needsActiveRefresh.value = true
    return
  }
  if (activeMode.value === 'guard') {
    if (!guardLoaded.value) void refreshGuards(true)
  } else if (!rankLoaded.value[activeRankType.value]) {
    void refreshRank(true)
  } else if (!guardLoaded.value) {
    // 预取大航海总人数，使一级切换栏在房间观众页也能显示准确数量。
    void refreshGuards(true)
  }
}

const handleRefresh = () => {
  if (activeMode.value === 'guard') void refreshGuards(false)
  else void refreshRank(false)
}

const shouldAutoRefresh = computed(() =>
  isTabActive.value &&
  activeMode.value === 'audience' &&
  settingsStore.audienceAutoRefreshEnabled &&
  !!settingsStore.settings.cookie &&
  danmakuStore.isConnected
)

const clearAutoRefreshTimer = () => {
  if (autoRefreshTimer === null) return
  window.clearInterval(autoRefreshTimer)
  autoRefreshTimer = null
}

const resetAutoRefreshTimer = () => {
  clearAutoRefreshTimer()
  if (!shouldAutoRefresh.value) return
  autoRefreshTimer = window.setInterval(() => {
    void refreshRank(true)
  }, autoRefreshIntervalSeconds.value * 1000)
}

watch(
  [activeMode, activeRankType, () => danmakuStore.isConnected],
  ensureActiveData,
  { immediate: true }
)
watch([shouldAutoRefresh, autoRefreshIntervalSeconds, activeRankType], resetAutoRefreshTimer, {
  immediate: true
})

onActivated(() => {
  isTabActive.value = true
  ensureActiveData()
})

onDeactivated(() => {
  isTabActive.value = false
  clearAutoRefreshTimer()
})

onUnmounted(clearAutoRefreshTimer)

const isCurrentSpecialFollow = computed(() =>
  currentUser.value ? settingsStore.isSpecialFollow(currentUser.value.uid) : false
)

const toggleCurrentSpecialFollow = () => {
  if (!currentUser.value) return
  if (settingsStore.isSpecialFollow(currentUser.value.uid)) {
    settingsStore.removeSpecialFollow(currentUser.value.uid)
  } else {
    settingsStore.addSpecialFollow(currentUser.value.uid)
  }
}

const openUserPage = async () => {
  if (!currentUser.value) return
  const url = `https://space.bilibili.com/${currentUser.value.uid}`
  try {
    await invoke('open_url', { url })
  } catch {
    window.open(url, '_blank')
  }
}

const copyUsername = () => {
  if (currentUser.value) void navigator.clipboard.writeText(currentUser.value.name)
}

const menuItems = computed<MenuItem[]>(() => [
  { label: '打开用户主页', icon: '🔗', action: openUserPage },
  { label: '复制用户名', icon: '📋', action: copyUsername },
  { divider: true, label: '', action: () => {} },
  {
    label: isCurrentSpecialFollow.value ? '取消特别关注' : '特别关注',
    icon: '⭐',
    action: toggleCurrentSpecialFollow
  }
])

const handleContextMenu = (event: MouseEvent, user: DisplayUser) => {
  event.preventDefault()
  event.stopPropagation()
  currentUser.value = user
  contextMenuRef.value?.show(event.clientX, event.clientY)
}

const handleAvatarError = (event: Event) => {
  const image = event.currentTarget as HTMLImageElement
  image.style.display = 'none'
}
</script>

<template>
  <div class="audience-tab" :style="audienceStyle">
    <div class="rank-navigation">
      <div class="primary-tabs" role="tablist" aria-label="观众榜单类型">
        <button
          class="primary-tab"
          :class="{ active: activeMode === 'audience' }"
          type="button"
          @click="activeMode = 'audience'"
        >
          房间观众<span class="tab-count">({{ audienceCount }})</span>
        </button>
        <button
          class="primary-tab"
          :class="{ active: activeMode === 'guard' }"
          type="button"
          @click="activeMode = 'guard'"
        >
          大航海<span class="tab-count">({{ guardCount }})</span>
        </button>
      </div>

      <div v-if="activeMode === 'audience'" class="secondary-tabs" role="tablist" aria-label="贡献榜周期">
        <button
          v-for="tab in rankTabs"
          :key="tab.type"
          class="secondary-tab"
          :class="{ active: activeRankType === tab.type }"
          type="button"
          @click="activeRankType = tab.type"
        >
          {{ tab.label }}
        </button>
      </div>
    </div>

    <div class="rank-summary">
      <span class="summary-text" :title="summaryText">{{ summaryText }}</span>
      <span v-if="showScore" class="score-heading">{{ scoreLabel }}</span>
      <button
        class="refresh-btn"
        type="button"
        :disabled="isRefreshing"
        :title="isRefreshing ? '刷新中…' : '刷新当前榜单'"
        @click="handleRefresh"
      >
        ↻
      </button>
    </div>

    <div v-if="loadError" class="load-error">{{ loadError }}</div>

    <div class="audience-list" :class="{ loading: isRefreshing && displayList.length === 0 }">
      <div
        v-for="user in displayList"
        :key="`${activeMode}-${activeRankType}-${user.uid}`"
        class="audience-item"
        :class="{
          'has-guard': user.guardLevel > 0,
          'is-special-follow': settingsStore.isSpecialFollow(user.uid)
        }"
        @contextmenu="handleContextMenu($event, user)"
      >
        <div class="rank-mark" :class="`rank-${Math.min(user.rank, 4)}`">
          <span v-if="user.rank <= 3">榜{{ user.rank }}</span>
          <span v-else>{{ user.rank }}</span>
        </div>

        <div class="avatar-wrap">
          <img
            v-if="user.face"
            class="avatar"
            :src="user.face"
            :alt="user.name"
            referrerpolicy="no-referrer"
            crossorigin="anonymous"
            @error="handleAvatarError"
          />
          <span class="avatar-fallback">{{ user.name.slice(0, 1) }}</span>
        </div>

        <div class="user-details">
          <div class="name-row">
            <span class="name">{{ user.name }}</span>
          </div>
          <div v-if="user.guardLevel || (settingsStore.audienceShowMedal && user.medalName)" class="badge-row">
            <span v-if="user.guardLevel" class="guard-badge" :class="`guard-${user.guardLevel}`">
              {{ getGuardName(user.guardLevel) }}
            </span>
            <span
              v-if="settingsStore.audienceShowMedal && user.medalName"
              class="medal-badge"
              :style="{ backgroundImage: getMedalGradient(user.medalLevel ?? 0) }"
            >
              {{ user.medalName }}{{ user.medalLevel }}
            </span>
          </div>
        </div>

        <div v-if="user.score" class="user-score">{{ user.score }}</div>
      </div>

      <div v-if="displayList.length === 0" class="empty-state">
        <span class="empty-icon">{{ isRefreshing ? '↻' : activeMode === 'guard' ? '⚓' : '👥' }}</span>
        <span>{{ isRefreshing ? '正在加载榜单…' : activeMode === 'guard' ? '暂无大航海数据' : '暂无排行数据' }}</span>
      </div>
    </div>

    <EntryPanel v-if="settingsStore.entryPanelShowInAudience" />
    <ContextMenu ref="contextMenuRef" :items="menuItems" />
  </div>
</template>

<style scoped lang="scss">
.audience-tab {
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  color: var(--text-primary);
  font-family: var(--audience-font-family, var(--font-family));
  font-weight: var(--audience-font-weight, 400);
}

.rank-navigation {
  padding: 6px 6px 5px;
  background: var(--bg-card);
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}

.primary-tabs {
  height: 31px;
  display: grid;
  grid-template-columns: 1fr 1fr;
  align-items: stretch;
  padding: 2px;
  border-radius: 12px 12px 7px 7px;
  background: rgba(255, 255, 255, 0.09);
}

.primary-tab,
.secondary-tab {
  border: 0;
  color: var(--text-muted);
  background: transparent;
  cursor: pointer;
  white-space: nowrap;
  transition: color 0.15s, background 0.15s;
}

.primary-tab {
  border-radius: 9px 9px 5px 5px;
  font-size: var(--font-size-xs);
  font-weight: 600;

  &.active {
    color: var(--text-primary);
    background: rgba(0, 0, 0, 0.2);
  }
}

.tab-count {
  margin-left: 1px;
  font-variant-numeric: tabular-nums;
}

.secondary-tabs {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 7px;
  margin-top: 5px;
  padding: 0 4px;
}

.secondary-tab {
  height: 22px;
  padding: 0 5px;
  border-radius: 999px;
  font-size: var(--font-size-xs);
  background: rgba(92, 181, 210, 0.14);

  &:hover,
  &.active {
    color: #eafaff;
    background: rgba(49, 162, 198, 0.55);
  }
}

.rank-summary {
  min-height: 28px;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 9px;
  color: var(--text-muted);
  background: var(--bg-card);
  border-bottom: 1px solid var(--border-color);
  font-size: var(--font-size-xs);
  flex-shrink: 0;
}

.summary-text {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.score-heading {
  margin-left: auto;
  flex-shrink: 0;
}

.refresh-btn {
  width: 23px;
  height: 22px;
  display: grid;
  place-items: center;
  padding: 0;
  border: 0;
  border-radius: 5px;
  color: var(--text-secondary);
  background: transparent;
  cursor: pointer;
  font-size: 15px;

  &:hover:not(:disabled) { background: var(--bg-hover); }
  &:disabled { opacity: 0.45; cursor: wait; }
}

.load-error {
  padding: 4px 9px;
  color: #ffaaa4;
  background: rgba(231, 76, 60, 0.12);
  font-size: var(--font-size-xs);
  flex-shrink: 0;
}

.audience-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 2px 0 5px;
}

.audience-item {
  min-height: 52px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 10px 5px 8px;
  transition: background 0.15s;

  &:hover { background: var(--bg-hover); }
  &.has-guard { background: rgba(91, 142, 201, 0.06); }
  &.has-guard:hover { background: rgba(91, 142, 201, 0.13); }

  &.is-special-follow {
    border-left: 3px solid #f5c842;
    padding-left: 5px;
    background: rgba(245, 200, 66, 0.12);

    .name { color: var(--accent-gold); }
  }
}

.rank-mark {
  width: 28px;
  flex: 0 0 28px;
  text-align: center;
  color: var(--text-muted);
  font-size: var(--content-font-size-xs);
  font-variant-numeric: tabular-nums;

  &.rank-1,
  &.rank-2,
  &.rank-3 {
    width: 27px;
    height: 19px;
    display: grid;
    place-items: center;
    border-radius: 8px 8px 8px 2px;
    color: #fff;
    font-size: 10px;
    font-weight: 700;
  }

  &.rank-1 { background: #ff6f61; }
  &.rank-2 { background: #ff806f; }
  &.rank-3 { background: #ff9586; }
}

.avatar-wrap {
  width: 34px;
  height: 34px;
  position: relative;
  flex: 0 0 34px;
  display: grid;
  place-items: center;
  overflow: hidden;
  border: 1px solid rgba(255, 255, 255, 0.14);
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.08);
}

.avatar {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
  z-index: 1;
}

.avatar-fallback {
  color: var(--text-muted);
  font-size: var(--font-size-xs);
}

.user-details {
  min-width: 0;
  flex: 1;
  align-self: stretch;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 3px;
}

.name-row,
.badge-row {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 4px;
}

.name {
  overflow: hidden;
  color: var(--audience-font-color, var(--text-primary));
  font-size: var(--content-font-size-sm);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.guard-badge,
.medal-badge {
  max-width: 92px;
  overflow: hidden;
  padding: 1px 5px;
  border-radius: 5px;
  color: #fff;
  font-size: 10px;
  line-height: 15px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.guard-badge {
  &.guard-1 { color: #33280c; background: var(--guard-governor); }
  &.guard-2 { background: var(--guard-admiral); }
  &.guard-3 { background: var(--guard-captain); }
}

.medal-badge {
  background-color: #527bd5;
  background-size: 100% 100%;
}

.user-score {
  flex-shrink: 0;
  min-width: 28px;
  text-align: right;
  color: var(--audience-score-color, var(--accent-gold));
  font-size: var(--content-font-size-xs);
  font-variant-numeric: tabular-nums;
}

.empty-state {
  height: 100%;
  min-height: 150px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 7px;
  color: var(--text-muted);
  font-size: var(--font-size-sm);
}

.empty-icon {
  font-size: 28px;
  opacity: 0.55;
}
</style>

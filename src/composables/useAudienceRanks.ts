import { computed, onScopeDispose, ref, watch, type Ref } from 'vue'
import { useDanmakuStore } from '@/stores/danmaku'
import { useSettingsStore } from '@/stores/settings'
import { refreshContributionRank, refreshGuardTopList } from '@/services/blive-client'
import { createLogger } from '@/services/logger'
import type { ContributionRankType, ContributionRankUser, GuardTopListUser } from '@/types'

export type AudienceMode = 'audience' | 'guard'
type RankKey = ContributionRankType | 'guard'

const emptyFlags = () => ({ online: false, daily: false, weekly: false, monthly: false, guard: false })
const emptyErrors = () => ({ online: '', daily: '', weekly: '', monthly: '', guard: '' })

export function useAudienceRanks(activeMode: Ref<AudienceMode>, activeRankType: Ref<ContributionRankType>) {
  const danmakuStore = useDanmakuStore()
  const settingsStore = useSettingsStore()
  const logger = createLogger('AudienceRanks')
  const rankCache = ref<Record<ContributionRankType, ContributionRankUser[]>>({
    online: [], daily: [], weekly: [], monthly: []
  })
  const rankLoaded = ref(emptyFlags())
  const loading = ref(emptyFlags())
  const errors = ref(emptyErrors())
  const guardUsers = ref<GuardTopListUser[]>([])
  const guardCount = ref(0)
  const generation = ref(0)
  let disposed = false

  const activeKey = computed<RankKey>(() => activeMode.value === 'guard' ? 'guard' : activeRankType.value)
  const isRefreshing = computed(() => loading.value[activeKey.value])
  const loadError = computed(() => errors.value[activeKey.value])

  // 同步失效旧请求，防止断线、换房或更新 Cookie 后旧响应回填新会话。
  watch([
    () => danmakuStore.isConnected,
    () => danmakuStore.roomInfo.roomId,
    () => settingsStore.settings.cookie
  ], () => {
    generation.value++
    rankCache.value = { online: [], daily: [], weekly: [], monthly: [] }
    rankLoaded.value = emptyFlags()
    loading.value = emptyFlags()
    errors.value = emptyErrors()
    guardUsers.value = []
    guardCount.value = 0
  }, { flush: 'sync' })

  watch(() => danmakuStore.contributionRankFull, (rank) => {
    if (!danmakuStore.isConnected) return
    rankCache.value.online = rank
    if (rank.length > 0) rankLoaded.value.online = true
  }, { immediate: true })

  const refresh = async (key: RankKey, silent: boolean) => {
    const cookie = settingsStore.settings.cookie
    if (disposed || !danmakuStore.isConnected || !cookie || loading.value[key]) return

    const requestGeneration = generation.value
    loading.value[key] = true
    errors.value[key] = ''
    try {
      if (key === 'guard') {
        const response = await refreshGuardTopList(cookie)
        if (requestGeneration !== generation.value) return
        guardUsers.value = response.list
        guardCount.value = response.count
      } else {
        const response = await refreshContributionRank(cookie, key)
        if (requestGeneration !== generation.value) return
        rankCache.value[key] = response.list
      }
      rankLoaded.value[key] = true
    } catch (error) {
      if (requestGeneration !== generation.value) return
      errors.value[key] = `${key === 'guard' ? '大航海榜' : '贡献榜'}加载失败，请点击刷新重试`
      logger.error(`${silent ? 'auto' : 'manual'} ${key} rank refresh failed:`, error)
    } finally {
      if (requestGeneration === generation.value) loading.value[key] = false
    }
  }

  const refreshRank = (silent = false, rankType = activeRankType.value) => refresh(rankType, silent)
  const handleRefresh = () => { void refresh(activeKey.value, false) }
  const ensureActiveData = () => {
    if (!rankLoaded.value[activeKey.value]) void refresh(activeKey.value, true)
    // 大航海预取独立进行，不会被贡献榜的慢请求或失败阻塞。
    if (activeMode.value === 'audience' && !rankLoaded.value.guard) void refresh('guard', true)
  }

  watch([
    activeMode, activeRankType, generation
  ], ensureActiveData, { immediate: true })

  onScopeDispose(() => {
    disposed = true
    generation.value++
  })

  return { rankCache, rankLoaded, guardUsers, guardCount, isRefreshing, loadError,
    refreshRank, handleRefresh, ensureActiveData }
}

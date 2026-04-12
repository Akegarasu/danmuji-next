<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useVotingStore } from '@/stores/voting'
import ExtSelect from '@/components/common/ExtSelect.vue'
import type { SelectOption } from '@/components/common/ExtSelect.vue'
import type { Poll, PollOption, Voter, VoteKeyType } from '@/types'

const votingStore = useVotingStore()

// ==================== 创建表单 ====================

const showCreateForm = ref(false)
const formTitle = ref('')
const formKeyType = ref<VoteKeyType>('letter')
const formDuration = ref<string>('none')
const formCustomMinutes = ref(10)
const formOptions = ref<{ key: string; label: string }[]>([
  { key: 'A', label: '' },
  { key: 'B', label: '' },
])

const keyTypeOptions: SelectOption[] = [
  { value: 'letter', label: '字母 (A/B/C)' },
  { value: 'number', label: '数字 (1/2/3)' },
]

const durationOptions: SelectOption[] = [
  { value: 'none', label: '不限时' },
  { value: '1m', label: '1 分钟' },
  { value: '3m', label: '3 分钟' },
  { value: '5m', label: '5 分钟' },
  { value: '10m', label: '10 分钟' },
  { value: '30m', label: '30 分钟' },
  { value: 'custom', label: '自定义' },
]

/** 获取下一个选项键 */
const getNextKey = (index: number): string => {
  if (formKeyType.value === 'letter') {
    return String.fromCharCode(65 + index) // A, B, C, ...
  }
  return String(index + 1) // 1, 2, 3, ...
}

/** 切换键类型时重新生成所有键 */
const onKeyTypeChange = () => {
  formOptions.value.forEach((opt, i) => {
    opt.key = getNextKey(i)
  })
}

/** 添加选项 */
const addOption = () => {
  if (formOptions.value.length >= 26) return
  formOptions.value.push({
    key: getNextKey(formOptions.value.length),
    label: '',
  })
}

/** 移除选项 */
const removeOption = (index: number) => {
  if (formOptions.value.length <= 2) return
  formOptions.value.splice(index, 1)
  // 重新编号
  formOptions.value.forEach((opt, i) => {
    opt.key = getNextKey(i)
  })
}

/** 提交创建投票 */
const submitCreate = async () => {
  const title = formTitle.value.trim()
  if (!title) return

  const options: [string, string][] = formOptions.value
    .filter(o => o.label.trim())
    .map(o => [o.key, o.label.trim()])

  if (options.length < 2) return

  let durationSecs: number | undefined
  switch (formDuration.value) {
    case '1m': durationSecs = 60; break
    case '3m': durationSecs = 180; break
    case '5m': durationSecs = 300; break
    case '10m': durationSecs = 600; break
    case '30m': durationSecs = 1800; break
    case 'custom': durationSecs = formCustomMinutes.value * 60; break
    default: durationSecs = undefined
  }

  await votingStore.createPoll(title, options, formKeyType.value, durationSecs)

  // 重置表单
  formTitle.value = ''
  formOptions.value = [
    { key: getNextKey(0), label: '' },
    { key: getNextKey(1), label: '' },
  ]
  showCreateForm.value = false
}

// ==================== 排序 ====================

const sortByVotes = ref(false)

const sortedOptions = (options: PollOption[]): PollOption[] => {
  if (!sortByVotes.value) return options
  return [...options].sort((a, b) => b.vote_count - a.vote_count)
}

// ==================== 投票者查看 ====================

const viewingVoters = ref<{ pollId: string; optionKey: string; voters: Voter[] } | null>(null)
const loadingVoters = ref(false)

const showVoters = async (pollId: string, optionKey: string) => {
  // 点击同一选项则关闭
  if (viewingVoters.value?.pollId === pollId && viewingVoters.value?.optionKey === optionKey) {
    viewingVoters.value = null
    return
  }
  loadingVoters.value = true
  try {
    const voters = await votingStore.getVoters(pollId, optionKey)
    viewingVoters.value = { pollId, optionKey, voters }
  } catch (e) {
    console.error('[Voting] Failed to load voters:', e)
  } finally {
    loadingVoters.value = false
  }
}

// ==================== 倒计时 ====================

let countdownTimer: ReturnType<typeof setInterval> | null = null
const now = ref(Date.now())

onMounted(() => {
  countdownTimer = setInterval(() => {
    now.value = Date.now()
  }, 1000)
})

onUnmounted(() => {
  if (countdownTimer) {
    clearInterval(countdownTimer)
  }
})

const formatCountdown = (endAt: number): string => {
  const remaining = Math.max(0, endAt - now.value)
  const totalSecs = Math.ceil(remaining / 1000)
  const m = Math.floor(totalSecs / 60)
  const s = totalSecs % 60
  return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`
}

// ==================== 辅助函数 ====================

const getMaxVoteCount = (poll: Poll): number => {
  return Math.max(1, ...poll.options.map(o => o.vote_count))
}

const getPercentage = (count: number, total: number): string => {
  if (total <= 0) return '0'
  return ((count / total) * 100).toFixed(1)
}

const formatTime = (ts: number): string => {
  const d = new Date(ts)
  const pad = (n: number) => n.toString().padStart(2, '0')
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`
}
</script>

<template>
  <div class="voting-tab">
    <!-- 工具栏 -->
    <div class="ext-toolbar">
      <button
        class="ext-btn"
        :class="showCreateForm ? 'ext-btn--danger' : 'ext-btn--primary'"
        @click="showCreateForm = !showCreateForm"
      >
        {{ showCreateForm ? '取消' : '✦ 创建投票' }}
      </button>
      <div class="toolbar-right">
        <button
          class="ext-btn"
          :class="{ 'ext-btn--active': sortByVotes }"
          @click="sortByVotes = !sortByVotes"
        >
          按票数排序
        </button>
      </div>
    </div>

    <div class="ext-list">
      <!-- 创建表单 -->
      <Transition name="ext-expand">
        <div v-if="showCreateForm" class="create-form">
          <div class="ext-form-group">
            <label>标题</label>
            <input
              v-model="formTitle"
              class="ext-input"
              placeholder="输入投票标题"
              @keydown.enter="submitCreate"
            />
          </div>

          <div class="ext-form-group">
            <label>选项</label>
            <div class="options-list">
              <TransitionGroup name="ext-list">
                <div v-for="(opt, index) in formOptions" :key="opt.key + index" class="option-row">
                  <span class="option-key">{{ opt.key }}</span>
                  <input
                    v-model="opt.label"
                    class="ext-input option-input"
                    :placeholder="`选项 ${opt.key} 描述`"
                  />
                  <button
                    v-if="formOptions.length > 2"
                    class="ext-icon-btn ext-icon-btn--danger"
                    @click="removeOption(index)"
                  >
                    ✕
                  </button>
                </div>
              </TransitionGroup>
              <button class="add-option-btn" @click="addOption">+ 添加选项</button>
            </div>
          </div>

          <div class="ext-form-row">
            <div class="ext-form-group">
              <label>类型</label>
              <ExtSelect
                :model-value="formKeyType"
                :options="keyTypeOptions"
                @update:model-value="v => { formKeyType = v as VoteKeyType; onKeyTypeChange() }"
              />
            </div>
            <div class="ext-form-group">
              <label>时长</label>
              <ExtSelect
                v-model="formDuration"
                :options="durationOptions"
              />
            </div>
            <div v-if="formDuration === 'custom'" class="ext-form-group">
              <label>分钟</label>
              <input
                v-model.number="formCustomMinutes"
                type="number"
                class="ext-input ext-number-input"
                min="1"
                max="1440"
              />
            </div>
          </div>

          <button class="ext-btn ext-btn--submit" @click="submitCreate">开始投票</button>
        </div>
      </Transition>

      <!-- 空状态 -->
      <div
        v-if="votingStore.polls.length === 0 && !showCreateForm"
        class="ext-empty"
      >
        <div class="ext-empty__icon">📊</div>
        <div class="ext-empty__title">暂无投票</div>
        <div class="ext-empty__hint">点击「创建投票」发起弹幕投票</div>
      </div>

      <!-- 进行中的投票 -->
      <template v-if="votingStore.activePolls.length > 0">
        <div class="ext-divider">
          <span>进行中 ({{ votingStore.activePolls.length }})</span>
        </div>
        <TransitionGroup name="ext-list">
          <div v-for="poll in votingStore.activePolls" :key="poll.id" class="ext-card poll-card active">
            <div class="poll-header">
              <div class="poll-title">{{ poll.title }}</div>
              <div class="poll-actions">
                <button class="ext-btn ext-btn--warning" @click="votingStore.endPoll(poll.id)">结束</button>
                <button class="ext-btn ext-btn--danger" @click="votingStore.deletePoll(poll.id)">删除</button>
              </div>
            </div>
            <div class="poll-meta">
              <span v-if="poll.end_at" class="countdown">
                ⏱ {{ formatCountdown(poll.end_at) }}
              </span>
              <span v-else class="no-limit">∞ 不限时</span>
              <span class="total-votes">{{ poll.total_votes }} 票</span>
            </div>
            <div class="poll-options">
              <div
                v-for="option in sortedOptions(poll.options)"
                :key="option.key"
                class="option-bar"
                @click="showVoters(poll.id, option.key)"
              >
                <div class="option-info">
                  <span class="option-key-badge">{{ option.key }}</span>
                  <span class="option-label">{{ option.label }}</span>
                </div>
                <div class="bar-container">
                  <div
                    class="bar-fill"
                    :style="{ width: `${(option.vote_count / getMaxVoteCount(poll)) * 100}%` }"
                  />
                </div>
                <div class="option-count">
                  {{ option.vote_count }}
                  <span class="option-percent">({{ getPercentage(option.vote_count, poll.total_votes) }}%)</span>
                </div>
              </div>
            </div>
            <!-- 投票者列表 -->
            <Transition name="ext-expand">
              <div
                v-if="viewingVoters?.pollId === poll.id"
                class="voters-panel"
              >
                <div class="voters-header">
                  选项「{{ viewingVoters.optionKey }}」的投票者
                  <button class="ext-icon-btn ext-icon-btn--close" @click="viewingVoters = null">✕</button>
                </div>
                <div v-if="loadingVoters" class="voters-loading">加载中...</div>
                <div v-else-if="viewingVoters.voters.length === 0" class="voters-empty">暂无投票者</div>
                <div v-else class="voters-list">
                  <div v-for="voter in viewingVoters.voters" :key="voter.uid" class="voter-item ext-animate-fade">
                    <span class="voter-name">{{ voter.username }}</span>
                    <span class="voter-time">{{ formatTime(voter.timestamp) }}</span>
                  </div>
                </div>
              </div>
            </Transition>
          </div>
        </TransitionGroup>
      </template>

      <!-- 已结束的投票 -->
      <template v-if="votingStore.endedPolls.length > 0">
        <div class="ext-divider">
          <span>已结束 ({{ votingStore.endedPolls.length }})</span>
        </div>
        <TransitionGroup name="ext-list">
          <div v-for="poll in votingStore.endedPolls" :key="poll.id" class="ext-card ext-card--dimmed poll-card ended">
            <div class="poll-header">
              <div class="poll-title">{{ poll.title }}</div>
              <div class="poll-actions">
                <button class="ext-btn ext-btn--danger" @click="votingStore.deletePoll(poll.id)">删除</button>
              </div>
            </div>
            <div class="poll-meta">
              <span class="ended-label">已结束</span>
              <span class="total-votes">{{ poll.total_votes }} 票</span>
            </div>
            <div class="poll-options">
              <div
                v-for="option in sortedOptions(poll.options)"
                :key="option.key"
                class="option-bar"
                @click="showVoters(poll.id, option.key)"
              >
                <div class="option-info">
                  <span class="option-key-badge ended">{{ option.key }}</span>
                  <span class="option-label">{{ option.label }}</span>
                </div>
                <div class="bar-container">
                  <div
                    class="bar-fill ended"
                    :style="{ width: `${(option.vote_count / getMaxVoteCount(poll)) * 100}%` }"
                  />
                </div>
                <div class="option-count">
                  {{ option.vote_count }}
                  <span class="option-percent">({{ getPercentage(option.vote_count, poll.total_votes) }}%)</span>
                </div>
              </div>
            </div>
            <!-- 投票者列表 -->
            <Transition name="ext-expand">
              <div
                v-if="viewingVoters?.pollId === poll.id"
                class="voters-panel"
              >
                <div class="voters-header">
                  选项「{{ viewingVoters.optionKey }}」的投票者
                  <button class="ext-icon-btn ext-icon-btn--close" @click="viewingVoters = null">✕</button>
                </div>
                <div v-if="loadingVoters" class="voters-loading">加载中...</div>
                <div v-else-if="viewingVoters.voters.length === 0" class="voters-empty">暂无投票者</div>
                <div v-else class="voters-list">
                  <div v-for="voter in viewingVoters.voters" :key="voter.uid" class="voter-item ext-animate-fade">
                    <span class="voter-name">{{ voter.username }}</span>
                    <span class="voter-time">{{ formatTime(voter.timestamp) }}</span>
                  </div>
                </div>
              </div>
            </Transition>
          </div>
        </TransitionGroup>
      </template>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/extension-shared.scss';

.voting-tab {
  display: flex;
  flex-direction: column;
  height: 100%;
}

// ==================== 创建表单 ====================

.create-form {
  background: var(--bg-card);
  border-radius: var(--border-radius);
  padding: 12px;
  margin-bottom: 8px;
  border: 1px solid var(--border-color);
}

.options-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.option-row {
  display: flex;
  align-items: center;
  gap: 6px;

  .option-key {
    width: 20px;
    text-align: center;
    font-weight: 600;
    font-size: var(--font-size-sm);
    color: var(--accent-primary);
    flex-shrink: 0;
  }

  .option-input {
    flex: 1;
  }
}

.add-option-btn {
  padding: 5px 8px;
  border: 1px dashed var(--border-color);
  border-radius: var(--border-radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: var(--font-size-xs);
  cursor: pointer;
  margin-top: 2px;
  transition: border-color 0.2s, color 0.2s, background 0.2s;

  &:hover {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
    background: rgba(92, 158, 255, 0.05);
  }
}

// ==================== 投票卡片 ====================

.poll-card {
  &.active {
    border-color: rgba(92, 158, 255, 0.15);
  }
}

.poll-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 6px;
}

.poll-title {
  font-size: var(--content-font-size-base);
  font-weight: 600;
  color: var(--text-primary);
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.poll-actions {
  display: flex;
  gap: 4px;
  flex-shrink: 0;
  margin-left: 8px;
}

.poll-meta {
  display: flex;
  gap: 12px;
  font-size: var(--content-font-size-xs);
  color: var(--text-secondary);
  margin-bottom: 8px;

  .countdown {
    color: var(--accent-primary);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }

  .ended-label {
    color: var(--text-muted);
  }

  .no-limit {
    color: var(--text-muted);
  }

  .total-votes {
    margin-left: auto;
    font-variant-numeric: tabular-nums;
  }
}

// ==================== 选项柱状图 ====================

.poll-options {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.option-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  padding: 4px 6px;
  border-radius: var(--border-radius-sm);
  transition: background 0.15s;

  &:hover {
    background: var(--bg-hover);
  }
}

.option-info {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 60px;
  flex-shrink: 0;
}

.option-key-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border-radius: var(--border-radius-sm);
  background: rgba(92, 158, 255, 0.15);
  color: var(--accent-primary);
  font-size: 11px;
  font-weight: 600;
  flex-shrink: 0;
  transition: background 0.2s, color 0.2s;

  &.ended {
    background: rgba(107, 107, 107, 0.15);
    color: var(--text-muted);
  }
}

.option-label {
  font-size: var(--content-font-size-sm);
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 80px;
}

.bar-container {
  flex: 1;
  height: 18px;
  background: var(--bg-hover);
  border-radius: 4px;
  overflow: hidden;
  min-width: 40px;
}

.bar-fill {
  height: 100%;
  background: linear-gradient(90deg, var(--accent-primary), #7ab4ff);
  border-radius: 4px;
  transition: width 0.4s cubic-bezier(0.25, 0.8, 0.25, 1);
  min-width: 2px;
  animation: extBarGrow 0.5s ease-out;

  &.ended {
    background: linear-gradient(90deg, #555, #6b6b6b);
  }
}

@keyframes extBarGrow {
  from { width: 0; }
}

.option-count {
  font-size: var(--content-font-size-xs);
  color: var(--text-primary);
  font-weight: 500;
  min-width: 60px;
  text-align: right;
  flex-shrink: 0;
  font-variant-numeric: tabular-nums;

  .option-percent {
    color: var(--text-muted);
    font-weight: 400;
  }
}

// ==================== 投票者面板 ====================

.voters-panel {
  margin-top: 8px;
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  padding: 8px;
  max-height: 200px;
  overflow-y: auto;
}

.voters-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: var(--content-font-size-xs);
  color: var(--text-secondary);
  margin-bottom: 6px;
  padding-bottom: 4px;
  border-bottom: 1px solid var(--border-color);
}

.voters-loading,
.voters-empty {
  text-align: center;
  font-size: var(--content-font-size-xs);
  color: var(--text-muted);
  padding: 8px;
}

.voters-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.voter-item {
  display: flex;
  justify-content: space-between;
  font-size: var(--content-font-size-xs);
  padding: 2px 4px;
  border-radius: 2px;
  transition: background 0.15s;

  &:hover {
    background: var(--bg-hover);
  }

  .voter-name {
    color: var(--text-primary);
  }

  .voter-time {
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
}
</style>

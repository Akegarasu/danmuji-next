<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useVotingStore } from '@/stores/voting'
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
    <div class="toolbar">
      <button class="tool-btn primary" @click="showCreateForm = !showCreateForm">
        {{ showCreateForm ? '取消' : '创建投票' }}
      </button>
      <div class="toolbar-right">
        <button
          class="tool-btn"
          :class="{ active: sortByVotes }"
          @click="sortByVotes = !sortByVotes"
        >
          按票数排序
        </button>
      </div>
    </div>

    <div class="list-container">
      <!-- 创建表单 -->
      <div v-if="showCreateForm" class="create-form">
        <div class="form-group">
          <label>标题</label>
          <input
            v-model="formTitle"
            class="form-input"
            placeholder="输入投票标题"
            @keydown.enter="submitCreate"
          />
        </div>

        <div class="form-group">
          <label>选项</label>
          <div class="options-list">
            <div v-for="(opt, index) in formOptions" :key="index" class="option-row">
              <span class="option-key">{{ opt.key }}</span>
              <input
                v-model="opt.label"
                class="form-input option-input"
                :placeholder="`选项 ${opt.key} 描述`"
              />
              <button
                v-if="formOptions.length > 2"
                class="option-remove"
                @click="removeOption(index)"
              >
                x
              </button>
            </div>
            <button class="add-option-btn" @click="addOption">+ 添加选项</button>
          </div>
        </div>

        <div class="form-row">
          <div class="form-group">
            <label>类型</label>
            <select v-model="formKeyType" class="form-select" @change="onKeyTypeChange">
              <option value="letter">字母 (A/B/C)</option>
              <option value="number">数字 (1/2/3)</option>
            </select>
          </div>
          <div class="form-group">
            <label>时长</label>
            <select v-model="formDuration" class="form-select">
              <option value="none">不限时</option>
              <option value="1m">1 分钟</option>
              <option value="3m">3 分钟</option>
              <option value="5m">5 分钟</option>
              <option value="10m">10 分钟</option>
              <option value="30m">30 分钟</option>
              <option value="custom">自定义</option>
            </select>
          </div>
          <div v-if="formDuration === 'custom'" class="form-group">
            <label>分钟</label>
            <input
              v-model.number="formCustomMinutes"
              type="number"
              class="form-input"
              min="1"
              max="1440"
              style="width: 70px"
            />
          </div>
        </div>

        <button class="submit-btn" @click="submitCreate">开始投票</button>
      </div>

      <!-- 空状态 -->
      <div
        v-if="votingStore.polls.length === 0 && !showCreateForm"
        class="empty-state"
      >
        <div class="empty-text">暂无投票</div>
        <div class="empty-hint">点击「创建投票」发起弹幕投票</div>
      </div>

      <!-- 进行中的投票 -->
      <template v-if="votingStore.activePolls.length > 0">
        <div class="section-divider">
          <span>进行中 ({{ votingStore.activePolls.length }})</span>
        </div>
        <div v-for="poll in votingStore.activePolls" :key="poll.id" class="poll-card active">
          <div class="poll-header">
            <div class="poll-title">{{ poll.title }}</div>
            <div class="poll-actions">
              <button class="action-btn end-btn" @click="votingStore.endPoll(poll.id)">结束</button>
              <button class="action-btn delete-btn" @click="votingStore.deletePoll(poll.id)">删除</button>
            </div>
          </div>
          <div class="poll-meta">
            <span v-if="poll.end_at" class="countdown">
              剩余 {{ formatCountdown(poll.end_at) }}
            </span>
            <span v-else class="no-limit">不限时</span>
            <span class="total-votes">总票数: {{ poll.total_votes }}</span>
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
          <div
            v-if="viewingVoters?.pollId === poll.id"
            class="voters-panel"
          >
            <div class="voters-header">
              选项「{{ viewingVoters.optionKey }}」的投票者
              <button class="voters-close" @click="viewingVoters = null">x</button>
            </div>
            <div v-if="loadingVoters" class="voters-loading">加载中...</div>
            <div v-else-if="viewingVoters.voters.length === 0" class="voters-empty">暂无投票者</div>
            <div v-else class="voters-list">
              <div v-for="voter in viewingVoters.voters" :key="voter.uid" class="voter-item">
                <span class="voter-name">{{ voter.username }}</span>
                <span class="voter-time">{{ formatTime(voter.timestamp) }}</span>
              </div>
            </div>
          </div>
        </div>
      </template>

      <!-- 已结束的投票 -->
      <template v-if="votingStore.endedPolls.length > 0">
        <div class="section-divider">
          <span>已结束 ({{ votingStore.endedPolls.length }})</span>
        </div>
        <div v-for="poll in votingStore.endedPolls" :key="poll.id" class="poll-card ended">
          <div class="poll-header">
            <div class="poll-title">{{ poll.title }}</div>
            <div class="poll-actions">
              <button class="action-btn delete-btn" @click="votingStore.deletePoll(poll.id)">删除</button>
            </div>
          </div>
          <div class="poll-meta">
            <span class="ended-label">已结束</span>
            <span class="total-votes">总票数: {{ poll.total_votes }}</span>
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
          <div
            v-if="viewingVoters?.pollId === poll.id"
            class="voters-panel"
          >
            <div class="voters-header">
              选项「{{ viewingVoters.optionKey }}」的投票者
              <button class="voters-close" @click="viewingVoters = null">x</button>
            </div>
            <div v-if="loadingVoters" class="voters-loading">加载中...</div>
            <div v-else-if="viewingVoters.voters.length === 0" class="voters-empty">暂无投票者</div>
            <div v-else class="voters-list">
              <div v-for="voter in viewingVoters.voters" :key="voter.uid" class="voter-item">
                <span class="voter-name">{{ voter.username }}</span>
                <span class="voter-time">{{ formatTime(voter.timestamp) }}</span>
              </div>
            </div>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped lang="scss">
.voting-tab {
  display: flex;
  flex-direction: column;
  height: 100%;
}

// ==================== 工具栏 ====================

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;

  .toolbar-right {
    display: flex;
    gap: 4px;
  }
}

.tool-btn {
  padding: 3px 10px;
  border: none;
  border-radius: var(--border-radius-sm);
  background: var(--bg-hover);
  color: var(--text-secondary);
  font-size: var(--font-size-xs);
  cursor: pointer;
  transition: background 0.15s, color 0.15s;

  &:hover {
    background: var(--bg-active);
    color: var(--text-primary);
  }

  &.primary {
    background: var(--accent-primary);
    color: white;

    &:hover {
      filter: brightness(1.1);
    }
  }

  &.active {
    background: rgba(92, 158, 255, 0.2);
    color: var(--accent-primary);
  }
}

// ==================== 列表容器 ====================

.list-container {
  flex: 1;
  overflow-y: auto;
  padding: 8px;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 200px;
  color: var(--text-muted);

  .empty-text {
    font-size: var(--content-font-size-base);
    margin-bottom: 4px;
  }

  .empty-hint {
    font-size: var(--content-font-size-xs);
  }
}

// ==================== 创建表单 ====================

.create-form {
  background: var(--bg-card);
  border-radius: var(--border-radius);
  padding: 12px;
  margin-bottom: 8px;
}

.form-group {
  margin-bottom: 8px;

  label {
    display: block;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    margin-bottom: 4px;
  }
}

.form-input {
  width: 100%;
  padding: 5px 8px;
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  background: var(--bg-primary);
  color: var(--text-primary);
  font-size: var(--font-size-sm);
  outline: none;
  box-sizing: border-box;

  &:focus {
    border-color: var(--accent-primary);
  }
}

.form-select {
  padding: 5px 8px;
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius-sm);
  background: var(--bg-primary);
  color: var(--text-primary);
  font-size: var(--font-size-sm);
  outline: none;

  &:focus {
    border-color: var(--accent-primary);
  }
}

.form-row {
  display: flex;
  gap: 8px;
  align-items: flex-end;
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

  .option-remove {
    width: 22px;
    height: 22px;
    border: none;
    border-radius: var(--border-radius-sm);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;

    &:hover {
      background: rgba(220, 60, 60, 0.2);
      color: #dc3c3c;
    }
  }
}

.add-option-btn {
  padding: 4px 8px;
  border: 1px dashed var(--border-color);
  border-radius: var(--border-radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: var(--font-size-xs);
  cursor: pointer;
  margin-top: 2px;

  &:hover {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }
}

.submit-btn {
  width: 100%;
  padding: 6px;
  border: none;
  border-radius: var(--border-radius-sm);
  background: var(--accent-primary);
  color: white;
  font-size: var(--font-size-sm);
  font-weight: 500;
  cursor: pointer;
  margin-top: 4px;

  &:hover {
    filter: brightness(1.1);
  }
}

// ==================== 分隔线 ====================

.section-divider {
  display: flex;
  align-items: center;
  margin: 12px 0 8px;
  font-size: var(--content-font-size-xs);
  color: var(--text-muted);

  &::before,
  &::after {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--border-color);
  }

  span {
    padding: 0 8px;
  }
}

// ==================== 投票卡片 ====================

.poll-card {
  background: var(--bg-card);
  border-radius: var(--border-radius);
  padding: 10px 12px;
  margin-bottom: 8px;

  &.ended {
    opacity: 0.6;

    &:hover {
      opacity: 0.8;
    }
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

.action-btn {
  padding: 2px 8px;
  border: none;
  border-radius: var(--border-radius-sm);
  background: var(--bg-hover);
  color: var(--text-secondary);
  font-size: var(--font-size-xs);
  cursor: pointer;
  transition: background 0.15s, color 0.15s;

  &:hover {
    background: var(--bg-active);
    color: var(--text-primary);
  }

  &.end-btn:hover {
    background: rgba(255, 160, 40, 0.2);
    color: #ffa028;
  }

  &.delete-btn:hover {
    background: rgba(220, 60, 60, 0.2);
    color: #dc3c3c;
  }
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
  }

  .ended-label {
    color: var(--text-muted);
  }

  .no-limit {
    color: var(--text-muted);
  }
}

// ==================== 选项柱状图 ====================

.poll-options {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.option-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  padding: 3px 0;
  border-radius: var(--border-radius-sm);

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
  border-radius: 3px;
  overflow: hidden;
  min-width: 40px;
}

.bar-fill {
  height: 100%;
  background: var(--accent-primary);
  border-radius: 3px;
  transition: width 0.3s ease;
  min-width: 2px;

  &.ended {
    background: var(--text-muted);
  }
}

.option-count {
  font-size: var(--content-font-size-xs);
  color: var(--text-primary);
  font-weight: 500;
  min-width: 60px;
  text-align: right;
  flex-shrink: 0;

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

.voters-close {
  border: none;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  font-size: 12px;
  padding: 0 4px;

  &:hover {
    color: var(--text-primary);
  }
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
  padding: 2px 0;

  .voter-name {
    color: var(--text-primary);
  }

  .voter-time {
    color: var(--text-muted);
  }
}
</style>

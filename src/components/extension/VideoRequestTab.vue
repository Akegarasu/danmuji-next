<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useVideoRequestStore } from '@/stores/video-request'
import { invoke } from '@tauri-apps/api/core'

const videoStore = useVideoRequestStore()

// 挂载时加载持久化的点播数据
onMounted(() => {
  videoStore.loadPersistedRequests()
})

const unwatchedList = computed(() => videoStore.unwatchedRequests)
const watchedList = computed(() => videoStore.watchedRequests)

/** 格式化播放量 */
const formatView = (view: number): string => {
  if (view >= 100_000_000) return `${(view / 100_000_000).toFixed(1)}亿`
  if (view >= 10000) return `${(view / 10000).toFixed(1)}万`
  return String(view)
}

/** 格式化时长 */
const formatDuration = (seconds: number): string => {
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  const s = seconds % 60
  const pad = (n: number) => n.toString().padStart(2, '0')
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`
}

/** 格式化时间 */
const formatTime = (ts: number): string => {
  const d = new Date(ts)
  const pad = (n: number) => n.toString().padStart(2, '0')
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** 格式化 SC 价格 */
const formatScPrice = (battery: number): string => {
  const rmb = battery / 10
  return `¥${rmb}`
}

const openVideo = async (bvid: string) => {
  const url = `https://www.bilibili.com/video/${bvid}`
  await invoke('open_url', { url })
}
</script>

<template>
  <div class="video-request-tab">
    <!-- 工具栏 -->
    <div class="ext-toolbar">
      <span class="count">
        <span class="label">待看</span>
        <span class="ext-badge ext-badge--primary">{{ videoStore.unwatchedCount }}</span>
      </span>
      <div class="toolbar-actions">
        <button class="ext-btn" @click="videoStore.clearWatched" title="清除已看">
          清除已看
        </button>
        <button class="ext-btn ext-btn--danger" @click="videoStore.clearAll" title="清空全部">
          清空
        </button>
      </div>
    </div>

    <!-- 列表 -->
    <div class="ext-list">
      <div v-if="unwatchedList.length === 0 && watchedList.length === 0" class="ext-empty">
        <div class="ext-empty__icon">🎬</div>
        <div class="ext-empty__title">暂无点播请求</div>
        <div class="ext-empty__hint">观众在弹幕或 SC 中发送 BV/AV号 时会自动捕获</div>
      </div>

      <!-- 未看列表 -->
      <TransitionGroup name="ext-list">
        <div v-for="item in unwatchedList" :key="item.id" class="video-card">
          <div class="card-content">
            <!-- 封面 -->
            <div class="cover-wrapper" @click="item.video_info && openVideo(item.video_info.bvid)">
              <template v-if="item.loading">
                <div class="cover-placeholder loading">
                  <span class="spinner"></span>
                </div>
              </template>
              <template v-else-if="item.error">
                <div class="cover-placeholder error">
                  <span>✕</span>
                </div>
              </template>
              <template v-else-if="item.video_info">
                <img
                  :src="item.video_info.cover"
                  class="cover"
                  referrerpolicy="no-referrer"
                  crossorigin="anonymous"
                />
                <span class="duration">{{ formatDuration(item.video_info.duration) }}</span>
              </template>
            </div>

            <!-- 信息 -->
            <div class="info">
              <div class="video-title" :title="item.video_info?.title || item.video_id"
                @click="item.video_info && openVideo(item.video_info.bvid)">
                {{ item.video_info?.title || item.video_id }}
              </div>
              <div class="meta">
                <span v-if="item.video_info" class="views">▶ {{ formatView(item.video_info.view) }}</span>
                <span v-if="item.video_info" class="owner">{{ item.video_info.owner_name }}</span>
              </div>
              <div class="requester">
                <span class="ext-badge" :class="item.source === 'superchat' ? 'ext-badge--pink' : 'ext-badge--blue'">
                  {{ item.source === 'superchat' ? 'SC' : '弹幕' }}
                </span>
                <span v-if="item.source === 'superchat' && item.sc_price" class="sc-price">
                  {{ formatScPrice(item.sc_price) }}
                </span>
                <span class="username">{{ item.username }}</span>
                <span class="time">{{ formatTime(item.timestamp) }}</span>
              </div>
            </div>

            <!-- 操作 -->
            <div class="card-actions">
              <button class="ext-icon-btn ext-icon-btn--success" @click="videoStore.markWatched(item.id)" title="标记已看">
                ✓
              </button>
              <button class="ext-icon-btn ext-icon-btn--danger" @click="videoStore.removeRequest(item.id)" title="删除">
                ✕
              </button>
            </div>
          </div>
        </div>
      </TransitionGroup>

      <!-- 已看分隔线 -->
      <div v-if="watchedList.length > 0" class="ext-divider">
        <span>已看 ({{ watchedList.length }})</span>
      </div>

      <!-- 已看列表 -->
      <TransitionGroup name="ext-list">
        <div v-for="item in watchedList" :key="item.id" class="video-card watched">
          <div class="card-content">
            <div class="cover-wrapper" @click="item.video_info && openVideo(item.video_info.bvid)">
              <template v-if="item.video_info">
                <img
                  :src="item.video_info.cover"
                  class="cover"
                  referrerpolicy="no-referrer"
                  crossorigin="anonymous"
                />
                <span class="duration">{{ formatDuration(item.video_info.duration) }}</span>
              </template>
              <template v-else>
                <div class="cover-placeholder">
                  <span>🎬</span>
                </div>
              </template>
            </div>

            <div class="info">
              <div class="video-title" :title="item.video_info?.title || item.video_id"
                @click="item.video_info && openVideo(item.video_info.bvid)">
                {{ item.video_info?.title || item.video_id }}
              </div>
              <div class="requester">
                <span class="ext-badge" :class="item.source === 'superchat' ? 'ext-badge--pink' : 'ext-badge--blue'">
                  {{ item.source === 'superchat' ? 'SC' : '弹幕' }}
                </span>
                <span class="username">{{ item.username }}</span>
              </div>
            </div>

            <div class="card-actions">
              <button class="ext-icon-btn ext-icon-btn--primary" @click="videoStore.markWatched(item.id, false)" title="撤销">
                ↩
              </button>
              <button class="ext-icon-btn ext-icon-btn--danger" @click="videoStore.removeRequest(item.id)" title="删除">
                ✕
              </button>
            </div>
          </div>
        </div>
      </TransitionGroup>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/extension-shared.scss';

.video-request-tab {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.count {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--font-size-sm);
  color: var(--text-secondary);
}

// ==================== 视频卡片 ====================

.video-card {
  margin-bottom: 8px;
  background: var(--bg-card);
  border-radius: var(--border-radius);
  overflow: hidden;
  transition: opacity 0.25s, transform 0.25s, box-shadow 0.25s;
  border: 1px solid transparent;

  &:hover {
    border-color: rgba(92, 158, 255, 0.1);
  }

  &.watched {
    opacity: 0.45;

    &:hover {
      opacity: 0.7;
    }
  }

  .card-content {
    display: flex;
    gap: 8px;
    padding: 8px;
  }
}

// ==================== 封面 ====================

.cover-wrapper {
  position: relative;
  width: 120px;
  min-width: 120px;
  height: 68px;
  border-radius: var(--border-radius-sm);
  overflow: hidden;
  cursor: pointer;
  background: var(--bg-hover);
  transition: transform 0.2s;

  &:hover {
    transform: scale(1.03);
  }

  .cover {
    width: 100%;
    height: 100%;
    object-fit: cover;
    transition: filter 0.2s;
  }

  &:hover .cover {
    filter: brightness(1.1);
  }

  .duration {
    position: absolute;
    bottom: 3px;
    right: 3px;
    padding: 1px 5px;
    background: rgba(0, 0, 0, 0.75);
    color: white;
    font-size: 10px;
    border-radius: 3px;
    font-variant-numeric: tabular-nums;
    backdrop-filter: blur(4px);
  }
}

.cover-placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;
  color: var(--text-muted);

  &.loading .spinner {
    width: 20px;
    height: 20px;
    border: 2px solid var(--border-color);
    border-top-color: var(--accent-primary);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  &.error {
    color: #dc3c3c;
    font-size: 14px;
  }
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

// ==================== 信息 ====================

.info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;

  .video-title {
    font-size: var(--content-font-size-sm);
    color: var(--text-primary);
    line-height: 1.3;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    transition: color 0.15s;

    &:hover {
      color: var(--accent-primary);
    }
  }

  .meta {
    display: flex;
    gap: 8px;
    font-size: var(--content-font-size-xs);
    color: var(--text-muted);
  }

  .requester {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: var(--content-font-size-xs);
    color: var(--text-secondary);

    .sc-price {
      color: var(--accent-secondary);
      font-weight: 500;
    }

    .time {
      margin-left: auto;
      color: var(--text-muted);
      font-variant-numeric: tabular-nums;
    }
  }
}

// ==================== 操作按钮 ====================

.card-actions {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex-shrink: 0;
}
</style>

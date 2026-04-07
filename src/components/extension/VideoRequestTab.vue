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
    <div class="toolbar">
      <span class="count">
        <span class="label">待看</span>
        <span class="badge">{{ videoStore.unwatchedCount }}</span>
      </span>
      <div class="actions">
        <button class="tool-btn" @click="videoStore.clearWatched" title="清除已看">
          清除已看
        </button>
        <button class="tool-btn danger" @click="videoStore.clearAll" title="清空全部">
          清空
        </button>
      </div>
    </div>

    <!-- 列表 -->
    <div class="list-container">
      <div v-if="unwatchedList.length === 0 && watchedList.length === 0" class="empty-state">
        <div class="empty-icon">🎬</div>
        <div class="empty-text">暂无点播请求</div>
        <div class="empty-hint">观众在弹幕或 SC 中发送 BV/AV号 时会自动捕获</div>
      </div>

      <!-- 未看列表 -->
      <div v-for="item in unwatchedList" :key="item.id" class="video-card animate-fade-in">
        <div class="card-content">
          <!-- 封面 -->
          <div class="cover-wrapper" @click="item.video_info && openVideo(item.video_info.bvid)">
            <template v-if="item.loading">
              <div class="cover-placeholder loading">
                <span class="spinner">⏳</span>
              </div>
            </template>
            <template v-else-if="item.error">
              <div class="cover-placeholder error">
                <span>❌</span>
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
              <span class="source-badge" :class="item.source">
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
            <button class="action-btn watched-btn" @click="videoStore.markWatched(item.id)" title="标记已看">
              ✓
            </button>
            <button class="action-btn remove-btn" @click="videoStore.removeRequest(item.id)" title="删除">
              ✕
            </button>
          </div>
        </div>
      </div>

      <!-- 已看分隔线 -->
      <div v-if="watchedList.length > 0" class="watched-divider">
        <span>已看 ({{ watchedList.length }})</span>
      </div>

      <!-- 已看列表 -->
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
              <span class="source-badge" :class="item.source">
                {{ item.source === 'superchat' ? 'SC' : '弹幕' }}
              </span>
              <span class="username">{{ item.username }}</span>
            </div>
          </div>

          <div class="card-actions">
            <button class="action-btn undo-btn" @click="videoStore.markWatched(item.id, false)" title="撤销">
              ↩
            </button>
            <button class="action-btn remove-btn" @click="videoStore.removeRequest(item.id)" title="删除">
              ✕
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.video-request-tab {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;

  .count {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);

    .badge {
      background: var(--accent-primary);
      color: white;
      padding: 0 6px;
      border-radius: 10px;
      font-size: var(--font-size-xs);
      min-width: 18px;
      text-align: center;
    }
  }

  .actions {
    display: flex;
    gap: 4px;
  }

  .tool-btn {
    padding: 3px 8px;
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

    &.danger:hover {
      background: rgba(220, 60, 60, 0.2);
      color: #dc3c3c;
    }
  }
}

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

  .empty-icon {
    font-size: 32px;
    margin-bottom: 8px;
  }

  .empty-text {
    font-size: var(--content-font-size-base);
    margin-bottom: 4px;
  }

  .empty-hint {
    font-size: var(--content-font-size-xs);
    text-align: center;
    padding: 0 20px;
  }
}

.video-card {
  margin-bottom: 8px;
  background: var(--bg-card);
  border-radius: var(--border-radius);
  overflow: hidden;
  transition: opacity 0.2s;

  &.watched {
    opacity: 0.5;

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

.cover-wrapper {
  position: relative;
  width: 120px;
  min-width: 120px;
  height: 68px;
  border-radius: var(--border-radius-sm);
  overflow: hidden;
  cursor: pointer;
  background: var(--bg-hover);

  .cover {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .duration {
    position: absolute;
    bottom: 2px;
    right: 2px;
    padding: 1px 4px;
    background: rgba(0, 0, 0, 0.7);
    color: white;
    font-size: 10px;
    border-radius: 2px;
  }
}

.cover-placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;

  &.loading .spinner {
    animation: spin 1s linear infinite;
  }
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

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

    .source-badge {
      padding: 0 4px;
      border-radius: 2px;
      font-size: 10px;

      &.danmaku {
        background: rgba(92, 158, 255, 0.2);
        color: var(--accent-primary);
      }

      &.superchat {
        background: rgba(255, 126, 179, 0.2);
        color: var(--accent-secondary);
      }
    }

    .sc-price {
      color: var(--accent-secondary);
      font-weight: 500;
    }

    .time {
      margin-left: auto;
      color: var(--text-muted);
    }
  }
}

.card-actions {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex-shrink: 0;

  .action-btn {
    width: 24px;
    height: 24px;
    border: none;
    border-radius: var(--border-radius-sm);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.15s, color 0.15s;

    &:hover {
      background: var(--bg-hover);
    }

    &.watched-btn:hover {
      background: rgba(34, 197, 94, 0.2);
      color: #22c55e;
    }

    &.remove-btn:hover {
      background: rgba(220, 60, 60, 0.2);
      color: #dc3c3c;
    }

    &.undo-btn:hover {
      background: rgba(92, 158, 255, 0.2);
      color: var(--accent-primary);
    }
  }
}

.watched-divider {
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
</style>

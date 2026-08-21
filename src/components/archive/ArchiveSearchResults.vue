<script setup lang="ts">
import type { ArchiveSearchItem, PagedResult } from '@/types'
import { formatEventTime, formatPrice } from '@/types'

defineProps<{
  result: PagedResult<ArchiveSearchItem>
  loading: boolean
  showRoom?: boolean
}>()

defineEmits<{
  page: [page: number]
}>()

const labels = { danmaku: '弹幕', gift: '礼物', superchat: 'SC' } as const
const totalPages = (result: PagedResult<ArchiveSearchItem>) =>
  Math.max(1, Math.ceil(result.total / result.page_size))
</script>

<template>
  <div class="result-shell">
    <div v-if="loading" class="result-state">正在检索归档…</div>
    <div v-else-if="result.items.length === 0" class="result-state">没有匹配的归档记录</div>
    <div v-else class="result-list">
      <article v-for="item in result.items" :key="`${item.event_type}-${item.id}`" class="result-item" :class="item.event_type">
        <div class="type-mark">{{ labels[item.event_type] }}</div>
        <span class="event-time">{{ formatEventTime(item.timestamp) }}</span>
        <img
          v-if="item.image_url"
          :src="item.image_url"
          :alt="item.content"
          class="event-image"
          loading="lazy"
          referrerpolicy="no-referrer"
          crossorigin="anonymous"
        />
        <div class="event-user" :title="`UID ${item.user_uid}`">
          <span>{{ item.user_name || '未知用户' }}</span>
          <small>{{ item.user_uid }}</small>
        </div>
        <span class="separator">:</span>
        <div class="event-content">
          <span>{{ item.content || '（无内容）' }}</span>
          <small v-if="item.detail">{{ item.detail }}</small>
          <b v-if="item.quantity && item.event_type === 'gift'">×{{ item.quantity }}</b>
        </div>
        <span v-if="showRoom" class="room-meta" :title="`${item.room_title} · 房间 ${item.room_id}`">
          {{ item.room_title || `房间 ${item.room_id}` }}
        </span>
        <div v-if="item.amount" class="amount">{{ formatPrice(item.amount) }}</div>
      </article>
    </div>

    <div v-if="!loading && totalPages(result) > 1" class="pagination">
      <button :disabled="result.page <= 1" @click="$emit('page', result.page - 1)">上一页</button>
      <span>{{ result.page }} / {{ totalPages(result) }} · {{ result.total }} 条</span>
      <button :disabled="result.page >= totalPages(result)" @click="$emit('page', result.page + 1)">下一页</button>
    </div>
  </div>
</template>

<style scoped lang="scss">
.result-shell { min-height: 100px; }
.result-state { display: grid; min-height: 120px; place-items: center; color: var(--text-muted); font-size: var(--font-size-sm); }
.result-list { display: grid; gap: 2px; }

.result-item {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  min-height: 29px;
  padding: 3px 7px;
  border: 1px solid transparent;
  border-radius: var(--border-radius-sm);
  background: transparent;
  transition: border-color 0.15s, background 0.15s;

  &:nth-child(even) { background: rgba(255, 255, 255, 0.018); }
  &:hover { border-color: var(--border-color); background: var(--bg-hover); }
}

.type-mark {
  width: 34px;
  flex: 0 0 34px;
  color: var(--accent-primary);
  font-size: 10px;
  text-align: center;

  .gift & { color: var(--accent-secondary); }
  .superchat & { color: var(--accent-gold); }
}

.event-time { flex: 0 0 auto; color: var(--text-muted); font-family: monospace; font-size: 10px; }
.event-image { width: 22px; height: 22px; flex: 0 0 22px; border-radius: var(--border-radius-sm); object-fit: contain; }
.event-user { display: flex; max-width: 155px; min-width: 0; flex: 0 1 auto; align-items: baseline; gap: 4px; color: #adbcd9; font-size: var(--content-font-size-xs); white-space: nowrap; }
.event-user > span { overflow: hidden; text-overflow: ellipsis; }
.event-user small { color: var(--text-muted); font-size: 9px; }
.separator { color: var(--text-muted); }
.event-content { display: flex; align-items: baseline; gap: 6px; min-width: 40px; flex: 1; color: var(--text-primary); font-size: var(--content-font-size-xs); }
.event-content > span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.event-content small { overflow: hidden; color: var(--text-muted); text-overflow: ellipsis; white-space: nowrap; }
.event-content b { flex: 0 0 auto; color: var(--accent-secondary); }
.room-meta { max-width: 110px; overflow: hidden; color: var(--text-muted); font-size: 10px; text-overflow: ellipsis; white-space: nowrap; }
.amount { flex: 0 0 auto; color: var(--accent-gold); font-size: var(--font-size-xs); font-weight: 600; }

.pagination { display: flex; align-items: center; justify-content: center; gap: 12px; padding: 13px 0 3px; color: var(--text-secondary); font-size: var(--font-size-xs); }
.pagination button { padding: 5px 10px; border: 1px solid var(--border-color); border-radius: 4px; background: var(--bg-card); color: var(--text-primary); cursor: pointer; }
.pagination button:disabled { opacity: 0.35; cursor: default; }

@media (max-width: 620px) {
  .event-user small, .room-meta { display: none; }
  .event-user { max-width: 100px; }
}
</style>

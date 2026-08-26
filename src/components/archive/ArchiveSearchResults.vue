<script setup lang="ts">
import { computed, ref } from 'vue'
import type { ArchiveSearchItem, PagedResult } from '@/types'
import { formatPrice } from '@/types'

const props = withDefaults(defineProps<{
  result: PagedResult<ArchiveSearchItem>
  loading: boolean
  showRoom?: boolean
  error?: string
  emptyText?: string
}>(), {
  showRoom: false,
  error: '',
  emptyText: '没有匹配的归档记录',
})

const emit = defineEmits<{
  page: [page: number]
  retry: []
}>()

type ArchiveDateGroup = {
  key: string
  label: string
  items: ArchiveSearchItem[]
}

const shell = ref<HTMLElement | null>(null)
const labels = { danmaku: '弹幕', gift: '礼物', superchat: 'SC' } as const
const dayFormatter = new Intl.DateTimeFormat('zh-CN', {
  year: 'numeric',
  month: 'long',
  day: 'numeric',
  weekday: 'short',
})
const timeFormatter = new Intl.DateTimeFormat('zh-CN', {
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
  hour12: false,
})

const hasResults = computed(() => props.result.items.length > 0)
const totalPages = computed(() =>
  Math.max(1, Math.ceil(props.result.total / props.result.page_size))
)

const toDate = (timestamp: number) =>
  new Date(timestamp < 1_000_000_000_000 ? timestamp * 1000 : timestamp)
const padTwoDigits = (value: number) => String(value).padStart(2, '0')

const dayKey = (timestamp: number) => {
  const date = toDate(timestamp)
  return `${date.getFullYear()}-${padTwoDigits(date.getMonth() + 1)}-${padTwoDigits(date.getDate())}`
}

const formatDay = (timestamp: number) => dayFormatter.format(toDate(timestamp))
const formatFullTime = (timestamp: number) => timeFormatter.format(toDate(timestamp))

const dateGroups = computed<ArchiveDateGroup[]>(() => {
  const groupedItems: ArchiveDateGroup[] = []
  for (const item of props.result.items) {
    const key = dayKey(item.timestamp)
    const currentGroup = groupedItems[groupedItems.length - 1]
    if (currentGroup?.key === key) currentGroup.items.push(item)
    else groupedItems.push({ key, label: formatDay(item.timestamp), items: [item] })
  }
  return groupedItems
})

const goPage = (page: number) => {
  emit('page', page)
  shell.value?.scrollIntoView({ behavior: 'smooth', block: 'start' })
}
</script>

<template>
  <div ref="shell" class="result-shell" :aria-busy="loading">
    <div v-if="error && !hasResults" class="result-state error-state">
      <span>归档检索失败</span>
      <small>{{ error }}</small>
      <button @click="$emit('retry')">重新检索</button>
    </div>

    <div v-else-if="loading && !hasResults" class="result-state loading-state">
      <i class="result-spinner" aria-hidden="true" />
      <span>正在检索归档…</span>
    </div>

    <div v-else-if="!hasResults" class="result-state">
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M4 5h16v14H4zM8 3v4M16 3v4M4 9h16" />
      </svg>
      <span>{{ emptyText }}</span>
    </div>

    <div v-else class="results-content" :class="{ dimmed: loading }">
      <div v-if="loading" class="inline-loading"><i />正在更新结果…</div>
      <div v-if="error" class="inline-error">
        <span>{{ error }}</span>
        <button @click="$emit('retry')">重试</button>
      </div>

      <section v-for="group in dateGroups" :key="group.key" class="date-group">
        <header class="date-heading">
          <strong>{{ group.label }}</strong>
          <span>{{ group.items.length }} 条</span>
        </header>

        <div class="result-list">
          <article
            v-for="item in group.items"
            :key="`${item.event_type}-${item.id}`"
            class="result-item"
            :class="item.event_type"
          >
            <div class="type-mark">{{ labels[item.event_type] }}</div>
            <span class="event-time">{{ formatFullTime(item.timestamp) }}</span>
            <div class="event-image-slot">
              <img
                v-if="item.image_url"
                :src="item.image_url"
                :alt="item.content"
                class="event-image"
                loading="lazy"
                referrerpolicy="no-referrer"
                crossorigin="anonymous"
              />
            </div>

            <div class="event-main">
              <div class="event-line">
                <span class="event-user" :title="`${item.user_name || '未知用户'} · UID ${item.user_uid}`">
                  {{ item.user_name || '未知用户' }}
                </span>
                <span class="separator">：</span>
                <span class="event-content" :title="item.content || '（无内容）'">
                  {{ item.content || '（无内容）' }}
                </span>
                <b v-if="item.quantity && item.event_type === 'gift'" class="quantity">×{{ item.quantity }}</b>
              </div>
              <div v-if="item.detail || showRoom" class="event-detail">
                <span v-if="item.detail" :title="item.detail">{{ item.detail }}</span>
                <span v-if="showRoom" class="room-meta" :title="`${item.room_title} · 房间 ${item.room_id}`">
                  {{ item.room_title || `房间 ${item.room_id}` }} · #{{ item.room_id }}
                </span>
              </div>
            </div>

            <span v-if="item.event_type === 'gift' && !item.is_paid" class="free-mark">免费</span>
            <div v-if="item.amount && item.amount > 0" class="amount">{{ formatPrice(item.amount) }}</div>
          </article>
        </div>
      </section>
    </div>

    <div v-if="!loading && totalPages > 1" class="pagination">
      <button :disabled="result.page <= 1" @click="goPage(result.page - 1)">上一页</button>
      <span>第 {{ result.page }} / {{ totalPages }} 页 · 共 {{ result.total }} 条</span>
      <button :disabled="result.page >= totalPages" @click="goPage(result.page + 1)">下一页</button>
    </div>
  </div>
</template>

<style scoped lang="scss">
.result-shell { position: relative; min-height: 100px; scroll-margin-top: 64px; }
.result-state {
  display: flex;
  min-height: 128px;
  align-items: center;
  justify-content: center;
  flex-direction: column;
  gap: 7px;
  color: var(--text-muted);
  font-size: var(--font-size-sm);

  svg { width: 25px; height: 25px; fill: none; stroke: currentColor; stroke-width: 1.4; }
  small { max-width: 420px; color: #ef8a8a; text-align: center; word-break: break-word; }
  button { padding: 5px 10px; border: 1px solid var(--border-color); border-radius: 5px; background: var(--bg-card); color: var(--text-primary); cursor: pointer; }
}
.error-state > span { color: #ef8a8a; }
.loading-state { flex-direction: row; }
.result-spinner {
  width: 14px;
  height: 14px;
  border: 2px solid var(--border-color);
  border-top-color: var(--accent-primary);
  border-radius: 50%;
  animation: resultSpin 0.7s linear infinite;
}

.results-content { position: relative; transition: opacity 0.15s; }
.results-content.dimmed { opacity: 0.72; }
.inline-loading {
  position: sticky;
  z-index: 2;
  top: 0;
  display: flex;
  width: max-content;
  align-items: center;
  gap: 6px;
  margin: 0 auto 7px;
  padding: 4px 9px;
  border: 1px solid var(--border-color);
  border-radius: 12px;
  background: rgb(38, 38, 38);
  color: var(--text-secondary);
  font-size: 10px;

  i { width: 8px; height: 8px; border: 1px solid var(--text-muted); border-top-color: var(--accent-primary); border-radius: 50%; animation: resultSpin 0.7s linear infinite; }
}
.inline-error { display: flex; align-items: center; justify-content: center; gap: 8px; margin-bottom: 7px; color: #ef8a8a; font-size: var(--font-size-xs); }
.inline-error button { border: 0; background: transparent; color: var(--accent-primary); cursor: pointer; }

.date-group + .date-group { margin-top: 12px; }
.date-heading {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0 4px 5px;
  color: var(--text-secondary);

  strong { font-size: var(--font-size-xs); font-weight: 500; }
  span { color: var(--text-muted); font-size: 9px; }
  &::after { height: 1px; flex: 1; background: var(--border-color); content: ''; }
}
.result-list { display: grid; gap: 3px; }
.result-item {
  display: grid;
  grid-template-columns: 38px 58px 26px minmax(0, 1fr) auto;
  min-width: 0;
  min-height: 40px;
  align-items: center;
  column-gap: 7px;
  padding: 5px 8px;
  border: 1px solid transparent;
  border-radius: var(--border-radius-sm);
  background: transparent;
  transition: border-color 0.15s, background 0.15s;

  &:nth-child(even) { background: rgba(255, 255, 255, 0.018); }
  &:hover { border-color: var(--border-color); background: var(--bg-hover); }
}
.type-mark {
  display: grid;
  min-height: 20px;
  place-items: center;
  border-radius: 4px;
  background: rgba(92, 158, 255, 0.12);
  color: var(--accent-primary);
  font-size: 9px;

  .gift & { background: rgba(255, 126, 179, 0.12); color: var(--accent-secondary); }
  .superchat & { background: rgba(245, 200, 66, 0.12); color: var(--accent-gold); }
}
.event-time { color: var(--text-muted); font-family: monospace; font-size: 10px; }
.event-image-slot { display: grid; width: 26px; height: 26px; place-items: center; }
.event-image { width: 26px; height: 26px; border-radius: var(--border-radius-sm); object-fit: contain; }
.event-main { min-width: 0; }
.event-line { display: flex; min-width: 0; align-items: baseline; }
.event-user { max-width: 155px; flex: 0 1 auto; overflow: hidden; color: #adbcd9; font-size: var(--content-font-size-xs); text-overflow: ellipsis; white-space: nowrap; }
.separator { flex: 0 0 auto; color: var(--text-muted); }
.event-content { display: -webkit-box; min-width: 30px; overflow: hidden; color: var(--text-primary); font-size: var(--content-font-size-xs); line-height: 1.45; word-break: break-word; -webkit-box-orient: vertical; -webkit-line-clamp: 2; }
.quantity { flex: 0 0 auto; margin-left: 6px; color: var(--accent-secondary); font-size: var(--font-size-xs); }
.event-detail { display: flex; min-width: 0; gap: 8px; margin-top: 2px; color: var(--text-muted); font-size: 9px; }
.event-detail > span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.room-meta { flex: 0 1 auto; }
.free-mark { color: var(--text-muted); font-size: 9px; }
.amount { color: var(--accent-gold); font-size: var(--font-size-xs); font-weight: 600; }

.pagination { display: flex; align-items: center; justify-content: center; gap: 12px; padding: 13px 0 3px; color: var(--text-secondary); font-size: var(--font-size-xs); }
.pagination button { padding: 5px 10px; border: 1px solid var(--border-color); border-radius: 4px; background: var(--bg-card); color: var(--text-primary); cursor: pointer; }
.pagination button:hover:not(:disabled) { background: var(--bg-hover); }
.pagination button:disabled { opacity: 0.35; cursor: default; }

@keyframes resultSpin { to { transform: rotate(360deg); } }

@media (max-width: 700px) {
  .result-item { grid-template-columns: 34px 52px 26px minmax(0, 1fr) auto; column-gap: 5px; padding-right: 5px; padding-left: 5px; }
  .event-user { max-width: 105px; }
}

@media (max-width: 580px) {
  .result-item { grid-template-columns: 34px 48px minmax(0, 1fr) auto; }
  .event-image-slot { display: none; }
  .event-user { max-width: 85px; }
  .event-detail { display: block; }
  .event-detail > span { display: block; }
}
</style>

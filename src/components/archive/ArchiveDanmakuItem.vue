<script setup lang="ts">
import type { ArchivedDanmaku } from '@/types'
import { formatEventTime } from '@/types'

defineProps<{
  item: ArchivedDanmaku
}>()
</script>

<template>
  <div class="archive-danmaku-item">
    <span class="time">{{ formatEventTime(item.timestamp) }}</span>
    <span class="user-name">{{ item.user_name }}</span>
    <span class="separator">：</span>
    <template v-if="item.is_emoticon && item.emoticon_url">
      <img
        :src="item.emoticon_url + '@40h.webp'"
        :alt="item.content"
        class="emoticon"
        loading="lazy"
        referrerpolicy="no-referrer"
        crossorigin="anonymous"
      />
    </template>
    <span v-else class="content">{{ item.content }}</span>
  </div>
</template>

<style scoped lang="scss">
.archive-danmaku-item {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 0 0.35em;
  padding: 0.35em 0.6em;
  font-size: var(--font-size-sm);
  line-height: 1.6;
  word-break: break-all;
  border-radius: var(--border-radius-sm);
  transition: background 0.15s;

  &:hover {
    background: var(--bg-hover);
  }
}

.time {
  color: var(--text-muted);
  font-size: 0.85em;
  flex-shrink: 0;
}

.user-name {
  color: #adbcd9;
  font-weight: 500;
  flex-shrink: 0;
  max-width: 120px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.separator {
  color: var(--text-muted);
}

.content {
  color: var(--text-primary);
}

.emoticon {
  height: 32px;
  max-height: 32px;
  min-height: 18px;
  width: auto;
  vertical-align: middle;
  border-radius: 3px;
  object-fit: contain;
  flex-shrink: 0;
}
</style>

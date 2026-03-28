<script setup lang="ts">
import { computed } from 'vue'
import type { ArchivedSuperChat } from '@/types'
import { formatEventTime, formatPrice } from '@/types'

const props = defineProps<{
  item: ArchivedSuperChat
}>()

// 根据电池价格获取对应颜色（与 SuperChatItem 保持一致）
const scColor = computed(() => {
  const battery = props.item.price
  if (battery >= 20000) return '#ab1a32'
  if (battery >= 10000) return '#e54d4d'
  if (battery >= 5000) return '#e09443'
  if (battery >= 1000) return '#e2b52b'
  if (battery >= 500) return '#427d9e'
  return '#2a60b2'
})
</script>

<template>
  <div class="archive-sc-item" :style="{ '--sc-bg': scColor }">
    <div class="sc-header">
      <div class="user-info">
        <span class="username">{{ item.user_name }}</span>
        <span class="price">{{ formatPrice(item.price) }}</span>
      </div>
      <span class="send-time">{{ formatEventTime(item.start_time) }}</span>
    </div>
    <div v-if="item.content" class="sc-content">{{ item.content }}</div>
  </div>
</template>

<style scoped lang="scss">
.archive-sc-item {
  border-radius: var(--border-radius);
  overflow: hidden;
  margin-bottom: 6px;
}

.sc-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 7px 12px;
  background: var(--sc-bg);
  filter: brightness(0.75);
}

.user-info {
  display: flex;
  align-items: center;
  gap: 8px;
}

.username {
  color: white;
  font-weight: 500;
  font-size: var(--font-size-sm);
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
}

.price {
  color: white;
  font-size: var(--font-size-xs);
  font-weight: 600;
  background: rgba(0, 0, 0, 0.2);
  padding: 2px 8px;
  border-radius: var(--border-radius-sm);
}

.send-time {
  color: rgba(255, 255, 255, 0.6);
  font-size: var(--font-size-xs);
  font-family: monospace;
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
}

.sc-content {
  padding: 8px 12px;
  background: var(--sc-bg);
  color: white;
  font-size: var(--font-size-sm);
  line-height: 1.5;
  min-height: 32px;
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.2);
  word-break: break-all;
}
</style>

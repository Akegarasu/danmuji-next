<script setup lang="ts">
import type { ArchivedGift } from '@/types'
import { formatEventTime, formatPrice } from '@/types'
import { computed } from 'vue'

const props = defineProps<{
  item: ArchivedGift
}>()

const guardClass = computed(() => {
  if (!props.item.guard_level) return ''
  return `guard-${props.item.guard_level}`
})

const giftIconUrl = computed(() => {
  if (!props.item.gift_icon) return ''
  const url = props.item.gift_icon
  if (url.endsWith('.png')) return url + '@100w_100h.webp'
  return url
})
</script>

<template>
  <div class="archive-gift-item" :class="[{ paid: item.is_paid, 'is-guard': item.guard_level }, guardClass]">
    <div class="gift-icon-wrap" :class="guardClass">
      <img
        v-if="giftIconUrl"
        :src="giftIconUrl"
        :alt="item.gift_name"
        class="gift-icon-img"
        loading="lazy"
        referrerpolicy="no-referrer"
      />
      <span v-else class="gift-icon-placeholder">{{ item.guard_level ? '舰' : '礼' }}</span>
    </div>

    <div class="gift-info">
      <div class="user-line">
        <span class="time">{{ formatEventTime(item.timestamp) }}</span>
        <span class="username">{{ item.user_name }}</span>
      </div>
      <div class="gift-line">
        <span class="gift-name">{{ item.gift_name }}</span>
        <span class="gift-num">× {{ item.num }}</span>
      </div>
    </div>

    <div v-if="item.is_paid && item.total_value > 0" class="gift-price">
      {{ formatPrice(item.total_value) }}
    </div>
    <div v-else class="gift-free">免费</div>
  </div>
</template>

<style scoped lang="scss">
.archive-gift-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  background: var(--bg-card);
  border-radius: var(--border-radius);
  margin-bottom: 4px;
  transition: background 0.15s;

  &:hover {
    background: var(--bg-hover);
  }

  &.paid {
    background: rgba(245, 200, 66, 0.1);
    border-left: 3px solid var(--accent-gold);

    &:hover {
      background: rgba(245, 200, 66, 0.15);
    }
  }

  &.is-guard.guard-3 {
    background: rgba(0, 209, 241, 0.15);
    border-left: 3px solid #00d1f1;
    &:hover { background: rgba(0, 209, 241, 0.2); }
  }

  &.is-guard.guard-2 {
    background: rgba(225, 122, 255, 0.15);
    border-left: 3px solid #e17aff;
    &:hover { background: rgba(225, 122, 255, 0.2); }
  }

  &.is-guard.guard-1 {
    background: rgba(255, 149, 0, 0.15);
    border-left: 3px solid #ff9500;
    &:hover { background: rgba(255, 149, 0, 0.2); }
  }
}

.gift-icon-wrap {
  width: 32px;
  height: 32px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;

  .gift-icon-img {
    width: 100%;
    height: 100%;
    object-fit: contain;
    border-radius: var(--border-radius-sm);
  }

  .gift-icon-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--accent-secondary);
    border-radius: var(--border-radius-sm);
    font-size: 13px;
    font-weight: 500;
    color: white;
  }

  &.guard-3 .gift-icon-placeholder { background: #00d1f1; }
  &.guard-2 .gift-icon-placeholder { background: #e17aff; }
  &.guard-1 .gift-icon-placeholder { background: #ff9500; }
}

.gift-info {
  flex: 1;
  min-width: 0;
  overflow: hidden;
}

.user-line {
  display: flex;
  align-items: center;
  gap: 0.35em;
  margin-bottom: 1px;
  font-size: var(--content-font-size-xs);
}

.time {
  color: var(--text-muted);
  font-size: 0.9em;
  flex-shrink: 0;
}

.username {
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.gift-line {
  display: flex;
  align-items: center;
  gap: 4px;
}

.gift-name {
  color: var(--text-primary);
  font-size: var(--content-font-size-sm);
  font-weight: 500;
}

.gift-num {
  color: var(--accent-secondary);
  font-size: var(--content-font-size-sm);
  font-weight: 600;
}

.gift-price {
  color: var(--accent-gold);
  font-size: var(--content-font-size-sm);
  font-weight: 600;
  flex-shrink: 0;
}

.gift-free {
  color: var(--text-muted);
  font-size: var(--content-font-size-xs);
  flex-shrink: 0;
}
</style>

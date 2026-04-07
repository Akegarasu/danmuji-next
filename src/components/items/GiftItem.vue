<script setup lang="ts">
import { computed } from 'vue'
import type { ProcessedGift } from '@/types'
import { formatEventTime, formatPrice, getMedalGradient } from '@/types'
import { getGiftIcon, getGuardIcon } from '@/services/gift-icons'

const props = withDefaults(defineProps<{
  gift: ProcessedGift
  showTime?: boolean
  showMedal?: boolean
  isSpecialFollow?: boolean
  expired?: boolean
}>(), {
  showTime: true,
  showMedal: true,
  isSpecialFollow: false,
  expired: false
})

// 是否是大航海购买
const isGuardBuy = computed(() => !!props.gift.guard_level)

// 大航海等级样式类
const guardClass = computed(() => {
  if (!props.gift.guard_level) return ''
  return `guard-${props.gift.guard_level}`
})

// 大航海图标文字
const guardIcon = computed(() => {
  switch (props.gift.guard_level) {
    case 1: return '督'
    case 2: return '提'
    case 3: return '舰'
    default: return '礼'
  }
})

const giftIconUrl = computed(() => {
  if (props.gift.guard_level) {
    return getGuardIcon(props.gift.guard_level)
  }

  if (props.gift.gift_icon) {
    console.log("giftIconUrl from ws: ", props.gift.gift_icon)
    if (props.gift.gift_icon.endsWith('.webp')) {
      return props.gift.gift_icon
    } else if (props.gift.gift_icon.endsWith('.png')) {
      return props.gift.gift_icon + '@100w_100h.webp'
    } else {
      return props.gift.gift_icon
    }
  }

  return getGiftIcon(props.gift.gift_id, props.gift.gift_name)
})

const timeText = computed(() => formatEventTime(props.gift.timestamp))
</script>

<template>
  <div 
    class="gift-item animate-slide-in" 
    :class="[
      { paid: gift.is_paid && !expired },
      { 'is-guard': isGuardBuy && !expired },
      { 'is-special-follow': isSpecialFollow && !isGuardBuy && !expired },
      { expired },
      expired ? '' : guardClass
    ]"
  >
    <div class="gift-icon" :class="guardClass">
      <img 
        v-if="giftIconUrl" 
        :src="giftIconUrl" 
        :alt="gift.gift_name"
        class="gift-icon-img"
        loading="lazy"
        referrerpolicy="no-referrer"
      />
      <span v-else class="gift-icon-placeholder">{{ isGuardBuy ? guardIcon : '礼' }}</span>
    </div>
    
    <div class="gift-info">
      <div class="user-line">
        <span v-if="showTime && timeText" class="time">{{ timeText }}</span>
        <span 
          v-if="showMedal && gift.user.medal" 
          class="medal"
          :style="{ backgroundImage: getMedalGradient(gift.user.medal.level) }"
        >
          {{ gift.user.medal.name }}
        </span>
        <span class="username">{{ gift.user.name }}</span>
      </div>
      
      <div class="gift-line">
        <span class="gift-name">{{ gift.gift_name }}</span>
        <span class="gift-num">× {{ isGuardBuy ? `${gift.num}个月` : gift.num }}</span>
      </div>
    </div>
    
    <div v-if="gift.is_paid && gift.total_value > 0" class="gift-price">
      {{ formatPrice(gift.total_value) }}
    </div>
    <div v-else class="gift-free">
      免费
    </div>
  </div>
</template>

<style scoped lang="scss">
.gift-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  background: var(--bg-card);
  border-radius: var(--border-radius);
  margin-bottom: 6px;
  transition: background 0.15s;
  cursor: default;
  
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
  
  // 大航海购买 - 舰长（青色）
  &.is-guard.guard-3 {
    background: rgba(0, 209, 241, 0.15);
    border-left: 3px solid #00d1f1;
    
    &:hover {
      background: rgba(0, 209, 241, 0.2);
    }
  }
  
  // 大航海购买 - 提督（紫色）
  &.is-guard.guard-2 {
    background: rgba(225, 122, 255, 0.15);
    border-left: 3px solid #e17aff;
    
    &:hover {
      background: rgba(225, 122, 255, 0.2);
    }
  }
  
  // 大航海购买 - 总督（橙色）
  &.is-guard.guard-1 {
    background: rgba(255, 149, 0, 0.15);
    border-left: 3px solid #ff9500;
    
    &:hover {
      background: rgba(255, 149, 0, 0.2);
    }
  }

  &.is-special-follow {
    background: rgba(245, 66, 111, 0.12);
    border-left: 3px solid #fd5858b7;

    &:hover {
      background: rgba(245, 66, 111, 0.192);
    }
  }

  &.expired {
    opacity: 0.4;
    border-left-color: transparent;

    &:hover {
      opacity: 0.6;
    }
  }
}

.gift-icon {
  width: 36px;
  height: 36px;
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
    font-size: 14px;
    font-weight: 500;
    color: white;
  }
  
  // 大航海图标颜色
  &.guard-3 .gift-icon-placeholder {
    background: #00d1f1;
  }
  
  &.guard-2 .gift-icon-placeholder {
    background: #e17aff;
  }
  
  &.guard-1 .gift-icon-placeholder {
    background: #ff9500;
  }
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
  margin-bottom: 2px;
  font-size: var(--content-font-size-xs);
}

.time {
  color: var(--text-muted);
  font-size: 0.9em;
  flex-shrink: 0;
}

.medal {
  display: inline-flex;
  align-items: center;
  padding: 0.2em 0.35em;
  border-radius: 0.2em;
  font-size: 0.9em;
  color: white;
  line-height: 1.2;
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

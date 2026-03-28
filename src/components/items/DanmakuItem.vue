<script setup lang="ts">
import { computed } from 'vue'
import type { ProcessedDanmaku } from '@/types'
import { formatEventTime, getMedalGradient } from '@/types'
import adminIcon from '@/assets/admin-icon.png'

const props = withDefaults(defineProps<{
  message: ProcessedDanmaku
  showMedal?: boolean
  showGuard?: boolean
  showAdmin?: boolean
  showTime?: boolean
  showGuardBorder?: boolean
  emoticonSize?: number
  isSpecialFollow?: boolean
}>(), {
  showMedal: true,
  showGuard: true,
  showAdmin: true,
  showTime: true,
  showGuardBorder: true,
  emoticonSize: 32,
  isSpecialFollow: false
})

// 大航海等级样式类
const guardClass = computed(() => {
  if (!props.showGuard || !props.message.user.guard_level) return ''
  return `danmaku-guard-${props.message.user.guard_level}`
})

const timeText = computed(() => formatEventTime(props.message.timestamp))

const getUserColor = () => {
  if (props.showGuard) {
    switch (props.message.user.guard_level) {
      case 1: return '#ff9500' // 总督 - 橙色
      case 2: return '#e17aff' // 提督 - 紫色
      case 3: return '#00d1f1' // 舰长 - 青色
    }
  }
  if (props.isSpecialFollow) return '#f5c842'
  if (props.showAdmin && props.message.user.is_admin) {
    return 'var(--accent-gold)'
  }
  return '#adbcd9'
}
</script>

<template>
  <div 
    class="danmaku-item animate-fade-in"
    :class="[guardClass, { 'no-guard-border': !showGuardBorder, 'danmaku-special-follow': isSpecialFollow }]"
  >
    <span v-if="showTime && timeText" class="time">
      {{ timeText }}
    </span>

    <!-- 勋章 -->
    <span 
      v-if="showMedal && message.user.medal" 
      class="medal"
      :style="{ backgroundImage: getMedalGradient(message.user.medal.level) }"
    >
      {{ message.user.medal.name }} {{ message.user.medal.level }}
    </span>
    
    <!-- 房管图标 -->
    <img 
      v-if="showAdmin && message.user.is_admin" 
      :src="adminIcon" 
      class="admin-icon"
      alt="房管"
    />
    
    <!-- 用户名 -->
    <span class="username" :style="{ color: getUserColor() }">
      {{ message.user.name }}
    </span>
    
    <span class="separator">：</span>
    
    <!-- 弹幕内容 -->
    <template v-if="message.is_emoticon && message.emoticon_url">
      <!-- 表情弹幕 - 使用 no-referrer 绕过 B站防盗链 -->
      <img 
        :src="message.emoticon_url + '@40h.webp'" 
        :alt="message.content"
        class="emoticon"
        :style="{ height: emoticonSize + 'px', maxHeight: emoticonSize + 'px' }"
        loading="lazy"
        referrerpolicy="no-referrer"
        crossorigin="anonymous"
      />
    </template>
    <span v-else class="content">{{ message.content }}</span>
  </div>
</template>

<style scoped lang="scss">
.danmaku-item {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  padding: 0.4em 0.6em;
  margin: 0.1em 0;
  font-size: var(--font-size-sm);
  line-height: 1.6;
  word-break: break-all;
  cursor: default;
  border-radius: var(--border-radius-sm);
  transition: background 0.15s;
  
  &:hover {
    background: var(--bg-hover);
  }
  
  &.danmaku-guard-3 {
    background: rgba(91, 142, 201, 0.2);
    border-left: 3px solid var(--guard-captain);
    
    &:hover {
      background: rgba(91, 142, 201, 0.3);
    }
  }
  
  &.danmaku-guard-2 {
    background: rgba(147, 112, 219, 0.2);
    border-left: 3px solid var(--guard-admiral);
    
    &:hover {
      background: rgba(147, 112, 219, 0.3);
    }
  }
  
  &.danmaku-guard-1 {
    background: rgba(230, 162, 60, 0.2);
    border-left: 3px solid var(--guard-governor);
    
    &:hover {
      background: rgba(230, 162, 60, 0.3);
    }
  }

  &.danmaku-special-follow {
    background: rgba(245, 200, 66, 0.12);
    border-left: 3px solid #f5c842;
    
    &:hover {
      background: rgba(245, 200, 66, 0.2);
    }
  }
}

.no-guard-border {
  border-left: none !important;
}

.time {
  color: var(--text-muted);
  font-size: 0.85em;
  margin-right: 0.45em;
  flex-shrink: 0;
}

.medal {
  display: inline-flex;
  align-items: center;
  padding: 0.2em 0.35em;
  margin-right: 0.35em;
  border-radius: 0.2em;
  font-size: 0.85em;
  color: white;
  white-space: nowrap;
  flex-shrink: 0;
  line-height: 1.2;
}

.admin-icon {
  width: 1.15em;
  height: 1.15em;
  margin-right: 0.35em;
  flex-shrink: 0;
}

.username {
  font-weight: 500;
  flex-shrink: 0;
}

.separator {
  color: var(--text-muted);
  margin: 0 0.15em;
}

.content {
  color: var(--text-primary);
}

.emoticon {
  height: 36px;
  max-height: 36px;
  min-height: 18px;
  width: auto;
  vertical-align: middle;
  border-radius: 3px;
  object-fit: contain;
  flex-shrink: 0;
}
</style>

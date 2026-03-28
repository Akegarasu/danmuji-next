<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import type { ProcessedSuperChat } from '@/types'
import { formatPrice } from '@/types'

const props = defineProps<{
  superchat: ProcessedSuperChat
}>()

const now = ref(Date.now())
let timer: number

onMounted(() => {
  timer = window.setInterval(() => {
    now.value = Date.now()
  }, 1000)
})

onUnmounted(() => {
  clearInterval(timer)
})

// 根据电池价格获取对应颜色
const scColor = computed(() => {
  const battery = props.superchat.price
  if (battery >= 20000) return '#ab1a32'
  if (battery >= 10000) return '#e54d4d'
  if (battery >= 5000) return '#e09443'
  if (battery >= 1000) return '#e2b52b'
  if (battery >= 500) return '#427d9e'
  return '#2a60b2' // 300及以下
})

// 发送时间（格式化为 HH:mm）
const sendTime = computed(() => {
  const date = new Date(props.superchat.start_time * 1000)
  const hours = date.getHours().toString().padStart(2, '0')
  const minutes = date.getMinutes().toString().padStart(2, '0')
  return `${hours}:${minutes}`
})

// 剩余时间
const remainingTime = computed(() => {
  const elapsed = (now.value - props.superchat.start_time * 1000) / 1000
  const remaining = Math.max(0, props.superchat.duration - elapsed)
  const minutes = Math.floor(remaining / 60)
  const seconds = Math.floor(remaining % 60)
  return `${minutes}:${seconds.toString().padStart(2, '0')}`
})

// 进度百分比
const progress = computed(() => {
  const elapsed = (now.value - props.superchat.start_time * 1000) / 1000
  return Math.max(0, Math.min(100, (1 - elapsed / props.superchat.duration) * 100))
})

// 是否过期
const isExpired = computed(() => progress.value <= 0)
</script>

<template>
  <div 
    class="sc-item animate-fade-in" 
    :class="{ expired: isExpired }"
    :style="{
      '--sc-bg': scColor
    }"
  >
    <div class="sc-header">
      <div class="user-info">
        <span class="username">{{ superchat.user.name }}</span>
        <span class="price">{{ formatPrice(superchat.price) }}</span>
      </div>
      <div class="timer">
        <span class="send-time">{{ sendTime }}</span>
        <span v-if="!isExpired" class="time">{{ remainingTime }}</span>
      </div>
    </div>
    
    <div class="sc-content">
      {{ superchat.content }}
    </div>
    
    <div class="progress-bar">
      <div class="progress" :style="{ width: progress + '%' }" />
    </div>
  </div>
</template>

<style scoped lang="scss">
.sc-item {
  border-radius: var(--border-radius);
  overflow: hidden;
  margin-bottom: 8px;
  transition: filter 0.3s, opacity 0.3s;
  cursor: default;
  flex-shrink: 0;
  
  &.expired {
    filter: grayscale(0.7) brightness(0.8);
    opacity: 0.7;
    
    .sc-header {
      filter: none;
    }
    
    .timer .send-time {
      color: rgba(255, 255, 255, 0.5);
    }
    
    .progress-bar {
      display: none;
    }
  }
}

.sc-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
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

.timer {
  display: flex;
  align-items: center;
  gap: 8px;
  
  .send-time {
    color: rgba(255, 255, 255, 0.6);
    font-size: var(--font-size-xs);
    font-family: monospace;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
  }
  
  .time {
    color: rgba(255, 255, 255, 0.9);
    font-size: var(--font-size-xs);
    font-family: monospace;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
    background: rgba(0, 0, 0, 0.15);
    padding: 1px 6px;
    border-radius: var(--border-radius-sm);
  }
}

.sc-content {
  padding: 10px 12px;
  background: var(--sc-bg);
  color: white;
  font-size: var(--font-size-sm);
  line-height: 1.5;
  min-height: 40px;
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.2);
}

.progress-bar {
  height: 3px;
  background: rgba(0, 0, 0, 0.25);
  
  .progress {
    height: 100%;
    background: rgba(255, 255, 255, 0.6);
    transition: width 1s linear;
  }
}
</style>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import {
  getRawDumpStatus, startRawDump, stopRawDump, openRawDumpDirectory,
  type RawDumpStatus
} from '@/services/raw-event-dump'

const status = ref<RawDumpStatus | null>(null)
const busy = ref(false)
const error = ref('')
const statusError = ref('')
let timer: ReturnType<typeof setInterval> | null = null
let revision = 0
let disposed = false
let refreshing = false

const fileSize = computed(() => {
  const bytes = status.value?.bytes_written ?? 0
  return bytes < 1024 * 1024
    ? `${(bytes / 1024).toFixed(1)} KB`
    : `${(bytes / 1024 / 1024).toFixed(1)} MB`
})
const statusText = computed(() => {
  if (!status.value) return '读取状态中…'
  if (status.value.recording) return '正在保存'
  return status.value.path ? '已停止保存' : '尚未开始'
})

const refresh = async () => {
  if (busy.value || refreshing) return
  refreshing = true
  const current = ++revision
  try {
    const result = await getRawDumpStatus()
    if (!disposed && current === revision) {
      status.value = result
      statusError.value = ''
    }
  } catch (cause) {
    if (!disposed && current === revision) statusError.value = String(cause)
  } finally {
    refreshing = false
  }
}

const toggle = async () => {
  if (busy.value || !status.value) return
  busy.value = true
  revision += 1
  error.value = ''
  try {
    status.value = await (status.value.recording ? stopRawDump() : startRawDump())
  } catch (cause) {
    error.value = String(cause)
  } finally {
    busy.value = false
  }
}

const openDirectory = async () => {
  error.value = ''
  try {
    await openRawDumpDirectory()
  } catch (cause) {
    error.value = String(cause)
  }
}

onMounted(() => {
  void refresh()
  timer = setInterval(() => void refresh(), 1000)
})
onUnmounted(() => {
  disposed = true
  revision += 1
  if (timer) clearInterval(timer)
  // 关闭设置页后继续记录，只有用户停止或退出应用才结束。
})
</script>

<template>
  <section class="dump-panel" aria-labelledby="dump-heading">
    <div class="dump-heading">
      <h4 id="dump-heading">问题排查日志（dump）</h4>
      <span class="dump-status" :class="{ recording: status?.recording }" role="status">
        {{ statusText }}
      </span>
    </div>
    <p class="dump-hint">先开始保存，再复现礼物缺失等问题，完成后停止保存并提供生成的文件。</p>
    <p class="dump-hint">仅记录开始后收到的原始消息。关闭设置页仍会继续记录，单文件达到 100 MB 时自动停止。</p>
    <div class="dump-actions">
      <button type="button" class="dump-button primary" :disabled="busy || !status" @click="toggle">
        {{ busy ? '处理中…' : status?.recording ? '停止保存' : '开始保存 dump' }}
      </button>
      <button type="button" class="dump-button" :disabled="busy || !status?.path" @click="openDirectory">
        打开保存目录
      </button>
    </div>
    <div v-if="status?.path" class="dump-file">
      <p>{{ status.event_count.toLocaleString() }} 条消息 · {{ fileSize }}</p>
      <p class="dump-path">{{ status.path }}</p>
    </div>
    <p v-if="error || statusError || status?.error" class="dump-error" role="alert">{{ error || statusError || status?.error }}</p>
  </section>
</template>

<style scoped>
.dump-panel {
  margin-bottom: 24px;
  padding: 16px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-secondary);
}
.dump-heading, .dump-actions { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
.dump-heading { justify-content: space-between; margin-bottom: 10px; }
h4 { margin: 0; font-size: 14px; color: var(--text-primary); }
.dump-status { font-size: 12px; color: var(--text-secondary); }
.dump-status.recording { color: var(--accent-green, #34d399); }
.dump-hint, .dump-file { margin: 6px 0; font-size: 12px; line-height: 1.6; color: var(--text-secondary); }
.dump-actions { margin: 14px 0; }
.dump-button {
  padding: 8px 12px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: var(--bg-primary);
  color: var(--text-primary);
  cursor: pointer;
  font: inherit;
  font-size: 12px;
}
.dump-button.primary { background: var(--accent-primary); color: #fff; border-color: transparent; }
.dump-button:disabled { opacity: 0.5; cursor: not-allowed; }
.dump-button:not(:disabled):hover { filter: brightness(1.1); }
.dump-file p { margin: 4px 0; }
.dump-path { overflow-wrap: anywhere; user-select: text; }
.dump-error { margin: 10px 0 0; color: var(--accent-red, #ff6b6b); font-size: 12px; line-height: 1.6; }
</style>

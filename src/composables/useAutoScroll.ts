/**
 * 自动滚动 composable
 * 提取自 DanmakuTab / GiftTab 的通用滚动逻辑
 */

import { ref, watch, onMounted, onUnmounted, type WatchSource } from 'vue'

export function useAutoScroll(watchSource: WatchSource<number>) {
  const listRef = ref<HTMLElement>()
  const autoScroll = ref(true)
  const isScrolling = ref(false) // 防止程序化滚动触发检测
  let resizeObserver: ResizeObserver | null = null

  // 滚动到底部的核心函数
  const doScrollToBottom = () => {
    if (!listRef.value) return

    isScrolling.value = true

    listRef.value.scrollTo({
      top: listRef.value.scrollHeight,
      behavior: 'instant'
    })

    // 使用 requestAnimationFrame 确保浏览器完成渲染后再允许检测
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        isScrolling.value = false
      })
    })
  }

  // 自动滚动（列表长度变化时）
  watch(watchSource, () => {
    if (autoScroll.value && listRef.value) {
      requestAnimationFrame(() => {
        doScrollToBottom()
      })
    }
  })

  // 检测是否手动滚动
  const onScroll = () => {
    if (isScrolling.value) return
    if (!listRef.value) return

    const el = listRef.value
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight
    autoScroll.value = distanceFromBottom < 100
  }

  // 用户点击"回到底部"按钮
  const scrollToBottom = () => {
    autoScroll.value = true
    doScrollToBottom()
  }

  onMounted(() => {
    if (listRef.value) {
      resizeObserver = new ResizeObserver(() => {
        if (autoScroll.value) {
          requestAnimationFrame(() => {
            doScrollToBottom()
          })
        }
      })
      resizeObserver.observe(listRef.value)

      // 如果已有内容，初始滚动到底部
      if (listRef.value.scrollHeight > listRef.value.clientHeight) {
        doScrollToBottom()
      }
    }
  })

  onUnmounted(() => {
    if (resizeObserver) {
      resizeObserver.disconnect()
      resizeObserver = null
    }
  })

  return { listRef, autoScroll, onScroll, scrollToBottom }
}

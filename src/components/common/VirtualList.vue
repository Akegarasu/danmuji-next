<script setup lang="ts" generic="T">
import { computed, nextTick, onMounted, onUnmounted, ref, watch, type ComponentPublicInstance } from 'vue'

type ItemKey = string | number

const props = withDefaults(defineProps<{
  items: readonly T[]
  itemKey: (item: T, index: number) => ItemKey
  estimateSize?: number
  overscan?: number
  stickToBottom?: boolean
  bottomThreshold?: number
  autoScroll?: boolean
  layoutVersion?: string | number
}>(), {
  estimateSize: 48,
  overscan: 8,
  stickToBottom: true,
  bottomThreshold: 100,
  autoScroll: true
})

const emit = defineEmits<{
  'update:autoScroll': [value: boolean]
}>()

defineSlots<{
  default(props: { item: T; index: number }): unknown
  empty(): unknown
}>()

const viewportRef = ref<HTMLElement>()
const scrollTop = ref(0)
const viewportHeight = ref(0)
const sizeVersion = ref(0)
const internalAutoScroll = ref(props.autoScroll)

const itemSizes = new Map<ItemKey, number>()
const observedElements = new Map<ItemKey, HTMLElement>()
const elementKeys = new WeakMap<Element, ItemKey>()

let itemResizeObserver: ResizeObserver | null = null
let viewportResizeObserver: ResizeObserver | null = null
let scrollRaf: number | null = null
let unlockScrollRaf: number | null = null
let programmaticScroll = false

const getItemKey = (item: T, index: number) => props.itemKey(item, index)

const setAutoScroll = (value: boolean) => {
  if (internalAutoScroll.value === value) return
  internalAutoScroll.value = value
  emit('update:autoScroll', value)
}

watch(() => props.autoScroll, (value) => {
  internalAutoScroll.value = value
})

const measuredAverageSize = computed(() => {
  if (itemSizes.size === 0) return props.estimateSize

  let total = 0
  itemSizes.forEach(size => {
    total += size
  })
  return Math.max(1, total / itemSizes.size)
})

const getItemSize = (item: T, index: number) => {
  return itemSizes.get(getItemKey(item, index)) ?? measuredAverageSize.value
}

const offsets = computed(() => {
  sizeVersion.value

  const result = new Array(props.items.length + 1)
  result[0] = 0

  for (let i = 0; i < props.items.length; i += 1) {
    result[i + 1] = result[i] + getItemSize(props.items[i], i)
  }

  return result as number[]
})

const totalHeight = computed(() => offsets.value[props.items.length] ?? 0)

const findItemIndex = (offset: number) => {
  const count = props.items.length
  if (count === 0) return 0

  const values = offsets.value
  let low = 0
  let high = count - 1
  let result = 0

  while (low <= high) {
    const mid = Math.floor((low + high) / 2)
    if (values[mid] <= offset) {
      result = mid
      low = mid + 1
    } else {
      high = mid - 1
    }
  }

  return Math.min(result, count - 1)
}

const visibleStart = computed(() => {
  return Math.max(0, findItemIndex(scrollTop.value) - props.overscan)
})

const visibleEnd = computed(() => {
  if (props.items.length === 0) return 0

  const endOffset = scrollTop.value + viewportHeight.value
  return Math.min(props.items.length, findItemIndex(endOffset) + props.overscan + 1)
})

const visibleItems = computed(() => {
  const result: Array<{ item: T; index: number; key: ItemKey; top: number }> = []
  const start = visibleStart.value
  const end = visibleEnd.value
  const currentOffsets = offsets.value

  for (let index = start; index < end; index += 1) {
    const item = props.items[index]
    result.push({
      item,
      index,
      key: getItemKey(item, index),
      top: currentOffsets[index]
    })
  }

  return result
})

const updateViewportMetrics = () => {
  const el = viewportRef.value
  if (!el) return

  scrollTop.value = el.scrollTop
  viewportHeight.value = el.clientHeight
}

const scrollToBottomNow = () => {
  const el = viewportRef.value
  if (!el) return

  programmaticScroll = true
  el.scrollTop = el.scrollHeight
  updateViewportMetrics()
  setAutoScroll(true)

  if (unlockScrollRaf !== null) {
    cancelAnimationFrame(unlockScrollRaf)
  }

  unlockScrollRaf = requestAnimationFrame(() => {
    unlockScrollRaf = requestAnimationFrame(() => {
      programmaticScroll = false
      unlockScrollRaf = null
    })
  })
}

const scheduleScrollToBottom = () => {
  if (!props.stickToBottom) return

  void nextTick(() => {
    if (scrollRaf !== null) {
      cancelAnimationFrame(scrollRaf)
    }

    scrollRaf = requestAnimationFrame(() => {
      scrollRaf = null
      scrollToBottomNow()
    })
  })
}

const scrollToBottom = () => {
  setAutoScroll(true)
  scheduleScrollToBottom()
}

const onScroll = () => {
  updateViewportMetrics()

  if (programmaticScroll) return

  const el = viewportRef.value
  if (!el) return

  const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight
  setAutoScroll(distanceFromBottom < props.bottomThreshold)
}

const measureElement = (key: ItemKey, el: HTMLElement) => {
  const nextSize = el.offsetHeight
  if (nextSize <= 0) return

  const prevSize = itemSizes.get(key)
  if (prevSize === nextSize) return

  itemSizes.set(key, nextSize)
  sizeVersion.value += 1

  if (props.stickToBottom && internalAutoScroll.value) {
    scheduleScrollToBottom()
  }
}

const resetMeasuredSizes = () => {
  itemSizes.clear()
  sizeVersion.value += 1

  void nextTick(() => {
    observedElements.forEach((el, key) => measureElement(key, el))
    updateViewportMetrics()

    if (props.stickToBottom && internalAutoScroll.value) {
      scheduleScrollToBottom()
    }
  })
}

const setItemElement = (el: Element | ComponentPublicInstance | null, key: ItemKey) => {
  const current = observedElements.get(key)

  if (!el) {
    if (current) {
      itemResizeObserver?.unobserve(current)
      observedElements.delete(key)
      elementKeys.delete(current)
    }
    return
  }

  const element = el instanceof HTMLElement
    ? el
    : el instanceof Element
      ? null
      : el.$el
  if (!(element instanceof HTMLElement)) return

  if (current === element) {
    measureElement(key, element)
    return
  }

  if (current) {
    itemResizeObserver?.unobserve(current)
    elementKeys.delete(current)
  }

  observedElements.set(key, element)
  elementKeys.set(element, key)
  itemResizeObserver?.observe(element)
  measureElement(key, element)
}

const pruneMeasuredSizes = () => {
  const liveKeys = new Set<ItemKey>()
  props.items.forEach((item, index) => {
    liveKeys.add(getItemKey(item, index))
  })

  itemSizes.forEach((_, key) => {
    if (!liveKeys.has(key)) {
      itemSizes.delete(key)
    }
  })

  sizeVersion.value += 1
}

watch(
  () => [
    props.items.length,
    props.items[0] ? getItemKey(props.items[0], 0) : '',
    props.items[props.items.length - 1]
      ? getItemKey(props.items[props.items.length - 1], props.items.length - 1)
      : ''
  ],
  () => {
    pruneMeasuredSizes()

    if (props.stickToBottom && internalAutoScroll.value) {
      scheduleScrollToBottom()
    }
  }
)

watch(() => props.layoutVersion, () => {
  resetMeasuredSizes()
})

onMounted(() => {
  itemResizeObserver = new ResizeObserver((entries) => {
    for (const entry of entries) {
      const key = elementKeys.get(entry.target)
      if (key !== undefined && entry.target instanceof HTMLElement) {
        measureElement(key, entry.target)
      }
    }
  })

  observedElements.forEach(el => itemResizeObserver?.observe(el))

  if (viewportRef.value) {
    viewportResizeObserver = new ResizeObserver(() => {
      updateViewportMetrics()

      if (props.stickToBottom && internalAutoScroll.value) {
        scheduleScrollToBottom()
      }
    })
    viewportResizeObserver.observe(viewportRef.value)
  }

  updateViewportMetrics()

  if (props.stickToBottom && internalAutoScroll.value) {
    scheduleScrollToBottom()
  }
})

onUnmounted(() => {
  itemResizeObserver?.disconnect()
  viewportResizeObserver?.disconnect()

  if (scrollRaf !== null) {
    cancelAnimationFrame(scrollRaf)
  }
  if (unlockScrollRaf !== null) {
    cancelAnimationFrame(unlockScrollRaf)
  }
})

defineExpose({
  scrollToBottom
})
</script>

<template>
  <div ref="viewportRef" class="virtual-list" @scroll="onScroll">
    <div v-if="items.length === 0" class="virtual-list__empty">
      <slot name="empty" />
    </div>

    <div
      v-else
      class="virtual-list__spacer"
      :style="{ height: `${totalHeight}px` }"
    >
      <div
        v-for="entry in visibleItems"
        :key="entry.key"
        :ref="el => setItemElement(el, entry.key)"
        class="virtual-list__item"
        :style="{ transform: `translateY(${entry.top}px)` }"
      >
        <slot :item="entry.item" :index="entry.index" />
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.virtual-list {
  overflow-y: auto;
  overflow-x: hidden;
  min-height: 0;
  overflow-anchor: none;
}

.virtual-list__empty {
  height: 100%;
}

.virtual-list__spacer {
  position: relative;
  min-height: 100%;
}

.virtual-list__item {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  display: flow-root;
  contain: layout paint;
}

.virtual-list :deep(.animate-fade-in),
.virtual-list :deep(.animate-slide-in) {
  animation: none;
}
</style>

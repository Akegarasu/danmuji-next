<script setup lang="ts" generic="T extends string">
import { ref, computed, nextTick, useId, onMounted, onBeforeUnmount, onDeactivated, type CSSProperties } from 'vue'

export interface SelectOption<T extends string = string> {
  value: T
  label: string
}

const props = defineProps<{
  modelValue: T
  options: SelectOption<T>[]
  placeholder?: string
  ariaLabel?: string
}>()

const emit = defineEmits<{
  'update:modelValue': [value: T]
}>()

const open = ref(false)
const triggerRef = ref<HTMLElement>()
const dropdownRef = ref<HTMLElement>()
const dropdownId = useId()
const activeIndex = ref(-1)
const dropdownStyle = ref<CSSProperties>({ visibility: 'hidden' })

const selectedLabel = computed(() => {
  const opt = props.options.find(o => o.value === props.modelValue)
  return opt?.label ?? props.placeholder ?? ''
})

// 菜单挂到 body，避免被页面滚动容器或其他卡片裁剪。
async function showDropdown() {
  const trigger = triggerRef.value
  if (!trigger) return
  const style = getComputedStyle(trigger)
  activeIndex.value = Math.max(0, props.options.findIndex(option => option.value === props.modelValue))
  dropdownStyle.value = { visibility: 'hidden', fontSize: style.fontSize, fontFamily: style.fontFamily }
  open.value = true
  await nextTick()
  const dropdown = dropdownRef.value
  if (!open.value || !dropdown) return
  const rect = trigger.getBoundingClientRect()
  const margin = 8
  const gap = 4
  const width = Math.min(Math.max(rect.width, dropdown.offsetWidth), window.innerWidth - margin * 2)
  const below = window.innerHeight - rect.bottom - gap - margin
  const above = rect.top - gap - margin
  const openAbove = below < Math.min(dropdown.offsetHeight, 240) && above > below
  const maxHeight = Math.max(0, Math.min(240, openAbove ? above : below))
  const height = Math.min(dropdown.offsetHeight, maxHeight)
  dropdownStyle.value = {
    left: `${Math.max(margin, Math.min(rect.left, window.innerWidth - width - margin))}px`,
    top: `${openAbove ? rect.top - gap - height : rect.bottom + gap}px`,
    width: `${width}px`,
    maxHeight: `${maxHeight}px`,
    fontSize: style.fontSize,
    fontFamily: style.fontFamily,
  }
  await nextTick()
  if (open.value) document.getElementById(`${dropdownId}-${activeIndex.value}`)?.scrollIntoView({ block: 'nearest' })
}

const toggle = () => { if (open.value) open.value = false; else void showDropdown() }

const select = (value: T) => {
  emit('update:modelValue', value)
  open.value = false
}

const onClickOutside = (event: MouseEvent) => {
  if (!triggerRef.value?.contains(event.target as Node) && !dropdownRef.value?.contains(event.target as Node)) {
    open.value = false
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Tab') { open.value = false; return }
  if (event.key === 'Escape') { open.value = false; event.preventDefault(); return }
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End', 'Enter', ' '].includes(event.key)) return
  event.preventDefault()
  if (!open.value) { void showDropdown(); return }
  const count = props.options.length
  if (!count) return
  if (event.key === 'Enter' || event.key === ' ') {
    const option = props.options[activeIndex.value]
    if (option) select(option.value)
    return
  }
  if (event.key === 'Home') activeIndex.value = 0
  else if (event.key === 'End') activeIndex.value = count - 1
  else activeIndex.value = (activeIndex.value + (event.key === 'ArrowDown' ? 1 : -1) + count) % count
  void nextTick(() => document.getElementById(`${dropdownId}-${activeIndex.value}`)?.scrollIntoView({ block: 'nearest' }))
}

function onScroll(event: Event) {
  if (!dropdownRef.value?.contains(event.target as Node)) open.value = false
}

const close = () => { open.value = false }

onMounted(() => {
  document.addEventListener('mousedown', onClickOutside)
  document.addEventListener('scroll', onScroll, true)
  window.addEventListener('resize', close)
})

onDeactivated(close)

onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onClickOutside)
  document.removeEventListener('scroll', onScroll, true)
  window.removeEventListener('resize', close)
})
</script>

<template>
  <div class="ext-select" :class="{ 'ext-select--open': open }">
    <button ref="triggerRef" type="button" class="ext-select__trigger" role="combobox"
      :aria-label="ariaLabel" aria-haspopup="listbox" :aria-expanded="open" :aria-controls="dropdownId"
      :aria-activedescendant="open && activeIndex >= 0 ? `${dropdownId}-${activeIndex}` : undefined"
      @click="toggle" @keydown="onKeydown">
      <span class="ext-select__label">{{ selectedLabel }}</span>
      <svg class="ext-select__arrow" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="6 9 12 15 18 9" />
      </svg>
    </button>
    <Teleport to="body">
      <Transition name="ext-dropdown">
        <div v-if="open" :id="dropdownId" ref="dropdownRef" class="ext-select__dropdown" role="listbox"
          :aria-label="ariaLabel" :style="dropdownStyle">
          <div
            v-for="(opt, index) in options"
            :id="`${dropdownId}-${index}`"
            :key="opt.value"
            class="ext-select__option"
            role="option"
            :aria-selected="opt.value === modelValue"
            :class="{ 'ext-select__option--active': opt.value === modelValue, 'ext-select__option--highlighted': index === activeIndex }"
            @pointermove="activeIndex = index"
            @mousedown.prevent
            @click="select(opt.value)"
          >
            {{ opt.label }}
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/settings-controls' as controls;
.ext-select {
  position: relative;
  display: inline-flex;
  min-width: 0;
  max-width: 100%;

  &__trigger {
    @include controls.control;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    outline: none;
    transition: border-color 0.2s, box-shadow 0.2s;
    white-space: nowrap;

    &:hover {
      border-color: var(--text-muted);
    }

    &:focus-visible {
      border-color: var(--accent-primary);
      box-shadow: 0 0 0 2px rgba(92, 158, 255, 0.15);
    }
  }

  &--open .ext-select__trigger {
    border-color: var(--accent-primary);
    box-shadow: 0 0 0 2px rgba(92, 158, 255, 0.15);
  }

  &__label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: left;
  }

  &__arrow {
    color: var(--text-muted);
    flex-shrink: 0;
    transition: transform 0.2s;
  }

  &--open .ext-select__arrow {
    transform: rotate(180deg);
  }

  &__dropdown {
    position: fixed;
    width: max-content;
    max-width: calc(100vw - 16px);
    overflow-y: auto;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-sm);
    padding: 3px;
    z-index: 1000;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
    transform-origin: top;
  }

  &__option {
    padding: 8px 12px;
    line-height: 1.5;
    color: var(--text-secondary);
    border-radius: 2px;
    cursor: pointer;
    overflow-wrap: anywhere;
    transition: background 0.1s, color 0.1s;

    &:hover,
    &--highlighted {
      background: var(--bg-hover);
      color: var(--text-primary);
    }

    &--active {
      color: var(--accent-primary);
      background: rgba(92, 158, 255, 0.1);
    }
  }
}

// 下拉菜单过渡
.ext-dropdown-enter-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}

.ext-dropdown-leave-active {
  transition: opacity 0.1s ease, transform 0.1s ease;
}

.ext-dropdown-enter-from,
.ext-dropdown-leave-to {
  opacity: 0;
  transform: scaleY(0.9) translateY(-2px);
}
</style>

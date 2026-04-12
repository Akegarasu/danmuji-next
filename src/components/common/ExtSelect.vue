<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'

export interface SelectOption {
  value: string
  label: string
}

const props = defineProps<{
  modelValue: string
  options: SelectOption[]
  placeholder?: string
}>()

const emit = defineEmits<{
  'update:modelValue': [value: string]
}>()

const open = ref(false)
const triggerRef = ref<HTMLElement>()
const dropdownRef = ref<HTMLElement>()

const selectedLabel = computed(() => {
  const opt = props.options.find(o => o.value === props.modelValue)
  return opt?.label ?? props.placeholder ?? ''
})

const toggle = () => {
  open.value = !open.value
}

const select = (value: string) => {
  emit('update:modelValue', value)
  open.value = false
}

const onClickOutside = (e: MouseEvent) => {
  if (!triggerRef.value?.contains(e.target as Node) && !dropdownRef.value?.contains(e.target as Node)) {
    open.value = false
  }
}

onMounted(() => {
  document.addEventListener('mousedown', onClickOutside)
})

onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onClickOutside)
})
</script>

<template>
  <div class="ext-select" :class="{ 'ext-select--open': open }">
    <button ref="triggerRef" class="ext-select__trigger" @click="toggle">
      <span class="ext-select__label">{{ selectedLabel }}</span>
      <svg class="ext-select__arrow" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="6 9 12 15 18 9" />
      </svg>
    </button>
    <Transition name="ext-dropdown">
      <div v-if="open" ref="dropdownRef" class="ext-select__dropdown">
        <div
          v-for="opt in options"
          :key="opt.value"
          class="ext-select__option"
          :class="{ 'ext-select__option--active': opt.value === modelValue }"
          @click="select(opt.value)"
        >
          {{ opt.label }}
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped lang="scss">
.ext-select {
  position: relative;
  display: inline-flex;

  &__trigger {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-sm);
    background: var(--bg-primary);
    color: var(--text-primary);
    font-size: var(--font-size-sm);
    font-family: var(--font-family);
    cursor: pointer;
    outline: none;
    transition: border-color 0.2s, box-shadow 0.2s;
    white-space: nowrap;

    &:hover {
      border-color: var(--text-muted);
    }
  }

  &--open .ext-select__trigger {
    border-color: var(--accent-primary);
    box-shadow: 0 0 0 2px rgba(92, 158, 255, 0.15);
  }

  &__label {
    flex: 1;
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
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    min-width: 100%;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-sm);
    padding: 3px;
    z-index: 100;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
    transform-origin: top;
  }

  &__option {
    padding: 5px 10px;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    border-radius: 2px;
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.1s, color 0.1s;

    &:hover {
      background: var(--bg-hover);
      color: var(--text-primary);
    }

    &--active {
      color: var(--accent-primary);
      background: rgba(92, 158, 255, 0.1);
    }
  }
}

// dropdown transition
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

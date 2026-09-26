<script setup lang="ts">
withDefaults(defineProps<{
  modelValue: boolean
  label: string
  disabled?: boolean
}>(), {
  disabled: false
})

const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void
  (event: 'change', value: boolean): void
}>()

const handleChange = (event: Event) => {
  const checked = (event.target as HTMLInputElement).checked
  emit('update:modelValue', checked)
  emit('change', checked)
}
</script>

<template>
  <label class="settings-toggle" :class="{ disabled }">
    <span class="settings-toggle-label">{{ label }}</span>
    <input
      type="checkbox"
      class="settings-toggle-input"
      :checked="modelValue"
      :disabled="disabled"
      @change="handleChange"
    />
  </label>
</template>

<style scoped lang="scss">
.settings-toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  color: var(--text-secondary);
  font-size: var(--font-size-sm);
  line-height: 1.5;
  cursor: pointer;

  &.disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
}

.settings-toggle-label {
  min-width: 0;
}

.settings-toggle-input {
  appearance: none;
  position: relative;
  flex-shrink: 0;
  width: 40px;
  height: 20px;
  margin: 0;
  background: var(--bg-card);
  border: none;
  border-radius: 10px;
  cursor: inherit;
  transition: background 0.2s;

  &::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    background: var(--text-muted);
    border-radius: 50%;
    transition: left 0.2s, background 0.2s;
  }

  &:checked {
    background: var(--accent-primary);

    &::after {
      left: 22px;
      background: white;
    }
  }

  &:focus-visible {
    outline: 2px solid var(--accent-primary);
    outline-offset: 3px;
  }
}
</style>

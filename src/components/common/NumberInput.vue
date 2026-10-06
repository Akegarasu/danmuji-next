<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const props = withDefaults(defineProps<{
  modelValue: number
  label: string
  min: number
  max: number
  step?: number
  disabled?: boolean
}>(), { step: 1, disabled: false })
const emit = defineEmits<{ 'update:modelValue': [value: number] }>()
const draft = ref(String(props.modelValue))
watch(() => props.modelValue, value => { draft.value = String(value) })
const current = computed(() => draft.value.trim() && Number.isFinite(Number(draft.value)) ? Number(draft.value) : props.modelValue)
function normalize(value: number) {
  const aligned = props.min + Math.round((value - props.min) / props.step) * props.step
  return Math.min(props.max, Math.max(props.min, Number(aligned.toFixed(8))))
}
function commit(value = current.value) {
  const next = normalize(value)
  draft.value = String(next)
  if (next !== props.modelValue) emit('update:modelValue', next)
}
function input(event: Event) {
  draft.value = (event.target as HTMLInputElement).value
  const value = Number(draft.value)
  if (draft.value.trim() && Number.isFinite(value) && value >= props.min && value <= props.max) {
    emit('update:modelValue', value)
  }
}
function stepBy(direction: number) { if (!props.disabled) commit(normalize(current.value) + direction * props.step) }
</script>

<template>
  <div class="number-input" :class="{ disabled }">
    <button type="button" :aria-label="`减少${label}`" :disabled="disabled || current <= min" @click="stepBy(-1)">−</button>
    <input :value="draft" type="number" :aria-label="label" :min="min" :max="max" :step="step" :disabled="disabled" required
      @input="input" @blur="commit()" @keydown.enter.prevent="commit()" @keydown.up.prevent="stepBy(1)" @keydown.down.prevent="stepBy(-1)">
    <button type="button" :aria-label="`增加${label}`" :disabled="disabled || current >= max" @click="stepBy(1)">+</button>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/settings-controls' as controls;
.number-input { display: flex; width: 100%; min-width: 0; height: controls.$height; border: 1px solid var(--border-color); border-radius: controls.$radius; background: var(--bg-card); overflow: hidden; &:focus-within { border-color: var(--accent-primary); } &.disabled { opacity: .5; } }
input { @include controls.control; flex: 1; height: 100%; padding: 0 4px; border: 0; border-radius: 0; text-align: center; background: transparent; appearance: textfield; &::-webkit-inner-spin-button, &::-webkit-outer-spin-button { appearance: none; margin: 0; } }
button { @include controls.icon-button; flex: 0 0 30px; font-size: 16px; &:first-child { border-right: 1px solid var(--border-color); } &:last-child { border-left: 1px solid var(--border-color); } }
</style>

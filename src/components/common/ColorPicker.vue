<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { useControlPopover } from '@/composables/useControlPopover'
import { hexToHsv, hsvToHex, normalizeHex } from '@/utils/color'

const props = withDefaults(defineProps<{
  modelValue: string
  label: string
  disabled?: boolean
  presets?: string[]
}>(), { disabled: false, presets: () => ['#ffffff', '#9b9b9b', '#000000', '#ff6b6b', '#ffb347', '#f5c842', '#7ed4a5', '#80deea', '#5c9eff', '#b794f4', '#ff7eb3', '#c5a880'] })
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
const { anchor, panel, open, panelStyle, show, close } = useControlPopover(272, 340)
const plane = ref<HTMLElement>()
const draft = ref(props.modelValue)
const hsv = ref(hexToHsv(props.modelValue))
const color = computed(() => normalizeHex(props.modelValue) ?? '#ffffff')
watch(() => props.modelValue, value => {
  draft.value = value
  const next = hexToHsv(value)
  // 灰度色保留用户选择的色相，黑色保留饱和度，以便继续拖动调色。
  hsv.value = { h: next.s > 0 ? next.h : hsv.value.h, s: next.v > 0 ? next.s : hsv.value.s, v: next.v }
})
watch(() => props.disabled, disabled => { if (disabled) close() })
function input(event: Event) {
  draft.value = (event.target as HTMLInputElement).value
  if (/^#?[\da-f]{6}$/i.test(draft.value.trim())) emit('update:modelValue', normalizeHex(draft.value)!)
}
function commit() {
  const value = normalizeHex(draft.value)
  draft.value = value ?? props.modelValue
  if (value && value !== props.modelValue) emit('update:modelValue', value)
}
async function toggle() {
  if (open.value) { close(); return }
  await show()
  await nextTick()
  if (open.value) plane.value?.focus({ preventScroll: true })
}
function publish() { emit('update:modelValue', hsvToHex(hsv.value)) }
function pick(event: PointerEvent) {
  if (!plane.value || (event.type === 'pointermove' && !plane.value.hasPointerCapture(event.pointerId))) return
  if (event.type === 'pointerdown') {
    if (event.button !== 0) return
    plane.value.setPointerCapture(event.pointerId)
    plane.value.focus({ preventScroll: true })
  }
  const rect = plane.value.getBoundingClientRect()
  hsv.value.s = Math.min(100, Math.max(0, (event.clientX - rect.left) / rect.width * 100))
  hsv.value.v = Math.min(100, Math.max(0, 100 - (event.clientY - rect.top) / rect.height * 100))
  publish()
}
function release(event: PointerEvent) {
  if (plane.value?.hasPointerCapture(event.pointerId)) plane.value.releasePointerCapture(event.pointerId)
}
function moveCursor(event: KeyboardEvent) {
  const delta = event.shiftKey ? 10 : 1
  if (event.key === 'ArrowLeft') hsv.value.s = Math.max(0, hsv.value.s - delta)
  else if (event.key === 'ArrowRight') hsv.value.s = Math.min(100, hsv.value.s + delta)
  else if (event.key === 'ArrowUp') hsv.value.v = Math.min(100, hsv.value.v + delta)
  else if (event.key === 'ArrowDown') hsv.value.v = Math.max(0, hsv.value.v - delta)
  else return
  event.preventDefault(); publish()
}
function hueInput(event: Event) { hsv.value.h = Number((event.target as HTMLInputElement).value); publish() }
</script>

<template>
  <div class="color-picker">
    <button ref="anchor" type="button" class="color-trigger" :aria-label="`${label}取色`" :disabled="disabled" aria-haspopup="dialog" :aria-expanded="open" @click="toggle">
      <span :style="{ backgroundColor: color }" />
    </button>
    <input :value="draft" type="text" class="hex-input" :aria-label="`${label}色值`" :disabled="disabled" maxlength="7" autocomplete="off" :spellcheck="false"
      @input="input" @blur="commit" @keydown.enter.prevent="commit">
    <Teleport to="body">
      <div v-if="open" ref="panel" class="color-popover" :style="panelStyle" role="dialog" :aria-label="`${label}调色板`" @keydown.esc.stop.prevent="close(true)">
        <div class="color-heading"><span>{{ label }}</span><button type="button" aria-label="关闭调色板" @click="close(true)">×</button></div>
        <div ref="plane" class="color-plane" tabindex="0" role="slider" aria-label="饱和度与亮度" :aria-valuemin="0" :aria-valuemax="100" :aria-valuenow="Math.round(hsv.s)"
          :aria-valuetext="`饱和度 ${Math.round(hsv.s)}%，亮度 ${Math.round(hsv.v)}%，左右键调整饱和度，上下键调整亮度`"
          :style="{ backgroundColor: `hsl(${hsv.h} 100% 50%)` }" @pointerdown="pick" @pointermove="pick" @pointerup="release" @pointercancel="release" @keydown="moveCursor">
          <span class="color-cursor" :style="{ left: `${hsv.s}%`, top: `${100 - hsv.v}%`, backgroundColor: color }" />
        </div>
        <label class="hue-control"><span>色相</span><input class="hue-slider" type="range" min="0" max="360" step="1" :value="hsv.h" aria-label="色相" @input="hueInput"></label>
        <div class="color-presets">
          <button v-for="preset in presets" :key="preset" type="button" :aria-label="`选择颜色 ${preset}`" :aria-pressed="color === preset" :style="{ backgroundColor: preset }" @click="emit('update:modelValue', preset)">
            <span v-if="color === preset" aria-hidden="true">✓</span>
          </button>
        </div>
        <div class="color-value"><span class="current-color" :style="{ backgroundColor: color }" /><input :value="draft" type="text" :aria-label="`${label}调色板色值`" maxlength="7" :spellcheck="false" @input="input" @blur="commit" @keydown.enter.prevent="commit"><span>HEX</span></div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/settings-controls' as controls;
.color-picker { display: flex; gap: 8px; width: 100%; min-width: 0; }
.color-trigger { @include controls.control; flex: 0 0 40px; width: 40px; padding: 5px; cursor: pointer; span { display: block; height: 100%; border: 1px solid rgba(255,255,255,.16); border-radius: 3px; } &:focus-visible { outline: 2px solid var(--accent-primary); outline-offset: 2px; } }
.hex-input { @include controls.control; flex: 1; font-family: 'Cascadia Mono', Consolas, monospace; }
.color-popover { @include controls.popover; display: grid; gap: 12px; }
.color-heading { display: flex; align-items: center; justify-content: space-between; gap: 8px; button { @include controls.icon-button; width: 22px; height: 22px; border-radius: 4px; font-size: 18px; } }
.color-plane { position: relative; height: 144px; border-radius: 5px; background-image: linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, transparent); touch-action: none; cursor: crosshair; &:focus-visible { outline: 2px solid var(--accent-primary); outline-offset: 3px; } }
.color-cursor { position: absolute; width: 12px; height: 12px; transform: translate(-50%, -50%); border: 2px solid #fff; border-radius: 50%; box-shadow: 0 0 0 1px rgba(0,0,0,.5); pointer-events: none; }
.hue-control { display: flex; align-items: center; gap: 10px; color: var(--text-secondary); span { flex-shrink: 0; } }
.hue-slider { appearance: none; flex: 1; min-width: 0; height: 10px; border: 0; border-radius: 6px; background: linear-gradient(to right, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00); cursor: pointer; &::-webkit-slider-thumb { appearance: none; width: 14px; height: 14px; border: 2px solid #fff; border-radius: 50%; background: transparent; box-shadow: 0 0 0 1px rgba(0,0,0,.4); } &::-moz-range-thumb { width: 10px; height: 10px; border: 2px solid #fff; border-radius: 50%; background: transparent; box-shadow: 0 0 0 1px rgba(0,0,0,.4); } &:focus-visible { outline: 2px solid var(--accent-primary); outline-offset: 4px; } }
.color-presets { display: grid; grid-template-columns: repeat(6, 1fr); gap: 8px; button { height: 22px; border: 1px solid rgba(255,255,255,.16); border-radius: 4px; cursor: pointer; color: white; text-shadow: 0 1px 2px black, 0 0 2px black; &:focus-visible { outline: 2px solid var(--accent-primary); outline-offset: 2px; } } }
.color-value { display: flex; align-items: center; gap: 8px; color: var(--text-secondary); input { @include controls.control; flex: 1; font-family: 'Cascadia Mono', Consolas, monospace; } }
.current-color { width: 24px; height: 24px; border: 1px solid var(--border-color); border-radius: 4px; flex-shrink: 0; }
</style>

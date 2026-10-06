<script setup lang="ts">
import SettingsToggle from '@/components/common/SettingsToggle.vue'
import FontPicker from '@/components/common/FontPicker.vue'
import ColorPicker from '@/components/common/ColorPicker.vue'
import NumberInput from '@/components/common/NumberInput.vue'
import { defaultOverlayStyle, type OverlayStyle } from '@/types/overlay-style'

const props = defineProps<{
  modelValue: OverlayStyle
  preset: 'overtime' | 'song-request'
  disabled?: boolean
}>()
const emit = defineEmits<{ 'update:modelValue': [value: OverlayStyle] }>()
const colors = [
  { key: 'text_color', label: '主要文字颜色' },
  { key: 'secondary_color', label: '辅助文字颜色' },
] as const
function update<K extends keyof OverlayStyle>(key: K, value: OverlayStyle[K]) {
  if (props.modelValue[key] === value) return
  emit('update:modelValue', { ...props.modelValue, [key]: value })
}
</script>

<template>
  <fieldset class="overlay-style-settings" :disabled="disabled">
    <div class="style-toolbar">
      <span>自定义 OBS 外观</span>
      <button type="button" @click="emit('update:modelValue', defaultOverlayStyle(preset))">恢复默认外观</button>
    </div>
    <div class="style-grid">
      <div class="style-field font-field"><span>字体名称</span>
        <FontPicker :model-value="modelValue.font_family" :disabled="disabled"
          :placeholder="preset === 'overtime' ? '默认：内置加班机字体' : '默认：微软雅黑'"
          @update:model-value="update('font_family', $event)" />
      </div>
      <div class="style-field"><span>字号（px）</span>
        <NumberInput :model-value="modelValue.font_size" label="字号（px）" :min="12" :max="200" :disabled="disabled" @update:model-value="update('font_size', $event)" />
      </div>
      <div class="style-field"><span>字重（400 常规 / 700 粗体）</span>
        <NumberInput :model-value="modelValue.font_weight" label="字重" :min="100" :max="900" :step="100" :disabled="disabled" @update:model-value="update('font_weight', $event)" />
      </div>
      <div v-for="color in colors" :key="color.key" class="style-field"><span>{{ color.label }}</span>
        <ColorPicker :model-value="modelValue[color.key]" :label="color.label" :disabled="disabled" @update:model-value="update(color.key, $event)" />
      </div>
    </div>
    <p>支持搜索本机字体，也可手动指定 OBS 所在电脑已安装的字体。字号以{{ preset === 'overtime' ? '倒计时' : '歌名' }}为基准，其余文字按比例缩放。<template v-if="preset === 'overtime'">倒计时过宽时会自动缩小以完整显示。</template></p>
    <p>辅助文字颜色用于{{ preset === 'overtime' ? '礼物规则、礼物提示和状态' : '序号和点歌人（保留原有透明度）' }}。</p>
    <SettingsToggle :model-value="modelValue.shadow_enabled" label="显示文字阴影" :disabled="disabled" @update:model-value="update('shadow_enabled', $event)" />
    <div v-if="modelValue.shadow_enabled" class="style-grid shadow-fields">
      <div class="style-field"><span>阴影颜色</span>
        <ColorPicker :model-value="modelValue.shadow_color" label="阴影颜色" :disabled="disabled" @update:model-value="update('shadow_color', $event)" />
      </div>
      <div class="style-field"><span>阴影模糊（px）</span><NumberInput :model-value="modelValue.shadow_blur" label="阴影模糊（px）" :min="0" :max="50" :disabled="disabled" @update:model-value="update('shadow_blur', $event)" /></div>
      <div class="style-field"><span>水平偏移（px）</span><NumberInput :model-value="modelValue.shadow_offset_x" label="水平偏移（px）" :min="-50" :max="50" :disabled="disabled" @update:model-value="update('shadow_offset_x', $event)" /></div>
      <div class="style-field"><span>垂直偏移（px）</span><NumberInput :model-value="modelValue.shadow_offset_y" label="垂直偏移（px）" :min="-50" :max="50" :disabled="disabled" @update:model-value="update('shadow_offset_y', $event)" /></div>
    </div>
    <p>保存后同步到 OBS 和浏览器源预览。恢复默认外观也需要保存。</p>
  </fieldset>
</template>

<style scoped lang="scss">
@use '@/styles/settings-controls' as controls;
.overlay-style-settings { min-width: 0; margin: 0; padding: 0; border: 0; }
.style-toolbar { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 8px; margin-bottom: 16px; color: var(--text-primary); font-size: var(--font-size-sm); }
.style-toolbar button { @include controls.button(8px); border: 0; background: transparent; color: var(--accent-primary); cursor: pointer; &:disabled { opacity: .5; cursor: not-allowed; } }
.style-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 180px), 1fr)); gap: 16px; }
.style-field { display: flex; flex-direction: column; align-items: stretch; min-width: 0; gap: 8px; color: var(--text-secondary); font-size: var(--font-size-sm); }
.font-field { grid-column: 1 / -1; }
p { @include controls.hint; margin: 10px 0 16px; }
.shadow-fields { margin-top: 16px; }
</style>

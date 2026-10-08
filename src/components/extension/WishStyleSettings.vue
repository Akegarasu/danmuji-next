<script setup lang="ts">
import { computed, ref } from 'vue'
import FontPicker from '@/components/common/FontPicker.vue'
import baseCss from '../../../public/overlays/wish-machine/overlay.css?raw'
import testACss from '../../../public/overlays/wish-machine/theme-test-a.css?raw'
import type { WishConfig, WishGoal, WishTheme } from '@/types/wish-machine'

const props = defineProps<{ config: WishConfig; goals: WishGoal[]; disabled: boolean }>()
const emit = defineEmits<{ change: [value: WishConfig] }>()
const feedback = ref('')
const themes: { id: WishTheme; label: string; description: string }[] = [
  { id: 'default', label: '默认样式', description: '极简白字 · 无覆盖' },
  { id: 'test-a', label: '测试主题 A', description: '蓝色横条 · 白色数字框' },
  { id: 'custom', label: '自定义', description: '编辑 CSS · 自由调整' },
]
const css = computed(() => {
  if (props.config.theme === 'default') return ''
  if (props.config.theme === 'custom') return props.config.custom_css
  const family = (props.config.theme_a_font_family || '').trim().replace(/[\\"]/g, '\\$&').replace(/[\x00-\x1f\x7f]/g, '')
  // 按单个字体名称引用，预览、编辑框和复制内容共用同一份 CSS。
  return family ? `${testACss}\n#wish-machine { font-family: "${family}", WishHeavy, "Microsoft YaHei", sans-serif; }\n` : testACss
})
function changeFont(family: string) {
  feedback.value = ''
  emit('change', { ...props.config, theme_a_font_family: family })
}
const starterCss = '/* 修改下面的样式，再复制到 OBS「自定义 CSS」。 */\n#wish-machine { font-size: 28px; color: #ffffff; }\n#wish-list { gap: 8px; }\n#wish-list .wish-row { padding: 4px 6px; }\n#wish-list .wish-name { }\n#wish-list .wish-count { }\n#wish-list .wish-current { }\n#wish-list .wish-target { }\n'
function choose(theme: WishTheme) {
  feedback.value = ''
  emit('change', { ...props.config, theme, custom_css: theme === 'custom' ? props.config.custom_css || css.value || starterCss : props.config.custom_css })
}
function edit(event: Event) {
  feedback.value = ''
  emit('change', { ...props.config, theme: 'custom', custom_css: (event.target as HTMLTextAreaElement).value })
}
async function copy() {
  try { await navigator.clipboard.writeText(css.value); feedback.value = 'CSS 已复制，请粘贴到 OBS 浏览器源的「自定义 CSS」。' }
  catch { feedback.value = '复制失败，请选中下面的 CSS 手动复制。' }
}
const escapeHtml = (text: string | number) => String(text).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!))
// 禁止脚本、表单和外部资源；仅复用同源内置字体，避免每次预览内嵌整个字体。
const preview = computed(() => {
  const rows = props.goals.map(goal => {
    const id = escapeHtml(goal.id)
    return `<li id="wish-goal-${id}" class="wish-row" data-complete="${goal.current >= goal.target}"><span id="wish-name-${id}" class="wish-name">${escapeHtml(goal.gift_name)}</span><span id="wish-count-${id}" class="wish-count"><span id="wish-current-${id}" class="wish-current">${goal.current}</span><span id="wish-separator-${id}" class="wish-separator">/</span><span id="wish-target-${id}" class="wish-target">${goal.target}</span></span></li>`
  }).join('')
  // 阻止 CSS 中的结束标签逃离 style 节点。
  const styles = (baseCss.replace('../overtime/timer-heavy.otf', '/overlays/overtime/timer-heavy.otf') + '\n' + css.value).replace(/<\/style/gi, '<\\/style')
  return `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; font-src 'self'; base-uri 'none'; form-action 'none'"><style>${styles}</style></head><body><main id="wish-machine"><ul id="wish-list" aria-label="礼物心愿">${rows}</ul></main></body></html>`
})
</script>

<template>
  <div class="wish-style-settings">
    <div class="themes" role="group" aria-label="心愿机主题">
      <button v-for="theme in themes" :key="theme.id" class="theme" :class="{ selected: config.theme === theme.id }" :aria-pressed="config.theme === theme.id" :disabled="disabled" @click="choose(theme.id)">
        <span class="swatch" :class="theme.id"><span>心愿</span><b>0/5</b></span><strong>{{ theme.label }}</strong><small>{{ theme.description }}</small>
      </button>
    </div>
    <div v-if="config.theme === 'test-a'" class="theme-font">
      <span>主题字体</span>
      <FontPicker :model-value="config.theme_a_font_family || ''" label="主题字体" placeholder="默认：内置加班机字体" :disabled="disabled" @update:model-value="changeFont" />
      <p>支持搜索本机字体，也可手动输入字体名称。选择后同步更新预览和 CSS，OBS 所在电脑需要安装同一字体。</p>
    </div>
    <div class="preview-heading"><span>实时样式预览</span><small>宽度 400 px · 使用当前心愿数据</small></div>
    <div class="preview-surface"><iframe :srcdoc="preview" sandbox="allow-same-origin" referrerpolicy="no-referrer" title="心愿机主题预览" :style="{ height: `${Math.max(190, goals.length * 66 + 12)}px` }" /></div>
    <p v-if="!goals.length">当前没有心愿，添加后可查看样式效果。</p>
    <p>主题仅在此预览。保存可保留主题和 CSS 草稿；复制到 OBS 浏览器源的「自定义 CSS」后应用。建议源尺寸 400 × {{ Math.max(200, goals.length * 64 + 12) }}。</p>
    <div class="css-heading"><span>OBS 样式覆盖</span><button class="ext-btn ext-btn--primary" :disabled="!css" @click="copy">复制 CSS</button></div>
    <p v-if="config.theme === 'default'" class="default-hint">默认样式无需覆盖。恢复默认时，请清空 OBS 浏览器源中已粘贴的「自定义 CSS」。</p>
    <textarea v-else :value="css" :disabled="disabled" maxlength="32768" spellcheck="false" aria-label="OBS 自定义 CSS" @input="edit" />
    <p v-if="feedback" role="status">{{ feedback }}</p>
    <details>
      <summary>可自定义的节点与选择器</summary>
      <p>心愿 ID 在改名和排序后保持不变，可给单个目标设置专属样式。</p>
      <dl><dt>#wish-machine</dt><dd>整体字体、宽度与留白</dd><dt>#wish-list</dt><dd>列表排列与间距</dd><dt>#wish-list .wish-row</dt><dd>每行背景、边框与高度</dd><dt>.wish-name / .wish-count</dt><dd>礼物名称 / 数量框</dd><dt>.wish-current / .wish-separator / .wish-target</dt><dd>已获得数量 / 分隔符 / 目标数量</dd><dt>.wish-row[data-complete="true"]</dt><dd>已达成目标的行</dd></dl>
      <div v-for="goal in goals" :key="goal.id" class="node-reference"><span>{{ goal.gift_name }}</span><code>#wish-goal-{{ goal.id }}</code><code>#wish-name-{{ goal.id }}</code><code>#wish-count-{{ goal.id }}</code></div>
    </details>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/extension-shared';
@use '@/styles/settings-controls' as controls;
.themes { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 140px), 1fr)); gap: 10px; }
.theme { display: flex; flex-direction: column; text-align: left; gap: 7px; min-width: 0; padding: 12px; border: 1px solid var(--border-color); border-radius: 8px; background: var(--bg-primary); color: var(--text-primary); cursor: pointer; font: inherit; }
.theme.selected { border-color: var(--accent-primary); box-shadow: 0 0 0 1px var(--accent-primary); }
.theme:disabled { opacity: .5; cursor: not-allowed; }
.theme small { color: var(--text-muted); font-size: var(--font-size-xs); }
.theme strong { font-size: var(--font-size-sm); }
.swatch { display: flex; justify-content: space-between; align-items: center; gap: 8px; width: 100%; padding: 8px; background: #282930; color: white; border-radius: 4px; font-weight: 700; }
.swatch.test-a { background: #6978ff; }
.swatch.test-a b { background: white; color: #6978ff; padding: 0 3px; }
.swatch.custom { border: 1px dashed #777; }
.theme-font { display: grid; gap: 8px; margin-top: 16px; color: var(--text-secondary); font-size: var(--font-size-sm); }
.theme-font p { margin: 0; }
.preview-heading, .css-heading { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; margin: 16px 0 8px; color: var(--text-secondary); font-size: var(--font-size-sm); }
.preview-heading small { color: var(--text-muted); }
.preview-surface { overflow: auto; max-height: 350px; border-radius: 8px; border: 1px solid var(--border-color); background: repeating-conic-gradient(#292a2e 0% 25%, #202126 0% 50%) 50% / 20px 20px; }
iframe { display: block; width: 400px; border: 0; }
p { @include controls.hint; }
textarea { @include controls.control; height: 250px; padding: 12px; resize: vertical; font: 12px/1.7 Consolas, monospace; tab-size: 2; }
.default-hint { padding: 12px; background: var(--bg-primary); border-radius: 6px; }
details { margin-top: 16px; font-size: var(--font-size-xs); color: var(--text-secondary); }
summary { cursor: pointer; }
dl { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 8px; }
dt, code { overflow-wrap: anywhere; font-family: Consolas, monospace; }
dd { margin: 0; }
.node-reference { display: grid; gap: 4px; padding: 8px 0; border-top: 1px solid var(--border-color); }
.node-reference span { color: var(--text-primary); }
</style>

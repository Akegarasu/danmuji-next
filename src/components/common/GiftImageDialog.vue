<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef } from 'vue'
import { giftImageThemes, getAutoGiftImageTheme, giftImageBlob, loadGiftImageAssets, renderGiftImage, saveGiftImage } from '@/services/gift-image'
import type { GiftImageData, GiftImageTheme } from '@/services/gift-image'

const props = defineProps<{ gift: GiftImageData }>()
const emit = defineEmits<{ close: [] }>()
// 固定本次打开时的礼物数据，避免实时合并礼物使预览和导出的数量不一致。
const gift: GiftImageData = { ...props.gift, user: { ...props.gift.user } }
const selectedTheme = ref<GiftImageTheme | 'auto'>('auto')
const showAvatar = ref(true)
const assets = shallowRef<Awaited<ReturnType<typeof loadGiftImageAssets>>>()
const autoTheme = ref<GiftImageTheme>('pink')
const loading = ref(true)
const busy = ref(false)
const message = ref('')
const error = ref(false)
const closeButton = ref<HTMLButtonElement>()
const dialog = ref<HTMLElement>()
const previousFocus = document.activeElement as HTMLElement | null
let disposed = false
const theme = computed(() => selectedTheme.value === 'auto' ? autoTheme.value : selectedTheme.value)
const canvas = computed(() => assets.value ? renderGiftImage(gift, assets.value, theme.value, showAvatar.value) : null)
const preview = computed(() => canvas.value?.toDataURL('image/png'))
const close = () => { if (!busy.value) emit('close') }

async function exportImage(action: 'copy' | 'save') {
  if (!canvas.value || busy.value) return
  busy.value = true
  message.value = ''
  error.value = false
  try {
    if (action === 'copy') {
      if (!navigator.clipboard?.write || typeof ClipboardItem === 'undefined') throw new Error('当前环境不支持复制图片，请使用保存图片')
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': giftImageBlob(canvas.value) })])
      message.value = '图片已复制'
    } else if (await saveGiftImage(canvas.value, gift)) {
      message.value = '图片已保存'
    }
  } catch (e) {
    error.value = true
    message.value = `${action === 'copy' ? '复制' : '保存'}失败：${e instanceof Error ? e.message : String(e)}`
  } finally {
    busy.value = false
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close() }
  if (event.key !== 'Tab') return
  const buttons = dialog.value?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled)')
  if (!buttons?.length) return
  const first = buttons[0], last = buttons[buttons.length - 1]
  if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
}

onMounted(async () => {
  closeButton.value?.focus()
  document.addEventListener('keydown', onKeydown, true)
  try {
    const loaded = await loadGiftImageAssets(gift)
    if (disposed) return
    autoTheme.value = getAutoGiftImageTheme(gift, loaded.icon)
    assets.value = loaded
    message.value = loaded.warnings.join('；')
  } catch (e) {
    error.value = true
    message.value = `图片生成失败：${String(e)}`
  } finally {
    loading.value = false
  }
})
onBeforeUnmount(() => {
  disposed = true
  document.removeEventListener('keydown', onKeydown, true)
  previousFocus?.focus()
})
</script>

<template>
  <Teleport to="body">
    <div class="gift-image-mask" @mousedown.self="close" @contextmenu.prevent.stop>
      <section ref="dialog" class="gift-image-dialog" role="dialog" aria-modal="true" aria-labelledby="gift-image-title">
        <header>
          <h2 id="gift-image-title">礼物图片</h2>
          <button ref="closeButton" class="close" aria-label="关闭" :disabled="busy" @click="close">×</button>
        </header>
        <div class="dialog-body">
          <div class="preview" :aria-busy="loading">
            <span v-if="loading">正在生成图片…</span>
            <img v-else-if="preview" :src="preview" :alt="`${gift.user.name} ${gift.guard_level ? '开通' : '送出'} ${gift.gift_name} ×${gift.num}`" />
            <span v-else>图片生成失败，请关闭后重试</span>
          </div>
          <div class="options">
            <div class="themes" role="group" aria-label="图片配色">
              <button :class="{ active: selectedTheme === 'auto' }" :aria-pressed="selectedTheme === 'auto'" :disabled="busy" @click="selectedTheme = 'auto'">自动（{{ giftImageThemes[autoTheme].label }}）</button>
              <button v-for="(item, key) in giftImageThemes" :key="key" :class="{ active: selectedTheme === key }" :aria-pressed="selectedTheme === key" :disabled="busy" @click="selectedTheme = key">{{ item.label }}</button>
            </div>
            <label><input v-model="showAvatar" type="checkbox" :disabled="busy" />显示头像</label>
          </div>
          <p v-if="message" class="message" :class="{ error }" role="status">{{ message }}</p>
        </div>
        <footer>
          <button :disabled="!canvas || busy" @click="exportImage('copy')">复制图片</button>
          <button class="primary" :disabled="!canvas || busy" @click="exportImage('save')">{{ busy ? '处理中…' : '保存图片' }}</button>
        </footer>
      </section>
    </div>
  </Teleport>
</template>

<style scoped lang="scss">
@use '@/styles/settings-controls' as controls;

.gift-image-mask {
  position: fixed;
  inset: 0;
  z-index: 10000;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 8px;
  background: rgba(0, 0, 0, 0.45);
  // 弹窗挂载在 body，需独立裁剪遮罩和阴影，保留透明窗口的圆角。
  clip-path: inset(0 round var(--border-radius));
}
.gift-image-dialog {
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  width: 760px;
  max-width: 100%;
  max-height: 100%;
  overflow: hidden;
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius);
  background: var(--bg-secondary);
  color: var(--text-primary);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  header {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px 10px;
    border-bottom: 1px solid var(--border-color);
  }
  h2 { margin: 0; font-size: var(--font-size-base); font-weight: 600; }
  button {
    @include controls.button;
    border: 1px solid var(--border-color);
    border-radius: controls.$radius;
    background: var(--bg-card);
    color: var(--text-primary);
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
    &:hover:not(:disabled) { background: var(--bg-hover); }
    &:focus-visible { outline: 2px solid var(--accent-primary); outline-offset: 2px; }
    &:disabled { opacity: 0.5; cursor: not-allowed; }
    &.active, &.primary {
      border-color: var(--accent-primary);
      background: var(--accent-primary);
      color: white;
      font-weight: 500;
      &:hover:not(:disabled) { background: var(--accent-primary); opacity: 0.9; }
    }
    &.close {
      display: flex;
      align-items: center;
      justify-content: center;
      width: 28px;
      height: 28px;
      padding: 0;
      border: none;
      background: transparent;
      color: var(--text-muted);
      font-size: 18px;
      &:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
    }
  }
}
.dialog-body { min-height: 0; padding: 16px; overflow-y: auto; }
.preview {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 160px;
  border-radius: var(--border-radius-sm);
  overflow: hidden;
  color: var(--text-muted);
  background-color: var(--bg-secondary);
  background-image: conic-gradient(rgba(128,128,128,0.1) 25%, transparent 0 50%, rgba(128,128,128,0.1) 0 75%, transparent 0);
  background-size: 20px 20px;
  img { display: block; width: 100%; height: auto; }
}
.options { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; margin-top: 16px; }
.themes { display: flex; flex-wrap: wrap; gap: 6px; }
label { display: flex; align-items: center; gap: 8px; cursor: pointer; font-size: var(--font-size-sm); color: var(--text-secondary); }
input { accent-color: var(--accent-primary); }
.message { color: var(--text-secondary); margin: 12px 0 0; font-size: var(--font-size-sm); line-height: 1.6; overflow-wrap: anywhere; }
.message.error { color: #f87171; }
footer {
  display: flex;
  flex-shrink: 0;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 16px;
  border-top: 1px solid var(--border-color);
  button { padding-inline: 18px; background: var(--bg-active); font-weight: 500; }
}
@media (max-width: 480px) {
  .preview { min-height: 100px; }
}
</style>

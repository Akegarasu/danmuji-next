import { nextTick, onBeforeUnmount, onDeactivated, onMounted, ref, watch, type CSSProperties } from 'vue'

/** 颜色、字体等控件共用的浮层定位与关闭行为；Teleport 后仍跟随控件字号。 */
export function useControlPopover(width = 280, height = 360) {
  const anchor = ref<HTMLElement>()
  const panel = ref<HTMLElement>()
  const open = ref(false)
  const panelStyle = ref<CSSProperties>({ visibility: 'hidden' })
  let anchorBounds: DOMRect | undefined
  let observer: ResizeObserver | undefined

  function position() {
    if (!open.value || !anchor.value || !panel.value) return
    const rect = anchor.value.getBoundingClientRect()
    const below = window.innerHeight - rect.bottom - 14
    const above = rect.top - 14
    const aboveAnchor = below < height && above > below
    anchorBounds = rect
    panelStyle.value = {
      ...panelStyle.value, visibility: 'visible',
      left: `${Math.max(8, Math.min(rect.left, window.innerWidth - panel.value.offsetWidth - 8))}px`,
      top: `${aboveAnchor ? Math.max(8, rect.top - panel.value.offsetHeight - 6) : rect.bottom + 6}px`,
    }
  }

  function close(restoreFocus = false) {
    open.value = false
    if (restoreFocus) anchor.value?.focus({ preventScroll: true })
  }
  async function show() {
    if (!anchor.value || anchor.value.matches(':disabled')) return
    const rect = anchor.value.getBoundingClientRect()
    const style = getComputedStyle(anchor.value)
    const panelWidth = Math.min(Math.max(rect.width, width), window.innerWidth - 16)
    const below = window.innerHeight - rect.bottom - 14
    const above = rect.top - 14
    const aboveAnchor = below < height && above > below
    const available = Math.max(0, aboveAnchor ? above : below)
    panelStyle.value = {
      ...panelStyle.value,
      // 已打开的浮层刷新内容时保持可见，避免隐藏当前搜索框导致焦点和滚动跳动。
      visibility: open.value ? 'visible' : 'hidden', width: `${panelWidth}px`, maxHeight: `${available}px`,
      fontSize: style.fontSize, fontFamily: style.fontFamily,
    }
    open.value = true
    await nextTick()
    if (!open.value || !panel.value) return
    position()
  }
  function outside(event: Event) {
    if (!open.value || !(event.target instanceof Node)) return
    if (!anchor.value?.contains(event.target) && !panel.value?.contains(event.target)) close()
  }
  function scroll(event: Event) {
    if (!open.value || !anchor.value || !anchorBounds || !(event.target instanceof Node) || panel.value?.contains(event.target)) return
    const rect = anchor.value.getBoundingClientRect()
    if (Math.abs(rect.top - anchorBounds.top) > .5 || Math.abs(rect.left - anchorBounds.left) > .5) close()
  }
  const resize = () => close()
  onMounted(() => {
    observer = new ResizeObserver(position)
    document.addEventListener('pointerdown', outside)
    document.addEventListener('focusin', outside)
    document.addEventListener('scroll', scroll, true)
    window.addEventListener('resize', resize)
  })
  watch(panel, element => { observer?.disconnect(); if (element) observer?.observe(element) })
  onDeactivated(() => close())
  onBeforeUnmount(() => {
    observer?.disconnect()
    document.removeEventListener('pointerdown', outside)
    document.removeEventListener('focusin', outside)
    document.removeEventListener('scroll', scroll, true)
    window.removeEventListener('resize', resize)
  })
  return { anchor, panel, open, panelStyle, show, close }
}

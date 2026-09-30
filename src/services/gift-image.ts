import { invoke, isTauri } from '@tauri-apps/api/core'
import type { ProcessedGift } from '@/types'
import { getGiftIcon, getGuardIcon } from '@/services/gift-icons'

export type GiftImageData = Pick<ProcessedGift, 'user' | 'gift_id' | 'gift_name' | 'gift_icon' | 'num' | 'guard_level'>
export const giftImageThemes = {
  night: { label: '星夜', colors: ['#55569a', '#8599d4'] },
  pink: { label: '樱粉', colors: ['#f18ca9', '#ffb58c'] },
  mint: { label: '薄荷', colors: ['#389e9b', '#8bcbb0'] },
  amber: { label: '琥珀', colors: ['#ca813e', '#efbb67'] },
} as const
export type GiftImageTheme = keyof typeof giftImageThemes
export type GiftImageAssets = { avatar: HTMLImageElement | null; icon: HTMLImageElement | null; warnings: string[] }

function loadImage(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image()
    const timer = window.setTimeout(() => finish(new Error('图片加载超时')), 12000)
    const finish = (error?: Error) => {
      window.clearTimeout(timer)
      image.onload = image.onerror = null
      if (error) reject(error)
      else resolve(image)
    }
    image.crossOrigin = 'anonymous'
    image.referrerPolicy = 'no-referrer'
    image.onload = () => finish()
    image.onerror = () => finish(new Error('图片加载失败'))
    image.src = url
  })
}

export async function loadGiftImageAssets(gift: GiftImageData): Promise<GiftImageAssets> {
  const iconUrl = gift.guard_level
    ? getGuardIcon(gift.guard_level)
    : gift.gift_icon || getGiftIcon(gift.gift_id, gift.gift_name)
  const warnings: string[] = []
  const load = async (url: string | undefined, label: string) => {
    if (!url) return null
    try {
      // 由后端读取 B 站图片，避免跨域图片污染画布导致 PNG 无法导出。
      const source = isTauri() ? await invoke<string>('load_gift_image_asset', { url }) : url
      return await loadImage(source)
    } catch {
      warnings.push(`${label}加载失败，已使用文字占位`)
      return null
    }
  }
  const [avatar, icon] = await Promise.all([load(gift.user.face, '头像'), load(iconUrl, '礼物图标')])
  return { avatar, icon, warnings }
}

/** 优先取礼物图标主色；没有可用图标时按大航海等级选择。 */
export function getAutoGiftImageTheme(gift: GiftImageData, icon: HTMLImageElement | null): GiftImageTheme {
  if (icon) {
    const canvas = document.createElement('canvas')
    canvas.width = canvas.height = 24
    const ctx = canvas.getContext('2d')!
    ctx.drawImage(icon, 0, 0, 24, 24)
    const pixels = ctx.getImageData(0, 0, 24, 24).data
    const scores = { night: 0, pink: 0, mint: 0, amber: 0 }
    for (let i = 0; i < pixels.length; i += 4) {
      const [r, g, b, a] = Array.from(pixels.slice(i, i + 4))
      const max = Math.max(r, g, b), min = Math.min(r, g, b)
      if (a < 128 || max - min < 35) continue
      let hue = max === r ? ((g - b) / (max - min)) % 6 : max === g ? (b - r) / (max - min) + 2 : (r - g) / (max - min) + 4
      hue = (hue * 60 + 360) % 360
      const theme = hue < 18 || hue >= 300 ? 'pink' : hue < 75 ? 'amber' : hue < 185 ? 'mint' : 'night'
      scores[theme] += (max - min) * a
    }
    const best = (Object.keys(scores) as GiftImageTheme[]).sort((a, b) => scores[b] - scores[a])[0]
    if (scores[best] > 0) return best
  }
  return gift.guard_level === 1 ? 'amber' : gift.guard_level ? 'night' : 'pink'
}

export function renderGiftImage(gift: GiftImageData, assets: GiftImageAssets, theme: GiftImageTheme, showAvatar: boolean): HTMLCanvasElement {
  const canvas = document.createElement('canvas')
  const ctx = canvas.getContext('2d')!
  const font = '"Microsoft YaHei", "PingFang SC", sans-serif'
  ctx.font = `700 36px ${font}`
  const name = gift.user.name || '未知用户'
  const giftName = gift.gift_name || '礼物'
  const action = gift.guard_level ? '开通' : '送出'
  const count = `×${gift.num}`
  const nameWidth = ctx.measureText(name).width
  const giftWidth = ctx.measureText(giftName).width
  const countWidth = ctx.measureText(count).width
  const actionWidth = ctx.measureText(action).width
  const contentWidth = nameWidth + giftWidth + actionWidth + countWidth + 72 + 4 * 20
  // 长名称自动缩小文字，避免截图裁掉用户名或数量；导出固定使用两倍分辨率。
  const scale = Math.min(1, (1560 - (showAvatar ? 96 : 0) - 48) / contentWidth)
  const width = Math.ceil(contentWidth * scale + (showAvatar ? 96 : 0) + 48)
  canvas.width = (width + 48) * 2
  canvas.height = 160 * 2
  ctx.scale(2, 2)
  const gradient = ctx.createLinearGradient(24, 0, width + 24, 0)
  giftImageThemes[theme].colors.forEach((color, i) => gradient.addColorStop(i, color))
  ctx.fillStyle = gradient
  ctx.shadowColor = 'rgba(35, 46, 55, 0.22)'
  ctx.shadowBlur = 12
  ctx.shadowOffsetY = 4
  ctx.beginPath()
  ctx.roundRect(24, 28, width, 104, 52)
  ctx.fill()
  ctx.shadowColor = 'transparent'
  ctx.strokeStyle = 'rgba(255, 255, 255, 0.48)'
  ctx.lineWidth = 3
  ctx.stroke()
  let x = 48
  if (showAvatar) {
    ctx.save()
    ctx.beginPath()
    ctx.arc(76, 80, 43, 0, Math.PI * 2)
    ctx.clip()
    ctx.fillStyle = 'rgba(255,255,255,0.18)'
    ctx.fillRect(33, 37, 86, 86)
    if (assets.avatar) {
      const size = Math.min(assets.avatar.naturalWidth, assets.avatar.naturalHeight)
      ctx.drawImage(assets.avatar, (assets.avatar.naturalWidth - size) / 2, (assets.avatar.naturalHeight - size) / 2, size, size, 33, 37, 86, 86)
    } else {
      ctx.fillStyle = '#fff'
      ctx.font = `700 36px ${font}`
      ctx.textAlign = 'center'
      ctx.textBaseline = 'middle'
      ctx.fillText(Array.from(name)[0], 76, 81)
    }
    ctx.restore()
    ctx.beginPath()
    ctx.arc(76, 80, 43, 0, Math.PI * 2)
    ctx.strokeStyle = 'rgba(255,255,255,0.85)'
    ctx.stroke()
    x = 140
  }
  ctx.save()
  ctx.translate(x, 80)
  ctx.scale(scale, scale)
  ctx.textBaseline = 'middle'
  ctx.fillStyle = '#fff'
  ctx.font = `700 36px ${font}`
  ctx.fillText(name, 0, 1)
  x = nameWidth + 20
  ctx.fillStyle = 'rgba(255,255,255,0.8)'
  ctx.font = `400 36px ${font}`
  ctx.fillText(action, x, 1)
  x += actionWidth + 20
  ctx.fillStyle = '#fff'
  ctx.font = `700 36px ${font}`
  ctx.fillText(giftName, x, 1)
  x += giftWidth + 20
  if (assets.icon) {
    const ratio = Math.min(72 / assets.icon.naturalWidth, 72 / assets.icon.naturalHeight)
    const w = assets.icon.naturalWidth * ratio, h = assets.icon.naturalHeight * ratio
    ctx.drawImage(assets.icon, x + (72 - w) / 2, -h / 2, w, h)
  } else {
    ctx.save()
    ctx.fillStyle = 'rgba(255,255,255,0.2)'
    ctx.beginPath()
    ctx.roundRect(x + 6, -30, 60, 60, 16)
    ctx.fill()
    ctx.fillStyle = '#fff'
    ctx.textAlign = 'center'
    ctx.font = `700 28px ${font}`
    ctx.fillText('礼', x + 36, 1)
    ctx.restore()
  }
  ctx.fillText(count, x + 72 + 20, 1)
  ctx.restore()
  return canvas
}

export function giftImageBlob(canvas: HTMLCanvasElement): Promise<Blob> {
  return new Promise((resolve, reject) => canvas.toBlob(blob => blob ? resolve(blob) : reject(new Error('PNG 生成失败')), 'image/png'))
}

export async function saveGiftImage(canvas: HTMLCanvasElement, gift: GiftImageData): Promise<boolean> {
  const filename = `${gift.user.name}-${gift.gift_name}×${gift.num}`.replace(/[<>:"/\\|?*\u0000-\u001f]/g, '_').slice(0, 100) + '.png'
  if (isTauri()) {
    return invoke<boolean>('save_gift_image', { pngBase64: canvas.toDataURL('image/png').split(',')[1], filename })
  }
  const url = URL.createObjectURL(await giftImageBlob(canvas))
  const link = document.createElement('a')
  link.href = url
  link.download = filename
  link.click()
  window.setTimeout(() => URL.revokeObjectURL(url), 1000)
  return true
}

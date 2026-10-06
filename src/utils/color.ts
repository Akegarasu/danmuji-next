export interface HsvColor { h: number; s: number; v: number }

export function normalizeHex(value: string): string | null {
  const hex = value.trim().replace(/^#/, '')
  if (/^[\da-f]{6}$/i.test(hex)) return `#${hex.toLowerCase()}`
  if (/^[\da-f]{3}$/i.test(hex)) return `#${[...hex].map(char => char + char).join('').toLowerCase()}`
  return null
}

export function hexToHsv(value: string): HsvColor {
  const hex = normalizeHex(value) ?? '#ffffff'
  const [r, g, b] = [1, 3, 5].map(index => parseInt(hex.slice(index, index + 2), 16) / 255)
  const max = Math.max(r, g, b), min = Math.min(r, g, b), delta = max - min
  const h = delta === 0 ? 0 : max === r ? ((g - b) / delta + 6) % 6 : max === g ? (b - r) / delta + 2 : (r - g) / delta + 4
  return { h: h * 60, s: max === 0 ? 0 : delta / max * 100, v: max * 100 }
}

export function hsvToHex({ h, s, v }: HsvColor): string {
  const hue = ((h % 360) + 360) % 360 / 60
  const brightness = Math.min(100, Math.max(0, v)) / 100
  const chroma = brightness * Math.min(100, Math.max(0, s)) / 100
  const x = chroma * (1 - Math.abs(hue % 2 - 1))
  const rgb = hue < 1 ? [chroma, x, 0] : hue < 2 ? [x, chroma, 0] : hue < 3 ? [0, chroma, x] : hue < 4 ? [0, x, chroma] : hue < 5 ? [x, 0, chroma] : [chroma, 0, x]
  return `#${rgb.map(channel => Math.round((channel + brightness - chroma) * 255).toString(16).padStart(2, '0')).join('')}`
}

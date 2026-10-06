/** OBS 共用外观协议；字号以倒计时或歌名为基准，其余文字按比例缩放。 */
export interface OverlayStyle {
  font_family: string
  font_size: number
  font_weight: number
  text_color: string
  secondary_color: string
  shadow_enabled: boolean
  shadow_color: string
  shadow_blur: number
  shadow_offset_x: number
  shadow_offset_y: number
}

export function defaultOverlayStyle(preset: 'overtime' | 'song-request'): OverlayStyle {
  return {
    font_family: '',
    font_size: preset === 'overtime' ? 96 : 28,
    font_weight: preset === 'overtime' ? 400 : 600,
    text_color: '#ffffff',
    secondary_color: '#ffffff',
    shadow_enabled: true,
    shadow_color: '#000000',
    shadow_blur: preset === 'overtime' ? 10 : 2,
    shadow_offset_x: 0,
    shadow_offset_y: preset === 'overtime' ? 0 : 1,
  }
}

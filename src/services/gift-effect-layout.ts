export interface GiftEffectLayout {
  mode: 'fit-width' | 'native'
  width: number
  height: number
}

/**
 * 官方礼物存在 720×1280 的完整场景，以及 300×579、360×360、720×720 等小画布。
 * type 表示业务类型，JSON 的 scale 表示 Alpha 压缩比例，均不能作为显示缩放依据。
 * 完整场景按窗口宽度缩放；其他画布保守地保持原尺寸，仅在放不下时等比缩小。
 */
export function getGiftEffectLayout(
  width: number,
  height: number,
  containerWidth: number,
  containerHeight: number
): GiftEffectLayout {
  const isFullScene = Math.min(width, height) >= 720 && Math.max(width, height) >= 1280
  const mode = isFullScene ? 'fit-width' : 'native'
  if (![width, height, containerWidth, containerHeight].every(value => Number.isFinite(value) && value > 0)) {
    return { mode, width: 0, height: 0 }
  }

  const scale = isFullScene
    ? containerWidth / width
    : Math.min(1, containerWidth / width, containerHeight / height)
  return { mode, width: width * scale, height: height * scale }
}

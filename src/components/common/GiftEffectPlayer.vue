<script setup lang="ts">
/**
 * 礼物全屏特效播放器。
 *
 * Bilibili 的 web_mp4 不是普通带 alpha 的 MP4，而是把 RGB 和灰度 Alpha
 * 打包到同一帧的两个矩形中。这里复刻网页端的 WebGL 合成：左侧 RGB
 * 区域作为颜色，右侧 aFrame 的红色通道作为透明度。
 */

import {
  computed,
  onMounted,
  onUnmounted,
  ref,
  shallowReactive,
  shallowRef,
  watch
} from 'vue'
import { useDanmakuStore } from '@/stores/danmaku'
import { useSettingsStore } from '@/stores/settings'
import {
  findGiftEffect,
  getGiftEffectConfig
} from '@/services/blive-client'
import type {
  GiftEffectAnimationConfig,
  GiftEffectAnimationInfo,
  GiftEffectResource,
  GiftEffectTrigger
} from '@/types'
import { createLogger } from '@/services/logger'

const MAX_SLOTS = 3
const CANPLAY_TIMEOUT_MS = 20_000
const MIN_PLAYBACK_TIMEOUT_MS = 30_000
const MAX_PLAYBACK_TIMEOUT_MS = 5 * 60_000
const CONFIG_REFRESH_MAX_DELAY_MS = 24 * 60 * 60_000

interface PlaybackRequest {
  trigger: GiftEffectTrigger
  resource: GiftEffectResource
}

interface PlaybackSlot {
  token: number
  busy: boolean
  animationFrame: number | null
  cancelPlayback: (() => void) | null
  video: HTMLVideoElement | null
  canvas: HTMLCanvasElement | null
  gl: WebGLRenderingContext | null
  texture: WebGLTexture | null
  buffer: WebGLBuffer | null
  program: WebGLProgram | null
}

const logger = createLogger('GiftEffectPlayer')
const danmakuStore = useDanmakuStore()
const settingsStore = useSettingsStore()

const slots = shallowReactive<PlaybackSlot[]>(
  Array.from({ length: MAX_SLOTS }, () => ({
    token: 0,
    busy: false,
    animationFrame: null,
    cancelPlayback: null,
    video: null,
    canvas: null,
    gl: null,
    texture: null,
    buffer: null,
    program: null
  }))
)
const canvasRefs = ref<(HTMLCanvasElement | null)[]>([])
const videoRefs = ref<(HTMLVideoElement | null)[]>([])
// 官方配置约 1 MB 且只读，使用 shallowRef 避免为数千条资源创建深层代理。
const config = shallowRef<Awaited<ReturnType<typeof getGiftEffectConfig>> | null>(null)
const waitingTriggers: GiftEffectTrigger[] = []
const playQueue: PlaybackRequest[] = []
const animationConfigCache = new Map<string, Promise<GiftEffectAnimationConfig>>()
const seenTriggerIds = new Set<string>()
let seenTriggerGeneration = -1
let configLoadToken = 0
let configRefreshTimer: number | null = null
let targetRoomId = 0

const roomId = computed(() => {
  const value = Number.parseInt(danmakuStore.roomInfo.roomId, 10)
  return Number.isSafeInteger(value) && value > 0 ? value : 0
})

const maxConcurrent = computed(() => Math.min(
  MAX_SLOTS,
  Math.max(1, Math.trunc(settingsStore.giftEffectMaxConcurrent || 1))
))

const queueLimit = computed(() => Math.min(
  100,
  Math.max(1, Math.trunc(settingsStore.giftEffectQueueLimit || 1))
))

const setCanvasRef = (element: Element | null, index: number) => {
  canvasRefs.value[index] = element instanceof HTMLCanvasElement ? element : null
}

const setVideoRef = (element: Element | null, index: number) => {
  videoRefs.value[index] = element instanceof HTMLVideoElement ? element : null
}

function clampQueue(): void {
  // 队列积压时优先丢弃最旧特效，避免长时间播放已经过时的礼物动画。
  while (waitingTriggers.length + playQueue.length > queueLimit.value) {
    if (playQueue.length > 0) playQueue.shift()
    else waitingTriggers.shift()
  }
}

function clearQueue(): void {
  waitingTriggers.length = 0
  playQueue.length = 0
}

function compileShader(
  gl: WebGLRenderingContext,
  type: number,
  source: string
): WebGLShader {
  const shader = gl.createShader(type)
  if (!shader) throw new Error('创建 WebGL shader 失败')
  gl.shaderSource(shader, source)
  gl.compileShader(shader)
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    const message = gl.getShaderInfoLog(shader) || '未知 shader 编译错误'
    gl.deleteShader(shader)
    throw new Error(`WebGL shader 编译失败：${message}`)
  }
  return shader
}

function createProgram(gl: WebGLRenderingContext): WebGLProgram {
  const vertexShader = compileShader(gl, gl.VERTEX_SHADER, `
    attribute vec2 a_position;
    attribute vec2 a_texCoord;
    attribute vec2 a_alpha_texCoord;
    varying vec2 v_texCoord;
    varying vec2 v_alpha_texCoord;

    void main(void) {
      gl_Position = vec4(a_position, 0.0, 1.0);
      v_texCoord = a_texCoord;
      v_alpha_texCoord = a_alpha_texCoord;
    }
  `)
  const fragmentShader = compileShader(gl, gl.FRAGMENT_SHADER, `
    precision lowp float;
    varying vec2 v_texCoord;
    varying vec2 v_alpha_texCoord;
    uniform sampler2D u_image_video;

    void main(void) {
      vec4 color = texture2D(u_image_video, v_texCoord);
      float alpha = texture2D(u_image_video, v_alpha_texCoord).r;
      gl_FragColor = vec4(color.rgb, alpha);
    }
  `)
  const program = gl.createProgram()
  if (!program) {
    gl.deleteShader(vertexShader)
    gl.deleteShader(fragmentShader)
    throw new Error('创建 WebGL program 失败')
  }
  gl.attachShader(program, vertexShader)
  gl.attachShader(program, fragmentShader)
  gl.linkProgram(program)
  gl.deleteShader(vertexShader)
  gl.deleteShader(fragmentShader)
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    const message = gl.getProgramInfoLog(program) || '未知 program 链接错误'
    gl.deleteProgram(program)
    throw new Error(`WebGL program 链接失败：${message}`)
  }
  return program
}

function normalizedFrame(
  frame: [number, number, number, number] | undefined,
  fallback: [number, number, number, number]
): [number, number, number, number] {
  if (!frame || frame.length !== 4 || frame.some(value => !Number.isFinite(value))) {
    return fallback
  }
  return frame
}

function setupWebGl(
  slot: PlaybackSlot,
  info: GiftEffectAnimationInfo
): void {
  const canvas = slot.canvas
  if (!canvas) throw new Error('特效画布尚未准备好')

  const videoWidth = Number.isFinite(info.videoW) && Number(info.videoW) > 0
    ? Number(info.videoW)
    : 256
  const videoHeight = Number.isFinite(info.videoH) && Number(info.videoH) > 0
    ? Number(info.videoH)
    : 256
  const rgbFrame = normalizedFrame(info.rgbFrame, [0, 0, videoWidth, videoHeight])
  const alphaFrame = normalizedFrame(info.aFrame, [0, 0, 0, 0])
  const [rgbX, rgbY, rgbWidth, rgbHeight] = rgbFrame
  const [alphaX, alphaY, alphaWidth, alphaHeight] = alphaFrame

  if (
    rgbX < 0 || rgbY < 0 || alphaX < 0 || alphaY < 0
    || rgbWidth <= 0 || rgbHeight <= 0 || alphaWidth <= 0 || alphaHeight <= 0
    || rgbX + rgbWidth > videoWidth || rgbY + rgbHeight > videoHeight
    || alphaX + alphaWidth > videoWidth || alphaY + alphaHeight > videoHeight
  ) {
    throw new Error('特效 JSON 缺少有效的 RGB/Alpha 帧区域')
  }

  // 与 Bilibili 网页端一致：canvas 的实际像素尺寸是 RGB 输出区域。
  canvas.width = Math.round(rgbWidth)
  canvas.height = Math.round(rgbHeight)

  const gl = canvas.getContext('webgl', {
    alpha: true,
    premultipliedAlpha: true,
    depth: false,
    antialias: true
  })
  if (!gl) throw new Error('当前 WebView 不支持 WebGL，无法合成礼物透明特效')

  const program = createProgram(gl)
  const texture = gl.createTexture()
  const buffer = gl.createBuffer()
  if (!texture || !buffer) {
    if (texture) gl.deleteTexture(texture)
    if (buffer) gl.deleteBuffer(buffer)
    gl.deleteProgram(program)
    throw new Error('创建 WebGL 缓冲区失败')
  }

  const rgbTexCoord: [number, number, number, number] = [
    rgbX / videoWidth,
    (rgbX + rgbWidth) / videoWidth,
    (videoHeight - rgbY - rgbHeight) / videoHeight,
    (videoHeight - rgbY) / videoHeight
  ]
  const alphaTexCoord: [number, number, number, number] = [
    alphaX / videoWidth,
    (alphaX + alphaWidth) / videoWidth,
    (videoHeight - alphaY - alphaHeight) / videoHeight,
    (videoHeight - alphaY) / videoHeight
  ]

  // TRIANGLE_STRIP 的四个顶点：左下、右下、左上、右上。
  const vertices = new Float32Array([
    -1, -1, rgbTexCoord[0], rgbTexCoord[2], alphaTexCoord[0], alphaTexCoord[2],
     1, -1, rgbTexCoord[1], rgbTexCoord[2], alphaTexCoord[1], alphaTexCoord[2],
    -1,  1, rgbTexCoord[0], rgbTexCoord[3], alphaTexCoord[0], alphaTexCoord[3],
     1,  1, rgbTexCoord[1], rgbTexCoord[3], alphaTexCoord[1], alphaTexCoord[3]
  ])

  gl.useProgram(program)
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer)
  gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.STATIC_DRAW)
  const stride = 6 * Float32Array.BYTES_PER_ELEMENT
  const positionLocation = gl.getAttribLocation(program, 'a_position')
  const texLocation = gl.getAttribLocation(program, 'a_texCoord')
  const alphaTexLocation = gl.getAttribLocation(program, 'a_alpha_texCoord')
  gl.enableVertexAttribArray(positionLocation)
  gl.vertexAttribPointer(positionLocation, 2, gl.FLOAT, false, stride, 0)
  gl.enableVertexAttribArray(texLocation)
  gl.vertexAttribPointer(texLocation, 2, gl.FLOAT, false, stride, 2 * Float32Array.BYTES_PER_ELEMENT)
  gl.enableVertexAttribArray(alphaTexLocation)
  gl.vertexAttribPointer(
    alphaTexLocation,
    2,
    gl.FLOAT,
    false,
    stride,
    4 * Float32Array.BYTES_PER_ELEMENT
  )

  gl.activeTexture(gl.TEXTURE0)
  gl.bindTexture(gl.TEXTURE_2D, texture)
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE)
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE)
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR)
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR)
  const samplerLocation = gl.getUniformLocation(program, 'u_image_video')
  if (samplerLocation) gl.uniform1i(samplerLocation, 0)

  gl.clearColor(0, 0, 0, 0)
  gl.clear(gl.COLOR_BUFFER_BIT)
  gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, true)
  gl.enable(gl.BLEND)
  gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA)

  slot.gl = gl
  slot.program = program
  slot.texture = texture
  slot.buffer = buffer
}

function toHttps(url: string): string {
  return url.replace(/^http:/, 'https:')
}

function getAnimationConfig(url: string): Promise<GiftEffectAnimationConfig> {
  const normalizedUrl = toHttps(url)
  const cached = animationConfigCache.get(normalizedUrl)
  if (cached) return cached

  const controller = new AbortController()
  const timeout = window.setTimeout(() => controller.abort(), CANPLAY_TIMEOUT_MS)
  const request = fetch(normalizedUrl, { mode: 'cors', signal: controller.signal })
    .then(async response => {
      if (!response.ok) throw new Error(`获取特效 JSON 失败（HTTP ${response.status}）`)
      const value = await response.json() as GiftEffectAnimationConfig
      if (!value?.info) throw new Error('特效 JSON 格式无效')
      return value
    })
    .catch(error => {
      animationConfigCache.delete(normalizedUrl)
      throw error
    })
    .finally(() => window.clearTimeout(timeout))
  animationConfigCache.set(normalizedUrl, request)
  return request
}

function chooseResource(resource: GiftEffectResource): { videoUrl: string; jsonUrl: string } | null {
  // 网页端只使用成对下发的 web_mp4 与 web_mp4_json。横竖屏资源没有
  // 对应透明通道布局，不能把视频地址误当成 JSON 回退。
  if (!resource.web_mp4 || !resource.web_mp4_json) return null
  return {
    videoUrl: toHttps(resource.web_mp4),
    jsonUrl: toHttps(resource.web_mp4_json)
  }
}

function cleanupSlotResources(slot: PlaybackSlot): void {
  if (slot.animationFrame !== null) {
    cancelAnimationFrame(slot.animationFrame)
    slot.animationFrame = null
  }

  if (slot.video) {
    slot.video.pause()
    slot.video.removeAttribute('src')
    slot.video.load()
  }

  if (slot.gl) {
    if (slot.texture) slot.gl.deleteTexture(slot.texture)
    if (slot.buffer) slot.gl.deleteBuffer(slot.buffer)
    if (slot.program) slot.gl.deleteProgram(slot.program)
    slot.gl.clearColor(0, 0, 0, 0)
    slot.gl.clear(slot.gl.COLOR_BUFFER_BIT)
  }

  if (slot.canvas) {
    slot.canvas.width = 0
    slot.canvas.height = 0
  }

  slot.gl = null
  slot.texture = null
  slot.buffer = null
  slot.program = null
}

function stopSlot(slot: PlaybackSlot): void {
  slot.token += 1
  slot.cancelPlayback?.()
  slot.cancelPlayback = null
  cleanupSlotResources(slot)
  slot.busy = false
}

function stopAllSlots(): void {
  for (const slot of slots) stopSlot(slot)
}

function waitForCanPlay(video: HTMLVideoElement): Promise<void> {
  return new Promise((resolve, reject) => {
    let settled = false
    const timeout = window.setTimeout(() => finish(new Error('等待特效视频加载超时')), CANPLAY_TIMEOUT_MS)
    const cleanup = () => {
      window.clearTimeout(timeout)
      video.removeEventListener('canplay', onCanPlay)
      video.removeEventListener('error', onError)
    }
    const finish = (error?: Error) => {
      if (settled) return
      settled = true
      cleanup()
      if (error) reject(error)
      else resolve()
    }
    const onCanPlay = () => finish()
    const onError = () => finish(new Error('特效视频加载失败'))

    video.addEventListener('canplay', onCanPlay, { once: true })
    video.addEventListener('error', onError, { once: true })
    if (video.readyState >= video.HAVE_FUTURE_DATA) finish()
  })
}

async function playRequest(request: PlaybackRequest, slot: PlaybackSlot): Promise<void> {
  const token = ++slot.token
  const video = videoRefs.value[slots.indexOf(slot)]
  const canvas = canvasRefs.value[slots.indexOf(slot)]
  if (!video || !canvas) throw new Error('特效播放器节点尚未准备好')

  slot.video = video
  slot.canvas = canvas
  slot.busy = true

  const selected = chooseResource(request.resource)
  if (!selected) throw new Error('官方特效配置没有可用的 MP4 资源')

  const animationConfig = await getAnimationConfig(selected.jsonUrl)
  if (token !== slot.token) return
  setupWebGl(slot, animationConfig.info)

  video.crossOrigin = 'anonymous'
  video.muted = true
  video.playsInline = true
  video.src = selected.videoUrl
  video.load()
  await waitForCanPlay(video)
  if (token !== slot.token) return
  await video.play()

  await new Promise<void>((resolve, reject) => {
    let lastRenderedTime = -1
    const durationMs = Number.isFinite(video.duration)
      ? video.duration * 1000 + 10_000
      : MIN_PLAYBACK_TIMEOUT_MS
    const playbackTimeoutMs = Math.min(
      MAX_PLAYBACK_TIMEOUT_MS,
      Math.max(MIN_PLAYBACK_TIMEOUT_MS, durationMs)
    )
    const timeout = window.setTimeout(
      () => finish(new Error('礼物特效播放超时')),
      playbackTimeoutMs
    )
    const render = () => {
      if (token !== slot.token) return
      try {
        const gl = slot.gl
        if (
          gl
          && video.readyState >= video.HAVE_CURRENT_DATA
          && video.currentTime !== lastRenderedTime
        ) {
          lastRenderedTime = video.currentTime
          gl.viewport(0, 0, canvas.width, canvas.height)
          gl.clear(gl.COLOR_BUFFER_BIT)
          gl.bindTexture(gl.TEXTURE_2D, slot.texture)
          gl.texImage2D(
            gl.TEXTURE_2D,
            0,
            gl.RGBA,
            gl.RGBA,
            gl.UNSIGNED_BYTE,
            video
          )
          gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4)
        }
        slot.animationFrame = requestAnimationFrame(render)
      } catch (error) {
        finish(error instanceof Error ? error : new Error(String(error)))
      }
    }
    const onEnded = () => finish()
    const onError = () => finish(new Error('特效视频播放失败'))
    const cleanup = () => {
      window.clearTimeout(timeout)
      video.removeEventListener('ended', onEnded)
      video.removeEventListener('error', onError)
      if (slot.animationFrame !== null) {
        cancelAnimationFrame(slot.animationFrame)
        slot.animationFrame = null
      }
      slot.cancelPlayback = null
    }
    const finish = (error?: Error) => {
      cleanup()
      if (error) reject(error)
      else resolve()
    }

    slot.cancelPlayback = () => finish()
    video.addEventListener('ended', onEnded, { once: true })
    video.addEventListener('error', onError, { once: true })
    slot.animationFrame = requestAnimationFrame(render)
  })
}

function finishSlot(slot: PlaybackSlot, token: number): void {
  if (slot.token !== token) return
  cleanupSlotResources(slot)
  slot.busy = false
  slot.cancelPlayback = null
  pump()
}

function startNext(request: PlaybackRequest, slot: PlaybackSlot): void {
  const token = slot.token + 1
  void playRequest(request, slot)
    .catch(error => {
      if (slot.token === token) {
        logger.warn('礼物全屏特效播放失败:', error)
      }
    })
    .finally(() => finishSlot(slot, token))
}

function pump(): void {
  if (!settingsStore.giftEffectEnabled) return
  clampQueue()

  if (config.value) {
    while (waitingTriggers.length > 0) {
      const trigger = waitingTriggers.shift()
      if (!trigger) break
      const resource = findGiftEffect(config.value, trigger.gift_id, trigger.effect_id)
      if (resource) playQueue.push({ trigger, resource })
    }
  }
  clampQueue()

  let activeCount = slots.filter(slot => slot.busy).length
  while (activeCount < maxConcurrent.value && playQueue.length > 0) {
    const request = playQueue.shift()
    const slot = slots.find(candidate => !candidate.busy)
    if (!request || !slot) break
    activeCount += 1
    startNext(request, slot)
  }
}

function acceptTrigger(trigger: GiftEffectTrigger): void {
  if (!settingsStore.giftEffectEnabled || !danmakuStore.isConnected) return
  if (trigger.total_value < settingsStore.giftEffectMinPrice) return
  // 官方全屏礼物都是付费礼物；保留 is_paid 判断可避免误触发免费互动消息。
  if (!trigger.is_paid) return

  if (config.value) {
    const resource = findGiftEffect(config.value, trigger.gift_id, trigger.effect_id)
    if (!resource) return
    playQueue.push({ trigger, resource })
  } else {
    waitingTriggers.push(trigger)
  }
  clampQueue()
  pump()
}

function consumeStoreTriggers(): void {
  const generation = danmakuStore.giftEffectGeneration
  if (generation !== seenTriggerGeneration) {
    seenTriggerGeneration = generation
    seenTriggerIds.clear()
  }

  const triggers = danmakuStore.giftEffectTriggers
  for (const trigger of triggers) {
    if (seenTriggerIds.has(trigger.id)) continue
    seenTriggerIds.add(trigger.id)
    acceptTrigger(trigger)
  }

  // Store 本身只保留最近 100 条；同步裁剪 Set，避免长时间直播无限增长。
  const retainedIds = new Set(triggers.map(trigger => trigger.id))
  for (const id of seenTriggerIds) {
    if (!retainedIds.has(id)) seenTriggerIds.delete(id)
  }
}

function clearConfigRefreshTimer(): void {
  if (configRefreshTimer !== null) {
    window.clearTimeout(configRefreshTimer)
    configRefreshTimer = null
  }
}

function scheduleConfigRefresh(loaded: Awaited<ReturnType<typeof getGiftEffectConfig>>): void {
  clearConfigRefreshTimer()
  const ttl = Number(loaded.full_sc_resource.ttl)
  const now = Date.now()
  const remaining = Number.isFinite(ttl) && ttl > now
    ? ttl - now
    : Number.isFinite(ttl) && ttl > 0 && ttl <= 31_536_000
      ? ttl * 1000
      : 30 * 60_000
  const delay = Math.min(CONFIG_REFRESH_MAX_DELAY_MS, Math.max(60_000, remaining))
  configRefreshTimer = window.setTimeout(() => {
    configRefreshTimer = null
    void refreshRoomConfig()
  }, delay)
}

async function refreshRoomConfig(): Promise<void> {
  const nextRoomId = targetRoomId
  const token = configLoadToken
  if (!nextRoomId || !danmakuStore.isConnected || !settingsStore.giftEffectEnabled) return

  try {
    const loaded = await getGiftEffectConfig(nextRoomId)
    if (token !== configLoadToken || nextRoomId !== targetRoomId) return
    config.value = loaded
    scheduleConfigRefresh(loaded)
    pump()
  } catch (error) {
    if (token === configLoadToken) {
      logger.warn('刷新礼物全屏特效配置失败:', error)
      configRefreshTimer = window.setTimeout(() => {
        configRefreshTimer = null
        void refreshRoomConfig()
      }, 5 * 60_000)
    }
  }
}

async function loadRoomConfig(
  nextRoomId: number,
  connected: boolean,
  enabled: boolean
): Promise<void> {
  const token = ++configLoadToken
  const switchedRoom = targetRoomId !== 0 && targetRoomId !== nextRoomId
  targetRoomId = nextRoomId
  clearConfigRefreshTimer()
  config.value = null
  if (switchedRoom || !connected || !enabled) clearQueue()
  stopAllSlots()
  if (!nextRoomId || !connected || !enabled) return

  try {
    const loaded = await getGiftEffectConfig(nextRoomId)
    if (token !== configLoadToken) return
    config.value = loaded
    scheduleConfigRefresh(loaded)
    pump()
  } catch (error) {
    if (token !== configLoadToken) return
    clearQueue()
    logger.warn('获取礼物全屏特效配置失败:', error)
    configRefreshTimer = window.setTimeout(() => {
      configRefreshTimer = null
      void refreshRoomConfig()
    }, 60_000)
  }
}

watch(
  () => [
    roomId.value,
    danmakuStore.isConnected,
    settingsStore.giftEffectEnabled
  ] as const,
  ([nextRoomId, connected, enabled]) => {
    void loadRoomConfig(nextRoomId, connected, enabled)
  },
  { immediate: true }
)

watch(
  () => [
    danmakuStore.giftEffectGeneration,
    danmakuStore.giftEffectTriggers.length,
    danmakuStore.giftEffectTriggers[danmakuStore.giftEffectTriggers.length - 1]?.id
  ],
  consumeStoreTriggers
)

watch(
  () => [
    settingsStore.giftEffectMinPrice,
    settingsStore.giftEffectMaxConcurrent,
    settingsStore.giftEffectQueueLimit
  ],
  () => {
    // 价格阈值提高后，尚未播放的请求重新过滤。
    for (let index = waitingTriggers.length - 1; index >= 0; index -= 1) {
      if (waitingTriggers[index].total_value < settingsStore.giftEffectMinPrice) {
        waitingTriggers.splice(index, 1)
      }
    }
    for (let index = playQueue.length - 1; index >= 0; index -= 1) {
      if (playQueue[index].trigger.total_value < settingsStore.giftEffectMinPrice) {
        playQueue.splice(index, 1)
      }
    }
    clampQueue()
    pump()
  }
)

onMounted(() => {
  consumeStoreTriggers()
})

onUnmounted(() => {
  configLoadToken += 1
  clearConfigRefreshTimer()
  clearQueue()
  stopAllSlots()
  animationConfigCache.clear()
  seenTriggerIds.clear()
})
</script>

<template>
  <div class="gift-effect-layer" aria-hidden="true">
    <div v-for="(_, index) in slots" :key="index" class="gift-effect-slot">
      <!-- video 仅作为 WebGL 的纹理源，不直接显示打包后的双区域画面。 -->
      <video
        :ref="element => setVideoRef(element as Element | null, index)"
        class="gift-effect-video"
        muted
        playsinline
      />
      <canvas
        :ref="element => setCanvasRef(element as Element | null, index)"
        class="gift-effect-canvas"
      />
    </div>
  </div>
</template>

<style scoped lang="scss">
.gift-effect-layer {
  position: absolute;
  inset: 0;
  z-index: 1000;
  pointer-events: none;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
}

.gift-effect-slot {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}

.gift-effect-video {
  position: absolute;
  width: 1px;
  height: 1px;
  opacity: 0;
  pointer-events: none;
}

.gift-effect-canvas {
  display: block;
  width: auto;
  height: auto;
  max-width: 100%;
  max-height: 100%;
}
</style>

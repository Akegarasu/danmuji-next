<script setup lang="ts">
import { computed, ref } from 'vue'
import type { ArchiveDailyStat, ArchiveSummary } from '@/types'
import { formatPrice } from '@/types'

const props = withDefaults(defineProps<{
  summary: ArchiveSummary
  daily: ArchiveDailyStat[]
  loading?: boolean
}>(), {
  loading: false,
})

const numberFormatter = new Intl.NumberFormat('zh-CN', { notation: 'compact' })
const formatNumber = (value: number) => numberFormatter.format(value)

const formatDuration = (seconds: number) => {
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  if (hours >= 24) return `${Math.floor(hours / 24)}天 ${hours % 24}小时`
  if (hours > 0) return `${hours}小时 ${minutes}分`
  return `${minutes}分钟`
}

const averageDuration = computed(() =>
  props.summary.session_count > 0
    ? formatDuration(Math.floor(props.summary.live_duration / props.summary.session_count))
    : '0分钟'
)

const cards = computed(() => [
  {
    label: '累计收益',
    value: formatPrice(props.summary.total_revenue) || '¥0',
    detail: `礼物 ${formatPrice(props.summary.gift_revenue) || '¥0'} · SC ${formatPrice(props.summary.sc_revenue) || '¥0'} · 大航海 ${formatPrice(props.summary.guard_revenue) || '¥0'}`,
    tone: 'gold',
  },
  {
    label: '直播场次',
    value: formatNumber(props.summary.session_count),
    detail: `${formatNumber(props.summary.room_count)} 个直播间`,
    tone: 'blue',
  },
  {
    label: '直播时长',
    value: formatDuration(props.summary.live_duration),
    detail: `平均每场 ${averageDuration.value}`,
    tone: 'purple',
  },
  {
    label: '弹幕互动',
    value: formatNumber(props.summary.danmaku_count),
    detail: `礼物 ${formatNumber(props.summary.gift_count)} · SC ${formatNumber(props.summary.sc_count)}`,
    tone: 'pink',
  },
])

const showSkeleton = computed(() =>
  props.loading && props.summary.session_count === 0 && props.daily.length === 0
)

const chartWidth = 720
const chartHeight = 170
const inset = { left: 12, right: 12, top: 14, bottom: 24 }
const maxRevenue = computed(() => Math.max(1, ...props.daily.map(point => point.total_revenue)))
const dayValues = computed(() => props.daily.map(point => new Date(`${point.date}T00:00:00`).getTime()))
type RevenueKey = 'total_revenue' | 'gift_revenue' | 'sc_revenue'
const hoveredIndex = ref<number | null>(null)

const chartPoint = (index: number, key: RevenueKey) => {
  const values = props.daily
  const width = chartWidth - inset.left - inset.right
  const height = chartHeight - inset.top - inset.bottom
  const firstDay = dayValues.value[0] ?? 0
  const lastDay = dayValues.value[dayValues.value.length - 1] ?? firstDay
  const daySpan = lastDay - firstDay
  const x = values.length <= 1 || daySpan <= 0
    ? inset.left + width / 2
    : inset.left + ((dayValues.value[index] - firstDay) / daySpan) * width
  const y = inset.top + height - ((values[index]?.[key] ?? 0) / maxRevenue.value) * height
  return { x, y }
}

const pointString = (key: RevenueKey) => {
  const values = props.daily
  if (!values.length) return ''
  return values.map((point, index) => {
    const { x, y } = chartPoint(index, key)
    return `${x.toFixed(1)},${y.toFixed(1)}`
  }).join(' ')
}

const chartSeries = computed(() => [
  { key: 'total', dataKey: 'total_revenue' as RevenueKey, label: '总收益', color: '#f5c842', points: pointString('total_revenue') },
  { key: 'gift', dataKey: 'gift_revenue' as RevenueKey, label: '礼物', color: '#ff7eb3', points: pointString('gift_revenue') },
  { key: 'sc', dataKey: 'sc_revenue' as RevenueKey, label: '醒目留言', color: '#5c9eff', points: pointString('sc_revenue') },
])
const showSeriesPoints = computed(() => props.daily.length <= 31)

const hoveredPoint = computed(() =>
  hoveredIndex.value === null ? null : props.daily[hoveredIndex.value]
)
const hoveredX = computed(() =>
  hoveredIndex.value === null ? 0 : chartPoint(hoveredIndex.value, 'total_revenue').x
)
const tooltipStyle = computed(() => {
  const percentage = (hoveredX.value / chartWidth) * 100
  return { left: `${Math.min(88, Math.max(12, percentage))}%` }
})

const handleChartMove = (event: MouseEvent) => {
  if (!props.daily.length) return
  if (props.daily.length === 1) {
    hoveredIndex.value = 0
    return
  }
  const rect = (event.currentTarget as SVGElement).getBoundingClientRect()
  const viewX = ((event.clientX - rect.left) / rect.width) * chartWidth
  hoveredIndex.value = props.daily.reduce((closest, _, index) => {
    const currentDistance = Math.abs(chartPoint(index, 'total_revenue').x - viewX)
    const closestDistance = Math.abs(chartPoint(closest, 'total_revenue').x - viewX)
    return currentDistance < closestDistance ? index : closest
  }, 0)
}

const formatExactPrice = (battery: number) =>
  `¥${(battery / 10).toLocaleString('zh-CN', { maximumFractionDigits: 1 })}`

const dateLabels = computed(() => {
  const length = props.daily.length
  if (!length) return []
  const indexes = length <= 2 ? [...Array(length).keys()] : [0, Math.floor((length - 1) / 2), length - 1]
  const width = chartWidth - inset.left - inset.right
  return [...new Set(indexes)].map(index => ({
    index,
    text: props.daily[index].date.slice(5),
    left: ((chartPoint(index, 'total_revenue').x - inset.left) / width) * 100,
  }))
})
</script>

<template>
  <section class="stats-panel" :class="{ refreshing: loading, skeleton: showSkeleton }" :aria-busy="loading">
    <template v-if="showSkeleton">
      <div class="metric-grid skeleton-grid" aria-label="正在加载统计数据">
        <div v-for="index in 4" :key="index" class="metric-card skeleton-card">
          <i />
          <b />
          <small />
        </div>
      </div>
      <div class="trend-card skeleton-trend"><i /></div>
    </template>

    <template v-else>
      <div class="metric-grid">
        <article v-for="card in cards" :key="card.label" class="metric-card" :class="card.tone">
          <span>{{ card.label }}</span>
          <strong :title="card.value">{{ card.value }}</strong>
          <small :title="card.detail">{{ card.detail }}</small>
        </article>
      </div>

      <div class="trend-card">
        <div class="trend-header">
          <div>
            <h3>收益趋势</h3>
            <p>按真实直播日期间隔汇总</p>
          </div>
          <div class="legend">
            <span v-for="series in chartSeries" :key="series.key">
              <i :style="{ background: series.color }" />{{ series.label }}
            </span>
          </div>
        </div>

        <div v-if="daily.length" class="chart-wrap">
          <svg
            :viewBox="`0 0 ${chartWidth} ${chartHeight}`"
            preserveAspectRatio="none"
            role="img"
            aria-label="按日收益折线图"
            @mousemove="handleChartMove"
            @mouseleave="hoveredIndex = null"
          >
            <rect x="0" y="0" :width="chartWidth" :height="chartHeight" class="hit-area" />
            <line v-for="line in 4" :key="line" x1="12" x2="708" :y1="14 + (line - 1) * 44" :y2="14 + (line - 1) * 44" class="grid-line" />
            <polyline
              v-for="series in chartSeries"
              :key="series.key"
              :points="series.points"
              :stroke="series.color"
              class="trend-line"
            />
            <template v-if="showSeriesPoints">
              <template v-for="series in chartSeries" :key="`points-${series.key}`">
                <circle
                  v-for="(_, index) in daily"
                  :key="`${series.key}-${index}`"
                  :cx="chartPoint(index, series.dataKey).x"
                  :cy="chartPoint(index, series.dataKey).y"
                  r="2.4"
                  :style="{ '--point-color': series.color }"
                  class="data-point"
                />
              </template>
            </template>
            <template v-if="hoveredIndex !== null">
              <line :x1="hoveredX" :x2="hoveredX" y1="14" y2="146" class="hover-line" />
              <circle
                v-for="series in chartSeries"
                :key="`hover-${series.key}`"
                :cx="chartPoint(hoveredIndex, series.dataKey).x"
                :cy="chartPoint(hoveredIndex, series.dataKey).y"
                r="4"
                :fill="series.color"
                class="hover-point"
              />
            </template>
          </svg>
          <div v-if="hoveredPoint" class="chart-tooltip" :style="tooltipStyle">
            <strong>{{ hoveredPoint.date }}</strong>
            <div class="tooltip-total"><span>总收益</span><b>{{ formatExactPrice(hoveredPoint.total_revenue) }}</b></div>
            <div><span>礼物</span><b>{{ formatExactPrice(hoveredPoint.gift_revenue) }}</b></div>
            <div><span>醒目留言</span><b>{{ formatExactPrice(hoveredPoint.sc_revenue) }}</b></div>
            <div><span>场次 / 弹幕</span><b>{{ hoveredPoint.session_count }} / {{ hoveredPoint.danmaku_count }}</b></div>
          </div>
          <div class="date-axis">
            <span v-for="label in dateLabels" :key="label.index" :style="{ left: `${label.left}%` }">
              {{ label.text }}
            </span>
          </div>
        </div>
        <div v-else class="empty-chart">所选时间范围内暂无统计数据</div>
      </div>
    </template>
  </section>
</template>

<style scoped lang="scss">
.stats-panel { position: relative; display: grid; gap: 12px; }
.stats-panel.refreshing:not(.skeleton)::after {
  position: absolute;
  top: 0;
  right: 0;
  left: 0;
  height: 2px;
  border-radius: 2px;
  background: linear-gradient(90deg, transparent, var(--accent-primary), transparent);
  background-size: 45% 100%;
  content: '';
  animation: statsRefresh 1s linear infinite;
  pointer-events: none;
}

.metric-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 10px;
}

.metric-card {
  min-width: 0;
  padding: 13px 14px;
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius);
  background: var(--bg-card);
  box-shadow: inset 3px 0 var(--metric-color);

  span { display: block; color: var(--text-muted); font-size: var(--font-size-xs); }
  strong { display: block; margin-top: 5px; overflow: hidden; color: var(--text-primary); font-size: 19px; text-overflow: ellipsis; white-space: nowrap; }
  small { display: block; margin-top: 4px; overflow: hidden; color: var(--text-muted); font-size: 9px; text-overflow: ellipsis; white-space: nowrap; }
  &.gold { --metric-color: var(--accent-gold); }
  &.blue { --metric-color: var(--accent-primary); }
  &.purple { --metric-color: var(--accent-tertiary); }
  &.pink { --metric-color: var(--accent-secondary); }
}

.trend-card {
  min-height: 150px;
  padding: 13px 14px 9px;
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius);
  background: var(--bg-secondary);
}

.trend-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;

  h3 { font-size: var(--font-size-sm); font-weight: 600; }
  p { margin-top: 2px; color: var(--text-muted); font-size: var(--font-size-xs); }
}

.legend {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 8px;
  color: var(--text-secondary);
  font-size: var(--font-size-xs);

  span { display: inline-flex; align-items: center; gap: 4px; }
  i { width: 12px; height: 2px; border-radius: 2px; }
}

.chart-wrap { position: relative; height: 130px; margin-top: 7px; padding-bottom: 17px; }
svg { width: 100%; height: 113px; overflow: visible; }
.hit-area { fill: transparent; }
.grid-line { stroke: rgba(255, 255, 255, 0.07); stroke-width: 1; vector-effect: non-scaling-stroke; }
.trend-line { fill: none; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; vector-effect: non-scaling-stroke; }
.data-point { fill: var(--bg-secondary); stroke: var(--point-color); stroke-width: 1.5; vector-effect: non-scaling-stroke; }
.hover-line { stroke: var(--text-muted); stroke-width: 1; stroke-dasharray: 3 3; vector-effect: non-scaling-stroke; }
.hover-point { stroke: var(--bg-secondary); stroke-width: 2; vector-effect: non-scaling-stroke; }

.chart-tooltip {
  position: absolute;
  z-index: 2;
  top: 3px;
  min-width: 150px;
  padding: 7px 9px;
  transform: translateX(-50%);
  border: 1px solid var(--border-color);
  border-radius: var(--border-radius);
  background: rgb(32, 32, 32);
  box-shadow: 0 3px 10px rgba(0, 0, 0, 0.3);
  pointer-events: none;

  > strong { display: block; margin-bottom: 4px; color: var(--text-primary); font-size: var(--font-size-xs); }
  > div { display: flex; justify-content: space-between; gap: 14px; color: var(--text-muted); font-size: 10px; line-height: 1.6; }
  b { color: var(--text-secondary); font-weight: 500; }
  .tooltip-total b { color: var(--accent-gold); }
}

.date-axis {
  position: absolute;
  right: 12px;
  bottom: 0;
  left: 12px;
  color: var(--text-muted);
  font-size: 10px;

  span { position: absolute; transform: translateX(-50%); white-space: nowrap; }
}

.empty-chart { display: grid; height: 100px; place-items: center; color: var(--text-muted); font-size: var(--font-size-sm); }

.skeleton-card {
  box-shadow: none;

  i, b, small { display: block; border-radius: 4px; background: var(--bg-hover); animation: skeletonPulse 1.3s ease-in-out infinite; }
  i { width: 42%; height: 10px; }
  b { width: 58%; height: 20px; margin-top: 9px; }
  small { width: 75%; height: 8px; margin-top: 7px; }
}
.skeleton-trend { display: grid; height: 150px; place-items: center; }
.skeleton-trend i { width: 92%; height: 70%; border-radius: 6px; background: var(--bg-card); animation: skeletonPulse 1.3s ease-in-out infinite; }

@keyframes skeletonPulse {
  0%, 100% { opacity: 0.45; }
  50% { opacity: 0.9; }
}

@keyframes statsRefresh {
  from { background-position: -80% 0; }
  to { background-position: 180% 0; }
}

@media (max-width: 680px) {
  .metric-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .trend-header { align-items: flex-start; flex-direction: column; }
  .legend { justify-content: flex-start; }
}
</style>

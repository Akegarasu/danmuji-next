/**
 * 礼物图标服务
 * 以 giftId 为主键，从 gift.json 静态数据构建缓存
 */

import giftData from '@/assets/gift.json'

interface GiftEntry {
  id: number
  name: string
  url: string
}

const GUARD_ICONS: Record<number, string> = {
  1: 'https://s1.hdslb.com/bfs/static/blive/live-pay-mono/relation/relation/assets/governor-DpDXKEdA.png',
  2: 'https://s1.hdslb.com/bfs/static/blive/live-pay-mono/relation/relation/assets/supervisor-u43ElIjU.png',
  3: 'https://s1.hdslb.com/bfs/static/blive/live-pay-mono/relation/relation/assets/captain-Bjw5Byb5.png',
}

const idCache = new Map<number, string>()
const nameCache = new Map<string, string>()

for (const entry of giftData as GiftEntry[]) {
  idCache.set(entry.id, entry.url)
  nameCache.set(entry.name, entry.url)
}

/**
 * 根据礼物 ID 获取图标 URL，name 作为兜底回退
 */
export function getGiftIcon(giftId: number, giftName?: string): string | undefined {
  return idCache.get(giftId) ?? (giftName ? nameCache.get(giftName) : undefined)
}

/**
 * 根据大航海等级获取图标 URL（1=总督, 2=提督, 3=舰长）
 */
export function getGuardIcon(guardLevel: number): string | undefined {
  return GUARD_ICONS[guardLevel]
}

/**
 * 运行时动态添加礼物图标（用于缓存弹幕中出现的未知礼物）
 */
export function addGiftIcon(giftId: number, giftName: string, url: string): void {
  idCache.set(giftId, url)
  nameCache.set(giftName, url)
}

// Time rules, pure: the shop's calendar is Asia/Shanghai (UTC+8, no daylight saving), so
// order numbers, ETAs and every displayed date are shop dates whatever the server's TZ.
import type { OrderStatus, RememberStatus } from '@coffee/contract'

const HOUR_MS = 60 * 60 * 1000
const DAY_MS = 24 * HOUR_MS

/** The shop's offset from UTC. */
export const SHOP_UTC_OFFSET_MS = 8 * HOUR_MS

/** An order is 烘焙中 for this long, then 运输中 until its ETA. */
export const ROASTING_MS = DAY_MS

/** ETA = placed + 3 days. */
export const DELIVERY_MS = 3 * DAY_MS

/** `2026-11-22`: the shop-local calendar date of `instant`. */
export function shopDate(instant: Date): string {
  return new Date(instant.getTime() + SHOP_UTC_OFFSET_MS).toISOString().slice(0, 10)
}

/** `261122`: the yymmdd of the shop date of `instant` (order numbers). */
export function shopYymmdd(instant: Date): string {
  const date = shopDate(instant)
  return `${date.slice(2, 4)}${date.slice(5, 7)}${date.slice(8, 10)}`
}

/**
 * An order number for `instant`: `RT` + yymmdd + 4 digits from `random4` (0–9999,
 * zero-padded). The caller retries with a new draw while the number is taken.
 */
export function orderNumber(instant: Date, random4: number): string {
  if (!Number.isInteger(random4) || random4 < 0 || random4 > 9999)
    throw new RangeError(`random4 ${random4}`)
  return `RT${shopYymmdd(instant)}${String(random4).padStart(4, '0')}`
}

/** ETA of an order placed at `createdAt`. */
export function etaOf(createdAt: Date): Date {
  return new Date(createdAt.getTime() + DELIVERY_MS)
}

/** Status at `now`: 烘焙中 for 24 h, 运输中 until the ETA, then 已签收. */
export function orderStatus(createdAt: Date, etaAt: Date, now: Date): OrderStatus {
  if (now.getTime() < createdAt.getTime() + ROASTING_MS)
    return 'roasting'
  if (now.getTime() < etaAt.getTime())
    return 'shipping'
  return 'delivered'
}

/** ISO 8601 UTC with milliseconds (the contract's `InstantSchema`). */
export function instant(date: Date): string {
  return date.toISOString()
}

/**
 * The 记住我 status of a connection for `userId`: `no_key` without a fingerprint;
 * `remembered` when the key is bound to this user and not expired at `now`; else
 * `not_remembered`.
 */
export function rememberStatus(
  fingerprint: string | null,
  binding: { userId: number, expiresAt: Date } | null,
  userId: number,
  now: Date,
): RememberStatus {
  if (!fingerprint)
    return { state: 'no_key', expiresAt: null, expiresOn: null }
  if (!binding || binding.userId !== userId || binding.expiresAt.getTime() <= now.getTime())
    return { state: 'not_remembered', expiresAt: null, expiresOn: null }
  return {
    state: 'remembered',
    expiresAt: instant(binding.expiresAt),
    expiresOn: shopDate(binding.expiresAt),
  }
}

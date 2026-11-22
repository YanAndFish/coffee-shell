// Money rules (docs/design/data.md#计价), pure and server-side only: the TUI never computes
// money. Integer yuan everywhere; "round" is half up (JS `Math.round` on non-negative values),
// done in integers so no float can tip a .5 the wrong way.
import type { CartTotals, Pack, Weight } from '@coffee/contract'

/** Weight factors in hundredths: 100g ×0.45, 250g ×1, 500g ×1.9, 1kg ×3.6. */
export const WEIGHT_FACTOR_PERCENT: Record<Weight, number> = {
  '100g': 45,
  '250g': 100,
  '500g': 190,
  '1kg': 360,
}

/** Pack surcharges: 标准袋 +0, 礼盒装 +20. */
export const PACK_SURCHARGE: Record<Pack, number> = { STD: 0, GIFT: 20 }

/** Shipping fee below the free-shipping threshold. */
export const SHIPPING_FEE = 12

/** Free shipping when subtotal − discount reaches this. */
export const FREE_SHIPPING_THRESHOLD = 199

/** Quantity bounds of a line. */
export const MIN_QUANTITY = 1
export const MAX_QUANTITY = 9

/** round(numerator ÷ denominator), half up, for non-negative integers. */
export function divRound(numerator: number, denominator: number): number {
  if (!Number.isInteger(numerator) || !Number.isInteger(denominator) || numerator < 0 || denominator <= 0)
    throw new RangeError(`divRound(${numerator}, ${denominator})`)
  return Math.floor((2 * numerator + denominator) / (2 * denominator))
}

/** Unit price = round(base250 × weight factor) + pack surcharge. */
export function unitPrice(base250: number, weight: Weight, pack: Pack): number {
  return divRound(base250 * WEIGHT_FACTOR_PERCENT[weight], 100) + PACK_SURCHARGE[pack]
}

/** Line total = unit price × quantity. */
export function lineTotal(unit: number, quantity: number): number {
  return unit * quantity
}

/** The SKU panel's totals for quantity 1 … maxQuantity (`totals[n − 1]`). */
export function quantityTotals(unit: number, maxQuantity: number): number[] {
  return Array.from({ length: maxQuantity }, (_, i) => lineTotal(unit, i + 1))
}

/** The stepper's upper bound: min(9, stock), never below 0. */
export function maxQuantity(stock: number): number {
  return Math.max(0, Math.min(MAX_QUANTITY, stock))
}

/** A coupon as pricing sees it. */
export interface PricingCoupon {
  percentOff: number
}

/** Discount = round(subtotal × percentOff ÷ 100); 0 without a coupon. TTY10: round(subtotal × 10 %). */
export function couponDiscount(subtotal: number, coupon: PricingCoupon | null): number {
  if (!coupon)
    return 0
  return Math.min(subtotal, divRound(subtotal * coupon.percentOff, 100))
}

/** Shipping: 0 for an empty selection or when subtotal − discount ≥ 199; else 12. */
export function shippingFee(subtotal: number, discount: number): number {
  if (subtotal === 0 || subtotal - discount >= FREE_SHIPPING_THRESHOLD)
    return 0
  return SHIPPING_FEE
}

/** A line as the totals see it. */
export interface PricedLine {
  unitPrice: number
  quantity: number
  selected: boolean
}

/** The cart's amounts: only selected lines count toward money; counts cover all lines. */
export function cartTotals(lines: readonly PricedLine[], coupon: PricingCoupon | null): CartTotals {
  const itemCount = lines.reduce((n, line) => n + line.quantity, 0)
  const selected = lines.filter(line => line.selected)
  const selectedCount = selected.reduce((n, line) => n + line.quantity, 0)
  const subtotal = selected.reduce((sum, line) => sum + lineTotal(line.unitPrice, line.quantity), 0)
  const discount = couponDiscount(subtotal, coupon)
  const shipping = shippingFee(subtotal, discount)
  const net = subtotal - discount
  return {
    itemCount,
    selectedCount,
    subtotal,
    discount,
    shipping,
    total: net + shipping,
    freeShipping: {
      threshold: FREE_SHIPPING_THRESHOLD,
      reached: subtotal > 0 && net >= FREE_SHIPPING_THRESHOLD,
      progress: Math.min(100, divRound(net * 100, FREE_SHIPPING_THRESHOLD)),
      gap: Math.max(0, FREE_SHIPPING_THRESHOLD - net),
    },
  }
}

/** Canonical coupon code of what a member typed: trimmed, upper-case; `null` when empty. */
export function normalizeCouponCode(typed: string | null | undefined): string | null {
  const code = (typed ?? '').trim().toUpperCase()
  return code === ''
    ? null
    : code
}

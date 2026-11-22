// Example carts; every amount follows data.md#计价 (the server tests recompute them).
import type { Cart, CartLine, CartMergeResult, CartTotals } from '../cart.ts'
import type { Grind, Pack, Weight } from '../sku.ts'
import { skuCode } from '../sku.ts'
import { homeAddress, workAddress } from './addresses.ts'
import { beanFacts, unitPriceOf } from './catalog.ts'

/** A priced line of `productId` (stock as seeded unless `stock` is given). */
export function line(
  productId: string,
  weight: Weight,
  grind: Grind,
  pack: Pack,
  quantity: number,
  selected = true,
  stock?: number,
): CartLine {
  const facts = beanFacts(productId)
  const unitPrice = unitPriceOf(productId, weight, pack === 'GIFT')
  const left = stock ?? facts.stock
  return {
    sku: skuCode(facts.code, weight, grind, pack),
    productId,
    name: facts.name,
    origin: facts.origin,
    roast: facts.roast,
    weight,
    grind,
    pack,
    quantity,
    selected,
    unitPrice,
    lineTotal: unitPrice * quantity,
    maxQuantity: Math.min(9, left),
    problem: quantity > left
      ? 'out_of_stock'
      : null,
  }
}

/** Totals as given (worked out by hand in the comments). */
function totals(t: Omit<CartTotals, 'freeShipping'>, progress: number, gap: number): CartTotals {
  return {
    ...t,
    freeShipping: {
      threshold: 199,
      reached: gap === 0 && t.subtotal > 0,
      progress,
      gap,
    },
  }
}

/** The design's sample cart (data.md 例 1), all selected, no coupon. */
const sampleLines = [
  line('pinkb', '250g', 'PO', 'STD', 1),
  line('night', '500g', 'ES', 'STD', 1),
  line('geisha', '100g', 'WB', 'GIFT', 1),
]

/** `fixtures/v1/cart/cart.json`: 118 + 167 + 141 = 426 ≥ 199 → 包邮. */
export const cartExample: Cart = {
  lines: sampleLines,
  coupon: null,
  address: homeAddress,
  totals: totals({
    itemCount: 3,
    selectedCount: 3,
    subtotal: 426,
    discount: 0,
    shipping: 0,
    total: 426,
  }, 100, 0),
}

/** The sample cart with the first line unselected: 167 + 141 = 308. */
export const cartPartlySelectedExample: Cart = {
  lines: [
    { ...sampleLines[0]!, selected: false },
    sampleLines[1]!,
    sampleLines[2]!,
  ],
  coupon: null,
  address: homeAddress,
  totals: totals({
    itemCount: 3,
    selectedCount: 2,
    subtotal: 308,
    discount: 0,
    shipping: 0,
    total: 308,
  }, 100, 0),
}

/**
 * data.md 例 2: one 250g Yirgacheffe + TTY10: 98 − round(9.8) = 88 < 199 → +12 = 100;
 * progress round(88 ÷ 199 × 100) = 44; gap 111.
 */
export const cartCouponExample: Cart = {
  lines: [
    line('yirga', '250g', 'WB', 'STD', 1),
  ],
  coupon: { code: 'TTY10', description: '全单 9 折' },
  address: homeAddress,
  totals: totals({
    itemCount: 1,
    selectedCount: 1,
    subtotal: 98,
    discount: 10,
    shipping: 12,
    total: 100,
  }, 44, 111),
}

/** A new member: nothing in the cart, no address yet (`满 ¥199 包邮`). */
export const cartEmptyExample: Cart = {
  lines: [],
  coupon: null,
  address: null,
  totals: totals({
    itemCount: 0,
    selectedCount: 0,
    subtotal: 0,
    discount: 0,
    shipping: 0,
    total: 0,
  }, 0, 199),
}

/** Stock fell to 2 while 3 Geisha sit in the cart: flagged, still counted until checkout. */
export const cartProblemExample: Cart = {
  lines: [
    line('geisha', '100g', 'PO', 'GIFT', 3, true, 2),
  ],
  coupon: null,
  address: homeAddress,
  totals: totals({
    itemCount: 3,
    selectedCount: 3,
    subtotal: 423,
    discount: 0,
    shipping: 0,
    total: 423,
  }, 100, 0),
}

/**
 * `POST /v1/cart/quote` of a guest holding two 250g Yirgacheffe: 196 < 199 → +12 = 208
 * (data.md 例 3); progress round(196 ÷ 199 × 100) = 98; gap 3.
 */
export const guestQuoteExample: Cart = {
  lines: [
    line('yirga', '250g', 'PO', 'STD', 2),
  ],
  coupon: null,
  address: null,
  totals: totals({
    itemCount: 2,
    selectedCount: 2,
    subtotal: 196,
    discount: 0,
    shipping: 12,
    total: 208,
  }, 98, 3),
}

/** `POST /v1/cart/merge`: the member had two lines, the guest brought the Geisha gift bag. */
export const cartMergeExample: CartMergeResult = {
  cart: cartExample,
  mergedCount: 1,
  skippedCount: 0,
}

/** `DELETE /v1/cart/lines/RT-PNK-250G-PO-STD` on the sample cart: 167 + 141 = 308. */
export const cartRemovedExample: Cart = {
  lines: [sampleLines[1]!, sampleLines[2]!],
  coupon: null,
  address: homeAddress,
  totals: totals({
    itemCount: 2,
    selectedCount: 2,
    subtotal: 308,
    discount: 0,
    shipping: 0,
    total: 308,
  }, 100, 0),
}

/** `PUT /v1/cart/selection {"selected": false}`: nothing selected, so nothing to pay. */
export const cartNoneSelectedExample: Cart = {
  lines: sampleLines.map(l => ({ ...l, selected: false })),
  coupon: null,
  address: homeAddress,
  totals: totals({
    itemCount: 3,
    selectedCount: 0,
    subtotal: 0,
    discount: 0,
    shipping: 0,
    total: 0,
  }, 0, 199),
}

/** `DELETE /v1/cart/coupon` on 例 2: 98 < 199 → +12 = 110; progress 49; gap 101. */
export const cartCouponRemovedExample: Cart = {
  ...cartCouponExample,
  coupon: null,
  totals: totals({
    itemCount: 1,
    selectedCount: 1,
    subtotal: 98,
    discount: 0,
    shipping: 12,
    total: 110,
  }, 49, 101),
}

/** `PUT /v1/cart/address {"addressId": 2}`: 本单寄往 the office. */
export const cartAddressSetExample: Cart = {
  ...cartExample,
  address: workAddress,
}

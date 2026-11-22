import type { Cart, Order, PlacedOrder, ProductDetail } from '@coffee/contract'
import { readdirSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join, relative } from 'node:path'
import { describe, expect, it } from 'vitest'
import { CATALOG } from '../prisma/catalog.ts'
import { COUPONS } from '../prisma/coupons.ts'
import {
  cartTotals,
  couponDiscount,
  divRound,
  lineTotal,
  maxQuantity,
  normalizeCouponCode,
  quantityTotals,
  shippingFee,
  unitPrice,
} from '../src/services/pricing.ts'
import { readFixture } from './helpers.ts'

const TTY10 = COUPONS.find(c => c.code === 'TTY10')!

/** Committed fixture paths under `fixtures/` (e.g. `v1/cart/cart.json`). */
function fixturePaths(): string[] {
  const root = dirname(createRequire(import.meta.url).resolve('@coffee/contract/fixtures/v1/health.json'))
  return readdirSync(root, { recursive: true, withFileTypes: true })
    .filter(entry => entry.isFile())
    .map(entry => `v1/${relative(root, join(entry.parentPath, entry.name))}`)
}

function base(id: string): number {
  return CATALOG.find(p => p.id === id)!.basePrice
}

describe('unit price', () => {
  // data.md#单价表 (standard bag), verbatim.
  const TABLE: Record<string, [number, number, number, number]> = {
    yirga: [44, 98, 186, 353],
    pinkb: [53, 118, 224, 425],
    nyeri: [49, 108, 205, 389],
    geisha: [121, 268, 509, 965],
    yunnan: [31, 68, 129, 245],
    antigua: [35, 78, 148, 281],
    mandh: [32, 72, 137, 259],
    night: [40, 88, 167, 317],
  }

  it.each(Object.entries(TABLE))('%s matches data.md, gift +20', (id, prices) => {
    const weights = ['100g', '250g', '500g', '1kg'] as const
    expect(weights.map(w => unitPrice(base(id), w, 'STD'))).toEqual(prices)
    expect(weights.map(w => unitPrice(base(id), w, 'GIFT'))).toEqual(prices.map(p => p + 20))
  })

  it('rounds half up in integers', () => {
    expect(divRound(5, 10)).toBe(1)
    expect(divRound(15, 10)).toBe(2)
    expect(divRound(14, 10)).toBe(1)
    expect(divRound(0, 199)).toBe(0)
    expect(() => divRound(-1, 2)).toThrow(RangeError)
    expect(() => divRound(1.5, 2)).toThrow(RangeError)
  })

  it('line totals and the SKU panel table', () => {
    expect(lineTotal(141, 3)).toBe(423)
    expect(quantityTotals(268, 3)).toEqual([268, 536, 804])
    expect(maxQuantity(6)).toBe(6)
    expect(maxQuantity(120)).toBe(9)
    expect(maxQuantity(0)).toBe(0)
    expect(maxQuantity(-2)).toBe(0)
  })
})

describe('coupon and shipping', () => {
  it('tTY10 is round(subtotal × 10 %)', () => {
    expect(couponDiscount(98, TTY10)).toBe(10)
    expect(couponDiscount(186, TTY10)).toBe(19)
    expect(couponDiscount(45, TTY10)).toBe(5)
    expect(couponDiscount(0, TTY10)).toBe(0)
    expect(couponDiscount(98, null)).toBe(0)
  })

  it('normalizes typed codes', () => {
    expect(normalizeCouponCode(' tty10 ')).toBe('TTY10')
    expect(normalizeCouponCode('   ')).toBeNull()
    expect(normalizeCouponCode(undefined)).toBeNull()
  })

  it('shipping is 12 below ¥199 after the discount, 0 for nothing', () => {
    expect(shippingFee(0, 0)).toBe(0)
    expect(shippingFee(198, 0)).toBe(12)
    expect(shippingFee(199, 0)).toBe(0)
    expect(shippingFee(210, 21)).toBe(12)
    expect(shippingFee(222, 22)).toBe(0)
  })
})

describe('data.md worked examples', () => {
  it('例 1: the sample cart, 426, free shipping', () => {
    const t = cartTotals([
      {
        unitPrice: unitPrice(base('pinkb'), '250g', 'STD'),
        quantity: 1,
        selected: true,
      },
      {
        unitPrice: unitPrice(base('night'), '500g', 'STD'),
        quantity: 1,
        selected: true,
      },
      {
        unitPrice: unitPrice(base('geisha'), '100g', 'GIFT'),
        quantity: 1,
        selected: true,
      },
    ], null)
    expect(t).toEqual({
      itemCount: 3,
      selectedCount: 3,
      subtotal: 426,
      discount: 0,
      shipping: 0,
      total: 426,
      freeShipping: {
        threshold: 199,
        reached: true,
        progress: 100,
        gap: 0,
      },
    })
  })

  it('例 2: one Yirgacheffe + TTY10 = 100, 44 %, ¥111 to go', () => {
    const t = cartTotals([
      { unitPrice: 98, quantity: 1, selected: true },
    ], TTY10)
    expect(t).toMatchObject({
      subtotal: 98,
      discount: 10,
      shipping: 12,
      total: 100,
      freeShipping: { progress: 44, gap: 111, reached: false },
    })
  })

  it('例 3: the source orders 285 / 208 / 133', () => {
    expect(cartTotals([
      { unitPrice: 118, quantity: 1, selected: true },
      { unitPrice: 167, quantity: 1, selected: true },
    ], null).total).toBe(285)
    expect(cartTotals([
      { unitPrice: 98, quantity: 2, selected: true },
    ], null).total).toBe(208)
    expect(cartTotals([
      { unitPrice: 121, quantity: 1, selected: true },
    ], null).total).toBe(133)
  })

  it('unselected lines count as items, not as money', () => {
    const t = cartTotals([
      { unitPrice: 118, quantity: 2, selected: false },
      { unitPrice: 98, quantity: 1, selected: true },
    ], null)
    expect(t).toMatchObject({
      itemCount: 3,
      selectedCount: 1,
      subtotal: 98,
      total: 110,
    })
  })

  it('an empty selection costs nothing and shows ¥199 to go', () => {
    expect(cartTotals([], TTY10)).toEqual({
      itemCount: 0,
      selectedCount: 0,
      subtotal: 0,
      discount: 0,
      shipping: 0,
      total: 0,
      freeShipping: {
        threshold: 199,
        reached: false,
        progress: 0,
        gap: 199,
      },
    })
  })
})

describe('the contract fixtures follow these rules', () => {
  function checkCart(cart: Cart, path: string): void {
    for (const line of cart.lines) {
      expect(line.unitPrice, `${path} ${line.sku}`).toBe(unitPrice(base(line.productId), line.weight, line.pack))
      expect(line.lineTotal, `${path} ${line.sku}`).toBe(lineTotal(line.unitPrice, line.quantity))
    }
    const coupon = cart.coupon
      ? COUPONS.find(c => c.code === cart.coupon!.code)!
      : null
    expect(cart.totals, path).toEqual(cartTotals(cart.lines, coupon))
    if (coupon)
      expect(cart.coupon?.description).toBe(coupon.description)
  }

  function checkOrder(order: Order, path: string): void {
    for (const item of order.items) {
      expect(item.unitPrice, path).toBe(unitPrice(base(item.productId), item.weight, item.pack))
      expect(item.lineTotal, path).toBe(lineTotal(item.unitPrice, item.quantity))
    }
    const coupon = order.couponCode
      ? COUPONS.find(c => c.code === order.couponCode)!
      : null
    const t = cartTotals(order.items.map(i => ({ ...i, selected: true })), coupon)
    expect({
      subtotal: order.subtotal,
      discount: order.discount,
      shipping: order.shipping,
      total: order.total,
    }, path)
      .toEqual({
        subtotal: t.subtotal,
        discount: t.discount,
        shipping: t.shipping,
        total: t.total,
      })
  }

  it.each(fixturePaths().filter(p => p.startsWith('v1/cart/') || p.startsWith('v1/orders/')).map(p => [p] as const))('%s', (path) => {
    const value = readFixture(path) as Record<string, unknown>
    if ('lines' in value && 'totals' in value)
      checkCart(value as unknown as Cart, path)
    if ('cart' in value)
      checkCart((value as { cart: Cart }).cart, path)
    if ('order' in value)
      checkOrder((value as unknown as PlacedOrder).order, path)
    if ('items' in value && 'subtotal' in value)
      checkOrder(value as unknown as Order, path)
  })

  it.each(fixturePaths().filter(p => p.startsWith('v1/catalog/product-')).map(p => [p] as const))('%s price table', (path) => {
    const detail = readFixture(path) as ProductDetail
    expect(detail.maxQuantity).toBe(maxQuantity(detail.stock))
    for (const cell of detail.prices) {
      expect(cell.unitPrice).toBe(unitPrice(detail.price250, cell.weight, cell.pack))
      expect(cell.totals).toEqual(quantityTotals(cell.unitPrice, detail.maxQuantity))
    }
  })
})

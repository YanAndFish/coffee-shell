// Conformance cases of area `cart` (owner: server-shop). Carts are built directly in the
// database; priced answers are compared with the committed fixtures (address ids aside).
import type { Cart, CartMergeResult } from '@coffee/contract'
import type { TestLine } from '../support/members.ts'
import type { CaseContext, ConformanceCase } from './types.ts'
import { GRINDS, PACKS, WEIGHTS } from '@coffee/contract'
import { expect } from 'vitest'
import { readFixture } from '../helpers.ts'
import { addAddressRow, addCartRow } from '../support/members.ts'

const PINKB: TestLine = {
  productId: 'pinkb',
  weight: '250g',
  grind: 'PO',
  pack: 'STD',
  quantity: 1,
}
const NIGHT: TestLine = {
  productId: 'night',
  weight: '500g',
  grind: 'ES',
  pack: 'STD',
  quantity: 1,
}
const GEISHA: TestLine = {
  productId: 'geisha',
  weight: '100g',
  grind: 'WB',
  pack: 'GIFT',
  quantity: 1,
}
const YIRGA: TestLine = {
  productId: 'yirga',
  weight: '250g',
  grind: 'WB',
  pack: 'STD',
  quantity: 1,
}

/** A member holding `lines`, with the sample home address; returns the token. */
async function memberWith(ctx: CaseContext, lines: TestLine[], withAddress = true): Promise<string> {
  const m = await ctx.member()
  for (const line of lines)
    await addCartRow(ctx.prisma, m.id, line)
  if (withAddress)
    await addAddressRow(ctx.prisma, m.id)
  return m.token
}

/** `count` distinct yirga / nyeri SKUs (at most 80), one unit each. */
export function distinctLines(count: number): TestLine[] {
  const lines: TestLine[] = []
  for (const productId of ['yirga', 'nyeri']) {
    for (const weight of WEIGHTS) {
      for (const grind of GRINDS) {
        for (const pack of PACKS) {
          lines.push({
            productId,
            weight,
            grind,
            pack,
            quantity: 1,
          })
        }
      }
    }
  }
  return lines.slice(0, count)
}

/** Fills the member's cart with `count` distinct lines. */
async function fillCart(ctx: CaseContext, userId: number, count: number): Promise<void> {
  await ctx.prisma.cartItem.createMany({
    data: distinctLines(count).map((line, position) => ({ userId, ...line, position })),
  })
}

/** Compares lines and totals with a cart fixture (and the address but for its id). */
function expectCartLike(body: unknown, fixture: string): void {
  const cart = body as Cart
  const expected = readFixture(fixture) as Cart
  expect(cart.lines).toEqual(expected.lines)
  expect(cart.totals).toEqual(expected.totals)
  expect(cart.coupon).toEqual(expected.coupon)
  expect(cart.address && { ...cart.address, id: 0 }).toEqual(expected.address && { ...expected.address, id: 0 })
}

export const cartCases: ConformanceCase[] = [
  {
    operationId: 'getCart',
    name: 'the sample cart',
    request: async ctx => ({ path: '/v1/cart', token: await memberWith(ctx, [PINKB, NIGHT, GEISHA]) }),
    check: body => expectCartLike(body, 'v1/cart/cart.json'),
  },
  {
    operationId: 'getCart',
    name: 'a new member',
    request: async ctx => ({ path: '/v1/cart', token: await memberWith(ctx, [], false) }),
    check: body => expectCartLike(body, 'v1/cart/cart-empty.json'),
  },
  {
    operationId: 'getCart',
    name: 'guests need a session',
    request: async () => ({ path: '/v1/cart' }),
    expectError: 'login_required',
  },
  {
    operationId: 'addCartLine',
    name: 'adds the Geisha gift bag',
    request: async ctx => ({
      path: '/v1/cart/lines',
      token: await memberWith(ctx, [PINKB, NIGHT]),
      body: {
        productId: 'geisha',
        weight: '100g',
        grind: 'WB',
        pack: 'GIFT',
        quantity: 1,
      },
    }),
    check: body => expectCartLike(body, 'v1/cart/added.json'),
  },
  {
    operationId: 'addCartLine',
    name: 'refuses a sold-out weight',
    request: async ctx => ({
      path: '/v1/cart/lines',
      token: await memberWith(ctx, []),
      body: {
        productId: 'pinkb',
        weight: '1kg',
        grind: 'WB',
        pack: 'STD',
        quantity: 1,
      },
    }),
    expectError: 'sold_out',
  },
  {
    operationId: 'addCartLine',
    name: 'refuses more than the stock',
    request: async ctx => ({
      path: '/v1/cart/lines',
      token: await memberWith(ctx, [
        { ...GEISHA, quantity: 5 },
      ]),
      body: { ...GEISHA, quantity: 2 },
    }),
    expectError: 'out_of_stock',
  },
  {
    operationId: 'addCartLine',
    name: 'refuses quantity 10',
    request: async ctx => ({
      path: '/v1/cart/lines',
      token: await memberWith(ctx, []),
      body: { ...YIRGA, quantity: 10 },
    }),
    expectError: 'quantity_invalid',
  },
  {
    operationId: 'addCartLine',
    name: 're-adding a SKU sums the quantities and selects it again',
    request: async ctx => ({
      path: '/v1/cart/lines',
      token: await memberWith(ctx, [
        { ...YIRGA, quantity: 3, selected: false },
      ]),
      body: { ...YIRGA, quantity: 2 },
    }),
    check: (body) => {
      const cart = body as Cart
      expect(cart.lines).toHaveLength(1)
      expect(cart.lines[0]).toMatchObject({
        sku: 'RT-YIR-250G-WB-STD',
        quantity: 5,
        selected: true,
        lineTotal: 490,
      })
    },
  },
  {
    operationId: 'addCartLine',
    name: 'an unknown product',
    request: async ctx => ({
      path: '/v1/cart/lines',
      token: await memberWith(ctx, []),
      body: { ...YIRGA, productId: 'nope' },
    }),
    expectError: 'not_found',
  },
  {
    operationId: 'addCartLine',
    name: 'refuses a 51st line',
    request: async (ctx) => {
      const m = await ctx.member()
      await fillCart(ctx, m.id, 50)
      return {
        path: '/v1/cart/lines',
        token: m.token,
        body: { ...GEISHA, grind: 'CB', pack: 'STD' },
      }
    },
    expectError: 'cart_full',
  },
  {
    operationId: 'updateCartLine',
    name: 'unselects a line',
    request: async ctx => ({
      path: '/v1/cart/lines/RT-PNK-250G-PO-STD',
      token: await memberWith(ctx, [PINKB, NIGHT, GEISHA]),
      body: { selected: false },
    }),
    check: body => expectCartLike(body, 'v1/cart/cart-partly-selected.json'),
  },
  {
    operationId: 'updateCartLine',
    name: 'unknown line',
    request: async ctx => ({
      path: '/v1/cart/lines/RT-PNK-250G-PO-STD',
      token: await memberWith(ctx, []),
      body: { quantity: 2 },
    }),
    expectError: 'not_found',
  },
  {
    operationId: 'updateCartLine',
    name: 'more than the stock',
    request: async ctx => ({
      path: '/v1/cart/lines/RT-GSH-100G-WB-GIFT',
      token: await memberWith(ctx, [GEISHA]),
      body: { quantity: 7 },
    }),
    expectError: 'out_of_stock',
  },
  {
    operationId: 'removeCartLine',
    name: 'removes a line',
    request: async ctx => ({ path: '/v1/cart/lines/RT-PNK-250G-PO-STD', token: await memberWith(ctx, [PINKB, NIGHT, GEISHA]) }),
    check: body => expectCartLike(body, 'v1/cart/removed.json'),
  },
  {
    operationId: 'removeCartLine',
    name: 'a line not in the cart',
    request: async ctx => ({ path: '/v1/cart/lines/RT-PNK-250G-PO-STD', token: await memberWith(ctx, [NIGHT]) }),
    expectError: 'not_found',
  },
  {
    operationId: 'addCartLine',
    name: 'undo puts a removed line back at its index',
    request: async ctx => ({
      path: '/v1/cart/lines',
      token: await memberWith(ctx, [NIGHT, GEISHA]),
      body: { ...PINKB, index: 0 },
    }),
    check: body => expectCartLike(body, 'v1/cart/cart.json'),
  },
  {
    operationId: 'setCartSelection',
    name: 'selects nothing',
    request: async ctx => ({
      path: '/v1/cart/selection',
      token: await memberWith(ctx, [PINKB, NIGHT, GEISHA]),
      body: { selected: false },
    }),
    check: body => expectCartLike(body, 'v1/cart/none-selected.json'),
  },
  {
    operationId: 'setCartSelection',
    name: 'selects everything again',
    request: async ctx => ({
      path: '/v1/cart/selection',
      token: await memberWith(ctx, [
        { ...PINKB, selected: false },
        { ...NIGHT, selected: false },
        GEISHA,
      ]),
      body: { selected: true },
    }),
    check: body => expectCartLike(body, 'v1/cart/cart.json'),
  },
  {
    operationId: 'applyCoupon',
    name: 'TTY10, typed in lower case with spaces',
    request: async ctx => ({
      path: '/v1/cart/coupon',
      token: await memberWith(ctx, [YIRGA]),
      body: { code: ' tty10 ' },
    }),
    check: body => expectCartLike(body, 'v1/cart/cart-coupon.json'),
  },
  {
    operationId: 'applyCoupon',
    name: 'an unknown code',
    request: async ctx => ({
      path: '/v1/cart/coupon',
      token: await memberWith(ctx, [YIRGA]),
      body: { code: 'FREE' },
    }),
    expectError: 'coupon_invalid',
  },
  {
    operationId: 'applyCoupon',
    name: 'an empty code removes the coupon',
    request: async (ctx) => {
      const m = await ctx.member()
      await addCartRow(ctx.prisma, m.id, YIRGA)
      await addAddressRow(ctx.prisma, m.id)
      await ctx.prisma.cart.update({
        where: { userId: m.id },
        data: { couponCode: 'TTY10' },
      })
      return {
        path: '/v1/cart/coupon',
        token: m.token,
        body: { code: '  ' },
      }
    },
    check: body => expectCartLike(body, 'v1/cart/coupon-removed.json'),
  },
  {
    operationId: 'removeCoupon',
    name: 'removes the coupon',
    request: async (ctx) => {
      const m = await ctx.member()
      await addCartRow(ctx.prisma, m.id, YIRGA)
      await addAddressRow(ctx.prisma, m.id)
      await ctx.prisma.cart.update({
        where: { userId: m.id },
        data: { couponCode: 'TTY10' },
      })
      return { path: '/v1/cart/coupon', token: m.token }
    },
    check: body => expectCartLike(body, 'v1/cart/coupon-removed.json'),
  },
  {
    operationId: 'setCartAddress',
    name: 'ships this order to the office',
    request: async (ctx) => {
      const m = await ctx.member()
      for (const line of [PINKB, NIGHT, GEISHA])
        await addCartRow(ctx.prisma, m.id, line)
      await addAddressRow(ctx.prisma, m.id)
      const office = await addAddressRow(ctx.prisma, m.id, { region: '上海市 静安区', detail: '江宁路 300 号 12F（前台代收）', tag: 'work' })
      return {
        path: '/v1/cart/address',
        token: m.token,
        body: { addressId: office },
      }
    },
    check: body => expectCartLike(body, 'v1/cart/address-set.json'),
  },
  {
    operationId: 'setCartAddress',
    name: 'someone else\'s address',
    request: async (ctx) => {
      const other = await ctx.member()
      const theirs = await addAddressRow(ctx.prisma, other.id)
      return {
        path: '/v1/cart/address',
        token: await memberWith(ctx, []),
        body: { addressId: theirs },
      }
    },
    expectError: 'not_found',
  },
  {
    operationId: 'quoteCart',
    name: 'a guest with two bags',
    request: async () => ({
      path: '/v1/cart/quote',
      body: {
        lines: [
          {
            productId: 'yirga',
            weight: '250g',
            grind: 'PO',
            pack: 'STD',
            quantity: 2,
          },
        ],
      },
    }),
    check: body => expect(body).toEqual(readFixture('v1/cart/guest-quote.json')),
  },
  {
    operationId: 'quoteCart',
    name: 'an invalid coupon',
    request: async () => ({
      path: '/v1/cart/quote',
      body: { lines: [], couponCode: 'NOPE' },
    }),
    expectError: 'coupon_invalid',
  },
  {
    operationId: 'quoteCart',
    name: 'an unknown product',
    request: async () => ({
      path: '/v1/cart/quote',
      body: {
        lines: [
          { ...YIRGA, productId: 'nope' },
        ],
      },
    }),
    expectError: 'not_found',
  },
  {
    operationId: 'quoteCart',
    name: 'quantity 0',
    request: async () => ({
      path: '/v1/cart/quote',
      body: {
        lines: [
          { ...YIRGA, quantity: 0 },
        ],
      },
    }),
    expectError: 'quantity_invalid',
  },
  {
    operationId: 'quoteCart',
    name: 'more than 50 lines',
    request: async () => ({
      path: '/v1/cart/quote',
      body: {
        lines: Array.from({ length: 51 }).fill(YIRGA),
      },
    }),
    expectError: 'cart_full',
  },
  {
    operationId: 'quoteCart',
    name: 'TTY10, a sold-out weight and too many units are priced and flagged',
    request: async () => ({
      path: '/v1/cart/quote',
      body: {
        lines: [
          { ...PINKB, weight: '1kg' },
          { ...GEISHA, quantity: 4 },
          { ...GEISHA, quantity: 4 },
        ],
        couponCode: 'tty10',
      },
    }),
    check: (body) => {
      const cart = body as Cart
      expect(cart.lines.map(l => [l.sku, l.quantity, l.problem])).toEqual([
        ['RT-PNK-1KG-PO-STD', 1, 'sold_out'],
        ['RT-GSH-100G-WB-GIFT', 8, 'out_of_stock'],
      ])
      expect(cart.coupon?.code).toBe('TTY10')
      expect(cart.address).toBeNull()
      // 425 + 141 × 8 = 1553; discount round(155.3) = 155.
      expect(cart.totals).toMatchObject({
        subtotal: 1553,
        discount: 155,
        shipping: 0,
        total: 1398,
      })
    },
  },
  {
    operationId: 'mergeCart',
    name: 'merges the guest Geisha',
    request: async ctx => ({
      path: '/v1/cart/merge',
      token: await memberWith(ctx, [PINKB, NIGHT]),
      body: {
        lines: [
          {
            productId: 'geisha',
            weight: '100g',
            grind: 'WB',
            pack: 'GIFT',
            quantity: 1,
          },
        ],
      },
    }),
    check: (body) => {
      const result = body as CartMergeResult
      const expected = readFixture('v1/cart/merge.json') as CartMergeResult
      expect(result.mergedCount).toBe(expected.mergedCount)
      expect(result.skippedCount).toBe(expected.skippedCount)
      expectCartLike(result.cart, 'v1/cart/cart.json')
    },
  },
  {
    operationId: 'mergeCart',
    name: 'clamps to the stock, skips what cannot be bought, takes a valid coupon',
    request: async ctx => ({
      path: '/v1/cart/merge',
      token: await memberWith(ctx, [
        { ...GEISHA, quantity: 4 },
      ]),
      body: {
        lines: [
          { ...GEISHA, quantity: 5, selected: false },
          { ...PINKB, weight: '1kg' },
          { ...YIRGA, productId: 'nope' },
          { ...YIRGA, quantity: 9, selected: false },
          { ...YIRGA, quantity: 3 },
        ],
        couponCode: ' Tty10',
      },
    }),
    check: (body) => {
      const result = body as CartMergeResult
      // Geisha 4 → 6 (stock): +2; yirga new 9 (+9), then already 9: +0.
      expect(result.mergedCount).toBe(11)
      expect(result.skippedCount).toBe(2)
      expect(result.cart.lines.map(l => [l.sku, l.quantity, l.selected])).toEqual([
        ['RT-GSH-100G-WB-GIFT', 6, true],
        ['RT-YIR-250G-WB-STD', 9, true],
      ])
      expect(result.cart.coupon?.code).toBe('TTY10')
    },
  },
]

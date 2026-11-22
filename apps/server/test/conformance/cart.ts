// Conformance cases of area `cart` (owner: server-shop). Carts are built directly in the
// database; priced answers are compared with the committed fixtures (address ids aside).
import type { Cart, CartMergeResult } from '@coffee/contract'
import type { TestLine } from '../support/members.ts'
import type { CaseContext, ConformanceCase } from './types.ts'
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
    operationId: 'removeCartLine',
    name: 'removes a line',
    request: async ctx => ({ path: '/v1/cart/lines/RT-PNK-250G-PO-STD', token: await memberWith(ctx, [PINKB, NIGHT, GEISHA]) }),
    check: body => expectCartLike(body, 'v1/cart/removed.json'),
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
]

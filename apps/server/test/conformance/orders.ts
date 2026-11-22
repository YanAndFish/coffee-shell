// Conformance cases of area `orders` (owner: server-orders). The clock is NOW
// (2026-11-22T13:04Z = 21:04 shop time).
import type { Order, OrderList, PlacedOrder } from '@coffee/contract'
import type { TestLine } from '../support/members.ts'
import type { ConformanceCase } from './types.ts'
import { expect } from 'vitest'
import { readFixture } from '../helpers.ts'
import { addAddressRow, addCartRow, addOrderRow } from '../support/members.ts'
import { NOW } from './types.ts'

const SAMPLE: TestLine[] = [
  {
    productId: 'pinkb',
    weight: '250g',
    grind: 'PO',
    pack: 'STD',
    quantity: 1,
  },
  {
    productId: 'night',
    weight: '500g',
    grind: 'ES',
    pack: 'STD',
    quantity: 1,
  },
  {
    productId: 'geisha',
    weight: '100g',
    grind: 'WB',
    pack: 'GIFT',
    quantity: 1,
  },
]

const DAY = 24 * 60 * 60 * 1000

export const orderCases: ConformanceCase[] = [
  {
    operationId: 'placeOrder',
    name: 'places the sample cart (the design receipt)',
    request: async ({ member, prisma }) => {
      const m = await member()
      for (const line of SAMPLE)
        await addCartRow(prisma, m.id, line)
      await addAddressRow(prisma, m.id)
      return {
        path: '/v1/orders',
        token: m.token,
        body: { idempotencyKey: 'checkout-0001', expectedTotal: 426 },
      }
    },
    check: async (body, { prisma }) => {
      const placed = body as PlacedOrder
      const expected = readFixture('v1/orders/placed.json') as PlacedOrder
      expect(placed.order.number).toMatch(/^RT261122\d{4}$/)
      expect({ ...placed.order, number: '' }).toEqual({ ...expected.order, number: '' })
      expect(placed.cart.lines).toEqual([])
      expect(placed.cart.totals).toEqual(expected.cart.totals)
      const geisha = await prisma.product.findUniqueOrThrow({
        where: { id: 'geisha' },
      })
      expect(geisha.stock).toBe(5)
    },
  },
  {
    operationId: 'placeOrder',
    name: 'nothing selected',
    request: async ({ member, prisma }) => {
      const m = await member()
      await addCartRow(prisma, m.id, { ...SAMPLE[0]!, selected: false })
      await addAddressRow(prisma, m.id)
      return {
        path: '/v1/orders',
        token: m.token,
        body: { idempotencyKey: 'checkout-0002' },
      }
    },
    expectError: 'empty_selection',
  },
  {
    operationId: 'placeOrder',
    name: 'no address',
    request: async ({ member, prisma }) => {
      const m = await member()
      await addCartRow(prisma, m.id, SAMPLE[0]!)
      return {
        path: '/v1/orders',
        token: m.token,
        body: { idempotencyKey: 'checkout-0003' },
      }
    },
    expectError: 'address_required',
  },
  {
    operationId: 'placeOrder',
    name: 'the total moved',
    request: async ({ member, prisma }) => {
      const m = await member()
      await addCartRow(prisma, m.id, SAMPLE[0]!)
      await addAddressRow(prisma, m.id)
      return {
        path: '/v1/orders',
        token: m.token,
        body: { idempotencyKey: 'checkout-0004', expectedTotal: 1 },
      }
    },
    expectError: 'price_changed',
  },
  {
    operationId: 'placeOrder',
    name: 'a sold-out weight',
    request: async ({ member, prisma }) => {
      const m = await member()
      await addCartRow(prisma, m.id, { ...SAMPLE[2]!, weight: '1kg' })
      await addAddressRow(prisma, m.id)
      return {
        path: '/v1/orders',
        token: m.token,
        body: { idempotencyKey: 'checkout-0005' },
      }
    },
    expectError: 'sold_out',
  },
  {
    operationId: 'placeOrder',
    name: 'more than the stock over several lines of one product',
    request: async ({ member, prisma }) => {
      const m = await member()
      // 瑰夏 has at most 6 units: each line fits, together they do not.
      await addCartRow(prisma, m.id, { ...SAMPLE[2]!, quantity: 5 })
      await addCartRow(prisma, m.id, { ...SAMPLE[2]!, grind: 'PO', quantity: 2 })
      await addAddressRow(prisma, m.id)
      return {
        path: '/v1/orders',
        token: m.token,
        body: { idempotencyKey: 'checkout-0006' },
      }
    },
    expectError: 'out_of_stock',
  },
  {
    operationId: 'placeOrder',
    name: 'a used key answers the same order again',
    request: async ({ member, prisma }) => {
      const m = await member()
      const number = await addOrderRow(prisma, m.id, [SAMPLE[0]!], NOW)
      const order = await prisma.order.findUniqueOrThrow({
        where: { number },
      })
      await addCartRow(prisma, m.id, SAMPLE[1]!)
      return {
        path: '/v1/orders',
        token: m.token,
        body: { idempotencyKey: order.idempotencyKey, expectedTotal: 1 },
      }
    },
    check: (body) => {
      const placed = body as PlacedOrder
      expect(placed.order.items.map(i => i.productId)).toEqual(['pinkb'])
      expect(placed.cart.lines.map(l => l.productId)).toEqual(['night'])
    },
  },
  {
    operationId: 'listOrders',
    name: 'newest first, status by time',
    request: async ({ member, prisma }) => {
      const m = await member()
      await addOrderRow(prisma, m.id, [SAMPLE[0]!], new Date(NOW.getTime() - 16 * DAY))
      await addOrderRow(prisma, m.id, [SAMPLE[1]!], new Date(NOW.getTime() - 2 * DAY))
      await addOrderRow(prisma, m.id, [SAMPLE[2]!], new Date(NOW.getTime() - 60 * 60 * 1000))
      return { path: '/v1/orders', token: m.token }
    },
    check: (body) => {
      const list = body as OrderList
      expect(list.total).toBe(3)
      expect(list.items.map(o => o.status)).toEqual(['roasting', 'shipping', 'delivered'])
    },
  },
  {
    operationId: 'listOrders',
    name: 'limit keeps the total',
    request: async ({ member, prisma }) => {
      const m = await member()
      for (let i = 0; i < 3; i++)
        await addOrderRow(prisma, m.id, [SAMPLE[0]!], new Date(NOW.getTime() - i * DAY))
      return {
        path: '/v1/orders',
        token: m.token,
        query: { limit: '1' },
      }
    },
    check: (body) => {
      const list = body as OrderList
      expect(list.items).toHaveLength(1)
      expect(list.items[0]?.createdAt).toBe(NOW.toISOString())
      expect(list.total).toBe(3)
    },
  },
  {
    operationId: 'listOrders',
    name: 'a new member has none',
    request: async ({ member }) => ({ path: '/v1/orders', token: (await member()).token }),
    check: body => expect(body).toEqual(readFixture('v1/orders/list-empty.json')),
  },
  {
    operationId: 'getOrder',
    name: 'the receipt',
    request: async ({ member, prisma }) => {
      const m = await member()
      const number = await addOrderRow(prisma, m.id, [
        {
          productId: 'yirga',
          weight: '250g',
          grind: 'PO',
          pack: 'STD',
          quantity: 2,
        },
      ], new Date('2026-11-06T09:30:00.000Z'))
      return { path: `/v1/orders/${number}`, token: m.token }
    },
    check: (body) => {
      const order = body as Order
      const expected = readFixture('v1/orders/order.json') as Order
      expect({ ...order, number: '' }).toEqual({ ...expected, number: '' })
    },
  },
  {
    operationId: 'getOrder',
    name: 'someone else\'s order',
    request: async ({ member, prisma }) => {
      const other = await member()
      const number = await addOrderRow(prisma, other.id, [SAMPLE[0]!])
      return { path: `/v1/orders/${number}`, token: (await member()).token }
    },
    expectError: 'not_found',
  },
]

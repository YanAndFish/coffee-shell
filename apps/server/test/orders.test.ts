// Checkout, the order list and one order through the real app (P-order-1…5, U7), plus the
// checkout transaction called directly with a stubbed order-number generator.
import type { ApiError, ErrorCode, Order, OrderList, PlacedOrder } from '@coffee/contract'
import type { TestDatabase, TestServer } from './helpers.ts'
import type { TestLine, TestMember } from './support/members.ts'
import { ERRORS } from '@coffee/contract'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { MAX_NUMBER_DRAWS, placeOrderNow } from '../src/services/orders-checkout.ts'
import { NOW } from './conformance/types.ts'
import { call, createTestDatabase, listenApp } from './helpers.ts'
import { addAddressRow, addCartRow, addOrderRow, createMember } from './support/members.ts'

const HOUR = 60 * 60 * 1000
const DAY = 24 * HOUR

const PINK: TestLine = {
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
const YUNNAN: TestLine = {
  productId: 'yunnan',
  weight: '250g',
  grind: 'FP',
  pack: 'STD',
  quantity: 2,
}

let db: TestDatabase
let server: TestServer
/** The app's clock: tests move it. */
let clock = NOW
let keySerial = 0

beforeAll(async () => {
  db = await createTestDatabase()
  server = await listenApp(db.prisma, {
    now: () => clock,
  })
}, 60_000)

afterAll(async () => {
  await server?.close()
  await db?.cleanup()
})

function member(): Promise<TestMember> {
  return createMember(db.prisma, { now: NOW })
}

function newKey(): string {
  keySerial++
  return `test-key-${String(keySerial).padStart(4, '0')}`
}

async function stockOf(productId: string): Promise<number> {
  return (await db.prisma.product.findUniqueOrThrow({
    where: { id: productId },
  })).stock
}

async function setStock(productId: string, stock: number): Promise<void> {
  await db.prisma.product.update({
    where: { id: productId },
    data: { stock },
  })
}

function place(m: TestMember, body: Record<string, unknown> = {}) {
  return call(server.origin, 'POST', '/v1/orders', {
    token: m.token,
    body: {
      idempotencyKey: newKey(),
      ...body,
    },
  })
}

function expectError(res: { status: number, body: unknown }, code: ErrorCode): ApiError['error'] {
  const { error } = res.body as ApiError
  expect(error.code, JSON.stringify(res.body)).toBe(code)
  expect(error.message).toBe(ERRORS[code].message)
  expect(res.status).toBe(ERRORS[code].status)
  return error
}

/** Everything checkout may change for `m`, to prove a refusal changed nothing. */
async function snapshot(m: TestMember) {
  const [cart, items, orders, products] = await Promise.all([
    db.prisma.cart.findUniqueOrThrow({
      where: { userId: m.id },
    }),
    db.prisma.cartItem.findMany({
      where: { userId: m.id },
      orderBy: { id: 'asc' },
    }),
    db.prisma.order.count({
      where: { userId: m.id },
    }),
    db.prisma.product.findMany({
      select: { id: true, stock: true },
      orderBy: { id: 'asc' },
    }),
  ])
  return {
    cart: { couponCode: cart.couponCode, addressId: cart.addressId },
    items,
    orders,
    products,
  }
}

describe('placeOrder: happy path', () => {
  it('orders the selected lines with snapshots, decrements stock and keeps unselected lines', async () => {
    clock = NOW
    const m = await member()
    await addCartRow(db.prisma, m.id, PINK)
    await addCartRow(db.prisma, m.id, { ...YUNNAN, selected: false })
    await addCartRow(db.prisma, m.id, NIGHT)
    await addCartRow(db.prisma, m.id, { ...GEISHA, quantity: 2 })
    await addAddressRow(db.prisma, m.id, { recipient: '林晓' })
    const chosen = await addAddressRow(db.prisma, m.id, {
      recipient: '陈默',
      phone: '13900000817',
      region: '浙江省 杭州市 西湖区',
      detail: '文三路 128 号',
    })
    await db.prisma.cart.update({
      where: { userId: m.id },
      data: { addressId: chosen },
    })
    const before = {
      pinkb: await stockOf('pinkb'),
      night: await stockOf('night'),
      geisha: await stockOf('geisha'),
      yunnan: await stockOf('yunnan'),
    }

    // 118 + 167 + 2 × 141 = 567, free shipping.
    const res = await place(m, { expectedTotal: 567 })
    expect(res.status, JSON.stringify(res.body)).toBe(201)
    const { order, cart } = res.body as PlacedOrder
    expect(order.number).toMatch(/^RT261122\d{4}$/)
    expect(order).toMatchObject({
      createdAt: '2026-11-22T13:04:00.000Z',
      orderDate: '2026-11-22',
      etaAt: '2026-11-25T13:04:00.000Z',
      etaDate: '2026-11-25',
      status: 'roasting',
      subtotal: 567,
      discount: 0,
      shipping: 0,
      total: 567,
      couponCode: null,
      address: {
        recipient: '陈默',
        phone: '13900000817',
        phoneMasked: '139 **** 0817',
        region: '浙江省 杭州市 西湖区',
        detail: '文三路 128 号',
      },
    })
    expect(order.items).toEqual([
      {
        sku: 'RT-PNK-250G-PO-STD',
        productId: 'pinkb',
        name: '慧兰 粉红波旁',
        weight: '250g',
        grind: 'PO',
        pack: 'STD',
        quantity: 1,
        unitPrice: 118,
        lineTotal: 118,
      },
      {
        sku: 'RT-NGT-500G-ES-STD',
        productId: 'night',
        name: '「夜班」意式拼配',
        weight: '500g',
        grind: 'ES',
        pack: 'STD',
        quantity: 1,
        unitPrice: 167,
        lineTotal: 167,
      },
      {
        sku: 'RT-GSH-100G-WB-GIFT',
        productId: 'geisha',
        name: '波奎特 瑰夏',
        weight: '100g',
        grind: 'WB',
        pack: 'GIFT',
        quantity: 2,
        unitPrice: 141,
        lineTotal: 282,
      },
    ])
    // The unselected line stays; the chosen order address stays.
    expect(cart.lines.map(l => [l.sku, l.selected])).toEqual([
      ['RT-YUN-250G-FP-STD', false],
    ])
    expect(cart.address?.id).toBe(chosen)
    expect(cart.totals).toMatchObject({ itemCount: 2, selectedCount: 0, total: 0 })
    expect(await stockOf('pinkb')).toBe(before.pinkb - 1)
    expect(await stockOf('night')).toBe(before.night - 1)
    expect(await stockOf('geisha')).toBe(before.geisha - 2)
    expect(await stockOf('yunnan')).toBe(before.yunnan)

    // Snapshots survive later changes to the product and the address.
    await db.prisma.product.update({
      where: { id: 'pinkb' },
      data: { name: '改名了', basePrice: 999 },
    })
    await db.prisma.address.update({
      where: { id: chosen },
      data: { recipient: '别人' },
    })
    const again = await call(server.origin, 'GET', `/v1/orders/${order.number}`, { token: m.token })
    expect(again.body).toEqual(order)
    await db.prisma.product.update({
      where: { id: 'pinkb' },
      data: { name: '慧兰 粉红波旁', basePrice: 118 },
    })
  })

  it('applies the cart coupon, snapshots it and clears it', async () => {
    const m = await member()
    await addCartRow(db.prisma, m.id, {
      productId: 'yirga',
      weight: '250g',
      grind: 'PO',
      pack: 'STD',
      quantity: 1,
    })
    await addAddressRow(db.prisma, m.id)
    await db.prisma.cart.update({
      where: { userId: m.id },
      data: { couponCode: 'TTY10' },
    })
    // data.md 例 2: 98 − 10 + 12 = 100.
    const res = await place(m, { expectedTotal: 100 })
    expect(res.status, JSON.stringify(res.body)).toBe(201)
    const { order, cart } = res.body as PlacedOrder
    expect(order).toMatchObject({
      subtotal: 98,
      discount: 10,
      shipping: 12,
      total: 100,
      couponCode: 'TTY10',
    })
    expect(cart.coupon).toBeNull()
    expect(cart.lines).toEqual([])
    expect((await db.prisma.cart.findUniqueOrThrow({
      where: { userId: m.id },
    })).couponCode).toBeNull()
  })

  it('without expectedTotal it just places the order', async () => {
    const m = await member()
    await addCartRow(db.prisma, m.id, PINK)
    await addAddressRow(db.prisma, m.id)
    const res = await place(m)
    expect(res.status).toBe(201)
    expect((res.body as PlacedOrder).order.total).toBe(130)
  })
})

describe('placeOrder: refusals change nothing', () => {
  it('empty_selection: an empty cart', async () => {
    const m = await member()
    await addAddressRow(db.prisma, m.id)
    expectError(await place(m), 'empty_selection')
  })

  it('empty_selection: nothing selected (checked before the address)', async () => {
    const m = await member()
    await addCartRow(db.prisma, m.id, { ...PINK, selected: false })
    const before = await snapshot(m)
    expectError(await place(m), 'empty_selection')
    expect(await snapshot(m)).toEqual(before)
  })

  it('address_required', async () => {
    const m = await member()
    await addCartRow(db.prisma, m.id, PINK)
    const before = await snapshot(m)
    expectError(await place(m), 'address_required')
    expect(await snapshot(m)).toEqual(before)
  })

  it('sold_out: a sold-out weight, with details', async () => {
    const m = await member()
    await addCartRow(db.prisma, m.id, PINK)
    await addCartRow(db.prisma, m.id, {
      productId: 'geisha',
      weight: '500g',
      grind: 'WB',
      pack: 'STD',
      quantity: 1,
    })
    await addAddressRow(db.prisma, m.id)
    const before = await snapshot(m)
    const error = expectError(await place(m), 'sold_out')
    expect(error.details).toEqual({ productId: 'geisha', productName: '波奎特 瑰夏', weight: '500g' })
    expect(await snapshot(m)).toEqual(before)
  })

  it('a sold-out weight that is not selected does not block', async () => {
    const m = await member()
    await addCartRow(db.prisma, m.id, PINK)
    await addCartRow(db.prisma, m.id, {
      productId: 'pinkb',
      weight: '1kg',
      grind: 'WB',
      pack: 'STD',
      quantity: 1,
      selected: false,
    })
    await addAddressRow(db.prisma, m.id)
    expect((await place(m)).status).toBe(201)
  })

  it('out_of_stock: the sum of a product\'s lines above its stock, with details', async () => {
    const m = await member()
    await setStock('antigua', 10)
    await addCartRow(db.prisma, m.id, {
      productId: 'antigua',
      weight: '250g',
      grind: 'WB',
      pack: 'STD',
      quantity: 6,
    })
    await addCartRow(db.prisma, m.id, PINK)
    await addCartRow(db.prisma, m.id, {
      productId: 'antigua',
      weight: '100g',
      grind: 'FP',
      pack: 'GIFT',
      quantity: 5,
    })
    await addAddressRow(db.prisma, m.id)
    const before = await snapshot(m)
    const error = expectError(await place(m), 'out_of_stock')
    expect(error.details).toEqual({ productId: 'antigua', productName: '安提瓜 火山', available: 10 })
    expect(await snapshot(m)).toEqual(before)
    // Unselecting one line makes it fit.
    await db.prisma.cartItem.updateMany({
      where: { userId: m.id, productId: 'antigua', weight: '100g' },
      data: { selected: false },
    })
    expect((await place(m)).status).toBe(201)
    expect(await stockOf('antigua')).toBe(4)
  })

  it('out_of_stock: a product with no stock left', async () => {
    const m = await member()
    await setStock('mandh', 0)
    await addCartRow(db.prisma, m.id, {
      productId: 'mandh',
      weight: '250g',
      grind: 'WB',
      pack: 'STD',
      quantity: 1,
    })
    await addAddressRow(db.prisma, m.id)
    const error = expectError(await place(m), 'out_of_stock')
    expect(error.details).toEqual({ productId: 'mandh', productName: '曼特宁 G1', available: 0 })
    await setStock('mandh', 40)
  })

  it('price_changed: nothing changes, and the right total then succeeds', async () => {
    const m = await member()
    await addCartRow(db.prisma, m.id, PINK)
    await addAddressRow(db.prisma, m.id)
    await db.prisma.cart.update({
      where: { userId: m.id },
      data: { couponCode: 'TTY10' },
    })
    const before = await snapshot(m)
    const key = newKey()
    // 118 − 12 + 12 = 118
    expectError(await place(m, { idempotencyKey: key, expectedTotal: 130 }), 'price_changed')
    expect(await snapshot(m)).toEqual(before)
    // A refused attempt does not burn its key.
    const res = await place(m, { idempotencyKey: key, expectedTotal: 118 })
    expect(res.status).toBe(201)
  })

  it('a malformed key is validation_failed', async () => {
    const m = await member()
    expectError(await place(m, { idempotencyKey: 'short' }), 'validation_failed')
    expectError(await place(m, { idempotencyKey: 'has spaces in it' }), 'validation_failed')
  })
})

describe('placeOrder: idempotency', () => {
  it('the same key answers the same order again without touching stock or the cart', async () => {
    const m = await member()
    await addCartRow(db.prisma, m.id, NIGHT)
    await addAddressRow(db.prisma, m.id)
    const key = newKey()
    const first = await place(m, { idempotencyKey: key })
    expect(first.status).toBe(201)
    const stock = await stockOf('night')
    // The member fills the cart again; a retry of the old attempt must not buy it.
    await addCartRow(db.prisma, m.id, PINK)
    const replay = await place(m, { idempotencyKey: key, expectedTotal: 1 })
    expect(replay.status).toBe(201)
    const a = first.body as PlacedOrder
    const b = replay.body as PlacedOrder
    expect(b.order).toEqual(a.order)
    expect(b.cart.lines.map(l => l.sku)).toEqual(['RT-PNK-250G-PO-STD'])
    expect(await stockOf('night')).toBe(stock)
    expect(await db.prisma.order.count({
      where: { userId: m.id },
    })).toBe(1)
  })

  it('the same key from another member is a different order', async () => {
    const key = newKey()
    const numbers: string[] = []
    for (let i = 0; i < 2; i++) {
      const m = await member()
      await addCartRow(db.prisma, m.id, PINK)
      await addAddressRow(db.prisma, m.id)
      const res = await place(m, { idempotencyKey: key })
      expect(res.status).toBe(201)
      numbers.push((res.body as PlacedOrder).order.number)
    }
    expect(numbers[0]).not.toBe(numbers[1])
  })

  it('two concurrent requests with one key place one order', async () => {
    const m = await member()
    await addCartRow(db.prisma, m.id, { ...NIGHT, quantity: 2 })
    await addAddressRow(db.prisma, m.id)
    const stock = await stockOf('night')
    const key = newKey()
    const [a, b] = await Promise.all([
      place(m, { idempotencyKey: key }),
      place(m, { idempotencyKey: key }),
    ])
    expect([a.status, b.status]).toEqual([201, 201])
    expect((a.body as PlacedOrder).order.number).toBe((b.body as PlacedOrder).order.number)
    expect(await stockOf('night')).toBe(stock - 2)
    expect(await db.prisma.order.count({
      where: { userId: m.id },
    })).toBe(1)
  })
})

describe('placeOrder: concurrency', () => {
  it('two members competing for the last unit: exactly one wins', async () => {
    await setStock('nyeri', 1)
    const members = [await member(), await member()]
    for (const m of members) {
      await addCartRow(db.prisma, m.id, {
        productId: 'nyeri',
        weight: '250g',
        grind: 'PO',
        pack: 'STD',
        quantity: 1,
      })
      await addAddressRow(db.prisma, m.id)
    }
    const results = await Promise.all(members.map(m => place(m)))
    const statuses = results.map(r => r.status).sort()
    expect(statuses).toEqual([201, 409])
    const loser = results.find(r => r.status === 409)!
    expect(expectError(loser, 'out_of_stock').details).toEqual({ productId: 'nyeri', productName: '涅里 AA 双洗', available: 0 })
    expect(await stockOf('nyeri')).toBe(0)
    const orders = await db.prisma.orderItem.count({
      where: { productId: 'nyeri' },
    })
    expect(orders).toBe(1)
    await setStock('nyeri', 18)
  })

  it('many members competing for 3 units: exactly 3 orders, stock never negative', async () => {
    await setStock('yirga', 3)
    const members = await Promise.all(Array.from({ length: 6 }, () => member()))
    for (const m of members) {
      await addCartRow(db.prisma, m.id, {
        productId: 'yirga',
        weight: '100g',
        grind: 'CB',
        pack: 'STD',
        quantity: 1,
      })
      await addAddressRow(db.prisma, m.id)
    }
    const results = await Promise.all(members.map(m => place(m)))
    expect(results.filter(r => r.status === 201)).toHaveLength(3)
    expect(results.filter(r => r.status === 409)).toHaveLength(3)
    expect(await stockOf('yirga')).toBe(0)
    await setStock('yirga', 42)
  })
})

describe('order numbers', () => {
  it('rT + shop date + 4 zero-padded digits; the shop date is Asia/Shanghai', async () => {
    const m = await member()
    await addAddressRow(db.prisma, m.id)
    await addCartRow(db.prisma, m.id, PINK)
    // 2026-11-22 16:30 UTC is already 11-23 in Shanghai.
    const placed = await placeOrderNow(db.prisma, {
      userId: m.id,
      idempotencyKey: newKey(),
      now: new Date('2026-11-22T16:30:00.000Z'),
      draw: () => 7,
    })
    expect(placed.order.number).toBe('RT2611230007')
    expect(placed.order.orderDate).toBe('2026-11-23')
  })

  it('redraws on a collision', async () => {
    const m = await member()
    await addAddressRow(db.prisma, m.id)
    const now = new Date('2026-10-01T02:00:00.000Z')
    const draws = [417, 417, 31]
    const taken: string[] = []
    for (let i = 0; i < 2; i++) {
      await addCartRow(db.prisma, m.id, PINK)
      const placed = await placeOrderNow(db.prisma, {
        userId: m.id,
        idempotencyKey: newKey(),
        now,
        draw: () => draws.shift()!,
      })
      taken.push(placed.order.number)
    }
    expect(taken).toEqual(['RT2610010417', 'RT2610010031'])
    expect(draws).toEqual([])
  })

  it('gives up (500, nothing changed) when every draw collides', async () => {
    const m = await member()
    await addAddressRow(db.prisma, m.id)
    const now = new Date('2026-09-01T02:00:00.000Z')
    await addCartRow(db.prisma, m.id, PINK)
    await placeOrderNow(db.prisma, {
      userId: m.id,
      idempotencyKey: newKey(),
      now,
      draw: () => 1,
    })
    await addCartRow(db.prisma, m.id, PINK)
    const before = await snapshot(m)
    let calls = 0
    await expect(placeOrderNow(db.prisma, {
      userId: m.id,
      idempotencyKey: newKey(),
      now,
      draw: () => {
        calls++
        return 1
      },
    })).rejects.toThrow(/no free order number/)
    expect(calls).toBe(MAX_NUMBER_DRAWS)
    expect(await snapshot(m)).toEqual(before)
  })
})

describe('status from the injected clock', () => {
  it('roasting for 24 h, shipping until the ETA, then delivered', async () => {
    const t0 = new Date('2026-11-10T04:00:00.000Z')
    clock = t0
    const m = await createMember(db.prisma, { now: t0 })
    await addCartRow(db.prisma, m.id, PINK)
    await addAddressRow(db.prisma, m.id)
    const res = await place(m)
    expect(res.status).toBe(201)
    const { order } = res.body as PlacedOrder
    expect(order).toMatchObject({ status: 'roasting', etaAt: '2026-11-13T04:00:00.000Z', etaDate: '2026-11-13' })
    const at = async (offset: number) => {
      clock = new Date(t0.getTime() + offset)
      const one = (await call(server.origin, 'GET', `/v1/orders/${order.number}`, { token: m.token })).body as Order
      const list = (await call(server.origin, 'GET', '/v1/orders', { token: m.token })).body as OrderList
      expect(list.items[0]?.status).toBe(one.status)
      return one.status
    }
    expect(await at(0)).toBe('roasting')
    expect(await at(DAY - 1)).toBe('roasting')
    expect(await at(DAY)).toBe('shipping')
    expect(await at(3 * DAY - 1)).toBe('shipping')
    expect(await at(3 * DAY)).toBe('delivered')
    expect(await at(6 * DAY)).toBe('delivered')
    clock = NOW
  })
})

describe('listOrders and getOrder', () => {
  it('newest first, limit, total and summary lines', async () => {
    clock = NOW
    const m = await member()
    for (let i = 0; i < 4; i++) {
      await addOrderRow(db.prisma, m.id, [
        PINK,
        { ...NIGHT, quantity: 3 },
      ], new Date(NOW.getTime() - (4 - i) * DAY))
    }
    const all = (await call(server.origin, 'GET', '/v1/orders', { token: m.token })).body as OrderList
    expect(all.total).toBe(4)
    expect(all.items.map(o => o.createdAt)).toEqual([1, 2, 3, 4].map(d => new Date(NOW.getTime() - d * DAY).toISOString()))
    expect(all.items[0]).toMatchObject({
      itemCount: 4,
      total: 118 + 3 * 167,
      lines: [
        {
          productId: 'pinkb',
          name: '慧兰 粉红波旁',
          weight: '250g',
          quantity: 1,
        },
        {
          productId: 'night',
          name: '「夜班」意式拼配',
          weight: '500g',
          quantity: 3,
        },
      ],
    })
    const limited = (await call(server.origin, 'GET', '/v1/orders?limit=2', { token: m.token })).body as OrderList
    expect(limited.items.map(o => o.number)).toEqual(all.items.slice(0, 2).map(o => o.number))
    expect(limited.total).toBe(4)
    expectError(await call(server.origin, 'GET', '/v1/orders?limit=0', { token: m.token }), 'validation_failed')
    expectError(await call(server.origin, 'GET', '/v1/orders?limit=51', { token: m.token }), 'validation_failed')
  })

  it('isolation: another member\'s orders are invisible', async () => {
    const owner = await member()
    const other = await member()
    const number = await addOrderRow(db.prisma, owner.id, [PINK], NOW)
    expectError(await call(server.origin, 'GET', `/v1/orders/${number}`, { token: other.token }), 'not_found')
    const list = (await call(server.origin, 'GET', '/v1/orders', { token: other.token })).body as OrderList
    expect(list).toEqual({ items: [], total: 0 })
    expect((await call(server.origin, 'GET', `/v1/orders/${number}`, { token: owner.token })).status).toBe(200)
  })

  it('a malformed number is validation_failed; an unknown one not_found', async () => {
    const m = await member()
    expectError(await call(server.origin, 'GET', '/v1/orders/RT123', { token: m.token }), 'validation_failed')
    expectError(await call(server.origin, 'GET', '/v1/orders/RT0000000000', { token: m.token }), 'not_found')
  })
})

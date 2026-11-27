// The cart (server-shop) through the real app: every member mutation with its error codes,
// selection and totals, coupon and free-shipping boundaries, the order address fallback,
// guest quote ↔ member cart parity, the merge after login, and isolation between members.
import type { ApiError, ApiErrorDetails, Cart, CartMergeResult, ErrorCode } from '@coffee/contract'
import type { TestDatabase, TestServer } from './helpers.ts'
import type { TestLine, TestMember } from './support/members.ts'
import { ApiErrorSchema, CartMergeResultSchema, CartSchema, ERRORS } from '@coffee/contract'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { createApp } from '../src/app.ts'
import { createPrisma } from '../src/db.ts'
import { distinctLines } from './conformance/cart.ts'
import { NOW } from './conformance/types.ts'
import { call, createTestDatabase, listen, listenApp, readFixture } from './helpers.ts'
import { addAddressRow, addCartRow, createMember } from './support/members.ts'

let db: TestDatabase
let server: TestServer

beforeAll(async () => {
  db = await createTestDatabase()
  server = await listenApp(db.prisma, {
    now: () => NOW,
  })
}, 60_000)

afterAll(async () => {
  await server?.close()
  await db?.cleanup()
})

const YIRGA: TestLine = {
  productId: 'yirga',
  weight: '250g',
  grind: 'WB',
  pack: 'STD',
  quantity: 1,
}
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

async function member(lines: TestLine[] = []): Promise<TestMember> {
  const m = await createMember(db.prisma, { now: NOW })
  for (const line of lines)
    await addCartRow(db.prisma, m.id, line)
  return m
}

async function ok(method: string, path: string, token: string | undefined, body?: unknown): Promise<Cart> {
  const response = await call(server.origin, method, path, {
    ...(token
      ? { token }
      : {}),
    ...(body !== undefined
      ? { body }
      : {}),
  })
  expect(response.status, JSON.stringify(response.body)).toBe(200)
  return CartSchema.parse(response.body)
}

async function failure(method: string, path: string, token: string | undefined, body: unknown, code: ErrorCode, details?: ApiErrorDetails): Promise<void> {
  const response = await call(server.origin, method, path, {
    ...(token
      ? { token }
      : {}),
    ...(body !== undefined
      ? { body }
      : {}),
  })
  const envelope: ApiError = ApiErrorSchema.parse(response.body)
  expect(envelope.error.code, JSON.stringify(response.body)).toBe(code)
  expect(response.status).toBe(ERRORS[code].status)
  if (details)
    expect(envelope.error.details).toEqual(details)
}

const getCart = (token: string) => ok('GET', '/v1/cart', token)
const add = (token: string, line: TestLine & { index?: number }) => ok('POST', '/v1/cart/lines', token, line)
const skus = (cart: Cart) => cart.lines.map(l => l.sku)

describe('pOST /v1/cart/lines', () => {
  it('adds a selected line at the end, re-adding sums (capped at 9) and re-selects', async () => {
    const m = await member([PINKB])
    let cart = await add(m.token, { ...YIRGA, quantity: 4 })
    expect(skus(cart)).toEqual(['RT-PNK-250G-PO-STD', 'RT-YIR-250G-WB-STD'])
    await ok('PATCH', '/v1/cart/lines/RT-YIR-250G-WB-STD', m.token, { selected: false })
    cart = await add(m.token, { ...YIRGA, quantity: 3 })
    expect(cart.lines[1]).toMatchObject({
      quantity: 7,
      selected: true,
      unitPrice: 98,
      lineTotal: 686,
    })
    cart = await add(m.token, { ...YIRGA, quantity: 9 })
    expect(cart.lines[1]).toMatchObject({ quantity: 9, lineTotal: 882 })
    expect(cart.lines).toHaveLength(2)
    expect(cart.totals).toMatchObject({ itemCount: 10, selectedCount: 10, subtotal: 1000 })
  })

  it('index inserts a new line (undo of a delete); ignored for a SKU already there', async () => {
    const m = await member([PINKB, NIGHT, GEISHA])
    let cart = await ok('DELETE', '/v1/cart/lines/RT-NGT-500G-ES-STD', m.token)
    expect(skus(cart)).toEqual(['RT-PNK-250G-PO-STD', 'RT-GSH-100G-WB-GIFT'])
    cart = await add(m.token, { ...NIGHT, index: 1 })
    expect(cart).toEqual(await getCart(m.token))
    expect(skus(cart)).toEqual(['RT-PNK-250G-PO-STD', 'RT-NGT-500G-ES-STD', 'RT-GSH-100G-WB-GIFT'])
    cart = await add(m.token, { ...YIRGA, index: 0 })
    expect(skus(cart)[0]).toBe('RT-YIR-250G-WB-STD')
    cart = await add(m.token, { ...YIRGA, index: 3 })
    expect(skus(cart)[0]).toBe('RT-YIR-250G-WB-STD')
    expect(cart.lines[0]!.quantity).toBe(2)
    cart = await add(m.token, { ...YIRGA, grind: 'CB', index: 99 })
    expect(skus(cart)).toEqual([
      'RT-YIR-250G-WB-STD',
      'RT-PNK-250G-PO-STD',
      'RT-NGT-500G-ES-STD',
      'RT-GSH-100G-WB-GIFT',
      'RT-YIR-250G-CB-STD',
    ])
  })

  it('errors: not_found, sold_out, out_of_stock (new and merged), quantity_invalid, login_required', async () => {
    const m = await member([
      { ...GEISHA, quantity: 5 },
    ])
    await failure('POST', '/v1/cart/lines', m.token, { ...YIRGA, productId: 'nope' }, 'not_found')
    await failure('POST', '/v1/cart/lines', m.token, { ...GEISHA, weight: '500g' }, 'sold_out', {
      productId: 'geisha',
      productName: '波奎特 瑰夏',
      weight: '500g',
    })
    await failure('POST', '/v1/cart/lines', m.token, { ...PINKB, weight: '1kg' }, 'sold_out')
    await failure('POST', '/v1/cart/lines', m.token, { ...GEISHA, quantity: 2 }, 'out_of_stock', {
      productId: 'geisha',
      productName: '波奎特 瑰夏',
      available: 6,
    })
    await failure('POST', '/v1/cart/lines', m.token, { ...GEISHA, grind: 'PO', quantity: 7 }, 'out_of_stock')
    for (const quantity of [0, 10, 1.5, -1, '2'])
      await failure('POST', '/v1/cart/lines', m.token, { ...YIRGA, quantity }, 'quantity_invalid')
    await failure('POST', '/v1/cart/lines', undefined, YIRGA, 'login_required')
    await failure('POST', '/v1/cart/lines', m.token, { ...YIRGA, index: -1 }, 'validation_failed', { field: 'body.index' })
    // Nothing changed.
    expect((await getCart(m.token)).lines.map(l => [l.sku, l.quantity])).toEqual([
      ['RT-GSH-100G-WB-GIFT', 5],
    ])
  })

  it('cart_full at the 51st line, but an existing line still merges', async () => {
    const m = await member()
    await db.prisma.cartItem.createMany({
      data: distinctLines(50).map((line, position) => ({ userId: m.id, ...line, position })),
    })
    await failure('POST', '/v1/cart/lines', m.token, NIGHT, 'cart_full')
    const cart = await add(m.token, {
      ...distinctLines(1)[0]!,
      quantity: 2,
    })
    expect(cart.lines).toHaveLength(50)
    expect(cart.lines[0]!.quantity).toBe(3)
  })
})

describe('pATCH and DELETE /v1/cart/lines/{sku}', () => {
  it('changes quantity and selection; out_of_stock above stock; unselecting always works', async () => {
    const m = await member([GEISHA, YIRGA])
    let cart = await ok('PATCH', '/v1/cart/lines/RT-GSH-100G-WB-GIFT', m.token, { quantity: 6 })
    expect(cart.lines[0]).toMatchObject({ quantity: 6, lineTotal: 846 })
    await failure('PATCH', '/v1/cart/lines/RT-GSH-100G-WB-GIFT', m.token, { quantity: 7 }, 'out_of_stock', {
      productId: 'geisha',
      productName: '波奎特 瑰夏',
      available: 6,
    })
    await failure('PATCH', '/v1/cart/lines/RT-GSH-100G-WB-GIFT', m.token, { quantity: 10 }, 'quantity_invalid')
    await failure('PATCH', '/v1/cart/lines/RT-GSH-100G-WB-GIFT', m.token, {}, 'validation_failed')
    cart = await ok('PATCH', '/v1/cart/lines/RT-GSH-100G-WB-GIFT', m.token, { quantity: 2, selected: false })
    expect(cart.lines[0]).toMatchObject({ quantity: 2, selected: false })
    // Stock falls below the line: flagged, still unselectable and reducible.
    await db.prisma.product.update({
      where: { id: 'geisha' },
      data: { stock: 1 },
    })
    try {
      cart = await getCart(m.token)
      expect(cart.lines[0]).toMatchObject({ problem: 'out_of_stock', maxQuantity: 1 })
      cart = await ok('PATCH', '/v1/cart/lines/RT-GSH-100G-WB-GIFT', m.token, { selected: true })
      expect(cart.lines[0]!.selected).toBe(true)
      cart = await ok('PATCH', '/v1/cart/lines/RT-GSH-100G-WB-GIFT', m.token, { quantity: 1 })
      expect(cart.lines[0]!.problem).toBeNull()
    }
    finally {
      await db.prisma.product.update({
        where: { id: 'geisha' },
        data: { stock: 6 },
      })
    }
  })

  it('not_found for a line not in the cart or an unknown product code', async () => {
    const m = await member([YIRGA])
    await failure('PATCH', '/v1/cart/lines/RT-YIR-250G-PO-STD', m.token, { quantity: 1 }, 'not_found')
    await failure('PATCH', '/v1/cart/lines/RT-ZZZ-250G-WB-STD', m.token, { quantity: 1 }, 'not_found')
    await failure('DELETE', '/v1/cart/lines/RT-YIR-250G-PO-STD', m.token, undefined, 'not_found')
    await failure('DELETE', '/v1/cart/lines/rt-yir-250g-wb-std', m.token, undefined, 'validation_failed', { field: 'params.sku' })
    const cart = await ok('DELETE', '/v1/cart/lines/RT-YIR-250G-WB-STD', m.token)
    expect(cart).toEqual(readFixture('v1/cart/cart-empty.json'))
    await failure('DELETE', '/v1/cart/lines/RT-YIR-250G-WB-STD', m.token, undefined, 'not_found')
  })
})

describe('selection and totals', () => {
  it('only selected lines count toward money; counts cover all lines', async () => {
    const m = await member([PINKB, NIGHT, GEISHA])
    let cart = await ok('PATCH', '/v1/cart/lines/RT-NGT-500G-ES-STD', m.token, { selected: false })
    // 118 + 141 = 259 ≥ 199.
    expect(cart.totals).toEqual({
      itemCount: 3,
      selectedCount: 2,
      subtotal: 259,
      discount: 0,
      shipping: 0,
      total: 259,
      freeShipping: {
        threshold: 199,
        reached: true,
        progress: 100,
        gap: 0,
      },
    })
    cart = await ok('PATCH', '/v1/cart/lines/RT-GSH-100G-WB-GIFT', m.token, { selected: false })
    // 118 < 199: +12; progress round(118 / 199 × 100) = 59; gap 81.
    expect(cart.totals).toEqual({
      itemCount: 3,
      selectedCount: 1,
      subtotal: 118,
      discount: 0,
      shipping: 12,
      total: 130,
      freeShipping: {
        threshold: 199,
        reached: false,
        progress: 59,
        gap: 81,
      },
    })
    cart = await ok('PUT', '/v1/cart/selection', m.token, { selected: false })
    expect(cart.lines.every(l => !l.selected)).toBe(true)
    expect(cart.totals).toMatchObject({
      itemCount: 3,
      selectedCount: 0,
      subtotal: 0,
      shipping: 0,
      total: 0,
    })
    cart = await ok('PUT', '/v1/cart/selection', m.token, { selected: true })
    expect(cart.totals).toMatchObject({ selectedCount: 3, subtotal: 426, total: 426 })
    await failure('PUT', '/v1/cart/selection', m.token, {}, 'validation_failed', { field: 'body.selected' })
  })

  it('an empty cart selects nothing without error', async () => {
    const m = await member()
    expect(await ok('PUT', '/v1/cart/selection', m.token, { selected: true })).toEqual(readFixture('v1/cart/cart-empty.json'))
  })

  it('a line whose stock fell below its quantity: the cart-problem fixture', async () => {
    const m = await member([
      { ...GEISHA, grind: 'PO', quantity: 3 },
    ])
    await addAddressRow(db.prisma, m.id)
    await db.prisma.product.update({
      where: { id: 'geisha' },
      data: { stock: 2 },
    })
    try {
      const cart = await getCart(m.token)
      const expected = readFixture('v1/cart/cart-problem.json') as Cart
      expect({
        ...cart,
        address: { ...cart.address!, id: 1 },
      }).toEqual(expected)
    }
    finally {
      await db.prisma.product.update({
        where: { id: 'geisha' },
        data: { stock: 6 },
      })
    }
  })
})

describe('coupons and the free-shipping threshold', () => {
  it('¥198 pays shipping, ¥199 ships free (no coupon)', async () => {
    // night 500g 167 + yunnan 100g 31 = 198; mandh 100g 32 + night 500g 167 = 199.
    const below = await member([
      NIGHT,
      {
        productId: 'yunnan',
        weight: '100g',
        grind: 'WB',
        pack: 'STD',
        quantity: 1,
      },
    ])
    expect((await getCart(below.token)).totals).toEqual({
      itemCount: 2,
      selectedCount: 2,
      subtotal: 198,
      discount: 0,
      shipping: 12,
      total: 210,
      freeShipping: {
        threshold: 199,
        reached: false,
        progress: 99,
        gap: 1,
      },
    })
    const at = await member([
      {
        productId: 'mandh',
        weight: '100g',
        grind: 'WB',
        pack: 'STD',
        quantity: 1,
      },
      NIGHT,
    ])
    expect((await getCart(at.token)).totals).toEqual({
      itemCount: 2,
      selectedCount: 2,
      subtotal: 199,
      discount: 0,
      shipping: 0,
      total: 199,
      freeShipping: {
        threshold: 199,
        reached: true,
        progress: 100,
        gap: 0,
      },
    })
  })

  it('with TTY10 the threshold applies after the discount: ¥198 vs ¥199', async () => {
    // yirga 100g 44 × 5 = 220, discount 22 → 198: +12.
    const below = await member([
      { ...YIRGA, weight: '100g', quantity: 5 },
    ])
    let cart = await ok('PUT', '/v1/cart/coupon', below.token, { code: 'TTY10' })
    expect(cart.coupon).toEqual({ code: 'TTY10', description: '全单 9 折' })
    expect(cart.totals).toEqual({
      itemCount: 5,
      selectedCount: 5,
      subtotal: 220,
      discount: 22,
      shipping: 12,
      total: 210,
      freeShipping: {
        threshold: 199,
        reached: false,
        progress: 99,
        gap: 1,
      },
    })
    // antigua 100g 35 + yirga 500g 186 = 221, discount round(22.1) = 22 → 199: free.
    const at = await member([
      {
        productId: 'antigua',
        weight: '100g',
        grind: 'FP',
        pack: 'STD',
        quantity: 1,
      },
      { ...YIRGA, weight: '500g' },
    ])
    cart = await ok('PUT', '/v1/cart/coupon', at.token, { code: 'tty10' })
    expect(cart.totals).toEqual({
      itemCount: 2,
      selectedCount: 2,
      subtotal: 221,
      discount: 22,
      shipping: 0,
      total: 199,
      freeShipping: {
        threshold: 199,
        reached: true,
        progress: 100,
        gap: 0,
      },
    })
  })

  it('an invalid code leaves the cart and its coupon unchanged; empty or DELETE removes it', async () => {
    const m = await member([YIRGA])
    const withCoupon = await ok('PUT', '/v1/cart/coupon', m.token, { code: ' tty10 ' })
    expect(withCoupon.totals).toMatchObject({
      subtotal: 98,
      discount: 10,
      shipping: 12,
      total: 100,
    })
    await failure('PUT', '/v1/cart/coupon', m.token, { code: 'FREE' }, 'coupon_invalid')
    expect(await getCart(m.token)).toEqual(withCoupon)
    await db.prisma.coupon.create({
      data: {
        code: 'OLD5',
        description: '旧券',
        percentOff: 5,
        active: false,
      },
    })
    await failure('PUT', '/v1/cart/coupon', m.token, { code: 'old5' }, 'coupon_invalid')
    await failure('PUT', '/v1/cart/coupon', m.token, {
      code: 'x'.repeat(33),
    }, 'validation_failed')
    let cart = await ok('PUT', '/v1/cart/coupon', m.token, { code: '' })
    expect(cart.coupon).toBeNull()
    expect(cart.totals.discount).toBe(0)
    await ok('PUT', '/v1/cart/coupon', m.token, { code: 'TTY10' })
    cart = await ok('DELETE', '/v1/cart/coupon', m.token)
    expect(cart.coupon).toBeNull()
    cart = await ok('DELETE', '/v1/cart/coupon', m.token)
    expect(cart.coupon).toBeNull()
  })
})

describe('pUT /v1/cart/address', () => {
  it('falls back to the default, keeps the chosen one, and falls back again when it is deleted', async () => {
    const m = await member([YIRGA])
    expect((await getCart(m.token)).address).toBeNull()
    const home = await addAddressRow(db.prisma, m.id)
    const office = await addAddressRow(db.prisma, m.id, { region: '上海市 静安区', detail: '江宁路 300 号 12F（前台代收）', tag: 'work' })
    expect((await getCart(m.token)).address?.id).toBe(home)
    let cart = await ok('PUT', '/v1/cart/address', m.token, { addressId: office })
    expect(cart.address).toMatchObject({
      id: office,
      tag: 'work',
      isDefault: false,
      phoneMasked: '138 **** 5521',
    })
    expect((await getCart(m.token)).address?.id).toBe(office)
    await db.prisma.address.delete({
      where: { id: office },
    })
    expect((await getCart(m.token)).address?.id).toBe(home)
    cart = await ok('PUT', '/v1/cart/address', m.token, { addressId: home })
    expect(cart.address?.id).toBe(home)
  })

  it('not_found for an unknown or someone else\'s address', async () => {
    const m = await member()
    const other = await member()
    const theirs = await addAddressRow(db.prisma, other.id)
    await failure('PUT', '/v1/cart/address', m.token, { addressId: theirs }, 'not_found')
    await failure('PUT', '/v1/cart/address', m.token, { addressId: 999_999 }, 'not_found')
    await failure('PUT', '/v1/cart/address', m.token, { addressId: 0 }, 'validation_failed', { field: 'body.addressId' })
    expect((await getCart(m.token)).address).toBeNull()
    expect((await getCart(other.token)).address?.id).toBe(theirs)
  })
})

describe('pOST /v1/cart/quote', () => {
  it('prices exactly like the member cart (lines, coupon, totals)', async () => {
    const lines = [
      PINKB,
      { ...NIGHT, selected: false },
      { ...GEISHA, quantity: 2 },
      {
        ...YIRGA,
        weight: '100g',
        pack: 'GIFT',
        quantity: 3,
      },
    ] as TestLine[]
    const m = await member(lines)
    await ok('PUT', '/v1/cart/coupon', m.token, { code: 'TTY10' })
    const memberCart = await getCart(m.token)
    const quote = await ok('POST', '/v1/cart/quote', undefined, { lines, couponCode: 'TTY10' })
    expect(quote).toEqual({ ...memberCart, address: null })
    // Without a coupon, and with an empty one.
    const plain = await ok('POST', '/v1/cart/quote', undefined, { lines })
    expect(plain.coupon).toBeNull()
    expect(await ok('POST', '/v1/cart/quote', undefined, { lines, couponCode: '  ' })).toEqual(plain)
    expect(await ok('POST', '/v1/cart/quote', undefined, { lines, couponCode: null })).toEqual(plain)
  })

  it('folds the same SKU (capped at 9, selected if any), flags problems, needs no session', async () => {
    const quote = await ok('POST', '/v1/cart/quote', undefined, {
      lines: [
        { ...YIRGA, quantity: 6, selected: false },
        PINKB,
        { ...YIRGA, quantity: 6 },
        { ...GEISHA, weight: '1kg' },
      ],
    })
    expect(quote.lines.map(l => [
      l.sku,
      l.quantity,
      l.selected,
      l.problem,
    ])).toEqual([
      ['RT-YIR-250G-WB-STD', 9, true, null],
      ['RT-PNK-250G-PO-STD', 1, true, null],
      [
        'RT-GSH-1KG-WB-GIFT',
        1,
        true,
        'sold_out',
      ],
    ])
    expect(await ok('POST', '/v1/cart/quote', undefined, { lines: [] })).toEqual(readFixture('v1/cart/cart-empty.json'))
    // A member token is ignored (auth: none): still no address.
    const m = await member([YIRGA])
    await addAddressRow(db.prisma, m.id)
    expect((await ok('POST', '/v1/cart/quote', m.token, {
      lines: [YIRGA],
    })).address).toBeNull()
  })

  it('errors: not_found, quantity_invalid, cart_full, coupon_invalid; nothing is stored', async () => {
    const before = await db.prisma.cartItem.count()
    await failure('POST', '/v1/cart/quote', undefined, {
      lines: [
        YIRGA,
        { ...YIRGA, productId: 'nope' },
      ],
    }, 'not_found')
    await failure('POST', '/v1/cart/quote', undefined, {
      lines: [
        { ...YIRGA, quantity: 10 },
      ],
    }, 'quantity_invalid')
    await failure('POST', '/v1/cart/quote', undefined, {
      lines: distinctLines(51),
    }, 'cart_full')
    await failure('POST', '/v1/cart/quote', undefined, {
      lines: [YIRGA],
      couponCode: 'NOPE',
    }, 'coupon_invalid')
    await failure('POST', '/v1/cart/quote', undefined, {
      lines: [
        { ...YIRGA, index: 0 },
      ],
    }, 'validation_failed')
    expect((await ok('POST', '/v1/cart/quote', undefined, {
      lines: distinctLines(50),
    })).lines).toHaveLength(50)
    expect(await db.prisma.cartItem.count()).toBe(before)
  })
})

describe('pOST /v1/cart/merge', () => {
  async function merge(token: string, body: unknown): Promise<CartMergeResult> {
    const response = await call(server.origin, 'POST', '/v1/cart/merge', { token, body })
    expect(response.status, JSON.stringify(response.body)).toBe(200)
    return CartMergeResultSchema.parse(response.body)
  }

  it('同一请求标识重试时返回首次合并数量和当前购物车', async () => {
    const m = await member([YIRGA])
    const body = {
      idempotencyKey: 'merge-retry-0001',
      lines: [
        { ...YIRGA, quantity: 2 },
        { ...PINKB, productId: 'missing' },
      ],
      couponCode: 'tty10',
    }
    const first = await merge(m.token, body)
    expect(first).toMatchObject({ mergedCount: 2, skippedCount: 1 })
    await ok('PATCH', '/v1/cart/lines/RT-YIR-250G-WB-STD', m.token, { quantity: 1, selected: false })
    await ok('DELETE', '/v1/cart/coupon', m.token)
    const replay = await merge(m.token, { ...body, couponCode: ' TTY10 ' })
    expect(replay).toEqual({ cart: await getCart(m.token), mergedCount: 2, skippedCount: 1 })
    expect(replay.cart.lines[0]).toMatchObject({ quantity: 1, selected: false })
    expect(replay.cart.coupon).toBeNull()
    expect(await db.prisma.cartMerge.count({
      where: { userId: m.id },
    })).toBe(1)
  })

  it('合并成功后连接中断时重试不会重复累加商品', async () => {
    const m = await member([YIRGA])
    const body = {
      idempotencyKey: 'merge-response-lost-0001',
      lines: [
        { ...YIRGA, quantity: 2 },
      ],
    }
    const app = createApp({
      prisma: db.prisma,
      now: () => NOW,
      validateResponses: true,
      logError: () => {},
    })
    app.middleware.unshift(async (ctx, next) => {
      await next()
      // 业务处理已完成，关闭连接使客户端收不到成功响应。
      ctx.res.destroy()
    })
    const droppingServer = await listen(app)
    try {
      await expect(call(droppingServer.origin, 'POST', '/v1/cart/merge', { token: m.token, body })).rejects.toThrow()
      expect((await getCart(m.token)).lines[0]!.quantity).toBe(3)
      expect((await merge(m.token, body)).mergedCount).toBe(2)
      expect((await getCart(m.token)).lines[0]!.quantity).toBe(3)
    }
    finally {
      await droppingServer.close()
    }
  })

  it('同一标识携带不同的访客商品时拒绝合并', async () => {
    const m = await member()
    const body = {
      idempotencyKey: 'merge-content-0001',
      lines: [YIRGA],
    }
    await merge(m.token, body)
    await failure('POST', '/v1/cart/merge', m.token, {
      ...body,
      lines: [
        { ...YIRGA, quantity: 2 },
      ],
    }, 'validation_failed', { field: 'body.idempotencyKey' })
    await failure('POST', '/v1/cart/merge', m.token, {
      ...body,
      couponCode: 'TTY10',
    }, 'validation_failed', { field: 'body.idempotencyKey' })
    expect((await getCart(m.token)).lines[0]!.quantity).toBe(1)
  })

  it('不同标识代表两次合并', async () => {
    const m = await member()
    for (const idempotencyKey of ['merge-first-0001', 'merge-second-0001']) {
      expect((await merge(m.token, {
        idempotencyKey,
        lines: [YIRGA],
      })).mergedCount).toBe(1)
    }
    expect((await getCart(m.token)).lines[0]!.quantity).toBe(2)
  })

  it('省略标识时每次请求都会合并', async () => {
    const m = await member()
    await merge(m.token, {
      lines: [YIRGA],
    })
    await merge(m.token, {
      lines: [YIRGA],
    })
    expect((await getCart(m.token)).lines[0]!.quantity).toBe(2)
  })

  it('相同标识在不同账户中分别合并', async () => {
    for (const m of [await member(), await member()]) {
      const body = {
        idempotencyKey: 'merge-accounts-0001',
        lines: [YIRGA],
      }
      await merge(m.token, body)
      await merge(m.token, body)
      expect((await getCart(m.token)).lines[0]!.quantity).toBe(1)
    }
  })

  it('通过不同数据库连接并发重试时只合并一次', async () => {
    const m = await member([YIRGA])
    const otherPrisma = createPrisma(db.url)
    const otherServer = await listenApp(otherPrisma, {
      now: () => NOW,
    })
    try {
      const body = {
        idempotencyKey: 'merge-concurrent-0001',
        lines: [
          { ...YIRGA, quantity: 2 },
        ],
      }
      const replies = await Promise.all(Array.from({ length: 4 }, (_, i) => call(
        i % 2 === 0
          ? server.origin
          : otherServer.origin,
        'POST',
        '/v1/cart/merge',
        { token: m.token, body },
      )))
      for (const reply of replies) {
        expect(reply.status, JSON.stringify(reply.body)).toBe(200)
        expect(CartMergeResultSchema.parse(reply.body)).toMatchObject({ mergedCount: 2, skippedCount: 0 })
      }
      expect((await getCart(m.token)).lines[0]!.quantity).toBe(3)
      expect(await otherPrisma.cartMerge.count({
        where: { userId: m.id },
      })).toBe(1)
    }
    finally {
      await otherServer.close()
      await otherPrisma.$disconnect()
    }
  })

  it('重新连接数据库后重试时读取已保存的合并结果', async () => {
    const m = await member()
    const body = {
      idempotencyKey: 'merge-reopened-0001',
      lines: [
        { ...YIRGA, quantity: 2 },
      ],
    }
    const first = await merge(m.token, body)
    const reopened = createPrisma(db.url)
    const reopenedServer = await listenApp(reopened, {
      now: () => NOW,
    })
    try {
      const reply = await call(reopenedServer.origin, 'POST', '/v1/cart/merge', { token: m.token, body })
      expect(reply.status).toBe(200)
      expect(reply.body).toEqual(first)
    }
    finally {
      await reopenedServer.close()
      await reopened.$disconnect()
    }
  })

  it('合并记录写入失败时商品修改也会回滚', async () => {
    const m = await member([YIRGA])
    const body = {
      idempotencyKey: 'merge-rollback-0001',
      lines: [
        { ...YIRGA, quantity: 2 },
      ],
    }
    await db.prisma.$executeRawUnsafe(`CREATE TRIGGER fail_merge_record BEFORE UPDATE ON "CartMerge"
      BEGIN SELECT RAISE(ABORT, 'forced merge failure'); END`)
    try {
      const reply = await call(server.origin, 'POST', '/v1/cart/merge', { token: m.token, body })
      expect(reply.status).toBe(500)
      expect((await getCart(m.token)).lines[0]!.quantity).toBe(1)
      expect(await db.prisma.cartMerge.count({
        where: { userId: m.id },
      })).toBe(0)
    }
    finally {
      await db.prisma.$executeRawUnsafe('DROP TRIGGER fail_merge_record')
    }
    expect((await merge(m.token, body)).mergedCount).toBe(2)
    expect((await getCart(m.token)).lines[0]!.quantity).toBe(3)
  })

  it('请求标识不符合格式时拒绝合并', async () => {
    const m = await member()
    for (const idempotencyKey of [
      '',
      'short',
      'x'.repeat(65),
      'merge 中文',
      null,
    ]) {
      await failure('POST', '/v1/cart/merge', m.token, {
        idempotencyKey,
        lines: [YIRGA],
      }, 'validation_failed', { field: 'body.idempotencyKey' })
    }
    expect((await getCart(m.token)).lines).toEqual([])
  })

  it('an empty guest cart changes nothing', async () => {
    const m = await member([YIRGA])
    const before = await getCart(m.token)
    expect(await merge(m.token, { lines: [] })).toEqual({ cart: before, mergedCount: 0, skippedCount: 0 })
  })

  it('a new member gets the guest cart as quoted (order and selection kept)', async () => {
    const lines = [
      { ...NIGHT, selected: false },
      GEISHA,
      { ...PINKB, quantity: 2 },
    ] as TestLine[]
    const quote = await ok('POST', '/v1/cart/quote', undefined, { lines, couponCode: 'tty10' })
    const m = await member()
    const result = await merge(m.token, { lines, couponCode: 'tty10' })
    expect(result.mergedCount).toBe(4)
    expect(result.skippedCount).toBe(0)
    expect(result.cart).toEqual(quote)
    expect(await getCart(m.token)).toEqual(result.cart)
  })

  it('clamps to min(9, stock), never lowers a line, and merges selection', async () => {
    const m = await member([
      { ...GEISHA, quantity: 2, selected: false },
      { ...YIRGA, quantity: 8 },
    ])
    await db.prisma.product.update({
      where: { id: 'geisha' },
      data: { stock: 1 },
    })
    try {
      const result = await merge(m.token, {
        lines: [
          { ...GEISHA, quantity: 3 },
          { ...YIRGA, quantity: 5, selected: false },
          {
            ...GEISHA,
            grind: 'PO',
            quantity: 4,
            selected: false,
          },
        ],
      })
      // geisha WB stays 2 (above stock 1, not lowered) but is selected now; yirga 8 → 9 (+1);
      // geisha PO new, clamped to 1 (+1), keeps selected = false.
      expect(result.cart.lines.map(l => [
        l.sku,
        l.quantity,
        l.selected,
        l.problem,
      ])).toEqual([
        [
          'RT-GSH-100G-WB-GIFT',
          2,
          true,
          'out_of_stock',
        ],
        ['RT-YIR-250G-WB-STD', 9, true, null],
        ['RT-GSH-100G-PO-GIFT', 1, false, null],
      ])
      expect(result.mergedCount).toBe(2)
      expect(result.skippedCount).toBe(0)
    }
    finally {
      await db.prisma.product.update({
        where: { id: 'geisha' },
        data: { stock: 6 },
      })
    }
  })

  it('skips unknown products, sold-out weights, products without stock and lines beyond 50', async () => {
    const m = await member()
    await db.prisma.cartItem.createMany({
      data: distinctLines(48).map((line, position) => ({ userId: m.id, ...line, position })),
    })
    await db.prisma.product.update({
      where: { id: 'antigua' },
      data: { stock: 0 },
    })
    try {
      const result = await merge(m.token, {
        lines: [
          { ...YIRGA, productId: 'nope' },
          { ...PINKB, weight: '1kg' },
          {
            productId: 'antigua',
            weight: '250g',
            grind: 'FP',
            pack: 'STD',
            quantity: 1,
          },
          PINKB,
          NIGHT,
          GEISHA,
          {
            ...distinctLines(1)[0]!,
            quantity: 4,
          },
          { ...PINKB, quantity: 2 },
        ],
      })
      expect(result.cart.lines).toHaveLength(50)
      expect(result.cart.lines.slice(48).map(l => [l.sku, l.quantity])).toEqual([
        ['RT-PNK-250G-PO-STD', 3],
        ['RT-NGT-500G-ES-STD', 1],
      ])
      expect(result.cart.lines[0]!.quantity).toBe(5)
      // pinkb 1 + night 1 + yirga +4 + pinkb +2.
      expect(result.mergedCount).toBe(8)
      // nope, pinkb 1kg, antigua (no stock), geisha (51st line).
      expect(result.skippedCount).toBe(4)
    }
    finally {
      await db.prisma.product.update({
        where: { id: 'antigua' },
        data: { stock: 30 },
      })
    }
  })

  it('a valid coupon replaces the member\'s; an invalid one is ignored', async () => {
    const m = await member([YIRGA])
    let result = await merge(m.token, { lines: [], couponCode: 'NOPE' })
    expect(result.cart.coupon).toBeNull()
    result = await merge(m.token, { lines: [], couponCode: ' tty10 ' })
    expect(result.cart.coupon?.code).toBe('TTY10')
    result = await merge(m.token, { lines: [], couponCode: 'NOPE' })
    expect(result.cart.coupon?.code).toBe('TTY10')
    result = await merge(m.token, { lines: [] })
    expect(result.cart.coupon?.code).toBe('TTY10')
  })

  it('needs a session; refuses more than 50 guest lines', async () => {
    await failure('POST', '/v1/cart/merge', undefined, { lines: [] }, 'login_required')
    const m = await member()
    await failure('POST', '/v1/cart/merge', m.token, {
      lines: distinctLines(51),
    }, 'cart_full')
  })
})

describe('isolation between members', () => {
  it('the same SKU in two carts; one member never sees or changes the other\'s', async () => {
    const a = await member([YIRGA])
    const b = await member([
      { ...YIRGA, quantity: 3 },
    ])
    await add(a.token, { ...YIRGA, quantity: 2 })
    await ok('PUT', '/v1/cart/coupon', a.token, { code: 'TTY10' })
    await ok('PUT', '/v1/cart/selection', a.token, { selected: false })
    await add(a.token, PINKB)
    const bCart = await getCart(b.token)
    expect(bCart.lines.map(l => [l.sku, l.quantity, l.selected])).toEqual([
      ['RT-YIR-250G-WB-STD', 3, true],
    ])
    expect(bCart.coupon).toBeNull()
    await failure('DELETE', '/v1/cart/lines/RT-PNK-250G-PO-STD', b.token, undefined, 'not_found')
    await ok('DELETE', '/v1/cart/lines/RT-YIR-250G-WB-STD', b.token)
    expect((await getCart(a.token)).lines.map(l => [l.sku, l.quantity])).toEqual([
      ['RT-YIR-250G-WB-STD', 3],
      ['RT-PNK-250G-PO-STD', 1],
    ])
    expect((await getCart(b.token)).lines).toEqual([])
    const merged = await merge2(b.token)
    expect(merged.lines.map(l => l.sku)).toEqual(['RT-NGT-500G-ES-STD'])
    expect((await getCart(a.token)).lines).toHaveLength(2)
  })

  async function merge2(token: string): Promise<Cart> {
    const response = await call(server.origin, 'POST', '/v1/cart/merge', {
      token,
      body: {
        lines: [NIGHT],
      },
    })
    return CartMergeResultSchema.parse(response.body).cart
  }
})

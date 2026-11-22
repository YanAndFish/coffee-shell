// The profile (stats, taste profile, recommendations) and per-account settings
// (P-profile-1…6): the pure rules, then the endpoints through the real app.
import type { ApiError, Profile, Settings } from '@coffee/contract'
import type { PurchasedLine } from '../src/services/profile-taste.ts'
import type { TestDatabase, TestServer } from './helpers.ts'
import type { TestMember } from './support/members.ts'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { averageFlavor, recommend, tasteOf } from '../src/services/profile-taste.ts'
import { FINGERPRINT, NOW } from './conformance/types.ts'
import { call, createTestDatabase, listenApp, readFixture } from './helpers.ts'
import { addAddressRow, addOrderRow, createMember, rememberKey } from './support/members.ts'

const DAY = 24 * 60 * 60 * 1000

function line(flavor: number[], options: Partial<PurchasedLine> = {}): PurchasedLine {
  return {
    productId: 'x',
    flavor,
    roast: 1,
    grind: 'PO',
    weight: '250g',
    quantity: 1,
    ...options,
  }
}

describe('taste rules (pure)', () => {
  it('no purchases: no taste', () => {
    expect(tasteOf([])).toBeNull()
    expect(averageFlavor([])).toBeNull()
  })

  it('one line: its own flavor and phrases', () => {
    // 曼特宁: [2, 5, 9, 7, 8, 5], deep roast, whole beans.
    expect(tasteOf([
      line([2, 5, 9, 7, 8, 5], { roast: 5, grind: 'WB' }),
    ])).toEqual({
      flavor: [2, 5, 9, 7, 8, 5],
      prefer: '厚实醇度 · 悠长余韵 · 可可苦甜',
      often: '中深烘 / 深烘 · 整豆 · 250g',
      avoid: '浅烘 · 明亮果酸',
    })
  })

  it('quantity-weighted, each axis rounded half up', () => {
    const flavor = averageFlavor([
      line([7, 0, 1, 0, 0, 10], { quantity: 1 }),
      line([8, 1, 2, 0, 0, 0], { quantity: 1 }),
      line([0, 0, 0, 3, 0, 0], { quantity: 0 }),
    ])
    expect(flavor).toEqual([8, 1, 2, 0, 0, 5])
    expect(averageFlavor([
      line([10, 0, 0, 0, 0, 0], { quantity: 3 }),
      line([0, 0, 0, 0, 0, 0], { quantity: 1 }),
    ]))
      .toEqual([8, 0, 0, 0, 0, 0])
  })

  it('ties go to the axis order and the display order', () => {
    expect(tasteOf([
      line([5, 5, 5, 5, 5, 5], { roast: 5, grind: 'PO', weight: '1kg' }),
      line([5, 5, 5, 5, 5, 5], { roast: 1, grind: 'WB', weight: '100g' }),
    ])).toEqual({
      flavor: [5, 5, 5, 5, 5, 5],
      prefer: '明亮果酸 · 干净甜感 · 厚实醇度',
      often: '浅烘 / 中浅烘 · 整豆 · 100g',
      avoid: '浅烘 · 明亮果酸',
    })
  })

  it('the design example [8,7,4,2,7,8] reads 明亮果酸 · 花香 · 干净甜感', () => {
    const taste = tasteOf([
      line([8, 7, 4, 2, 7, 8], { roast: 3, grind: 'ES', weight: '500g' }),
    ])
    expect(taste).toMatchObject({ prefer: '明亮果酸 · 花香 · 干净甜感', often: '中烘 · 意式研磨 · 500g', avoid: '深烘 · 烟熏与草本调' })
  })

  it('every avoid phrase', () => {
    const phrases = [0, 1, 2, 3, 4, 5].map((low) => {
      const flavor = [9, 9, 9, 9, 9, 9]
      flavor[low] = 1
      return tasteOf([
        line(flavor),
      ])?.avoid
    })
    expect(phrases).toEqual([
      '浅烘 · 明亮果酸',
      '日晒 · 浓郁果甜',
      '深烘 · 厚重醇度',
      '深烘 · 烟熏与草本调',
      '湿刨 · 悠长余韵',
      '浅烘 · 花香调',
    ])
  })

  it('recommends the nearest unbought products in stock, then bought ones', () => {
    const products = [
      {
        id: 'a',
        name: 'A',
        flavor: [0, 0, 0, 0, 0, 0],
        stock: 5,
      },
      {
        id: 'b',
        name: 'B',
        flavor: [5, 5, 5, 5, 5, 5],
        stock: 5,
      },
      {
        id: 'c',
        name: 'C',
        flavor: [5, 5, 5, 5, 5, 5],
        stock: 5,
      },
      {
        id: 'd',
        name: 'D',
        flavor: [5, 5, 5, 5, 5, 5],
        stock: 0,
      },
      {
        id: 'e',
        name: 'E',
        flavor: [9, 9, 9, 9, 9, 9],
        stock: 5,
      },
    ]
    const target = [5, 5, 5, 5, 5, 5]
    // Equal distance: display order; out of stock skipped.
    expect(recommend(products, new Set(), target)).toEqual([
      { id: 'b', name: 'B' },
      { id: 'c', name: 'C' },
    ])
    expect(recommend(products, new Set(['b']), target).map(p => p.id)).toEqual(['c', 'e'])
    // Only one unbought product in stock: filled with the nearest bought one.
    expect(recommend(products, new Set(['a', 'b', 'c']), target).map(p => p.id)).toEqual(['e', 'b'])
  })
})

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

function member(): Promise<TestMember> {
  return createMember(db.prisma, { now: NOW })
}

async function profileOf(m: TestMember, fingerprint?: string): Promise<Profile> {
  const res = await call(server.origin, 'GET', '/v1/profile', {
    token: m.token,
    headers: fingerprint
      ? { 'X-SSH-Fingerprint': fingerprint }
      : {},
  })
  expect(res.status, JSON.stringify(res.body)).toBe(200)
  return res.body as Profile
}

describe('gET /v1/profile', () => {
  it('zero orders: stats, no taste, no recommendations', async () => {
    const m = await member()
    await addAddressRow(db.prisma, m.id)
    const profile = await profileOf(m)
    expect(profile.user).toMatchObject({
      id: m.id,
      username: m.username,
      tier: { level: 3, points: 1280, nextLevelPoints: 2000 },
    })
    expect(profile.stats).toEqual({ orders: 0, addresses: 1, coupons: 1 })
    expect(profile.taste).toBeNull()
    expect(profile.recommendations).toEqual([])
    expect(profile.remembered.state).toBe('no_key')
  })

  it('one order', async () => {
    const m = await member()
    await addOrderRow(db.prisma, m.id, [
      {
        productId: 'mandh',
        weight: '250g',
        grind: 'WB',
        pack: 'STD',
        quantity: 2,
      },
    ], NOW)
    const profile = await profileOf(m)
    expect(profile.stats).toEqual({ orders: 1, addresses: 0, coupons: 1 })
    expect(profile.taste).toEqual({
      flavor: [2, 5, 9, 7, 8, 5],
      prefer: '厚实醇度 · 悠长余韵 · 可可苦甜',
      often: '中深烘 / 深烘 · 整豆 · 250g',
      avoid: '浅烘 · 明亮果酸',
    })
    // Distances² to [2,5,9,7,8,5]: antigua 18, night 28, yunnan 36, …
    expect(profile.recommendations).toEqual([
      { id: 'antigua', name: '安提瓜 火山' },
      { id: 'night', name: '「夜班」意式拼配' },
    ])
  })

  it('several orders: the fixture profile', async () => {
    const m = await member()
    const lines = [
      [
        {
          productId: 'geisha',
          weight: '100g',
          grind: 'PO',
          pack: 'STD',
          quantity: 1,
        },
      ],
      [
        {
          productId: 'yirga',
          weight: '250g',
          grind: 'PO',
          pack: 'STD',
          quantity: 2,
        },
      ],
      [
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
      ],
      [
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
      ],
    ] as const
    for (const [i, order] of lines.entries())
      await addOrderRow(db.prisma, m.id, order, new Date(NOW.getTime() - (lines.length - i) * DAY))
    for (let i = 0; i < 3; i++)
      await addAddressRow(db.prisma, m.id)
    await rememberKey(db.prisma, m.id, FINGERPRINT, NOW)
    const profile = await profileOf(m, FINGERPRINT)
    const expected = readFixture('v1/profile/profile.json') as Profile
    expect(profile.stats).toEqual(expected.stats)
    expect(profile.taste).toEqual(expected.taste)
    expect(profile.recommendations).toEqual(expected.recommendations)
    expect(profile.remembered).toEqual(expected.remembered)
  })

  it('fills recommendations with bought products when too few others are in stock', async () => {
    const m = await member()
    await addOrderRow(db.prisma, m.id, [
      {
        productId: 'mandh',
        weight: '250g',
        grind: 'WB',
        pack: 'STD',
        quantity: 1,
      },
    ], NOW)
    const stocks = await db.prisma.product.findMany({
      select: { id: true, stock: true },
    })
    await db.prisma.product.updateMany({
      where: {
        id: {
          notIn: ['mandh', 'yunnan'],
        },
      },
      data: { stock: 0 },
    })
    try {
      expect((await profileOf(m)).recommendations.map(p => p.id)).toEqual(['yunnan', 'mandh'])
    }
    finally {
      for (const { id, stock } of stocks) {
        await db.prisma.product.update({
          where: { id },
          data: { stock },
        })
      }
    }
  })

  it('counts only active coupons', async () => {
    const m = await member()
    await db.prisma.coupon.update({
      where: { code: 'TTY10' },
      data: { active: false },
    })
    try {
      expect((await profileOf(m)).stats.coupons).toBe(0)
    }
    finally {
      await db.prisma.coupon.update({
        where: { code: 'TTY10' },
        data: { active: true },
      })
    }
  })

  it('isolation: another member\'s orders and addresses do not count', async () => {
    const a = await member()
    const b = await member()
    await addOrderRow(db.prisma, a.id, [
      {
        productId: 'yirga',
        weight: '250g',
        grind: 'PO',
        pack: 'STD',
        quantity: 1,
      },
    ], NOW)
    await addAddressRow(db.prisma, a.id)
    const profile = await profileOf(b)
    expect(profile.stats).toEqual({ orders: 0, addresses: 0, coupons: 1 })
    expect(profile.taste).toBeNull()
    expect(profile.user.id).toBe(b.id)
  })
})

describe('settings', () => {
  async function settings(m: TestMember, body?: unknown): Promise<{ status: number, body: unknown }> {
    return call(server.origin, body === undefined
      ? 'GET'
      : 'PATCH', '/v1/settings', {
      token: m.token,
      ...(body === undefined
        ? {}
        : { body }),
    })
  }

  it('round-trips and persists per account', async () => {
    const m = await member()
    const other = await member()
    expect((await settings(m)).body).toEqual({ animations: true, mouseTracking: true })
    expect((await settings(m, { animations: false })).body).toEqual({ animations: false, mouseTracking: true })
    expect((await settings(m, { mouseTracking: false })).body).toEqual({ animations: false, mouseTracking: false })
    expect((await settings(m, { animations: true, mouseTracking: false })).body).toEqual({ animations: true, mouseTracking: false })
    expect((await settings(m)).body as Settings).toEqual({ animations: true, mouseTracking: false })
    const row = await db.prisma.setting.findUniqueOrThrow({
      where: { userId: m.id },
    })
    expect(row).toMatchObject({ animations: true, mouseTracking: false })
    // Another account is untouched.
    expect((await settings(other)).body).toEqual({ animations: true, mouseTracking: true })
  })

  it('a member without a settings row gets the defaults', async () => {
    const m = await member()
    await db.prisma.setting.delete({
      where: { userId: m.id },
    })
    expect((await settings(m)).body).toEqual({ animations: true, mouseTracking: true })
  })

  it('rejects an empty, unknown or mistyped patch', async () => {
    const m = await member()
    for (const body of [
      {},
      { theme: 'dark' },
      { animations: 'no' },
    ]) {
      const res = await settings(m, body)
      expect(res.status).toBe(400)
      expect((res.body as ApiError).error.code).toBe('validation_failed')
    }
    expect((await settings(m)).body).toEqual({ animations: true, mouseTracking: true })
  })

  it('needs a session', async () => {
    const res = await call(server.origin, 'GET', '/v1/settings')
    expect((res.body as ApiError).error.code).toBe('login_required')
  })
})

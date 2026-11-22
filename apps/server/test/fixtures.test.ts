// The contract fixtures the TUI's fixture mode serves are what this server answers: every
// product detail for the seeded catalog (and nyeri out of stock), 林晓 — the one member every
// member fixture names — signing up, then (three weeks and four orders later) reading her
// session and profile with and without a key, and a new member's cart without an address.
// The money rules of the cart and order fixtures are checked in `pricing.test.ts`.
import type { AuthSession, Cart, Me, ProductDetail, Profile } from '@coffee/contract'
import type { TestDatabase, TestServer } from './helpers.ts'
import type { TestLine } from './support/members.ts'
import { readdirSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { SSH_FINGERPRINT_HEADER } from '@coffee/contract'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { CATALOG } from '../prisma/catalog.ts'
import { FINGERPRINT, NOW } from './conformance/types.ts'
import { call, createTestDatabase, listenApp, readFixture } from './helpers.ts'
import { addAddressRow, addCartRow, addOrderRow, createMember, createSession, rememberKey } from './support/members.ts'

const DAY = 24 * 60 * 60 * 1000

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

/** The committed `v1/catalog/product-*.json` file names. */
function detailFixtures(): string[] {
  const dir = join(dirname(createRequire(import.meta.url).resolve('@coffee/contract/fixtures/v1/health.json')), 'catalog')
  return readdirSync(dir).filter(name => name.startsWith('product-') && name.endsWith('.json')).sort()
}

describe('product detail fixtures', () => {
  it('there is one per seeded product', () => {
    expect(detailFixtures()).toEqual(CATALOG.map(p => `product-${p.id}.json`).sort())
  })

  it.each(CATALOG.map(p => [p.id] as const))('%s equals GET /v1/products/{id}', async (id) => {
    const response = await call(server.origin, 'GET', `/v1/products/${id}`)
    expect(response.status, JSON.stringify(response.body)).toBe(200)
    expect(response.body as ProductDetail).toEqual(readFixture(`v1/catalog/product-${id}.json`))
  })

  it('catalog/detail-no-stock.json is nyeri once its stock ran out', async () => {
    await db.prisma.product.update({
      where: { id: 'nyeri' },
      data: { stock: 0 },
    })
    try {
      const response = await call(server.origin, 'GET', '/v1/products/nyeri')
      expect(response.body as ProductDetail).toEqual(readFixture('v1/catalog/detail-no-stock.json'))
    }
    finally {
      await db.prisma.product.update({
        where: { id: 'nyeri' },
        data: { stock: 18 },
      })
    }
  })
})

describe('林晓, the member of the fixtures', () => {
  /** Her account was created at the register fixture's instant. */
  const CREATED = new Date('2026-11-01T02:00:00.000Z')
  const ORDERS: [number, TestLine[]][] = [
    [
      32,
      [
        {
          productId: 'geisha',
          weight: '100g',
          grind: 'PO',
          pack: 'STD',
          quantity: 1,
        },
      ],
    ],
    [
      16,
      [
        {
          productId: 'yirga',
          weight: '250g',
          grind: 'PO',
          pack: 'STD',
          quantity: 2,
        },
      ],
    ],
    [
      2,
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
    ],
    [
      0,
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
    ],
  ]
  let token = ''
  let userId = 0

  it('signing up is auth/register.json (the first user of a fresh database)', async () => {
    const signup = await listenApp(db.prisma, {
      now: () => CREATED,
    })
    try {
      const response = await call(signup.origin, 'POST', '/v1/auth/register', {
        body: { username: '林晓', password: 'espresso42', remember: false },
        headers: { [SSH_FINGERPRINT_HEADER]: FINGERPRINT },
      })
      expect(response.status, JSON.stringify(response.body)).toBe(201)
      const session = response.body as AuthSession
      const expected = readFixture('v1/auth/register.json') as AuthSession
      expect({ ...session, token: expected.token }).toEqual(expected)
      userId = session.user.id
    }
    finally {
      await signup.close()
    }
    for (const [daysAgo, lines] of ORDERS)
      await addOrderRow(db.prisma, userId, lines, new Date(NOW.getTime() - daysAgo * DAY))
    for (let i = 0; i < 3; i++)
      await addAddressRow(db.prisma, userId)
    token = await createSession(db.prisma, userId, NOW)
  })

  it('with her key remembered: auth/me.json and profile/profile.json', async () => {
    await rememberKey(db.prisma, userId, FINGERPRINT, NOW)
    const headers = { [SSH_FINGERPRINT_HEADER]: FINGERPRINT }
    const me = await call(server.origin, 'GET', '/v1/auth/me', { token, headers })
    expect(me.status, JSON.stringify(me.body)).toBe(200)
    expect(me.body as Me).toEqual(readFixture('v1/auth/me.json'))
    const profile = await call(server.origin, 'GET', '/v1/profile', { token, headers })
    expect(profile.status, JSON.stringify(profile.body)).toBe(200)
    expect(profile.body as Profile).toEqual(readFixture('v1/profile/profile.json'))
  })

  it('without a key: profile/profile-orders-no-key.json', async () => {
    const profile = await call(server.origin, 'GET', '/v1/profile', { token })
    expect(profile.status, JSON.stringify(profile.body)).toBe(200)
    expect(profile.body as Profile).toEqual(readFixture('v1/profile/profile-orders-no-key.json'))
  })
})

describe('cart fixtures', () => {
  it('cart/cart-no-address.json is a member with one line and no address', async () => {
    const m = await createMember(db.prisma, { username: 'new_member', now: NOW })
    await addCartRow(db.prisma, m.id, {
      productId: 'yirga',
      weight: '250g',
      grind: 'PO',
      pack: 'STD',
      quantity: 1,
    })
    const response = await call(server.origin, 'GET', '/v1/cart', { token: m.token })
    expect(response.status, JSON.stringify(response.body)).toBe(200)
    expect(response.body as Cart).toEqual(readFixture('v1/cart/cart-no-address.json'))
  })
})

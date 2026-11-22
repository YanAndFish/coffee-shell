import type { PrismaClient } from '../src/db.ts'
import type { TestDatabase, TestServer } from './helpers.ts'
import { ApiErrorSchema, HealthSchema, INTERNAL_KEY_HEADER, ProductListSchema } from '@coffee/contract'
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest'
import { seedCatalog } from '../prisma/catalog.ts'
import { seedCoupons } from '../prisma/coupons.ts'
import { createApp } from '../src/app.ts'
import { createTestDatabase, listen, readFixture } from './helpers.ts'

let db: TestDatabase

beforeAll(async () => {
  db = await createTestDatabase()
}, 60_000)

afterAll(async () => {
  await db?.cleanup()
})

async function getJson(origin: string, path: string, init?: RequestInit): Promise<{ status: number, headers: Headers, body: unknown }> {
  const response = await fetch(`${origin}${path}`, init)
  expect(response.headers.get('content-type')).toMatch(/^application\/json/)
  return { status: response.status, headers: response.headers, body: await response.json() }
}

describe('without an internal key', () => {
  let server: TestServer

  beforeAll(async () => {
    server = await listen(createApp({ prisma: db.prisma }))
  })

  afterAll(async () => {
    await server?.close()
  })

  it('gET /v1/health matches the contract and the fixture', async () => {
    const { status, body } = await getJson(server.origin, '/v1/health')
    expect(status).toBe(200)
    expect(HealthSchema.parse(body)).toEqual(readFixture('v1/health.json'))
  })

  it('gET /v1/products returns the 8 seeded products in seed order with facets, equal to the fixture', async () => {
    const { status, body } = await getJson(server.origin, '/v1/products')
    expect(status).toBe(200)
    const list = ProductListSchema.parse(body)
    expect(list.items).toHaveLength(8)
    expect(list).toEqual(readFixture('v1/products.json'))
  })

  it('unknown routes answer the 404 envelope (equal to the fixture)', async () => {
    const { status, body } = await getJson(server.origin, '/v1/nope')
    expect(status).toBe(404)
    expect(ApiErrorSchema.parse(body)).toEqual(readFixture('v1/error.json'))
  })

  it('wrong methods answer 405 with Allow and the envelope', async () => {
    const { status, headers, body } = await getJson(server.origin, '/v1/products', { method: 'POST' })
    expect(status).toBe(405)
    expect(headers.get('allow')).toContain('GET')
    expect(ApiErrorSchema.parse(body).error).toEqual({ code: 'method_not_allowed', message: '不支持该请求方法' })
  })

  it('seeding again changes nothing', async () => {
    expect(await seedCatalog(db.prisma)).toBe(0)
    expect(await seedCoupons(db.prisma)).toBe(0)
    const { body } = await getJson(server.origin, '/v1/products')
    expect(ProductListSchema.parse(body).items).toHaveLength(8)
  })
})

describe('with an internal key', () => {
  const key = 'test-internal-key'
  let server: TestServer

  beforeAll(async () => {
    server = await listen(createApp({ prisma: db.prisma, internalKey: key }))
  })

  afterAll(async () => {
    await server?.close()
  })

  it('/v1/health stays open', async () => {
    const { status, body } = await getJson(server.origin, '/v1/health')
    expect(status).toBe(200)
    HealthSchema.parse(body)
  })

  it.each([
    ['a missing', undefined],
    ['a wrong', 'not-the-key'],
  ])('other routes answer 401 with %s key', async (_label, given) => {
    const headers: Record<string, string> = given
      ? { [INTERNAL_KEY_HEADER]: given }
      : {}
    for (const path of ['/v1/products', '/v1/nope']) {
      const { status, body } = await getJson(server.origin, path, { headers })
      expect(status).toBe(401)
      expect(ApiErrorSchema.parse(body).error).toEqual({ code: 'unauthorized', message: '缺少或错误的内部密钥' })
    }
  })

  it('the right key passes', async () => {
    const { status, body } = await getJson(server.origin, '/v1/products', {
      headers: { [INTERNAL_KEY_HEADER]: key },
    })
    expect(status).toBe(200)
    expect(ProductListSchema.parse(body).items).toHaveLength(8)
  })
})

describe('unexpected errors', () => {
  it('answer the 500 envelope without details and are logged', async () => {
    const broken = {
      product: {
        findMany: () => Promise.reject(new Error('disk on fire')),
      },
    } as unknown as PrismaClient
    const logError = vi.fn()
    const server = await listen(createApp({ prisma: broken, logError }))
    try {
      const { status, body } = await getJson(server.origin, '/v1/products')
      expect(status).toBe(500)
      expect(ApiErrorSchema.parse(body).error).toEqual({ code: 'internal_error', message: '服务器内部错误' })
      expect(JSON.stringify(body)).not.toContain('disk on fire')
      expect(logError).toHaveBeenCalledOnce()
    }
    finally {
      await server.close()
    }
  })

  it('stored data that breaks the contract is a 500, not a bad response', async () => {
    const corrupt = {
      product: {
        findMany: () => Promise.resolve([
          { notes: '[]', flavor: '[1,2,3]' },
        ]),
      },
    } as unknown as PrismaClient
    const server = await listen(createApp({
      prisma: corrupt,
      logError: () => {},
    }))
    try {
      const { status, body } = await getJson(server.origin, '/v1/products')
      expect(status).toBe(500)
      ApiErrorSchema.parse(body)
    }
    finally {
      await server.close()
    }
  })
})

// Catalog (server-shop): every filter × sort × search combination against an independent
// reading of docs/design/data.md#筛选与排序, facets over the whole catalog, and the detail
// page for sold-out weights, low stock and no stock.
import type { ProductDetail, ProductList, ProductSort, ProductSummary, RoastBand } from '@coffee/contract'
import type { TestDatabase, TestServer } from './helpers.ts'
import { ProductDetailSchema, ProductListSchema } from '@coffee/contract'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { NOW } from './conformance/types.ts'
import { call, createTestDatabase, listenApp, readFixture } from './helpers.ts'

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

const ALL = (readFixture('v1/products.json') as ProductList).items

async function list(query: Record<string, string> = {}): Promise<ProductList> {
  const response = await call(server.origin, 'GET', `/v1/products?${new URLSearchParams(query)}`)
  expect(response.status, JSON.stringify(response.body)).toBe(200)
  return ProductListSchema.parse(response.body)
}

async function detail(id: string): Promise<ProductDetail> {
  const response = await call(server.origin, 'GET', `/v1/products/${encodeURIComponent(id)}`)
  expect(response.status, JSON.stringify(response.body)).toBe(200)
  return ProductDetailSchema.parse(response.body)
}

/** The spec, written out again: data.md#筛选与排序. */
function expected(query: { origin?: string | undefined, roast?: RoastBand | undefined, sort?: ProductSort | undefined, q?: string | undefined }): string[] {
  const band = (level: number): RoastBand => (level <= 2
    ? 'light'
    : level === 3
      ? 'medium'
      : 'dark')
  const needle = (query.q ?? '').trim().toLowerCase()
  const text = (p: ProductSummary) => [
    p.name,
    p.subtitle,
    p.origin,
    p.region,
    p.process,
    ...p.notes,
  ].join(' ').toLowerCase()
  const matched = ALL.filter(p => (!query.origin || p.origin === query.origin)
    && (!query.roast || band(p.roast) === query.roast)
    && (!needle || text(p).includes(needle)))
  const key: Record<ProductSort, (p: ProductSummary) => number> = {
    recommended: () => 0,
    price_asc: p => p.price250,
    price_desc: p => -p.price250,
    score: p => -p.score,
  }
  const by = key[query.sort ?? 'recommended']
  // Array.prototype.sort is stable: ties keep display order.
  return [...matched].sort((a, b) => by(a) - by(b)).map(p => p.id)
}

const ORIGINS = [
  undefined,
  '埃塞俄比亚',
  '哥伦比亚',
  '肯尼亚',
  '巴拿马',
  '中国云南',
  '危地马拉',
  '印尼',
  '拼配',
  '火星',
]
const ROASTS: (RoastBand | undefined)[] = [undefined, 'light', 'medium', 'dark']
const SORTS: (ProductSort | undefined)[] = [
  undefined,
  'recommended',
  'price_asc',
  'price_desc',
  'score',
]
const SEARCHES = [
  undefined,
  '',
  '  ',
  '水洗',
  ' ETHIOPIA ',
  '黑巧',
  'konga',
  '茉莉',
  'xyz',
]

describe('gET /v1/products', () => {
  it('every origin × roast × sort combination', async () => {
    const facets = (await list()).facets
    for (const origin of ORIGINS) {
      for (const roast of ROASTS) {
        for (const sort of SORTS) {
          const query = Object.fromEntries(Object.entries({ origin, roast, sort }).filter(([, v]) => v !== undefined)) as Record<string, string>
          const body = await list(query)
          expect(body.items.map(p => p.id), JSON.stringify(query)).toEqual(expected({ origin, roast, sort }))
          expect(body.facets).toEqual(facets)
        }
      }
    }
  })

  it('every search × sort, and search combined with the filters', async () => {
    for (const q of SEARCHES) {
      for (const sort of SORTS) {
        const query = Object.fromEntries(Object.entries({ q, sort }).filter(([, v]) => v !== undefined)) as Record<string, string>
        expect((await list(query)).items.map(p => p.id), JSON.stringify(query)).toEqual(expected({ q, sort }))
      }
      for (const roast of ROASTS) {
        for (const origin of [undefined, '埃塞俄比亚', '巴拿马']) {
          const query = Object.fromEntries(Object.entries({ q, roast, origin }).filter(([, v]) => v !== undefined)) as Record<string, string>
          expect((await list(query)).items.map(p => p.id), JSON.stringify(query)).toEqual(expected({ q, roast, origin }))
        }
      }
    }
  })

  it('known answers, worked out by hand', async () => {
    expect((await list({ sort: 'price_asc' })).items.map(p => p.id))
      .toEqual([
        'yunnan',
        'mandh',
        'antigua',
        'night',
        'yirga',
        'nyeri',
        'pinkb',
        'geisha',
      ])
    expect((await list({ sort: 'score' })).items.map(p => p.id))
      .toEqual([
        'geisha',
        'pinkb',
        'yirga',
        'nyeri',
        'night',
        'yunnan',
        'antigua',
        'mandh',
      ])
    expect((await list({ roast: 'medium' })).items.map(p => p.id)).toEqual(['yunnan'])
    expect((await list({ roast: 'light', origin: '巴拿马' })).items.map(p => p.id)).toEqual(['geisha'])
    // 水洗 is also in 双重水洗 (nyeri) and 日晒 + 水洗 (night).
    expect((await list({ q: '水洗' })).items.map(p => p.id)).toEqual([
      'yirga',
      'pinkb',
      'nyeri',
      'geisha',
      'antigua',
      'night',
    ])
    expect((await list({ q: 'SUMATRA' })).items.map(p => p.id)).toEqual(['mandh'])
    expect((await list({ origin: '' })).items).toHaveLength(8)
  })

  it('facets always count the whole catalog', async () => {
    const facets = {
      total: 8,
      origins: [
        '埃塞俄比亚',
        '哥伦比亚',
        '肯尼亚',
        '巴拿马',
        '中国云南',
        '危地马拉',
        '印尼',
        '拼配',
      ].map(origin => ({ origin, count: 1 })),
    }
    expect((await list()).facets).toEqual(facets)
    expect((await list({ q: 'xyz', roast: 'dark', origin: '火星' })).facets).toEqual(facets)
  })

  it('every product keeps the same fields whatever the filters', async () => {
    const byId = new Map(ALL.map(p => [p.id, p]))
    for (const p of (await list({ roast: 'light', sort: 'score' })).items)
      expect(p).toEqual(byId.get(p.id))
  })

  it('the unfiltered list is the frozen fixture', async () => {
    expect(await list()).toEqual(readFixture('v1/products.json'))
  })

  it('score and price ties keep display order', async () => {
    await db.prisma.product.updateMany({
      where: {
        id: {
          in: ['mandh', 'yirga'],
        },
      },
      data: { score: 99, basePrice: 98 },
    })
    try {
      expect((await list({ sort: 'score' })).items.slice(0, 2).map(p => p.id)).toEqual(['yirga', 'mandh'])
      expect((await list({ sort: 'price_desc' })).items.map(p => p.id).filter(id => id === 'yirga' || id === 'mandh'))
        .toEqual(['yirga', 'mandh'])
    }
    finally {
      await db.prisma.product.update({
        where: { id: 'mandh' },
        data: { score: 84, basePrice: 72 },
      })
      await db.prisma.product.update({
        where: { id: 'yirga' },
        data: { score: 87.5, basePrice: 98 },
      })
    }
  })

  it('refuses bad parameters', async () => {
    for (const query of [
      { roast: 'burnt' },
      { sort: 'cheapest' },
      {
        q: 'x'.repeat(65),
      },
      {
        origin: 'x'.repeat(33),
      },
      { page: '2' },
    ]) {
      const response = await call(server.origin, 'GET', `/v1/products?${new URLSearchParams(query)}`)
      expect(response.status, JSON.stringify(query)).toBe(400)
      expect((response.body as { error: { code: string } }).error.code).toBe('validation_failed')
    }
  })
})

describe('gET /v1/products/{id}', () => {
  it('matches the fixtures for products with sold-out weights', async () => {
    expect(await detail('pinkb')).toEqual(readFixture('v1/catalog/product-pinkb.json'))
    expect(await detail('geisha')).toEqual(readFixture('v1/catalog/product-geisha.json'))
  })

  it('every product: summary as listed, eight cells from the unit price table', async () => {
    // data.md 单价表, standard bag; gift +20.
    const table: Record<string, number[]> = {
      yirga: [44, 98, 186, 353],
      pinkb: [53, 118, 224, 425],
      nyeri: [49, 108, 205, 389],
      geisha: [121, 268, 509, 965],
      yunnan: [31, 68, 129, 245],
      antigua: [35, 78, 148, 281],
      mandh: [32, 72, 137, 259],
      night: [40, 88, 167, 317],
    }
    for (const summary of ALL) {
      const d = await detail(summary.id)
      const { code, farm, altitude, variety, harvest, brew, maxQuantity, weights, prices, ...rest } = d
      expect(rest).toEqual(summary)
      expect(maxQuantity).toBe(Math.min(9, summary.stock))
      expect(prices.map(c => `${c.weight}/${c.pack}`)).toEqual([
        '100g/STD',
        '100g/GIFT',
        '250g/STD',
        '250g/GIFT',
        '500g/STD',
        '500g/GIFT',
        '1kg/STD',
        '1kg/GIFT',
      ])
      expect(prices.map(c => c.unitPrice)).toEqual(table[summary.id]!.flatMap(u => [u, u + 20]))
      for (const cell of prices)
        expect(cell.totals).toEqual(Array.from({ length: maxQuantity }, (_, i) => cell.unitPrice * (i + 1)))
      expect(weights.map(w => w.weight)).toEqual(['100g', '250g', '500g', '1kg'])
      expect(code).toMatch(/^[A-Z]{3}$/)
      expect([farm, altitude, variety, harvest, brew].every(s => s.length > 0)).toBe(true)
    }
    expect((await detail('night')).altitude).toBe('—')
    expect((await detail('night')).brew).toBe('18g → 36g · 28s')
  })

  it('low stock bounds the stepper and the totals; no stock empties them', async () => {
    await db.prisma.product.update({
      where: { id: 'nyeri' },
      data: { stock: 3 },
    })
    try {
      const low = await detail('nyeri')
      expect(low.stock).toBe(3)
      expect(low.maxQuantity).toBe(3)
      expect(low.prices[2]).toEqual({
        weight: '250g',
        pack: 'STD',
        unitPrice: 108,
        totals: [108, 216, 324],
      })
      await db.prisma.product.update({
        where: { id: 'nyeri' },
        data: { stock: 0 },
      })
      const none = await detail('nyeri')
      expect(none.maxQuantity).toBe(0)
      expect(none.prices.every(c => c.totals.length === 0)).toBe(true)
      expect(none.weights.every(w => w.available)).toBe(true)
    }
    finally {
      await db.prisma.product.update({
        where: { id: 'nyeri' },
        data: { stock: 18 },
      })
    }
  })

  it('a product whose 250g is sold out starts on the first weight in stock', async () => {
    await db.prisma.product.update({
      where: { id: 'yunnan' },
      data: { soldOutWeights: '["100g","250g"]' },
    })
    try {
      const d = await detail('yunnan')
      expect(d.defaultWeight).toBe('500g')
      expect(d.weights.map(w => w.available)).toEqual([false, false, true, true])
    }
    finally {
      await db.prisma.product.update({
        where: { id: 'yunnan' },
        data: { soldOutWeights: '[]' },
      })
    }
  })

  it('unknown or malformed ids', async () => {
    for (const [id, status, code] of [
      ['nope', 404, 'not_found'],
      ['GEISHA', 404, 'not_found'],
      [
        'x'.repeat(33),
        400,
        'validation_failed',
      ],
    ] as const) {
      const response = await call(server.origin, 'GET', `/v1/products/${id}`)
      expect(response.status).toBe(status)
      expect((response.body as { error: { code: string } }).error.code).toBe(code)
    }
  })
})

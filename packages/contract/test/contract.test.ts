import type { z } from 'zod'
import { readdirSync, readFileSync } from 'node:fs'
import { join, relative } from 'node:path'
import { describe, expect, it } from 'vitest'
import { fixtures, renderGeneratedFiles } from '../src/generate.ts'
import {
  AddCartLineRequestSchema,
  AddressInputSchema,
  ApiErrorSchema,
  endpointErrorCodes,
  endpoints,
  ERRORS,
  HealthSchema,
  isErrorCode,
  LoginRequestSchema,
  ProductDetailSchema,
  ProductListSchema,
  RegisterRequestSchema,
  skuCode,
  SkuSchema,
  UpdateCartLineRequestSchema,
  UserSchema,
} from '../src/index.ts'

const root = join(import.meta.dirname, '..')

function readJson(path: string): unknown {
  return JSON.parse(readFileSync(join(root, path), 'utf8'))
}

function listFiles(dir: string): string[] {
  return readdirSync(join(root, dir), { recursive: true, withFileTypes: true })
    .filter(entry => entry.isFile())
    .map(entry => relative(root, join(entry.parentPath, entry.name)))
    .sort()
}

/** The error code the server answers for `value` (first issue's message, as the server maps it). */
function firstCode(schema: z.ZodType, value: unknown): string | null {
  const result = schema.safeParse(value)
  if (result.success)
    return null
  const message = result.error.issues[0]?.message
  return isErrorCode(message)
    ? message
    : 'validation_failed'
}

describe('generated files', () => {
  const generated = renderGeneratedFiles()

  it.each([
    ...generated.keys(),
  ])('%s is up to date (run `pnpm -F @coffee/contract generate`)', (path) => {
    expect(readFileSync(join(root, path), 'utf8')).toBe(generated.get(path))
  })

  it('fixtures/ holds exactly the generated fixtures', () => {
    const expected = [
      ...generated.keys(),
    ].filter(path => path.startsWith('fixtures/')).sort()
    expect(listFiles('fixtures')).toEqual(expected)
  })
})

describe('fixtures', () => {
  it('health.json', () => {
    expect(HealthSchema.parse(readJson('fixtures/v1/health.json'))).toEqual({
      ok: true,
      service: 'roast-tty',
      version: '0.1.0',
      demo: true,
    })
  })

  it('products.json lists the eight beans in display order with whole-catalog facets', () => {
    const list = ProductListSchema.parse(readJson('fixtures/v1/products.json'))
    expect(list.items.map(p => p.id)).toEqual([
      'yirga',
      'pinkb',
      'nyeri',
      'geisha',
      'yunnan',
      'antigua',
      'mandh',
      'night',
    ])
    expect(list.facets.total).toBe(8)
    expect(list.facets.origins.map(o => o.origin)).toEqual([
      '埃塞俄比亚',
      '哥伦比亚',
      '肯尼亚',
      '巴拿马',
      '中国云南',
      '危地马拉',
      '印尼',
      '拼配',
    ])
  })

  it('error.json is the 404 envelope', () => {
    expect(ApiErrorSchema.parse(readJson('fixtures/v1/error.json')).error.code).toBe('not_found')
  })

  it.each(fixtures.map(f => [f.path, f] as const))('%s matches its schema', (_path, fixture) => {
    expect(fixture.schema.safeParse(readJson(`fixtures/${fixture.path}`)).success).toBe(true)
  })

  it('every endpoint has a success fixture with its success schema and status', () => {
    for (const endpoint of endpoints) {
      const own = fixtures.filter(f => f.operationId === endpoint.operationId)
      const success = own.filter(f => f.status < 300)
      expect(success.length, endpoint.operationId).toBeGreaterThan(0)
      for (const fixture of success) {
        expect(fixture.status, fixture.path).toBe(endpoint.success.status)
        expect(fixture.schema, fixture.path).toBe(endpoint.success.schema)
      }
    }
  })

  it('every product of products.json has its detail fixture, consistent with the list', () => {
    const list = ProductListSchema.parse(readJson('fixtures/v1/products.json'))
    const details = fixtures.filter(f => f.path.startsWith('v1/catalog/product-')).map(f => f.path).sort()
    expect(details).toEqual(list.items.map(p => `v1/catalog/product-${p.id}.json`).sort())
    for (const summary of list.items) {
      const detail = ProductDetailSchema.parse(readJson(`fixtures/v1/catalog/product-${summary.id}.json`))
      const { code: _code, farm: _farm, altitude: _altitude, variety: _variety, harvest: _harvest, brew: _brew, maxQuantity, weights, prices, ...rest } = detail
      expect(rest, summary.id).toEqual(summary)
      expect(maxQuantity, summary.id).toBe(Math.min(9, summary.stock))
      expect(weights.map(w => w.weight), summary.id).toEqual(['100g', '250g', '500g', '1kg'])
      expect(prices, summary.id).toHaveLength(8)
    }
  })

  it('every member fixture names the same user (林晓)', () => {
    const users = fixtures
      .map(f => [
        f.path,
        readJson(`fixtures/${f.path}`),
      ] as const)
      .filter(([, value]) => typeof value === 'object' && value !== null && 'user' in value)
      .map(([path, value]) => [
        path,
        UserSchema.parse((value as { user: unknown }).user),
      ] as const)
    expect(users.map(([path]) => path).sort()).toEqual([
      'v1/auth/login-no-key.json',
      'v1/auth/login.json',
      'v1/auth/me.json',
      'v1/auth/register.json',
      'v1/auth/resume.json',
      'v1/profile/profile-new.json',
      'v1/profile/profile-no-key.json',
      'v1/profile/profile-orders-no-key.json',
      'v1/profile/profile.json',
    ])
    for (const [path, user] of users)
      expect(user, path).toEqual(users[0]![1])
    expect(users[0]![1].username).toBe('林晓')
  })

  it('every error code has an envelope fixture with its status and exact message', () => {
    for (const [code, { status, message }] of Object.entries(ERRORS)) {
      const fixture = fixtures.find(f => f.path === `v1/errors/${code}.json`)
      expect(fixture?.status, code).toBe(status)
      const envelope = ApiErrorSchema.parse(readJson(`fixtures/v1/errors/${code}.json`))
      expect(envelope.error.code).toBe(code)
      expect(envelope.error.message).toBe(message)
    }
  })
})

describe('error table', () => {
  it('has one distinct, non-empty message per code and a sane status', () => {
    const messages = Object.values(ERRORS).map(e => e.message)
    expect(new Set(messages).size).toBe(messages.length)
    for (const { status, message } of Object.values(ERRORS)) {
      expect(message.length).toBeGreaterThan(0)
      expect(message.startsWith('✗')).toBe(false)
      expect(status >= 400 && status < 600).toBe(true)
    }
  })

  it('implied codes follow access and auth', () => {
    const cart = endpoints.find(e => e.operationId === 'getCart')!
    expect(endpointErrorCodes(cart)).toEqual(expect.arrayContaining([
      'unauthorized',
      'login_required',
      'session_expired',
      'internal_error',
    ]))
    const health = endpoints.find(e => e.operationId === 'getHealth')!
    expect(endpointErrorCodes(health)).toEqual(['internal_error'])
  })
})

describe('request schemas carry the error codes', () => {
  it('register: required, pattern, password bounds', () => {
    expect(firstCode(RegisterRequestSchema, { password: 'espresso' })).toBe('username_required')
    expect(firstCode(RegisterRequestSchema, { username: '', password: 'espresso' })).toBe('username_required')
    expect(firstCode(RegisterRequestSchema, { username: 'a', password: 'espresso' })).toBe('username_invalid')
    expect(firstCode(RegisterRequestSchema, { username: 'ab c', password: 'espresso' })).toBe('username_invalid')
    expect(firstCode(RegisterRequestSchema, { username: '一二三四五六七八九十一二三四五六七', password: 'espresso' })).toBe('username_invalid')
    expect(firstCode(RegisterRequestSchema, { username: '林晓', password: '12345' })).toBe('password_too_short')
    expect(firstCode(RegisterRequestSchema, {
      username: '林晓',
      password: 'x'.repeat(129),
    })).toBe('password_too_long')
    expect(RegisterRequestSchema.parse({ username: '林晓_2026', password: 'espresso' })).toEqual({ username: '林晓_2026', password: 'espresso', remember: false })
    expect(firstCode(RegisterRequestSchema, { username: 'Lin', password: 'espresso', email: 'x' })).toBe('validation_failed')
  })

  it('login: no pattern check', () => {
    expect(firstCode(LoginRequestSchema, { username: 'a b', password: 'espresso' })).toBeNull()
    expect(firstCode(LoginRequestSchema, { username: '', password: 'espresso' })).toBe('username_required')
    expect(firstCode(LoginRequestSchema, { username: 'a', password: '1' })).toBe('password_too_short')
  })

  it('address: 收货人 → 所在地区 → 详细地址 → 手机号', () => {
    expect(firstCode(AddressInputSchema, {})).toBe('recipient_required')
    expect(firstCode(AddressInputSchema, { recipient: '  ', region: '', detail: '' })).toBe('recipient_required')
    expect(firstCode(AddressInputSchema, { recipient: '林晓', region: ' ', detail: '' })).toBe('region_required')
    expect(firstCode(AddressInputSchema, { recipient: '林晓', region: '上海', detail: '' })).toBe('detail_required')
    expect(firstCode(AddressInputSchema, {
      recipient: '林晓',
      region: '上海',
      detail: '天平路',
      phone: '138',
    })).toBe('phone_invalid')
    expect(firstCode(AddressInputSchema, {
      recipient: '林'.repeat(21),
      region: '上海',
      detail: '天平路',
    })).toBe('text_too_long')
    expect(AddressInputSchema.parse({
      recipient: ' 林晓 ',
      region: '上海',
      detail: '天平路',
      phone: '',
    }))
      .toEqual({
        recipient: '林晓',
        region: '上海',
        detail: '天平路',
        phone: '',
        tag: 'home',
        isDefault: false,
      })
  })

  it('cart: quantity 1–9, at least one field to update', () => {
    const line = {
      productId: 'yirga',
      weight: '250g',
      grind: 'WB',
      pack: 'STD',
    }
    expect(firstCode(AddCartLineRequestSchema, { ...line, quantity: 10 })).toBe('quantity_invalid')
    expect(firstCode(AddCartLineRequestSchema, { ...line, quantity: 0 })).toBe('quantity_invalid')
    expect(firstCode(AddCartLineRequestSchema, { ...line, quantity: 1.5 })).toBe('quantity_invalid')
    expect(firstCode(AddCartLineRequestSchema, { ...line, weight: '2kg', quantity: 1 })).toBe('validation_failed')
    expect(firstCode(UpdateCartLineRequestSchema, {})).toBe('validation_failed')
    expect(firstCode(UpdateCartLineRequestSchema, { selected: false })).toBeNull()
  })

  it('sku codes', () => {
    expect(skuCode('PNK', '250g', 'WB', 'STD')).toBe('RT-PNK-250G-WB-STD')
    expect(skuCode('NGT', '1kg', 'ES', 'GIFT')).toBe('RT-NGT-1KG-ES-GIFT')
    expect(SkuSchema.safeParse('RT-PNK-2KG-WB-STD').success).toBe(false)
  })
})

describe('schemas', () => {
  it('reject extra keys and out-of-range values', () => {
    const list = ProductListSchema.parse(readJson('fixtures/v1/products.json'))
    const item = list.items[0]!
    const withItem = (patch: Record<string, unknown>): unknown => ({
      ...list,
      items: [
        {
          ...item,
          ...patch,
        },
      ],
    })
    expect(ProductListSchema.safeParse(withItem({ extra: 1 })).success).toBe(false)
    expect(ProductListSchema.safeParse(withItem({
      flavor: [1, 2, 3],
    })).success).toBe(false)
    expect(ProductListSchema.safeParse(withItem({ roast: 6 })).success).toBe(false)
    expect(ProductListSchema.safeParse(withItem({ price250: 9.5 })).success).toBe(false)
  })
})

describe('openapi.json', () => {
  const doc = readJson('openapi.json') as { paths: Record<string, Record<string, { 'operationId': string, 'x-error-codes': string[] }>> }

  it('documents every endpoint with its error codes', () => {
    for (const endpoint of endpoints) {
      const operation = doc.paths[endpoint.path]?.[endpoint.method]
      expect(operation?.operationId).toBe(endpoint.operationId)
      expect(operation?.['x-error-codes']).toEqual(endpointErrorCodes(endpoint))
    }
  })

  it('has unique operation ids', () => {
    const ids = endpoints.map(e => e.operationId)
    expect(new Set(ids).size).toBe(ids.length)
  })
})

describe('rEADME', () => {
  it('lists every endpoint', () => {
    const readme = readFileSync(join(root, 'README.md'), 'utf8')
    for (const endpoint of endpoints)
      expect(readme, endpoint.operationId).toContain(`\`${endpoint.operationId}\``)
  })
})

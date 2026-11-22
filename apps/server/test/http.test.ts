// The shared request pipeline: sessions (requireUser / optionalUser), X-SSH-Fingerprint,
// body parsing and contract validation, the route table. Where it only needs the pipeline it
// asserts on what the pipeline rejects, never on what the handler behind it answers.
import type { TestDatabase, TestServer } from './helpers.ts'
import { ApiErrorSchema, endpoints, SSH_FINGERPRINT_HEADER } from '@coffee/contract'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { koaPath } from '../src/http/route.ts'
import { apiRouter } from '../src/routes/index.ts'
import { FINGERPRINT, NOW } from './conformance/types.ts'
import { call, createTestDatabase, listenApp } from './helpers.ts'
import { createMember, createSession } from './support/members.ts'

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

async function errorCode(method: string, path: string, options: Parameters<typeof call>[3] = {}): Promise<[number, string, unknown]> {
  const response = await call(server.origin, method, path, options)
  const envelope = ApiErrorSchema.parse(response.body)
  return [response.status, envelope.error.code, envelope.error.details]
}

describe('sessions', () => {
  it('a member endpoint without a token is 401 login_required', async () => {
    expect(await errorCode('GET', '/v1/cart')).toEqual([401, 'login_required', undefined])
  })

  it('a malformed or unknown token is 401 session_expired', async () => {
    expect((await errorCode('GET', '/v1/cart', {
      headers: { Authorization: 'Bearer nope' },
    }))[1]).toBe('session_expired')
    expect((await errorCode('GET', '/v1/cart', {
      token: 'x'.repeat(43),
    }))[1]).toBe('session_expired')
  })

  it('an expired session is 401 session_expired', async () => {
    const member = await createMember(db.prisma, { now: NOW })
    const token = await createSession(db.prisma, member.id, new Date(NOW.getTime() - 8 * 24 * 3600 * 1000))
    expect((await errorCode('GET', '/v1/cart', { token }))[1]).toBe('session_expired')
  })

  it('a valid session passes the session check', async () => {
    const member = await createMember(db.prisma, { now: NOW })
    const response = await call(server.origin, 'GET', '/v1/cart', { token: member.token })
    expect(response.status).not.toBe(401)
  })

  it('auth: none ignores tokens; optional would reject a bad one', async () => {
    const response = await call(server.origin, 'GET', '/v1/products', {
      token: 'x'.repeat(43),
    })
    expect(response.status).toBe(200)
  })
})

describe('x-SSH-Fingerprint', () => {
  it('is required where the contract says so', async () => {
    expect(await errorCode('POST', '/v1/auth/resume')).toEqual([400, 'fingerprint_required', undefined])
  })

  it('must be well-formed', async () => {
    expect(await errorCode('POST', '/v1/auth/resume', {
      headers: { [SSH_FINGERPRINT_HEADER]: 'SHA256:forged' },
    }))
      .toEqual([
        400,
        'validation_failed',
        { field: 'header.x-ssh-fingerprint' },
      ])
  })

  it('a good one passes the header check', async () => {
    const response = await call(server.origin, 'POST', '/v1/auth/resume', {
      headers: { [SSH_FINGERPRINT_HEADER]: FINGERPRINT },
    })
    expect(response.status).not.toBe(400)
  })
})

describe('input validation', () => {
  it('schema codes become the contract errors before the handler runs', async () => {
    const member = await createMember(db.prisma, { now: NOW })
    const body = {
      productId: 'yirga',
      weight: '250g',
      grind: 'WB',
      pack: 'STD',
      quantity: 10,
    }
    expect(await errorCode('POST', '/v1/cart/lines', { token: member.token, body })).toEqual([422, 'quantity_invalid', undefined])
    expect(await errorCode('POST', '/v1/cart/lines', {
      token: member.token,
      body: { ...body, quantity: 1, grind: 'XX' },
    }))
      .toEqual([
        400,
        'validation_failed',
        { field: 'body.grind' },
      ])
    expect(await errorCode('POST', '/v1/auth/register', {
      body: { username: 'a', password: 'espresso42' },
    })).toEqual([422, 'username_invalid', undefined])
  })

  it('bad JSON, a wrong content type and a missing body', async () => {
    const bad = await fetch(`${server.origin}/v1/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: '{"username":',
    })
    expect(ApiErrorSchema.parse(await bad.json()).error.code).toBe('bad_request')
    const text = await fetch(`${server.origin}/v1/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'text/plain' },
      body: 'hi',
    })
    expect(ApiErrorSchema.parse(await text.json()).error.code).toBe('bad_request')
    expect(await errorCode('POST', '/v1/auth/login')).toEqual([
      400,
      'validation_failed',
      { field: 'body' },
    ])
    expect((await errorCode('POST', '/v1/auth/login', { body: {} }))[1]).toBe('username_required')
  })

  it('path and query parameters', async () => {
    const member = await createMember(db.prisma, { now: NOW })
    expect(await errorCode('DELETE', '/v1/cart/lines/not-a-sku', { token: member.token })).toEqual([
      400,
      'validation_failed',
      { field: 'params.sku' },
    ])
    expect(await errorCode('DELETE', '/v1/addresses/abc', { token: member.token })).toEqual([
      400,
      'validation_failed',
      { field: 'params.id' },
    ])
    expect(await errorCode('GET', '/v1/products?roast=burnt')).toEqual([
      400,
      'validation_failed',
      { field: 'query.roast' },
    ])
  })
})

describe('the route table', () => {
  const router = apiRouter({
    prisma: db?.prisma,
    now: () => NOW,
    validateResponses: true,
  })

  it.each(endpoints.map(e => [
    e.method.toUpperCase(),
    e.path,
  ] as const))('%s %s is routed', (method, path) => {
    const concrete = koaPath(path)
      .replace(':sku', 'RT-PNK-250G-WB-STD')
      .replace(':id', '1')
      .replace(':number', 'RT2611220417')
    expect(router.match(concrete, method).route).toBe(true)
  })
})

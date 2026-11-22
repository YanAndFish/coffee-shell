// Focused tests of area `auth` beyond the conformance cases: validation per field, case-blind
// uniqueness, credential errors that do not tell names apart, session expiry, the 记住我
// lifecycle, rate limits (with a movable clock) and secrets kept out of answers and logs.
import type { AuthSession, ErrorCode } from '@coffee/contract'
import type { JsonResponse, TestDatabase, TestServer } from './helpers.ts'
import { randomUUID } from 'node:crypto'
import process from 'node:process'
import {
  ApiErrorSchema,
  AuthSessionSchema,
  CLIENT_IP_HEADER,
  ERRORS,
  MeSchema,
  OkSchema,
  SSH_FINGERPRINT_HEADER,
} from '@coffee/contract'
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { AttemptLimiter, FailureLimiter, RATE_WINDOW_MS } from '../src/auth/rate-limit.ts'
import { hashSessionToken, REMEMBER_TTL_MS, SESSION_TTL_MS } from '../src/auth/tokens.ts'
import { shortFingerprint } from '../src/services/auth.ts'
import { fakeFingerprint } from './conformance/types.ts'
import { call, createTestDatabase, listenApp } from './helpers.ts'

const T0 = new Date('2026-11-22T13:04:00.000Z')
const DAY = 24 * 60 * 60 * 1000
const PASSWORD = 'espresso42'

let db: TestDatabase

beforeAll(async () => {
  db = await createTestDatabase()
}, 60_000)

let serial = 0
/** A username no other test uses (letters, digits, `_`; ≤ 16 characters). */
function uniqueName(prefix = 'u'): string {
  serial++
  return `${prefix}_${serial}`
}

/** A fresh app (fresh rate-limit counters) on the shared database with a movable clock. */
function useApp(): { origin: () => string, clock: { now: Date } } {
  const clock = { now: T0 }
  let server: TestServer
  beforeAll(async () => {
    server = await listenApp(db.prisma, {
      now: () => clock.now,
    })
  })
  beforeEach(() => {
    clock.now = T0
  })
  afterAll(async () => {
    await server?.close()
  })
  return {
    origin: () => server.origin,
    clock,
  }
}

afterAll(async () => {
  await db?.cleanup()
})

interface Send {
  body?: unknown
  token?: string
  fingerprint?: string
  /** `X-Client-IP`; a random one by default so register limits do not leak between tests. */
  ip?: string
  headers?: Record<string, string>
}

function send(origin: string, method: string, path: string, options: Send = {}): Promise<JsonResponse> {
  const headers: Record<string, string> = {
    [CLIENT_IP_HEADER]: options.ip ?? `test-${randomUUID()}`,
    ...options.headers,
  }
  if (options.fingerprint)
    headers[SSH_FINGERPRINT_HEADER] = options.fingerprint
  return call(origin, method, path, {
    ...(options.body !== undefined
      ? { body: options.body }
      : {}),
    ...(options.token
      ? { token: options.token }
      : {}),
    headers,
  })
}

function expectError(response: JsonResponse, code: ErrorCode): void {
  const envelope = ApiErrorSchema.parse(response.body)
  expect(envelope.error.code, JSON.stringify(response.body)).toBe(code)
  expect(envelope.error.message).toBe(ERRORS[code].message)
  expect(response.status).toBe(ERRORS[code].status)
}

function session(response: JsonResponse, status = 200): AuthSession {
  expect(response.status, JSON.stringify(response.body)).toBe(status)
  return AuthSessionSchema.parse(response.body)
}

async function registerUser(origin: string, username: string, options: Send = {}): Promise<AuthSession> {
  return session(await send(origin, 'POST', '/v1/auth/register', {
    ...options,
    body: { username, password: PASSWORD, ...(options.body as object | undefined) },
  }), 201)
}

function login(origin: string, username: string, options: Send & { password?: string, remember?: boolean } = {}): Promise<JsonResponse> {
  return send(origin, 'POST', '/v1/auth/login', {
    ...options,
    body: {
      username,
      password: options.password ?? PASSWORD,
      ...(options.remember !== undefined
        ? { remember: options.remember }
        : {}),
    },
  })
}

async function me(origin: string, token: string, fingerprint?: string): Promise<JsonResponse> {
  return send(origin, 'GET', '/v1/auth/me', {
    token,
    ...(fingerprint
      ? { fingerprint }
      : {}),
  })
}

describe('validation', () => {
  const { origin } = useApp()

  it.each([
    [
      'missing username',
      { password: PASSWORD },
      'username_required',
    ],
    [
      'empty username',
      { username: '', password: PASSWORD },
      'username_required',
    ],
    [
      'username not a string',
      { username: 42, password: PASSWORD },
      'username_required',
    ],
    [
      'one character',
      { username: 'a', password: PASSWORD },
      'username_invalid',
    ],
    [
      '17 characters',
      {
        username: 'a'.repeat(17),
        password: PASSWORD,
      },
      'username_invalid',
    ],
    [
      'a hyphen',
      { username: 'bean-lover', password: PASSWORD },
      'username_invalid',
    ],
    [
      'a space',
      { username: 'bean lover', password: PASSWORD },
      'username_invalid',
    ],
    [
      'an emoji',
      { username: '咖啡☕', password: PASSWORD },
      'username_invalid',
    ],
    [
      'a kana',
      { username: 'コーヒー', password: PASSWORD },
      'username_invalid',
    ],
    [
      'missing password',
      { username: 'valid_name' },
      'password_too_short',
    ],
    [
      '5-character password',
      { username: 'valid_name', password: '12345' },
      'password_too_short',
    ],
    [
      '129-character password',
      {
        username: 'valid_name',
        password: 'x'.repeat(129),
      },
      'password_too_long',
    ],
  ] as const)('register: %s → %s', async (_name, body, code) => {
    expectError(await send(origin(), 'POST', '/v1/auth/register', { body }), code)
  })

  it('register: username is checked before password (field order)', async () => {
    expectError(await send(origin(), 'POST', '/v1/auth/register', {
      body: { username: 'a', password: '1' },
    }), 'username_invalid')
  })

  it.each([
    [
      'remember not a boolean',
      { username: 'valid_name', password: PASSWORD, remember: 'yes' },
      'body.remember',
    ],
    [
      'an unknown field',
      { username: 'valid_name', password: PASSWORD, email: 'x@example.com' },
      'body',
    ],
  ] as const)('register: %s → validation_failed', async (_name, body, field) => {
    const response = await send(origin(), 'POST', '/v1/auth/register', { body })
    expectError(response, 'validation_failed')
    expect(ApiErrorSchema.parse(response.body).error.details?.field).toBe(field)
  })

  it.each([
    '林晓',
    'ab',
    'A_1',
    '咖啡咖啡咖啡咖啡咖啡咖啡咖啡咖啡',
    'Bean_Lover_2026_',
  ])('register accepts %s', async (name) => {
    const s = await registerUser(origin(), name)
    expect(s.user.username).toBe(name)
  })

  it('register accepts passwords of exactly 6 and 128 characters', async () => {
    await registerUser(origin(), uniqueName('pw6'), {
      body: { password: '123456' },
    })
    await registerUser(origin(), uniqueName('pw128'), {
      body: {
        password: 'p'.repeat(128),
      },
    })
  })

  it.each([
    [
      'missing username',
      { password: PASSWORD },
      'username_required',
    ],
    [
      'empty username',
      { username: '', password: PASSWORD },
      'username_required',
    ],
    [
      'username over 64 characters',
      {
        username: 'a'.repeat(65),
        password: PASSWORD,
      },
      'invalid_credentials',
    ],
    [
      'missing password',
      { username: 'someone' },
      'password_too_short',
    ],
    [
      'short password',
      { username: 'someone', password: '12345' },
      'password_too_short',
    ],
    [
      'long password',
      {
        username: 'someone',
        password: 'x'.repeat(129),
      },
      'password_too_long',
    ],
  ] as const)('login: %s → %s', async (_name, body, code) => {
    expectError(await send(origin(), 'POST', '/v1/auth/login', { body }), code)
  })

  it('a malformed fingerprint is validation_failed on header.x-ssh-fingerprint', async () => {
    const response = await send(origin(), 'POST', '/v1/auth/resume', { fingerprint: 'SHA256:short' })
    expectError(response, 'validation_failed')
    expect(ApiErrorSchema.parse(response.body).error.details?.field).toBe('header.x-ssh-fingerprint')
  })

  it('a body that is not JSON is bad_request', async () => {
    const response = await fetch(`${origin()}/v1/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'text/plain' },
      body: 'username=a&password=b',
    })
    expect(response.status).toBe(400)
    expect(ApiErrorSchema.parse(await response.json()).error.code).toBe('bad_request')
  })
})

describe('register', () => {
  const { origin } = useApp()

  it('logs in: the token works, the member has a cart and default settings, the hash is argon2id', async () => {
    const name = uniqueName('Reg')
    const s = await registerUser(origin(), name)
    expect(s.expiresAt).toBe(new Date(T0.getTime() + SESSION_TTL_MS).toISOString())
    expect(s.user.createdAt).toBe(T0.toISOString())
    const m = MeSchema.parse((await me(origin(), s.token)).body)
    expect(m.user).toEqual(s.user)
    const row = await db.prisma.user.findUniqueOrThrow({
      where: { id: s.user.id },
      include: { cart: true, setting: true },
    })
    expect(row.usernameKey).toBe(name.toLowerCase())
    expect(row.passwordHash).toMatch(/^argon2id\$/)
    expect(row.cart).not.toBeNull()
    expect(row.setting).toMatchObject({ animations: true, mouseTracking: true })
  })

  it('refuses a name taken in any ASCII case and keeps the registered spelling', async () => {
    const name = `CoffeeFan${++serial}`
    await registerUser(origin(), name)
    for (const variant of [
      name,
      name.toLowerCase(),
      name.toUpperCase(),
    ]) {
      expectError(await send(origin(), 'POST', '/v1/auth/register', {
        body: { username: variant, password: PASSWORD },
      }), 'username_taken')
    }
    const s = session(await login(origin(), name.toUpperCase()))
    expect(s.user.username).toBe(name)
  })

  it('refuses a taken Chinese name', async () => {
    await registerUser(origin(), '拿铁爱好者')
    expectError(await send(origin(), 'POST', '/v1/auth/register', {
      body: { username: '拿铁爱好者', password: PASSWORD },
    }), 'username_taken')
  })

  it('two registrations of one name at once: one wins, the other is username_taken', async () => {
    const name = uniqueName('race')
    const responses = await Promise.all([
      send(origin(), 'POST', '/v1/auth/register', {
        body: { username: name, password: PASSWORD },
      }),
      send(origin(), 'POST', '/v1/auth/register', {
        body: {
          username: name.toUpperCase(),
          password: PASSWORD,
        },
      }),
    ])
    expect(responses.map(r => r.status).sort()).toEqual([201, 409])
    expectError(responses.find(r => r.status === 409)!, 'username_taken')
    expect(await db.prisma.user.count({
      where: {
        usernameKey: name.toLowerCase(),
      },
    })).toBe(1)
  })
})

describe('credentials', () => {
  const { origin } = useApp()

  it('a wrong password and an unknown username answer exactly the same', async () => {
    const name = uniqueName('cred')
    await registerUser(origin(), name)
    const wrong = await login(origin(), name, { password: 'wrong-password' })
    const unknown = await login(origin(), uniqueName('ghost'), { password: PASSWORD })
    expectError(wrong, 'invalid_credentials')
    expect(unknown.status).toBe(wrong.status)
    expect(unknown.body).toEqual(wrong.body)
    expect([
      ...unknown.headers.keys(),
    ].sort()).toEqual([
      ...wrong.headers.keys(),
    ].sort())
  })

  it('login matching ignores ASCII case only', async () => {
    await registerUser(origin(), '林晓Lin')
    session(await login(origin(), '林晓LIN'))
    session(await login(origin(), '林晓lin'))
    expectError(await login(origin(), '林晓 lin'), 'invalid_credentials')
  })
})

describe('sessions', () => {
  const { origin, clock } = useApp()

  it('a session lasts exactly 7 days, then is session_expired', async () => {
    const s = await registerUser(origin(), uniqueName('ttl'))
    clock.now = new Date(T0.getTime() + SESSION_TTL_MS - 1)
    expect((await me(origin(), s.token)).status).toBe(200)
    clock.now = new Date(T0.getTime() + SESSION_TTL_MS)
    expectError(await me(origin(), s.token), 'session_expired')
    expectError(await send(origin(), 'POST', '/v1/auth/logout', { token: s.token }), 'session_expired')
  })

  it('only the SHA-256 of the token is stored', async () => {
    const s = await registerUser(origin(), uniqueName('hash'))
    const rows = await db.prisma.session.findMany({
      where: { userId: s.user.id },
    })
    expect(rows.map(r => r.tokenHash)).toEqual([
      hashSessionToken(s.token),
    ])
    expect(rows[0]?.expiresAt.toISOString()).toBe(s.expiresAt)
  })

  it('an unknown or malformed token is session_expired; none is login_required', async () => {
    expectError(await me(origin(), 'A'.repeat(43)), 'session_expired')
    expectError(await send(origin(), 'GET', '/v1/auth/me', {
      headers: { Authorization: 'Bearer nope' },
    }), 'session_expired')
    expectError(await send(origin(), 'GET', '/v1/auth/me'), 'login_required')
  })

  it('logout ends this session only', async () => {
    const name = uniqueName('two')
    const first = await registerUser(origin(), name)
    const second = session(await login(origin(), name))
    expect(OkSchema.parse((await send(origin(), 'POST', '/v1/auth/logout', { token: first.token })).body)).toEqual({ ok: true })
    expectError(await me(origin(), first.token), 'session_expired')
    expect((await me(origin(), second.token)).status).toBe(200)
  })
})

describe('记住我 lifecycle', () => {
  const { origin, clock } = useApp()

  function at(days: number): Date {
    return new Date(T0.getTime() + days * DAY)
  }

  async function binding(fingerprint: string): Promise<{ userId: number, expiresAt: Date } | null> {
    return db.prisma.rememberedKey.findUnique({
      where: { fingerprint },
      select: { userId: true, expiresAt: true },
    })
  }

  it('binds only with remember, a key and a correct password; resume does not extend; rebinding moves; logout unbinds', async () => {
    const key = fakeFingerprint(`lifecycle-${randomUUID()}`)
    const otherKey = fakeFingerprint(`lifecycle-other-${randomUUID()}`)
    const alice = uniqueName('alice')
    const bob = uniqueName('bob')
    await registerUser(origin(), alice)
    await registerUser(origin(), bob)

    // remember without a key: nothing to bind.
    const noKey = session(await login(origin(), alice, { remember: true }))
    expect(noKey.remembered).toEqual({ state: 'no_key', expiresAt: null, expiresOn: null })
    // a key without remember: nothing bound.
    const notRemembered = session(await login(origin(), alice, { fingerprint: key }))
    expect(notRemembered.remembered.state).toBe('not_remembered')
    expect(await binding(key)).toBeNull()
    // a wrong password with remember binds nothing.
    expectError(await login(origin(), alice, { fingerprint: key, remember: true, password: 'wrong-password' }), 'invalid_credentials')
    expect(await binding(key)).toBeNull()

    // remember + key + password at day 0: bound until day 30.
    const a0 = session(await login(origin(), alice, { fingerprint: key, remember: true }))
    const until30 = new Date(T0.getTime() + REMEMBER_TTL_MS)
    expect(a0.remembered).toEqual({
      state: 'remembered',
      expiresAt: until30.toISOString(),
      expiresOn: '2026-12-22',
    })
    expect(await binding(key)).toEqual({ userId: a0.user.id, expiresAt: until30 })

    // me: this key → remembered; another key → not_remembered; no key → no_key.
    expect(MeSchema.parse((await me(origin(), a0.token, key)).body).remembered.state).toBe('remembered')
    expect(MeSchema.parse((await me(origin(), a0.token, otherKey)).body).remembered.state).toBe('not_remembered')
    expect(MeSchema.parse((await me(origin(), a0.token)).body).remembered.state).toBe('no_key')

    // resume at day 10: a new session, the binding keeps its expiry.
    clock.now = at(10)
    const r10 = session(await send(origin(), 'POST', '/v1/auth/resume', { fingerprint: key }))
    expect(r10.user.id).toBe(a0.user.id)
    expect(r10.token).not.toBe(a0.token)
    expect(r10.expiresAt).toBe(new Date(at(10).getTime() + SESSION_TTL_MS).toISOString())
    expect(r10.remembered.expiresAt).toBe(until30.toISOString())
    expect((await binding(key))?.expiresAt).toEqual(until30)
    expect((await me(origin(), r10.token, key)).status).toBe(200)

    // a password login without remember at day 11 leaves the binding alone.
    clock.now = at(11)
    const a11 = session(await login(origin(), alice, { fingerprint: key }))
    expect(a11.remembered.expiresAt).toBe(until30.toISOString())

    // with remember again at day 12: renewed to day 42.
    clock.now = at(12)
    const a12 = session(await login(origin(), alice, { fingerprint: key, remember: true }))
    const until42 = new Date(at(12).getTime() + REMEMBER_TTL_MS)
    expect(a12.remembered.expiresAt).toBe(until42.toISOString())

    // bob remembers the same key: it moves to bob.
    const b12 = session(await login(origin(), bob, { fingerprint: key, remember: true }))
    expect(b12.remembered.state).toBe('remembered')
    expect((await binding(key))?.userId).toBe(b12.user.id)
    expect(MeSchema.parse((await me(origin(), a12.token, key)).body).remembered.state).toBe('not_remembered')
    expect(session(await send(origin(), 'POST', '/v1/auth/resume', { fingerprint: key })).user.id).toBe(b12.user.id)

    // alice logging out with that key does not touch bob's binding.
    await send(origin(), 'POST', '/v1/auth/logout', { token: a12.token, fingerprint: key })
    expect((await binding(key))?.userId).toBe(b12.user.id)

    // bob logs out with the key: session and binding gone; resume fails; bob's other session lives.
    const bOther = session(await login(origin(), bob))
    expect((await send(origin(), 'POST', '/v1/auth/logout', { token: b12.token, fingerprint: key })).status).toBe(200)
    expect(await binding(key)).toBeNull()
    expectError(await me(origin(), b12.token), 'session_expired')
    expectError(await send(origin(), 'POST', '/v1/auth/resume', { fingerprint: key }), 'not_remembered')
    expect((await me(origin(), bOther.token)).status).toBe(200)
  })

  it('a binding ends after 30 days', async () => {
    const key = fakeFingerprint(`expiry-${randomUUID()}`)
    const name = uniqueName('exp')
    await registerUser(origin(), name)
    session(await login(origin(), name, { fingerprint: key, remember: true }))
    clock.now = new Date(T0.getTime() + REMEMBER_TTL_MS - 1)
    const last = session(await send(origin(), 'POST', '/v1/auth/resume', { fingerprint: key }))
    clock.now = new Date(T0.getTime() + REMEMBER_TTL_MS)
    expect(MeSchema.parse((await me(origin(), last.token, key)).body).remembered.state).toBe('not_remembered')
    expectError(await send(origin(), 'POST', '/v1/auth/resume', { fingerprint: key }), 'not_remembered')
    expect(await binding(key)).toBeNull()
  })

  it('register with remember and a key binds it (moving it from another member)', async () => {
    const key = fakeFingerprint(`register-${randomUUID()}`)
    const first = await registerUser(origin(), uniqueName('first'), {
      fingerprint: key,
      body: { remember: true },
    })
    expect(first.remembered.state).toBe('remembered')
    const second = await registerUser(origin(), uniqueName('second'), {
      fingerprint: key,
      body: { remember: true },
    })
    expect(second.remembered.state).toBe('remembered')
    expect((await binding(key))?.userId).toBe(second.user.id)
    expect(MeSchema.parse((await me(origin(), first.token, key)).body).remembered.state).toBe('not_remembered')
  })

  it('resume needs a key', async () => {
    expectError(await send(origin(), 'POST', '/v1/auth/resume'), 'fingerprint_required')
  })
})

describe('rate limits', () => {
  const { origin, clock } = useApp()

  function expectRateLimited(response: JsonResponse, retryAfter: number): void {
    expectError(response, 'rate_limited')
    expect(ApiErrorSchema.parse(response.body).error.details).toEqual({ retryAfter })
    expect(response.headers.get('retry-after')).toBe(String(retryAfter))
  }

  it('login: the 5th failure in 10 minutes locks the username (any case) for 10 minutes', async () => {
    const name = `Locked${++serial}`
    await registerUser(origin(), name)
    for (let i = 0; i < 5; i++) {
      clock.now = new Date(T0.getTime() + i * 1000)
      expectError(await login(origin(), name, { password: 'wrong-password', ip: `198.51.100.${i}` }), 'invalid_credentials')
    }
    // Locked from the 5th failure (T0 + 4 s): even the right password, from any IP, in any case.
    clock.now = new Date(T0.getTime() + 4000)
    expectRateLimited(await login(origin(), name), 600)
    clock.now = new Date(T0.getTime() + 4000 + 5 * 60 * 1000 + 500)
    expectRateLimited(await login(origin(), name.toLowerCase(), { ip: '203.0.113.9' }), 300)
    // Other names are not affected.
    const other = uniqueName('free')
    await registerUser(origin(), other)
    session(await login(origin(), other))
    // The lock ends 10 minutes after it started.
    clock.now = new Date(T0.getTime() + 4000 + RATE_WINDOW_MS)
    session(await login(origin(), name))
  })

  it('login: unknown usernames lock the same way (no hint that a name exists)', async () => {
    const ghost = uniqueName('ghost')
    for (let i = 0; i < 5; i++)
      expectError(await login(origin(), ghost), 'invalid_credentials')
    expectRateLimited(await login(origin(), ghost), 600)
  })

  it('login: failures older than 10 minutes do not count, and a success clears the count', async () => {
    const name = uniqueName('slow')
    await registerUser(origin(), name)
    for (let i = 0; i < 4; i++)
      expectError(await login(origin(), name, { password: 'wrong-password' }), 'invalid_credentials')
    clock.now = new Date(T0.getTime() + RATE_WINDOW_MS)
    expectError(await login(origin(), name, { password: 'wrong-password' }), 'invalid_credentials')
    session(await login(origin(), name))
    for (let i = 0; i < 4; i++)
      expectError(await login(origin(), name, { password: 'wrong-password' }), 'invalid_credentials')
    session(await login(origin(), name))
  })

  it('register: 5 per client IP in 10 minutes without a key; per key with one', async () => {
    const ip = '192.0.2.10'
    for (let i = 0; i < 4; i++) {
      clock.now = new Date(T0.getTime() + i * 60_000)
      await registerUser(origin(), uniqueName('ip'), { ip })
    }
    // A taken name counts as an attempt too.
    const taken = uniqueName('ip')
    await registerUser(origin(), taken, { ip: '192.0.2.99' })
    expectError(await send(origin(), 'POST', '/v1/auth/register', {
      ip,
      body: { username: taken, password: PASSWORD },
    }), 'username_taken')
    // 6th attempt at T0 + 5 min: the oldest (T0) leaves the window at T0 + 10 min.
    clock.now = new Date(T0.getTime() + 5 * 60_000)
    expectRateLimited(await send(origin(), 'POST', '/v1/auth/register', {
      ip,
      body: {
        username: uniqueName('ip'),
        password: PASSWORD,
      },
    }), 300)
    // Another IP, or a key (counted per key, not per IP), still registers.
    await registerUser(origin(), uniqueName('ip'), { ip: '192.0.2.11' })
    await registerUser(origin(), uniqueName('ip'), {
      ip,
      fingerprint: fakeFingerprint(`rl-${randomUUID()}`),
    })
    // Once T0 leaves the window, one more attempt is allowed.
    clock.now = new Date(T0.getTime() + RATE_WINDOW_MS)
    await registerUser(origin(), uniqueName('ip'), { ip })
    expectRateLimited(await send(origin(), 'POST', '/v1/auth/register', {
      ip,
      body: {
        username: uniqueName('ip'),
        password: PASSWORD,
      },
    }), 60)
  })

  it('register: 5 per key in 10 minutes, whatever the IP', async () => {
    const key = fakeFingerprint(`rl-key-${randomUUID()}`)
    for (let i = 0; i < 5; i++)
      await registerUser(origin(), uniqueName('k'), { fingerprint: key })
    expectRateLimited(await send(origin(), 'POST', '/v1/auth/register', {
      fingerprint: key,
      body: {
        username: uniqueName('k'),
        password: PASSWORD,
      },
    }), 600)
  })
})

describe('rate limiters (unit)', () => {
  it('attemptLimiter allows 5 per window and reports the wait for the oldest', () => {
    const limiter = new AttemptLimiter()
    for (let i = 0; i < 5; i++)
      expect(limiter.take('k', new Date(T0.getTime() + i * 1000))).toBeNull()
    expect(limiter.take('k', new Date(T0.getTime() + 5000))).toBe(595)
    expect(limiter.take('other', T0)).toBeNull()
    expect(limiter.take('k', new Date(T0.getTime() + RATE_WINDOW_MS))).toBeNull()
  })

  it('failureLimiter locks at the 5th failure and forgets on clear', () => {
    const limiter = new FailureLimiter()
    for (let i = 0; i < 4; i++)
      limiter.fail('k', T0)
    expect(limiter.lockedFor('k', T0)).toBeNull()
    limiter.fail('k', T0)
    expect(limiter.lockedFor('k', new Date(T0.getTime() + 1))).toBe(600)
    expect(limiter.lockedFor('k', new Date(T0.getTime() + RATE_WINDOW_MS))).toBeNull()
    limiter.fail('j', T0)
    limiter.clear('j')
    expect(limiter.size).toBe(1)
  })
})

describe('secrets stay out of answers and logs', () => {
  const { origin } = useApp()
  const logged: string[] = []

  beforeEach(() => {
    logged.length = 0
    const capture = (chunk: unknown): boolean => {
      logged.push(String(chunk))
      return true
    }
    vi.spyOn(process.stdout, 'write').mockImplementation(capture)
    vi.spyOn(process.stderr, 'write').mockImplementation(capture)
    for (const method of [
      'log',
      'info',
      'warn',
      'error',
      'debug',
    ] as const) {
      vi.spyOn(console, method).mockImplementation((...args: unknown[]) => {
        logged.push(args.map(String).join(' '))
      })
    }
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('no password, token, token hash, password hash or full fingerprint is answered or logged', async () => {
    const password = `Sup3r-secret-${randomUUID()}`
    const key = fakeFingerprint(`secret-${randomUUID()}`)
    const name = uniqueName('secret')
    const bodies: unknown[] = []
    const record = (response: JsonResponse): JsonResponse => {
      bodies.push(response.body)
      return response
    }
    const reg = session(record(await send(origin(), 'POST', '/v1/auth/register', {
      fingerprint: key,
      body: { username: name, password, remember: true },
    })), 201)
    record(await login(origin(), name, { password: `${password}-wrong`, fingerprint: key }))
    record(await login(origin(), uniqueName('nobody'), { password }))
    const logged2 = session(record(await login(origin(), name, { password, fingerprint: key, remember: true })))
    const resumed = session(record(await send(origin(), 'POST', '/v1/auth/resume', { fingerprint: key })))
    record(await me(origin(), resumed.token, key))
    record(await send(origin(), 'POST', '/v1/auth/logout', { token: resumed.token, fingerprint: key }))

    const user = await db.prisma.user.findUniqueOrThrow({
      where: { id: reg.user.id },
    })
    const tokens = [reg.token, logged2.token, resumed.token]
    const answers = JSON.stringify(bodies)
    const logs = logged.join('\n')
    expect(logs).toContain('[auth]')
    expect(logs).toContain(shortFingerprint(key))
    for (const secret of [
      password,
      user.passwordHash,
      ...tokens.map(hashSessionToken),
    ]) {
      expect(answers).not.toContain(secret)
      expect(logs).not.toContain(secret)
    }
    for (const token of tokens)
      expect(logs).not.toContain(token)
    expect(logs).not.toContain(key)
    expect(answers).not.toMatch(/passwordHash|tokenHash|argon2id/)
  })
})

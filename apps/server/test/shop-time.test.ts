import { Buffer } from 'node:buffer'
import { describe, expect, it } from 'vitest'
import { hashPassword, PASSWORD_ALGORITHM, verifyPassword } from '../src/auth/password.ts'
import { bearerToken, hashSessionToken, newSessionToken } from '../src/auth/tokens.ts'
import { defaultWeight } from '../src/services/products.ts'
import { etaOf, orderNumber, orderStatus, rememberStatus, shopDate, shopYymmdd } from '../src/services/shop-time.ts'

describe('shop time (Asia/Shanghai)', () => {
  it('dates follow the shop calendar, not UTC', () => {
    expect(shopDate(new Date('2026-11-22T13:04:00Z'))).toBe('2026-11-22')
    expect(shopDate(new Date('2026-11-22T16:00:00Z'))).toBe('2026-11-23')
    expect(shopDate(new Date('2026-12-31T15:59:59Z'))).toBe('2026-12-31')
    expect(shopDate(new Date('2026-12-31T16:00:00Z'))).toBe('2027-01-01')
    expect(shopYymmdd(new Date('2026-11-22T13:04:00Z'))).toBe('261122')
  })

  it('order numbers: RT + yymmdd + 4 digits', () => {
    expect(orderNumber(new Date('2026-11-22T13:04:00Z'), 417)).toBe('RT2611220417')
    expect(orderNumber(new Date('2026-11-22T13:04:00Z'), 9999)).toBe('RT2611229999')
    expect(() => orderNumber(new Date(), 10_000)).toThrow(RangeError)
  })

  it('status advances with time: 24 h roasting, shipping until the ETA', () => {
    const placed = new Date('2026-11-22T13:04:00Z')
    const eta = etaOf(placed)
    expect(eta.toISOString()).toBe('2026-11-25T13:04:00.000Z')
    expect(orderStatus(placed, eta, placed)).toBe('roasting')
    expect(orderStatus(placed, eta, new Date('2026-11-23T13:03:59Z'))).toBe('roasting')
    expect(orderStatus(placed, eta, new Date('2026-11-23T13:04:00Z'))).toBe('shipping')
    expect(orderStatus(placed, eta, new Date('2026-11-25T13:03:59Z'))).toBe('shipping')
    expect(orderStatus(placed, eta, eta)).toBe('delivered')
  })

  it('remember status', () => {
    const now = new Date('2026-11-22T13:04:00Z')
    const binding = {
      userId: 1,
      expiresAt: new Date('2026-12-22T13:04:00Z'),
    }
    expect(rememberStatus(null, binding, 1, now)).toEqual({ state: 'no_key', expiresAt: null, expiresOn: null })
    expect(rememberStatus('SHA256:x', null, 1, now).state).toBe('not_remembered')
    expect(rememberStatus('SHA256:x', binding, 2, now).state).toBe('not_remembered')
    expect(rememberStatus('SHA256:x', binding, 1, binding.expiresAt).state).toBe('not_remembered')
    expect(rememberStatus('SHA256:x', binding, 1, now)).toEqual({ state: 'remembered', expiresAt: '2026-12-22T13:04:00.000Z', expiresOn: '2026-12-22' })
  })
})

describe('products', () => {
  it('default weight is 250g, else the first in stock', () => {
    expect(defaultWeight([])).toBe('250g')
    expect(defaultWeight(['500g', '1kg'])).toBe('250g')
    expect(defaultWeight(['250g'])).toBe('100g')
    expect(defaultWeight(['100g', '250g'])).toBe('500g')
  })
})

describe('passwords and tokens', () => {
  it('uses argon2id on this Node and verifies', async () => {
    expect(PASSWORD_ALGORITHM).toBe('argon2id')
    const hash = await hashPassword('espresso42')
    expect(hash.startsWith('argon2id$m=19456,t=2,p=1$')).toBe(true)
    expect(hash).not.toContain('espresso42')
    expect(await verifyPassword('espresso42', hash)).toBe(true)
    expect(await verifyPassword('espresso43', hash)).toBe(false)
    expect(await verifyPassword('espresso42', 'garbage')).toBe(false)
    expect(await hashPassword('espresso42')).not.toBe(hash)
  })

  it('verifies scrypt hashes (the fallback format)', async () => {
    const { scryptSync } = await import('node:crypto')
    const salt = Buffer.from('0123456789abcdef')
    const derived = scryptSync('espresso42', salt, 32, {
      N: 32_768,
      r: 8,
      p: 1,
      maxmem: 64 * 1024 * 1024,
    })
    const stored = [
      'scrypt',
      'N=32768,r=8,p=1',
      salt.toString('base64'),
      derived.toString('base64'),
    ].join('$')
    expect(await verifyPassword('espresso42', stored)).toBe(true)
    expect(await verifyPassword('nope', stored)).toBe(false)
  })

  it('tokens are 43 base64url characters, stored as SHA-256 hex', () => {
    const token = newSessionToken()
    expect(token).toMatch(/^[\w-]{43}$/)
    expect(hashSessionToken(token)).toMatch(/^[0-9a-f]{64}$/)
    expect(bearerToken(`Bearer ${token}`)).toBe(token)
    expect(bearerToken(`Basic ${token}`)).toBeNull()
    expect(bearerToken('Bearer short')).toBeNull()
    expect(bearerToken(undefined)).toBeNull()
  })
})

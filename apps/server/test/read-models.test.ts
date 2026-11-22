// The shared read models (cart-view, address-book, members) against a real database.
import type { Cart } from '@coffee/contract'
import type { TestDatabase } from './helpers.ts'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { loadAddressList, loadOrderAddress } from '../src/services/address-book.ts'
import { loadCart } from '../src/services/cart-view.ts'
import { loadRememberStatus, toUser, usernameKey } from '../src/services/members.ts'
import { FINGERPRINT, NOW } from './conformance/types.ts'
import { createTestDatabase, readFixture } from './helpers.ts'
import { addAddressRow, addCartRow, createMember, rememberKey } from './support/members.ts'

let db: TestDatabase

beforeAll(async () => {
  db = await createTestDatabase()
}, 60_000)

afterAll(async () => {
  await db?.cleanup()
})

describe('loadCart', () => {
  it('prices the sample cart like the fixture', async () => {
    const m = await createMember(db.prisma, { now: NOW })
    await addCartRow(db.prisma, m.id, {
      productId: 'pinkb',
      weight: '250g',
      grind: 'PO',
      pack: 'STD',
      quantity: 1,
    })
    await addCartRow(db.prisma, m.id, {
      productId: 'night',
      weight: '500g',
      grind: 'ES',
      pack: 'STD',
      quantity: 1,
    })
    await addCartRow(db.prisma, m.id, {
      productId: 'geisha',
      weight: '100g',
      grind: 'WB',
      pack: 'GIFT',
      quantity: 1,
    })
    await addAddressRow(db.prisma, m.id)
    const cart = await loadCart(db.prisma, m.id)
    const expected = readFixture('v1/cart/cart.json') as Cart
    expect(cart.lines).toEqual(expected.lines)
    expect(cart.totals).toEqual(expected.totals)
    expect({ ...cart.address, id: 0 }).toEqual({ ...expected.address, id: 0 })
  })

  it('applies an active coupon and flags problems', async () => {
    const m = await createMember(db.prisma, { now: NOW })
    await addCartRow(db.prisma, m.id, {
      productId: 'yirga',
      weight: '250g',
      grind: 'WB',
      pack: 'STD',
      quantity: 1,
    })
    await db.prisma.cart.update({
      where: { userId: m.id },
      data: { couponCode: 'TTY10' },
    })
    const cart = await loadCart(db.prisma, m.id)
    expect(cart.totals).toEqual((readFixture('v1/cart/cart-coupon.json') as Cart).totals)
    expect(cart.coupon).toEqual({ code: 'TTY10', description: '全单 9 折' })
    expect(cart.address).toBeNull()

    const other = await createMember(db.prisma, { now: NOW })
    await addCartRow(db.prisma, other.id, {
      productId: 'pinkb',
      weight: '1kg',
      grind: 'WB',
      pack: 'STD',
      quantity: 1,
    })
    await addCartRow(db.prisma, other.id, {
      productId: 'geisha',
      weight: '100g',
      grind: 'WB',
      pack: 'STD',
      quantity: 7,
    })
    const flagged = await loadCart(db.prisma, other.id)
    expect(flagged.lines.map(l => l.problem)).toEqual(['sold_out', 'out_of_stock'])
    expect(flagged.lines[1]?.maxQuantity).toBe(6)
  })

  it('works inside a transaction', async () => {
    const m = await createMember(db.prisma, { now: NOW })
    const cart = await db.prisma.$transaction(tx => loadCart(tx, m.id))
    expect(cart.lines).toEqual([])
  })
})

describe('address book', () => {
  it('the order address is the chosen one, else the default', async () => {
    const m = await createMember(db.prisma, { now: NOW })
    const home = await addAddressRow(db.prisma, m.id)
    const work = await addAddressRow(db.prisma, m.id, { tag: 'work' })
    expect(await loadAddressList(db.prisma, m.id)).toMatchObject({ defaultAddressId: home, orderAddressId: home })
    await db.prisma.cart.update({
      where: { userId: m.id },
      data: { addressId: work },
    })
    expect((await loadOrderAddress(db.prisma, m.id))?.id).toBe(work)
    await db.prisma.address.delete({
      where: { id: work },
    })
    expect((await db.prisma.cart.findUniqueOrThrow({
      where: { userId: m.id },
    })).addressId).toBeNull()
    expect(await loadAddressList(db.prisma, m.id)).toMatchObject({ defaultAddressId: home, orderAddressId: home })
  })

  it('masks phones and keeps them whole', async () => {
    const m = await createMember(db.prisma, { now: NOW })
    await addAddressRow(db.prisma, m.id)
    await addAddressRow(db.prisma, m.id, { phone: null })
    const list = await loadAddressList(db.prisma, m.id)
    expect(list.items.map(a => [a.phone, a.phoneMasked])).toEqual([
      ['13800005521', '138 **** 5521'],
      [null, null],
    ])
  })
})

describe('members', () => {
  it('usernames are unique ignoring ASCII case only', () => {
    expect(usernameKey('Lin_Xiao林')).toBe('lin_xiao林')
  })

  it('maps users and the remember status', async () => {
    const m = await createMember(db.prisma, { username: 'Remember_Me', now: NOW })
    const row = await db.prisma.user.findUniqueOrThrow({
      where: { id: m.id },
    })
    expect(toUser(row)).toEqual({
      id: m.id,
      username: 'Remember_Me',
      createdAt: NOW.toISOString(),
      tier: (readFixture('v1/auth/me.json') as { user: { tier: unknown } }).user.tier,
    })
    expect((await loadRememberStatus(db.prisma, m.id, null, NOW)).state).toBe('no_key')
    expect((await loadRememberStatus(db.prisma, m.id, FINGERPRINT, NOW)).state).toBe('not_remembered')
    await rememberKey(db.prisma, m.id, FINGERPRINT, NOW)
    expect(await loadRememberStatus(db.prisma, m.id, FINGERPRINT, NOW)).toEqual({
      state: 'remembered',
      expiresAt: '2026-12-22T13:04:00.000Z',
      expiresOn: '2026-12-22',
    })
  })
})

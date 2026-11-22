// The address book through the real app: every validation code, the limit, and the rules
// that keep exactly one default (P-addr-1…4).
import type { AddressList, AddressSaved, ApiError, ErrorCode } from '@coffee/contract'
import type { TestDatabase, TestServer } from './helpers.ts'
import type { TestMember } from './support/members.ts'
import { ERRORS, MAX_ADDRESSES } from '@coffee/contract'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { NOW } from './conformance/types.ts'
import { call, createTestDatabase, listenApp } from './helpers.ts'
import { addAddressRow, createMember } from './support/members.ts'

const HOME = {
  recipient: '林晓',
  phone: '13800005521',
  region: '上海市 徐汇区',
  detail: '天平路 88 弄 3 号 602 室',
  tag: 'home',
}

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

async function create(m: TestMember, body: unknown): Promise<AddressSaved> {
  const res = await call(server.origin, 'POST', '/v1/addresses', { token: m.token, body })
  expect(res.status, JSON.stringify(res.body)).toBe(201)
  return res.body as AddressSaved
}

async function list(m: TestMember): Promise<AddressList> {
  const res = await call(server.origin, 'GET', '/v1/addresses', { token: m.token })
  expect(res.status).toBe(200)
  return res.body as AddressList
}

function expectError(res: { status: number, body: unknown }, code: ErrorCode): void {
  expect((res.body as ApiError).error.code, JSON.stringify(res.body)).toBe(code)
  expect(res.status).toBe(ERRORS[code].status)
}

function defaults(book: AddressList): number[] {
  return book.items.filter(a => a.isDefault).map(a => a.id)
}

describe('validation', () => {
  const cases: [string, Record<string, unknown>, ErrorCode][] = [
    [
      'missing recipient',
      { ...HOME, recipient: undefined },
      'recipient_required',
    ],
    [
      'blank recipient',
      { ...HOME, recipient: '   ' },
      'recipient_required',
    ],
    [
      'recipient not a string',
      { ...HOME, recipient: 7 },
      'recipient_required',
    ],
    [
      'recipient over 20',
      {
        ...HOME,
        recipient: '林'.repeat(21),
      },
      'text_too_long',
    ],
    [
      'missing region',
      { ...HOME, region: undefined },
      'region_required',
    ],
    [
      'blank region',
      { ...HOME, region: ' ' },
      'region_required',
    ],
    [
      'region over 40',
      {
        ...HOME,
        region: '区'.repeat(41),
      },
      'text_too_long',
    ],
    [
      'missing detail',
      { ...HOME, detail: undefined },
      'detail_required',
    ],
    [
      'blank detail',
      { ...HOME, detail: '' },
      'detail_required',
    ],
    [
      'detail over 80',
      {
        ...HOME,
        detail: '号'.repeat(81),
      },
      'text_too_long',
    ],
    [
      'phone too short',
      { ...HOME, phone: '1380000552' },
      'phone_invalid',
    ],
    [
      'phone too long',
      { ...HOME, phone: '138000055210' },
      'phone_invalid',
    ],
    [
      'phone with letters',
      { ...HOME, phone: '1380000552a' },
      'phone_invalid',
    ],
    [
      'phone with spaces',
      { ...HOME, phone: '138 0000 5521' },
      'phone_invalid',
    ],
    [
      'phone not a string',
      { ...HOME, phone: 13800005521 },
      'phone_invalid',
    ],
    // First failure wins in the order 收货人 → 所在地区 → 详细地址 → 手机号.
    [
      'recipient before region',
      { ...HOME, recipient: '', region: '' },
      'recipient_required',
    ],
    [
      'region before detail',
      { ...HOME, region: '', detail: '' },
      'region_required',
    ],
    [
      'detail before phone',
      { ...HOME, detail: '', phone: '1' },
      'detail_required',
    ],
    [
      'too long before a later missing field',
      {
        ...HOME,
        recipient: '林'.repeat(21),
        region: '',
      },
      'text_too_long',
    ],
  ]
  for (const [name, body, code] of cases) {
    it(`create: ${name} → ${code}`, async () => {
      const m = await member()
      expectError(await call(server.origin, 'POST', '/v1/addresses', { token: m.token, body }), code)
      expect(await list(m)).toEqual({ items: [], defaultAddressId: null, orderAddressId: null })
    })
  }

  it('update validates the same way and changes nothing', async () => {
    const m = await member()
    const { address } = await create(m, HOME)
    expectError(await call(server.origin, 'PUT', `/v1/addresses/${address.id}`, {
      token: m.token,
      body: { ...HOME, phone: '123' },
    }), 'phone_invalid')
    expectError(await call(server.origin, 'PUT', `/v1/addresses/${address.id}`, {
      token: m.token,
      body: {
        ...HOME,
        detail: '号'.repeat(81),
      },
    }), 'text_too_long')
    expect((await list(m)).items).toEqual([address])
  })

  it('accepts the limits exactly, trims text and defaults the tag to home', async () => {
    const m = await member()
    const { address } = await create(m, {
      recipient: ` ${'林'.repeat(20)} `,
      region: '区'.repeat(40),
      detail: '号'.repeat(80),
    })
    expect(address).toMatchObject({
      recipient: '林'.repeat(20),
      phone: null,
      phoneMasked: null,
      tag: 'home',
    })
  })

  it('unknown fields are validation_failed', async () => {
    const m = await member()
    const res = await call(server.origin, 'POST', '/v1/addresses', {
      token: m.token,
      body: { ...HOME, zip: '200000' },
    })
    expectError(res, 'validation_failed')
  })

  it('needs a session', async () => {
    expectError(await call(server.origin, 'GET', '/v1/addresses'), 'login_required')
  })
})

describe('phones', () => {
  it('stores the full number and masks it for display', async () => {
    const m = await member()
    const { address } = await create(m, HOME)
    expect(address.phone).toBe('13800005521')
    expect(address.phoneMasked).toBe('138 **** 5521')
    const row = await db.prisma.address.findUniqueOrThrow({
      where: { id: address.id },
    })
    expect(row.phone).toBe('13800005521')
  })

  it('"", null and absent all mean no phone', async () => {
    const m = await member()
    for (const phone of ['', null, undefined]) {
      const { address } = await create(m, { ...HOME, phone })
      expect(address.phone).toBeNull()
      expect(address.phoneMasked).toBeNull()
    }
  })
})

describe('the limit', () => {
  it(`keeps at most ${MAX_ADDRESSES} addresses`, async () => {
    const m = await member()
    for (let i = 0; i < MAX_ADDRESSES - 1; i++)
      await addAddressRow(db.prisma, m.id)
    await create(m, HOME)
    expectError(await call(server.origin, 'POST', '/v1/addresses', { token: m.token, body: HOME }), 'address_limit')
    expect((await list(m)).items).toHaveLength(MAX_ADDRESSES)
    // An invalid 21st reports its field first: input validation comes before the limit.
    expectError(await call(server.origin, 'POST', '/v1/addresses', {
      token: m.token,
      body: { ...HOME, recipient: '' },
    }), 'recipient_required')
  })
})

describe('exactly one default', () => {
  it('the first address becomes the default, later ones do not', async () => {
    const m = await member()
    const first = await create(m, { ...HOME, isDefault: false })
    expect(first.address.isDefault).toBe(true)
    expect(first.book).toMatchObject({ defaultAddressId: first.address.id, orderAddressId: first.address.id })
    const second = await create(m, { ...HOME, tag: 'work' })
    expect(second.address.isDefault).toBe(false)
    expect(defaults(second.book)).toEqual([first.address.id])
  })

  it('creating with isDefault moves the default', async () => {
    const m = await member()
    const first = await create(m, HOME)
    const second = await create(m, { ...HOME, tag: 'school', isDefault: true })
    expect(second.address.isDefault).toBe(true)
    expect(defaults(second.book)).toEqual([second.address.id])
    expect(second.book.items.map(a => a.id)).toEqual([first.address.id, second.address.id])
  })

  it('unticking 设为默认 on the default keeps it the default', async () => {
    const m = await member()
    const first = await create(m, HOME)
    await create(m, { ...HOME, tag: 'work' })
    const res = await call(server.origin, 'PUT', `/v1/addresses/${first.address.id}`, {
      token: m.token,
      body: { ...HOME, detail: '新地址 1 号', isDefault: false },
    })
    expect(res.status).toBe(200)
    const saved = res.body as AddressSaved
    expect(saved.address).toMatchObject({ detail: '新地址 1 号', isDefault: true })
    expect(defaults(saved.book)).toEqual([first.address.id])
  })

  it('updating another address with isDefault moves the default; without it does not', async () => {
    const m = await member()
    const first = await create(m, HOME)
    const second = await create(m, { ...HOME, tag: 'work' })
    let res = await call(server.origin, 'PUT', `/v1/addresses/${second.address.id}`, {
      token: m.token,
      body: { ...HOME, tag: 'work' },
    })
    expect(defaults((res.body as AddressSaved).book)).toEqual([first.address.id])
    res = await call(server.origin, 'PUT', `/v1/addresses/${second.address.id}`, {
      token: m.token,
      body: { ...HOME, isDefault: true },
    })
    const saved = res.body as AddressSaved
    expect(saved.address).toMatchObject({ isDefault: true, tag: 'home' })
    expect(defaults(saved.book)).toEqual([second.address.id])
  })

  it('setDefault moves it and is a no-op on the default', async () => {
    const m = await member()
    const first = await create(m, HOME)
    const second = await create(m, HOME)
    let res = await call(server.origin, 'POST', `/v1/addresses/${second.address.id}/default`, { token: m.token })
    expect(res.status).toBe(200)
    expect(defaults(res.body as AddressList)).toEqual([second.address.id])
    res = await call(server.origin, 'POST', `/v1/addresses/${second.address.id}/default`, { token: m.token })
    expect(defaults(res.body as AddressList)).toEqual([second.address.id])
    expect((res.body as AddressList).defaultAddressId).toBe(second.address.id)
    expect((res.body as AddressList).items.map(a => a.id)).toEqual([first.address.id, second.address.id])
  })

  it('deleting the default promotes the oldest remaining address', async () => {
    const m = await member()
    const a = await create(m, HOME)
    const b = await create(m, HOME)
    const c = await create(m, { ...HOME, isDefault: true })
    // Default is c; deleting it promotes a (oldest), not b.
    const res = await call(server.origin, 'DELETE', `/v1/addresses/${c.address.id}`, { token: m.token })
    expect(res.status).toBe(200)
    const book = res.body as AddressList
    expect(book.items.map(x => x.id)).toEqual([a.address.id, b.address.id])
    expect(defaults(book)).toEqual([a.address.id])
    expect(book.defaultAddressId).toBe(a.address.id)
  })

  it('deleting another address keeps the default; deleting the last empties the book', async () => {
    const m = await member()
    const a = await create(m, HOME)
    const b = await create(m, HOME)
    let book = (await call(server.origin, 'DELETE', `/v1/addresses/${b.address.id}`, { token: m.token })).body as AddressList
    expect(defaults(book)).toEqual([a.address.id])
    book = (await call(server.origin, 'DELETE', `/v1/addresses/${a.address.id}`, { token: m.token })).body as AddressList
    expect(book).toEqual({ items: [], defaultAddressId: null, orderAddressId: null })
    // The next address is the default again.
    expect((await create(m, HOME)).address.isDefault).toBe(true)
  })
})

describe('the order address', () => {
  it('deleting the address the cart points at clears it (falls back to the default)', async () => {
    const m = await member()
    const a = await create(m, HOME)
    const b = await create(m, { ...HOME, tag: 'work' })
    await db.prisma.cart.update({
      where: { userId: m.id },
      data: { addressId: b.address.id },
    })
    expect((await list(m)).orderAddressId).toBe(b.address.id)
    const book = (await call(server.origin, 'DELETE', `/v1/addresses/${b.address.id}`, { token: m.token })).body as AddressList
    expect(book.orderAddressId).toBe(a.address.id)
    const cart = await db.prisma.cart.findUniqueOrThrow({
      where: { userId: m.id },
    })
    expect(cart.addressId).toBeNull()
  })

  it('moving the default does not move a chosen order address', async () => {
    const m = await member()
    await create(m, HOME)
    const b = await create(m, HOME)
    const c = await create(m, HOME)
    await db.prisma.cart.update({
      where: { userId: m.id },
      data: { addressId: b.address.id },
    })
    const book = (await call(server.origin, 'POST', `/v1/addresses/${c.address.id}/default`, { token: m.token })).body as AddressList
    expect(book.defaultAddressId).toBe(c.address.id)
    expect(book.orderAddressId).toBe(b.address.id)
  })
})

describe('isolation', () => {
  it('a member sees and changes only their own addresses', async () => {
    const owner = await member()
    const other = await member()
    const { address } = await create(owner, HOME)
    await create(other, { ...HOME, recipient: '陈默' })
    expect((await list(other)).items.map(a => a.recipient)).toEqual(['陈默'])
    for (const [method, path, body] of [
      ['PUT', `/v1/addresses/${address.id}`, HOME],
      ['DELETE', `/v1/addresses/${address.id}`, undefined],
      ['POST', `/v1/addresses/${address.id}/default`, undefined],
    ] as const) {
      const res = await call(server.origin, method, path, {
        token: other.token,
        ...(body
          ? { body }
          : {}),
      })
      expectError(res, 'not_found')
    }
    expect((await list(owner)).items).toEqual([address])
  })

  it('an unknown id is not_found', async () => {
    const m = await member()
    expectError(await call(server.origin, 'DELETE', '/v1/addresses/999999', { token: m.token }), 'not_found')
  })
})

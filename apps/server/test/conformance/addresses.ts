// Conformance cases of area `addresses` (owner: server-orders).
import type { AddressList, AddressSaved } from '@coffee/contract'
import type { ConformanceCase } from './types.ts'
import { expect } from 'vitest'
import { addAddressRow } from '../support/members.ts'

const HOME = {
  recipient: '林晓',
  phone: '13800005521',
  region: '上海市 徐汇区',
  detail: '天平路 88 弄 3 号 602 室',
  tag: 'home',
}

export const addressCases: ConformanceCase[] = [
  {
    operationId: 'listAddresses',
    name: 'a new member has none',
    request: async ({ member }) => ({ path: '/v1/addresses', token: (await member()).token }),
    check: body => expect(body).toEqual({ items: [], defaultAddressId: null, orderAddressId: null }),
  },
  {
    operationId: 'listAddresses',
    name: 'the default is the order address',
    request: async ({ member, prisma }) => {
      const m = await member()
      await addAddressRow(prisma, m.id)
      await addAddressRow(prisma, m.id, { tag: 'work' })
      return { path: '/v1/addresses', token: m.token }
    },
    check: (body) => {
      const list = body as AddressList
      expect(list.items).toHaveLength(2)
      expect(list.items[0]?.phoneMasked).toBe('138 **** 5521')
      expect(list.defaultAddressId).toBe(list.items[0]?.id)
      expect(list.orderAddressId).toBe(list.items[0]?.id)
    },
  },
  {
    operationId: 'createAddress',
    name: 'the first address becomes the default',
    request: async ({ member }) => ({ path: '/v1/addresses', token: (await member()).token, body: HOME }),
    check: (body) => {
      const saved = body as AddressSaved
      expect(saved.address.isDefault).toBe(true)
      expect(saved.book.orderAddressId).toBe(saved.address.id)
    },
  },
  {
    operationId: 'createAddress',
    name: 'needs a recipient first',
    request: async ({ member }) => ({
      path: '/v1/addresses',
      token: (await member()).token,
      body: { ...HOME, recipient: ' ', region: '' },
    }),
    expectError: 'recipient_required',
  },
  {
    operationId: 'createAddress',
    name: 'a phone has 11 digits',
    request: async ({ member }) => ({
      path: '/v1/addresses',
      token: (await member()).token,
      body: { ...HOME, phone: '1380000' },
    }),
    expectError: 'phone_invalid',
  },
  {
    operationId: 'updateAddress',
    name: 'replaces the fields',
    request: async ({ member, prisma }) => {
      const m = await member()
      const id = await addAddressRow(prisma, m.id)
      return {
        path: `/v1/addresses/${id}`,
        token: m.token,
        body: { ...HOME, detail: '天平路 88 弄 3 号 702 室', phone: '' },
      }
    },
    check: (body) => {
      const saved = body as AddressSaved
      expect(saved.address.detail).toBe('天平路 88 弄 3 号 702 室')
      expect(saved.address.phone).toBeNull()
      expect(saved.address.isDefault).toBe(true)
    },
  },
  {
    operationId: 'updateAddress',
    name: 'someone else\'s address',
    request: async ({ member, prisma }) => {
      const other = await member()
      const id = await addAddressRow(prisma, other.id)
      return { path: `/v1/addresses/${id}`, token: (await member()).token, body: HOME }
    },
    expectError: 'not_found',
  },
  {
    operationId: 'deleteAddress',
    name: 'deleting the default promotes the oldest remaining one',
    request: async ({ member, prisma }) => {
      const m = await member()
      const first = await addAddressRow(prisma, m.id)
      await addAddressRow(prisma, m.id, { tag: 'work' })
      return { path: `/v1/addresses/${first}`, token: m.token }
    },
    check: (body) => {
      const list = body as AddressList
      expect(list.items).toHaveLength(1)
      expect(list.items[0]?.isDefault).toBe(true)
      expect(list.orderAddressId).toBe(list.items[0]?.id)
    },
  },
  {
    operationId: 'setDefaultAddress',
    name: 'moves the default',
    request: async ({ member, prisma }) => {
      const m = await member()
      await addAddressRow(prisma, m.id)
      const second = await addAddressRow(prisma, m.id, { tag: 'school' })
      return { path: `/v1/addresses/${second}/default`, token: m.token }
    },
    check: (body) => {
      const list = body as AddressList
      expect(list.defaultAddressId).toBe(list.items[1]?.id)
      expect(list.items.filter(a => a.isDefault)).toHaveLength(1)
    },
  },
]

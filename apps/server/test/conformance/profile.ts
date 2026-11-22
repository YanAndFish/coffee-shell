// Conformance cases of area `profile` (owner: server-orders).
import type { Profile, Settings } from '@coffee/contract'
import type { ConformanceCase } from './types.ts'
import { expect } from 'vitest'
import { readFixture } from '../helpers.ts'
import { addAddressRow, addOrderRow, rememberKey } from '../support/members.ts'
import { FINGERPRINT, NOW } from './types.ts'

const DAY = 24 * 60 * 60 * 1000

export const profileCases: ConformanceCase[] = [
  {
    operationId: 'getProfile',
    name: 'the example member (orders of the fixtures → the fixture taste)',
    request: async ({ member, prisma }) => {
      const m = await member()
      await addOrderRow(prisma, m.id, [
        {
          productId: 'geisha',
          weight: '100g',
          grind: 'PO',
          pack: 'STD',
          quantity: 1,
        },
      ], new Date(NOW.getTime() - 32 * DAY))
      await addOrderRow(prisma, m.id, [
        {
          productId: 'yirga',
          weight: '250g',
          grind: 'PO',
          pack: 'STD',
          quantity: 2,
        },
      ], new Date(NOW.getTime() - 16 * DAY))
      await addOrderRow(prisma, m.id, [
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
      ], new Date(NOW.getTime() - 2 * DAY))
      await addOrderRow(prisma, m.id, [
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
      ], NOW)
      for (let i = 0; i < 3; i++)
        await addAddressRow(prisma, m.id)
      await rememberKey(prisma, m.id, FINGERPRINT, new Date('2026-11-22T13:04:00.000Z'))
      return { path: '/v1/profile', token: m.token, fingerprint: FINGERPRINT }
    },
    check: (body) => {
      const profile = body as Profile
      const expected = readFixture('v1/profile/profile.json') as Profile
      expect(profile.stats).toEqual(expected.stats)
      expect(profile.taste).toEqual(expected.taste)
      expect(profile.recommendations).toEqual(expected.recommendations)
      expect(profile.remembered).toEqual(expected.remembered)
    },
  },
  {
    operationId: 'getProfile',
    name: 'a new member without a key',
    request: async ({ member }) => ({ path: '/v1/profile', token: (await member()).token }),
    check: (body) => {
      const profile = body as Profile
      const expected = readFixture('v1/profile/profile-no-key.json') as Profile
      expect({ ...profile, user: null }).toEqual({ ...expected, user: null })
    },
  },
  {
    operationId: 'getSettings',
    name: 'both on for a new account',
    request: async ({ member }) => ({ path: '/v1/settings', token: (await member()).token }),
    check: body => expect(body).toEqual(readFixture('v1/profile/settings.json')),
  },
  {
    operationId: 'updateSettings',
    name: 'turns the animations off',
    request: async ({ member }) => ({
      path: '/v1/settings',
      token: (await member()).token,
      body: { animations: false },
    }),
    check: body => expect(body as Settings).toEqual(readFixture('v1/profile/settings-animations-off.json')),
  },
  {
    operationId: 'updateSettings',
    name: 'needs a field',
    request: async ({ member }) => ({ path: '/v1/settings', token: (await member()).token, body: {} }),
    expectError: 'validation_failed',
  },
]

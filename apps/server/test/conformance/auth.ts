// Conformance cases of area `auth` (owner: server-auth). Add cases freely; keep at least one
// per endpoint.
import type { AuthSession, Me } from '@coffee/contract'
import type { ConformanceCase } from './types.ts'
import { expect } from 'vitest'
import { rememberKey } from '../support/members.ts'
import { fakeFingerprint, FINGERPRINT } from './types.ts'

export const authCases: ConformanceCase[] = [
  {
    operationId: 'register',
    name: 'creates a member and a session, remembering the key',
    request: async () => ({
      path: '/v1/auth/register',
      body: { username: '新会员_1', password: 'espresso42', remember: true },
      fingerprint: FINGERPRINT,
    }),
    check: (body) => {
      const session = body as AuthSession
      expect(session.user.username).toBe('新会员_1')
      expect(session.remembered.state).toBe('remembered')
    },
  },
  {
    operationId: 'register',
    name: 'refuses a taken username (ignoring ASCII case)',
    request: async ({ member }) => {
      await member({ username: 'Taken_Name' })
      return {
        path: '/v1/auth/register',
        body: { username: 'taken_name', password: 'espresso42' },
      }
    },
    expectError: 'username_taken',
  },
  {
    operationId: 'register',
    name: 'refuses a username against the rule',
    request: async () => ({
      path: '/v1/auth/register',
      body: { username: 'a', password: 'espresso42' },
    }),
    expectError: 'username_invalid',
  },
  {
    operationId: 'login',
    name: 'logs in without a key',
    request: async ({ member }) => {
      const m = await member({ username: 'login_ok' })
      return {
        path: '/v1/auth/login',
        body: { username: 'LOGIN_OK', password: m.password, remember: true },
      }
    },
    check: (body) => {
      const session = body as AuthSession
      expect(session.user.username).toBe('login_ok')
      expect(session.remembered.state).toBe('no_key')
    },
  },
  {
    operationId: 'login',
    name: 'refuses a wrong password',
    request: async ({ member }) => {
      await member({ username: 'login_bad' })
      return {
        path: '/v1/auth/login',
        body: { username: 'login_bad', password: 'not-the-one' },
      }
    },
    expectError: 'invalid_credentials',
  },
  {
    operationId: 'resume',
    name: 'resumes with a remembered key',
    request: async ({ member, prisma }) => {
      const m = await member()
      const fingerprint = fakeFingerprint('resume')
      await rememberKey(prisma, m.id, fingerprint, new Date('2026-11-20T00:00:00.000Z'))
      return { path: '/v1/auth/resume', fingerprint }
    },
    check: body => expect((body as AuthSession).remembered.state).toBe('remembered'),
  },
  {
    operationId: 'resume',
    name: 'an unknown key is not remembered',
    request: async () => ({
      path: '/v1/auth/resume',
      fingerprint: fakeFingerprint('nobody'),
    }),
    expectError: 'not_remembered',
  },
  {
    operationId: 'resume',
    name: 'needs a key',
    request: async () => ({ path: '/v1/auth/resume' }),
    expectError: 'fingerprint_required',
  },
  {
    operationId: 'logout',
    name: 'ends the session',
    request: async ({ member }) => ({ path: '/v1/auth/logout', token: (await member()).token, fingerprint: FINGERPRINT }),
  },
  {
    operationId: 'getMe',
    name: 'returns the member',
    request: async ({ member }) => {
      const m = await member({ username: 'me_myself' })
      return { path: '/v1/auth/me', token: m.token }
    },
    check: body => expect((body as Me).user.username).toBe('me_myself'),
  },
  {
    operationId: 'getMe',
    name: 'needs a session',
    request: async () => ({ path: '/v1/auth/me' }),
    expectError: 'login_required',
  },
]

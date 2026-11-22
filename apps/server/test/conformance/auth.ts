// Conformance cases of area `auth` (owner: server-auth). Add cases freely; keep at least one
// per endpoint.
import type { AuthSession, Me } from '@coffee/contract'
import type { ConformanceCase } from './types.ts'
import { expect } from 'vitest'
import { createSession, rememberKey } from '../support/members.ts'
import { fakeFingerprint, FINGERPRINT, NOW } from './types.ts'

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
    operationId: 'register',
    name: 'without a key the status is no_key',
    request: async () => ({
      path: '/v1/auth/register',
      body: { username: 'bean_lover', password: 'espresso42', remember: true },
    }),
    check: (body) => {
      const session = body as AuthSession
      expect(session.user.username).toBe('bean_lover')
      expect(session.user.createdAt).toBe(NOW.toISOString())
      expect(session.expiresAt).toBe('2026-11-29T13:04:00.000Z')
      expect(session.remembered).toEqual({ state: 'no_key', expiresAt: null, expiresOn: null })
    },
  },
  {
    operationId: 'register',
    name: 'with a key but without remember the key is not remembered',
    request: async () => ({
      path: '/v1/auth/register',
      body: { username: '豆子', password: 'espresso42' },
      fingerprint: fakeFingerprint('register-no-remember'),
    }),
    check: body => expect((body as AuthSession).remembered.state).toBe('not_remembered'),
  },
  {
    operationId: 'register',
    name: 'needs a username',
    request: async () => ({
      path: '/v1/auth/register',
      body: { username: '', password: 'espresso42' },
    }),
    expectError: 'username_required',
  },
  {
    operationId: 'register',
    name: 'refuses a short password',
    request: async () => ({
      path: '/v1/auth/register',
      body: { username: 'short_pw', password: '12345' },
    }),
    expectError: 'password_too_short',
  },
  {
    operationId: 'register',
    name: 'refuses a long password',
    request: async () => ({
      path: '/v1/auth/register',
      body: {
        username: 'long_pw',
        password: 'x'.repeat(129),
      },
    }),
    expectError: 'password_too_long',
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
    operationId: 'login',
    name: 'refuses an unknown username the same way',
    request: async () => ({
      path: '/v1/auth/login',
      body: { username: 'nobody_here', password: 'espresso42' },
    }),
    expectError: 'invalid_credentials',
  },
  {
    operationId: 'login',
    name: 'remembers the key for 30 days',
    request: async ({ member }) => {
      const m = await member({ username: '记住我' })
      return {
        path: '/v1/auth/login',
        body: { username: '记住我', password: m.password, remember: true },
        fingerprint: fakeFingerprint('login-remember'),
      }
    },
    check: async (body, { prisma }) => {
      const session = body as AuthSession
      expect(session.remembered).toEqual({
        state: 'remembered',
        expiresAt: '2026-12-22T13:04:00.000Z',
        expiresOn: '2026-12-22',
      })
      const row = await prisma.rememberedKey.findUniqueOrThrow({
        where: {
          fingerprint: fakeFingerprint('login-remember'),
        },
      })
      expect(row.userId).toBe(session.user.id)
    },
  },
  {
    operationId: 'login',
    name: 'needs a username',
    request: async () => ({
      path: '/v1/auth/login',
      body: { password: 'espresso42' },
    }),
    expectError: 'username_required',
  },
  {
    operationId: 'login',
    name: 'refuses a short password before looking anything up',
    request: async () => ({
      path: '/v1/auth/login',
      body: { username: 'login_ok', password: '123' },
    }),
    expectError: 'password_too_short',
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
    name: 'an expired binding is not remembered',
    request: async ({ member, prisma }) => {
      const m = await member()
      const fingerprint = fakeFingerprint('resume-expired')
      await rememberKey(prisma, m.id, fingerprint, new Date('2026-10-01T00:00:00.000Z'))
      return { path: '/v1/auth/resume', fingerprint }
    },
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
    operationId: 'logout',
    name: 'removes this key\'s binding and the session',
    request: async ({ member, prisma }) => {
      const m = await member()
      const fingerprint = fakeFingerprint('logout-unbind')
      await rememberKey(prisma, m.id, fingerprint, NOW)
      return { path: '/v1/auth/logout', token: m.token, fingerprint }
    },
    check: async (_body, { prisma }) => {
      expect(await prisma.rememberedKey.findUnique({
        where: {
          fingerprint: fakeFingerprint('logout-unbind'),
        },
      })).toBeNull()
    },
  },
  {
    operationId: 'logout',
    name: 'needs a session',
    request: async () => ({ path: '/v1/auth/logout' }),
    expectError: 'login_required',
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
    name: 'shows the remembered key',
    request: async ({ member, prisma }) => {
      const m = await member()
      const fingerprint = fakeFingerprint('me-remembered')
      await rememberKey(prisma, m.id, fingerprint, NOW)
      return { path: '/v1/auth/me', token: m.token, fingerprint }
    },
    check: body => expect((body as Me).remembered).toEqual({
      state: 'remembered',
      expiresAt: '2026-12-22T13:04:00.000Z',
      expiresOn: '2026-12-22',
    }),
  },
  {
    operationId: 'getMe',
    name: 'an expired session is session_expired',
    request: async ({ member, prisma }) => {
      const m = await member()
      const token = await createSession(prisma, m.id, new Date('2026-11-10T00:00:00.000Z'))
      return { path: '/v1/auth/me', token }
    },
    expectError: 'session_expired',
  },
  {
    operationId: 'getMe',
    name: 'needs a session',
    request: async () => ({ path: '/v1/auth/me' }),
    expectError: 'login_required',
  },
]

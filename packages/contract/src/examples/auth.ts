// Example sessions of the member 林晓; the clock reads 2026-11-22 21:04 (shop time).
import type { AuthSession, Me, RememberStatus, Tier, User } from '../auth.ts'
import type { Ok } from '../common.ts'

/** A fake token of the right shape (never a real one). */
export const EXAMPLE_TOKEN = 'ZGVtby1vbmx5LXJvYXN0LXR0eS1zZXNzaW9uLXRva2V'

/** A fake key fingerprint of the right shape. */
export const EXAMPLE_FINGERPRINT = 'SHA256:KpdRbDVLaISM29j1SiJqClWyHtE44getbFy7nACqWuo'

/** The static tier every member has for now (data.md#会员与个人统计). */
export const tierExample: Tier = {
  name: '熟豆会员',
  level: 3,
  points: 1280,
  nextLevel: 4,
  nextLevelPoints: 2000,
  pointsToNext: 720,
  progress: 64,
}

export const userExample: User = {
  id: 1,
  username: '林晓',
  createdAt: '2026-11-01T02:00:00.000Z',
  tier: tierExample,
}

export const rememberedExample: RememberStatus = {
  state: 'remembered',
  expiresAt: '2026-12-22T13:04:00.000Z',
  expiresOn: '2026-12-22',
}

export const notRememberedExample: RememberStatus = {
  state: 'not_remembered',
  expiresAt: null,
  expiresOn: null,
}

export const noKeyExample: RememberStatus = {
  state: 'no_key',
  expiresAt: null,
  expiresOn: null,
}

/** `POST /v1/auth/login` with `remember: true` on a connection with a key. */
export const loginExample: AuthSession = {
  token: EXAMPLE_TOKEN,
  expiresAt: '2026-11-29T13:04:00.000Z',
  user: userExample,
  remembered: rememberedExample,
}

/** `POST /v1/auth/login` on a connection without a key. */
export const loginNoKeyExample: AuthSession = {
  ...loginExample,
  remembered: noKeyExample,
}

/** `POST /v1/auth/register` (`remember: false`, key present). */
export const registerExample: AuthSession = {
  token: EXAMPLE_TOKEN,
  expiresAt: '2026-11-29T13:04:00.000Z',
  user: {
    id: 2,
    username: 'bean_lover',
    createdAt: '2026-11-22T13:04:00.000Z',
    tier: tierExample,
  },
  remembered: notRememberedExample,
}

/** `POST /v1/auth/resume`. */
export const resumeExample: AuthSession = loginExample

/** `POST /v1/auth/logout`. */
export const logoutExample: Ok = { ok: true }

/** `GET /v1/auth/me`. */
export const meExample: Me = {
  user: userExample,
  remembered: rememberedExample,
}

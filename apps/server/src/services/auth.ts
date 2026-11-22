// Area `auth` (owner: server-auth). One function per endpoint, typed from the contract; the
// rules are in each endpoint's `description` (packages/contract/src/auth.ts) and in
// docs/design/decisions.md (P-auth-1…6, P-api-1/2).
//
// Secrets never leave this module: passwords are only hashed / verified, tokens are returned
// once and stored as their SHA-256, and logs carry user ids, client IPs and shortened
// fingerprints only.
import type {
  AuthSession,
  getMe as getMeEndpoint,
  login as loginEndpoint,
  logout as logoutEndpoint,
  Me,
  Ok,
  register as registerEndpoint,
  RememberStatus,
  resume as resumeEndpoint,
} from '@coffee/contract'
import type { Context } from 'koa'
import type { AuthLimits } from '../auth/rate-limit.ts'
import type { Db } from '../db.ts'
import type { HandlerInput } from '../http/route.ts'
import { hashPassword, verifyPassword } from '../auth/password.ts'
import { hashSessionToken, newSessionToken, REMEMBER_TTL_MS, SESSION_TTL_MS } from '../auth/tokens.ts'
import { logInfo } from '../log.ts'
import { fail } from '../middleware/error-handler.ts'
import { ensureMemberRows, loadRememberStatus, toUser, usernameKey } from './members.ts'
import { instant, rememberStatus } from './shop-time.ts'

interface UserRow {
  id: number
  username: string
  createdAt: Date
}

const USER_SELECT = { id: true, username: true, createdAt: true } as const

/** `SHA256:KpdRbD…`: enough to tell keys apart in a log, never the whole fingerprint. */
export function shortFingerprint(fingerprint: string): string {
  return `${fingerprint.slice(0, 'SHA256:'.length + 6)}…`
}

function log(message: string): void {
  logInfo('auth', message)
}

let dummyHash: Promise<string> | undefined

/**
 * A hash no password was registered with: an unknown username is verified against it, so
 * both kinds of `invalid_credentials` cost the same time.
 */
function unknownUserHash(): Promise<string> {
  dummyHash ??= hashPassword('roast-tty: no such member')
  return dummyHash
}

/** `429 rate_limited` with `details.retryAfter` and the `Retry-After` header. */
function rateLimited(ctx: Context, retryAfter: number): never {
  ctx.set('Retry-After', String(retryAfter))
  fail('rate_limited', { retryAfter })
}

/** A new session of `userId` from `now` for 7 days; the token is returned, its hash stored. */
async function startSession(db: Db, userId: number, now: Date): Promise<{ token: string, expiresAt: Date }> {
  const token = newSessionToken()
  const expiresAt = new Date(now.getTime() + SESSION_TTL_MS)
  await db.session.create({
    data: {
      tokenHash: hashSessionToken(token),
      userId,
      createdAt: now,
      expiresAt,
    },
  })
  return { token, expiresAt }
}

/** 记住我: binds `fingerprint` to `userId` for 30 days from `now`, moving it from any other member. */
async function bindKey(db: Db, userId: number, fingerprint: string, now: Date): Promise<void> {
  const binding = {
    userId,
    createdAt: now,
    expiresAt: new Date(now.getTime() + REMEMBER_TTL_MS),
  }
  await db.rememberedKey.upsert({
    where: { fingerprint },
    create: { fingerprint, ...binding },
    update: binding,
  })
}

function authSession(user: UserRow, session: { token: string, expiresAt: Date }, remembered: RememberStatus): AuthSession {
  return {
    token: session.token,
    expiresAt: instant(session.expiresAt),
    user: toUser(user),
    remembered,
  }
}

function isUniqueViolation(error: unknown): boolean {
  return typeof error === 'object' && error !== null && (error as { code?: unknown }).code === 'P2002'
}

/** `POST /v1/auth/register` → 201. */
export async function register(
  { ctx, deps, body, fingerprint, clientIp }: HandlerInput<typeof registerEndpoint>,
  limits: AuthLimits,
): Promise<AuthSession> {
  const now = deps.now()
  const limitKey = fingerprint
    ? `key:${fingerprint}`
    : `ip:${clientIp ?? 'unknown'}`
  const wait = limits.register.take(limitKey, now)
  if (wait !== null) {
    log(`register rate-limited (${fingerprint
      ? shortFingerprint(fingerprint)
      : `ip ${clientIp ?? 'unknown'}`})`)
    rateLimited(ctx, wait)
  }

  const key = usernameKey(body.username)
  const taken = await deps.prisma.user.findUnique({
    where: { usernameKey: key },
    select: { id: true },
  })
  if (taken)
    fail('username_taken')
  const passwordHash = await hashPassword(body.password)
  const remember = body.remember && fingerprint !== null

  let result: { user: UserRow, session: { token: string, expiresAt: Date }, remembered: RememberStatus }
  try {
    result = await deps.prisma.$transaction(async (tx) => {
      const user = await tx.user.create({
        data: {
          username: body.username,
          usernameKey: key,
          passwordHash,
          createdAt: now,
        },
        select: USER_SELECT,
      })
      await ensureMemberRows(tx, user.id)
      const session = await startSession(tx, user.id, now)
      if (remember)
        await bindKey(tx, user.id, fingerprint, now)
      const remembered = await loadRememberStatus(tx, user.id, fingerprint, now)
      return { user, session, remembered }
    })
  }
  catch (error) {
    // Two registrations of one name at once: the unique key decides.
    if (isUniqueViolation(error) && await deps.prisma.user.findUnique({
      where: { usernameKey: key },
      select: { id: true },
    })) {
      fail('username_taken')
    }
    throw error
  }
  log(`registered user ${result.user.id}${remember
    ? `, remembering ${shortFingerprint(fingerprint!)}`
    : ''}`)
  return authSession(result.user, result.session, result.remembered)
}

/** `POST /v1/auth/login`. */
export async function login(
  { ctx, deps, body, fingerprint, clientIp }: HandlerInput<typeof loginEndpoint>,
  limits: AuthLimits,
): Promise<AuthSession> {
  const now = deps.now()
  const key = usernameKey(body.username)
  const wait = limits.login.lockedFor(key, now)
  if (wait !== null) {
    log(`login rate-limited (ip ${clientIp ?? 'unknown'})`)
    rateLimited(ctx, wait)
  }

  const user = await deps.prisma.user.findUnique({
    where: { usernameKey: key },
    select: { ...USER_SELECT, passwordHash: true },
  })
  // Verify even for an unknown name, so the answer and its timing do not tell them apart.
  const valid = await verifyPassword(body.password, user?.passwordHash ?? await unknownUserHash())
  if (!user || !valid) {
    limits.login.fail(key, now)
    log(`login failed (ip ${clientIp ?? 'unknown'})`)
    fail('invalid_credentials')
  }
  limits.login.clear(key)

  const remember = body.remember && fingerprint !== null
  const { session, remembered } = await deps.prisma.$transaction(async (tx) => {
    const session = await startSession(tx, user.id, now)
    if (remember)
      await bindKey(tx, user.id, fingerprint, now)
    const remembered = await loadRememberStatus(tx, user.id, fingerprint, now)
    return { session, remembered }
  })
  log(`user ${user.id} logged in${remember
    ? `, remembering ${shortFingerprint(fingerprint!)}`
    : ''}`)
  return authSession(user, session, remembered)
}

/** `POST /v1/auth/resume`. */
export async function resume({ deps, fingerprint }: HandlerInput<typeof resumeEndpoint>): Promise<AuthSession> {
  const now = deps.now()
  const binding = await deps.prisma.rememberedKey.findUnique({
    where: { fingerprint },
    select: {
      userId: true,
      expiresAt: true,
      user: { select: USER_SELECT },
    },
  })
  if (!binding || binding.expiresAt.getTime() <= now.getTime()) {
    if (binding) {
      await deps.prisma.rememberedKey.deleteMany({
        where: {
          fingerprint,
          expiresAt: { lte: now },
        },
      })
    }
    fail('not_remembered')
  }
  // A new session only: the binding keeps its expiry (P-auth-4).
  const session = await startSession(deps.prisma, binding.userId, now)
  log(`user ${binding.userId} resumed with ${shortFingerprint(fingerprint)}`)
  return authSession(binding.user, session, rememberStatus(fingerprint, binding, binding.userId, now))
}

/** `POST /v1/auth/logout`. */
export async function logout({ deps, user, fingerprint }: HandlerInput<typeof logoutEndpoint>): Promise<Ok> {
  await deps.prisma.$transaction(async (tx) => {
    await tx.session.deleteMany({
      where: { id: user.sessionId },
    })
    // Only this connection's key, and only when it is bound to this member (P-auth-6).
    if (fingerprint) {
      await tx.rememberedKey.deleteMany({
        where: { fingerprint, userId: user.id },
      })
    }
  })
  log(`user ${user.id} logged out`)
  return { ok: true }
}

/** `GET /v1/auth/me`. */
export async function getMe({ deps, user, fingerprint }: HandlerInput<typeof getMeEndpoint>): Promise<Me> {
  const row = await deps.prisma.user.findUnique({
    where: { id: user.id },
    select: USER_SELECT,
  })
  if (!row)
    fail('session_expired')
  return {
    user: toUser(row),
    remembered: await loadRememberStatus(deps.prisma, user.id, fingerprint, deps.now()),
  }
}

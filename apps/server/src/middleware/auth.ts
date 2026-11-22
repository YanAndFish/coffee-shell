// The user session: `Authorization: Bearer <token>` resolved against `Session` (SHA-256 of
// the token, not expired). `route()` (src/http/route.ts) applies it per the contract's
// `endpoint.auth`; plain Koa code can use `requireUser` / `optionalUser`.
import type { Context, Middleware } from 'koa'
import type { Deps } from '../deps.ts'
import { AUTHORIZATION_HEADER } from '@coffee/contract'
import { bearerToken, hashSessionToken } from '../auth/tokens.ts'
import { fail } from './error-handler.ts'

/** The member behind a valid session. */
export interface SessionUser {
  id: number
  username: string
  sessionId: number
  sessionExpiresAt: Date
}

interface AuthState {
  user?: SessionUser | null
}

/**
 * The session's member; `null` without an `Authorization` header. A header that is not a
 * well-formed bearer token, an unknown token or an expired session is `401 session_expired`.
 */
export async function resolveUser(ctx: Context, deps: Deps): Promise<SessionUser | null> {
  const state = ctx.state as AuthState
  if (state.user !== undefined)
    return state.user
  const header = ctx.get(AUTHORIZATION_HEADER)
  if (!header) {
    state.user = null
    return null
  }
  const token = bearerToken(header)
  if (!token)
    fail('session_expired')
  const session = await deps.prisma.session.findUnique({
    where: {
      tokenHash: hashSessionToken(token),
    },
    select: {
      id: true,
      expiresAt: true,
      user: {
        select: { id: true, username: true },
      },
    },
  })
  if (!session || session.expiresAt.getTime() <= deps.now().getTime())
    fail('session_expired')
  state.user = {
    id: session.user.id,
    username: session.user.username,
    sessionId: session.id,
    sessionExpiresAt: session.expiresAt,
  }
  return state.user
}

/** The member, or `401 login_required` without a session (`session_expired` for a bad one). */
export async function currentUser(ctx: Context, deps: Deps): Promise<SessionUser> {
  return (await resolveUser(ctx, deps)) ?? fail('login_required')
}

/** Koa middleware: requires a member; read it with {@link userOf}. */
export function requireUser(deps: Deps): Middleware {
  return async (ctx, next) => {
    await currentUser(ctx, deps)
    return next()
  }
}

/** Koa middleware: resolves the member when a token is sent; guests pass. */
export function optionalUser(deps: Deps): Middleware {
  return async (ctx, next) => {
    await resolveUser(ctx, deps)
    return next()
  }
}

/** The member resolved by {@link requireUser} / {@link optionalUser} (`null` for guests). */
export function userOf(ctx: Context): SessionUser | null {
  return (ctx.state as AuthState).user ?? null
}

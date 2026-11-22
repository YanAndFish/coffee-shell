// Session tokens: 32 random bytes as 43 base64url characters; only their SHA-256 is stored.
import { createHash, randomBytes } from 'node:crypto'

/** A session lasts 7 days from login (no sliding renewal). */
export const SESSION_TTL_MS = 7 * 24 * 60 * 60 * 1000

/** 记住我 binds a key for 30 days from the password login that set it. */
export const REMEMBER_TTL_MS = 30 * 24 * 60 * 60 * 1000

/** A new random session token (43 base64url characters). */
export function newSessionToken(): string {
  return randomBytes(32).toString('base64url')
}

/** The stored form of a token: SHA-256, hex. */
export function hashSessionToken(token: string): string {
  return createHash('sha256').update(token).digest('hex')
}

/** The bearer token of an `Authorization` header value, or `null` when it is not one. */
export function bearerToken(header: string | undefined): string | null {
  const match = /^Bearer ([\w-]{43})$/.exec(header?.trim() ?? '')
  return match?.[1] ?? null
}

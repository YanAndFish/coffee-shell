import type { Middleware } from 'koa'
import type { Buffer } from 'node:buffer'
import { createHash, timingSafeEqual } from 'node:crypto'
import { INTERNAL_KEY_HEADER } from '@coffee/contract'
import { ApiHttpError } from './error-handler.ts'

function digest(value: string): Buffer {
  return createHash('sha256').update(value).digest()
}

/**
 * When `key` is set, rejects (401) every request whose `X-Internal-Key` header does not match,
 * except the paths in `publicPaths`. Without a key, every request passes.
 */
export function internalKey(key: string | undefined, publicPaths: ReadonlySet<string>): Middleware {
  if (!key)
    return (_ctx, next) => next()
  const expected = digest(key)
  return async (ctx, next) => {
    if (publicPaths.has(ctx.path))
      return next()
    const given = ctx.get(INTERNAL_KEY_HEADER)
    // Compare fixed-length digests so the check takes the same time for any input.
    if (!given || !timingSafeEqual(digest(given), expected))
      throw new ApiHttpError(401, 'unauthorized')
    return next()
  }
}

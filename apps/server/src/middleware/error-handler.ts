import type { ApiError, ApiErrorDetails, ErrorCode } from '@coffee/contract'
import type { Context, Middleware } from 'koa'
import { apiError, ERRORS } from '@coffee/contract'

/**
 * An error that maps to the `{ error: { code, message, details? } }` envelope. The HTTP
 * status and the message come from the contract's error table (`ERRORS`): one place.
 */
export class ApiHttpError extends Error {
  readonly status: number
  readonly code: ErrorCode
  readonly details: ApiErrorDetails | undefined

  constructor(code: ErrorCode, details?: ApiErrorDetails) {
    super(ERRORS[code].message)
    this.name = 'ApiHttpError'
    this.code = code
    this.status = ERRORS[code].status
    this.details = details
  }
}

/** Throws the error `code` (status and message from the contract). */
export function fail(code: ErrorCode, details?: ApiErrorDetails): never {
  throw new ApiHttpError(code, details)
}

/** The answer of every endpoint that is not implemented yet: `501 not_implemented`. */
export function notImplemented(): never {
  throw new ApiHttpError('not_implemented')
}

/** Error code of a status set or thrown by Koa or its middleware (`ctx.throw`, router 405/501). */
function codeForStatus(status: number): ErrorCode {
  switch (status) {
    case 401: return 'unauthorized'
    case 404: return 'not_found'
    case 405: return 'method_not_allowed'
    case 413: return 'bad_request'
    case 422: return 'validation_failed'
    case 429: return 'rate_limited'
    case 501: return 'not_implemented'
    default: return status >= 500
      ? 'internal_error'
      : 'bad_request'
  }
}

function toApiError(error: unknown): { status: number, body: ApiError } {
  if (error instanceof ApiHttpError) {
    return {
      status: error.status,
      body: apiError(error.code, error.details),
    }
  }
  const status = (error as { status?: unknown }).status
  if (typeof status === 'number' && status >= 400 && status < 500) {
    // Koa / router HTTP errors carry English messages; answer in Chinese.
    return {
      status,
      body: apiError(codeForStatus(status)),
    }
  }
  return {
    status: 500,
    body: apiError('internal_error'),
  }
}

function send(ctx: Context, status: number, body: ApiError): void {
  ctx.status = status
  ctx.body = body
}

/**
 * Turns every non-2xx outcome into the JSON error envelope: thrown errors, unmatched routes
 * (404), bodiless error statuses such as the router's 405, and 5xx (logged, never detailed).
 */
export function errorHandler(log: (error: unknown, ctx: Context) => void): Middleware {
  return async (ctx, next) => {
    try {
      await next()
      if (ctx.status >= 400 && ctx.body == null) {
        const code = codeForStatus(ctx.status)
        send(ctx, ctx.status, apiError(code))
      }
    }
    catch (error) {
      const { status, body } = toApiError(error)
      if (status >= 500 && status !== 501)
        log(error, ctx)
      // Drop headers set by downstream middleware before it failed.
      for (const name of Object.keys(ctx.response.headers)) {
        if (name !== 'allow' && name !== 'retry-after')
          ctx.remove(name)
      }
      send(ctx, status, body)
    }
  }
}

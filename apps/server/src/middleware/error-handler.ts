import type { ApiError, ErrorCode } from '@coffee/contract'
import type { Context, Middleware } from 'koa'
import type { z } from 'zod'

/** Default Chinese message of every error code. */
export const ERROR_MESSAGES: Record<ErrorCode, string> = {
  bad_request: '请求格式错误',
  validation_failed: '请求参数不合法',
  unauthorized: '缺少或错误的内部密钥',
  not_found: '资源不存在',
  method_not_allowed: '不支持该请求方法',
  internal_error: '服务器内部错误',
}

/** An error that maps to an `{ error: { code, message } }` response. */
export class ApiHttpError extends Error {
  readonly status: number
  readonly code: ErrorCode

  constructor(status: number, code: ErrorCode, message: string = ERROR_MESSAGES[code]) {
    super(message)
    this.name = 'ApiHttpError'
    this.status = status
    this.code = code
  }
}

/** Error code of a status set or thrown by Koa or its middleware (`ctx.throw`, router 405/501). */
function codeForStatus(status: number): ErrorCode {
  switch (status) {
    case 401: return 'unauthorized'
    case 404: return 'not_found'
    case 405:
    case 501: return 'method_not_allowed'
    case 422: return 'validation_failed'
    default: return status >= 500
      ? 'internal_error'
      : 'bad_request'
  }
}

/**
 * Validates request input (query, params, body) with `schema`; failures become a 400
 * `validation_failed` naming the first offending field. Any other ZodError (e.g. corrupt
 * stored data) stays a 500.
 */
export function parseInput<T extends z.ZodType>(schema: T, value: unknown): z.infer<T> {
  const result = schema.safeParse(value)
  if (result.success)
    return result.data
  const path = result.error.issues[0]?.path.join('.')
  const message = path
    ? `${ERROR_MESSAGES.validation_failed}：${path}`
    : ERROR_MESSAGES.validation_failed
  throw new ApiHttpError(400, 'validation_failed', message)
}

function toApiError(error: unknown): { status: number, body: ApiError } {
  if (error instanceof ApiHttpError) {
    return {
      status: error.status,
      body: {
        error: { code: error.code, message: error.message },
      },
    }
  }
  const status = (error as { status?: unknown }).status
  if (typeof status === 'number' && status >= 400 && status < 500) {
    // Koa / router HTTP errors carry English messages; answer in Chinese.
    const code = codeForStatus(status)
    return {
      status,
      body: {
        error: { code, message: ERROR_MESSAGES[code] },
      },
    }
  }
  return {
    status: 500,
    body: {
      error: { code: 'internal_error', message: ERROR_MESSAGES.internal_error },
    },
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
        send(ctx, ctx.status, {
          error: { code, message: ERROR_MESSAGES[code] },
        })
      }
    }
    catch (error) {
      const { status, body } = toApiError(error)
      if (status >= 500)
        log(error, ctx)
      // Drop headers set by downstream middleware before it failed.
      for (const name of Object.keys(ctx.response.headers)) {
        if (name !== 'allow')
          ctx.remove(name)
      }
      send(ctx, status, body)
    }
  }
}

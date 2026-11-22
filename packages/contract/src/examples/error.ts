import type { ApiError, ApiErrorDetails, ErrorCode } from '../common.ts'
import { apiError, ERRORS } from '../common.ts'

/** `fixtures/v1/error.json`: the 404 envelope. */
export const notFoundExample: ApiError = apiError('not_found')

/** The details the examples of some codes carry. */
const DETAILS: Partial<Record<ErrorCode, ApiErrorDetails>> = {
  validation_failed: { field: 'lines.0.weight' },
  out_of_stock: { productId: 'geisha', productName: '波奎特 瑰夏', available: 6 },
  sold_out: { productId: 'pinkb', productName: '慧兰 粉红波旁', weight: '1kg' },
  rate_limited: { retryAfter: 60 },
}

/** `fixtures/v1/errors/<code>.json`: one envelope per error code. */
export const errorExamples: ReadonlyArray<readonly [ErrorCode, ApiError]> = (Object.keys(ERRORS) as ErrorCode[])
  .map(code => [
    code,
    apiError(code, DETAILS[code]),
  ] as const)

import { z } from 'zod'

/** The API version prefix every backend route lives under. */
export const API_PREFIX = '/v1'

/** The header the backend checks when it runs with an internal key. */
export const INTERNAL_KEY_HEADER = 'X-Internal-Key'

/** Machine-readable error codes; extend this list as endpoints are added. */
export const ErrorCodeSchema = z.enum([
  'bad_request',
  'validation_failed',
  'unauthorized',
  'not_found',
  'method_not_allowed',
  'internal_error',
])
export type ErrorCode = z.infer<typeof ErrorCodeSchema>

/** The body of an {@link ApiError}. */
export const ApiErrorBodySchema = z.strictObject({
  code: ErrorCodeSchema,
  /** Human-readable message in Simplified Chinese. */
  message: z.string().min(1),
})
export type ApiErrorBody = z.infer<typeof ApiErrorBodySchema>

/** The envelope of every non-2xx response. */
export const ApiErrorSchema = z.strictObject({
  error: ApiErrorBodySchema,
})
export type ApiError = z.infer<typeof ApiErrorSchema>

/** Named schemas this module contributes to `components.schemas`. */
export const commonComponents = {
  ApiErrorBody: ApiErrorBodySchema,
  ApiError: ApiErrorSchema,
}

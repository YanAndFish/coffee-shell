import type { z } from 'zod'
import type { ErrorCode } from './common.ts'

/**
 * Who may call an endpoint at the transport level: `public` stays open even when the backend
 * runs with an internal key; `internal` then requires a matching `X-Internal-Key`.
 */
export type EndpointAccess = 'public' | 'internal'

/**
 * The user session an endpoint needs (`Authorization: Bearer <token>`):
 * - `none`: ignores it;
 * - `optional`: works for guests, and for members when a valid token is sent (an invalid or
 *   expired token is still `401 session_expired`);
 * - `user`: requires it (`401 login_required` without one, `401 session_expired` when it is
 *   unknown or expired).
 */
export type EndpointAuth = 'none' | 'optional' | 'user'

/** How an endpoint uses `X-SSH-Fingerprint`: not at all, when present, or required. */
export type EndpointFingerprint = 'none' | 'optional' | 'required'

/** The API areas, one module (and one OpenAPI tag) each. */
export type EndpointArea = 'health' | 'auth' | 'catalog' | 'cart' | 'addresses' | 'orders' | 'profile'

/** Zod object schemas of an endpoint's inputs; path and query values arrive as strings. */
export interface EndpointRequest {
  /** Path parameters (`{id}` in {@link Endpoint.path}). */
  params?: z.ZodObject
  /** Query string parameters. */
  query?: z.ZodObject
  /** JSON body; must be registered in the components (`src/api.ts`). */
  body?: z.ZodType
}

/** One HTTP operation of the API: drives `openapi.json`, the server router and its tests. */
export interface Endpoint {
  method: 'get' | 'post' | 'put' | 'patch' | 'delete'
  /** Full path including the `/v1` prefix, with OpenAPI `{param}` placeholders. */
  path: string
  /** Unique; also the handler name on the server. */
  operationId: string
  /** The area module (and OpenAPI tag). */
  tag: EndpointArea
  /** One line in Chinese. */
  summary: string
  /** The rules a handler must follow, in Chinese or English; goes to OpenAPI `description`. */
  description?: string
  access: EndpointAccess
  auth: EndpointAuth
  fingerprint: EndpointFingerprint
  request?: EndpointRequest
  /** Status code and schema of the success response (`null`: no body). */
  success: { status: 200 | 201 | 204, schema: z.ZodType | null }
  /**
   * The error codes this endpoint answers besides the ones its `access` and `auth` imply
   * (`unauthorized`; `login_required`, `session_expired`) and `internal_error`.
   */
  errors: readonly ErrorCode[]
}

/** Identity helper that keeps an endpoint's literal type (its schemas) for typed handlers. */
export function defineEndpoint<const E extends Endpoint>(endpoint: E): E {
  return endpoint
}

/** Identity helper so area modules get type checking on their endpoint tables. */
export function defineEndpoints<const T extends readonly Endpoint[]>(endpoints: T): T {
  return endpoints
}

/** Every error code an endpoint may answer, implied ones included. */
export function endpointErrorCodes(endpoint: Endpoint): ErrorCode[] {
  const codes = new Set<ErrorCode>(endpoint.errors)
  if (endpoint.access === 'internal')
    codes.add('unauthorized')
  if (endpoint.auth !== 'none')
    codes.add('session_expired')
  if (endpoint.auth === 'user')
    codes.add('login_required')
  if (endpoint.request)
    codes.add('validation_failed')
  if (endpoint.request?.body)
    codes.add('bad_request')
  codes.add('internal_error')
  return [...codes]
}

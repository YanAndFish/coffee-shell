import type { z } from 'zod'

/**
 * Who may call an endpoint: `public` stays open even when the backend runs with an internal
 * key; `internal` then requires a matching `X-Internal-Key`.
 */
export type EndpointAccess = 'public' | 'internal'

/** One HTTP operation of the API, used to build `openapi.json`. */
export interface Endpoint {
  method: 'get' | 'post' | 'put' | 'patch' | 'delete'
  /** Full path including the `/v1` prefix, with OpenAPI `{param}` placeholders. */
  path: string
  operationId: string
  /** OpenAPI tag, one per area module. */
  tag: string
  summary: string
  access: EndpointAccess
  /** Status code and schema of the success response. */
  success: { status: 200 | 201 | 204, schema: z.ZodType | null }
  /** Error statuses this endpoint documents besides the default envelope. */
  errors: readonly number[]
}

/** Identity helper so area modules get type checking on their endpoint tables. */
export function defineEndpoints<const T extends readonly Endpoint[]>(endpoints: T): T {
  return endpoints
}

import { z } from 'zod'
import { API_PREFIX } from './common.ts'
import { defineEndpoint, defineEndpoints } from './endpoint.ts'

/** `GET /v1/health`. */
export const HealthSchema = z.strictObject({
  /** Always `true` when the backend answers. */
  ok: z.literal(true),
  /** The service name. */
  service: z.literal('roast-tty'),
  /** The backend's version. */
  version: z.string().min(1),
  /** Always `true`: this is a demo shop. */
  demo: z.literal(true),
})
export type Health = z.infer<typeof HealthSchema>

export const healthComponents = { Health: HealthSchema }

export const getHealth = defineEndpoint({
  method: 'get',
  path: `${API_PREFIX}/health`,
  operationId: 'getHealth',
  tag: 'health',
  summary: '健康检查（无需内部密钥）',
  access: 'public',
  auth: 'none',
  fingerprint: 'none',
  success: { status: 200, schema: HealthSchema },
  errors: [],
})

export const healthEndpoints = defineEndpoints([getHealth])

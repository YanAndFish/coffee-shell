import { z } from 'zod'
import { API_PREFIX } from './common.ts'
import { defineEndpoints } from './endpoint.ts'

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

export const healthEndpoints = defineEndpoints([
  {
    method: 'get',
    path: `${API_PREFIX}/health`,
    operationId: 'getHealth',
    tag: 'health',
    summary: '健康检查（无需内部密钥）',
    access: 'public',
    success: { status: 200, schema: HealthSchema },
    errors: [],
  },
])

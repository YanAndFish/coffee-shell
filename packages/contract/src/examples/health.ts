import type { Health } from '../health.ts'

/** `fixtures/v1/health.json`. */
export const healthExample: Health = {
  ok: true,
  service: 'roast-tty',
  version: '0.1.0',
  demo: true,
}

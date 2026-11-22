import type { Health } from '@coffee/contract'
import { API_PREFIX } from '@coffee/contract'
import Router from '@koa/router'
import pkg from '../../package.json' with { type: 'json' }

export const HEALTH_PATH = `${API_PREFIX}/health`

export function healthRouter(): Router {
  const router = new Router()
  router.get(HEALTH_PATH, (ctx) => {
    const body: Health = {
      ok: true,
      service: 'roast-tty',
      version: pkg.version,
      demo: true,
    }
    ctx.body = body
  })
  return router
}

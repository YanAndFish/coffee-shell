import type { Health } from '@coffee/contract'
import type { Deps } from '../deps.ts'
import { API_PREFIX, getHealth } from '@coffee/contract'
import Router from '@koa/router'
import pkg from '../../package.json' with { type: 'json' }
import { route } from '../http/route.ts'

export const HEALTH_PATH = `${API_PREFIX}/health`

export function healthRouter(deps: Deps): Router {
  const router = new Router()
  route(router, deps, getHealth, (): Health => ({
    ok: true,
    service: 'roast-tty',
    version: pkg.version,
    demo: true,
  }))
  return router
}

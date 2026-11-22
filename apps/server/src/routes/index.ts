import type { Deps } from '../deps.ts'
import Router from '@koa/router'
import { addressesRouter } from './addresses.ts'
import { authRouter } from './auth.ts'
import { cartRouter } from './cart.ts'
import { catalogRouter } from './catalog.ts'
import { healthRouter } from './health.ts'
import { ordersRouter } from './orders.ts'
import { profileRouter } from './profile.ts'

export { HEALTH_PATH } from './health.ts'

/**
 * Every `/v1` route: one router per contract area (`routes/<area>.ts`). Every endpoint of the
 * contract is registered (unimplemented ones answer `501 not_implemented`), so areas never
 * need to touch this file.
 */
export function apiRouter(deps: Deps): Router {
  const router = new Router()
  for (const area of [
    healthRouter(deps),
    authRouter(deps),
    catalogRouter(deps),
    cartRouter(deps),
    addressesRouter(deps),
    ordersRouter(deps),
    profileRouter(deps),
  ])
    router.use(area.routes())
  return router
}

import type { PrismaClient } from '../db.ts'
import Router from '@koa/router'
import { healthRouter } from './health.ts'
import { productsRouter } from './products.ts'

export { HEALTH_PATH } from './health.ts'

/** Every `/v1` route; add one router per area. */
export function apiRouter(prisma: PrismaClient): Router {
  const router = new Router()
  for (const area of [
    healthRouter(),
    productsRouter(prisma),
  ])
    router.use(area.routes())
  return router
}

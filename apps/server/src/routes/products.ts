import type { PrismaClient } from '../db.ts'
import { API_PREFIX } from '@coffee/contract'
import Router from '@koa/router'
import { listProducts } from '../services/products.ts'

export function productsRouter(prisma: PrismaClient): Router {
  const router = new Router({ prefix: `${API_PREFIX}/products` })
  router.get('/', async (ctx) => {
    ctx.body = await listProducts(prisma)
  })
  return router
}

// Area `catalog` routes (owner: server-shop): every endpoint of the contract's `catalogEndpoints`,
// wired to `services/catalog.ts`. Auth, headers and input validation come from the contract
// (see `http/route.ts`); handlers only implement the rules.
import type { Deps } from '../deps.ts'
import * as endpoints from '@coffee/contract'
import Router from '@koa/router'
import { route } from '../http/route.ts'
import * as service from '../services/catalog.ts'

export function catalogRouter(deps: Deps): Router {
  const router = new Router()
  route(router, deps, endpoints.listProducts, service.listProducts)
  route(router, deps, endpoints.getProduct, service.getProduct)
  return router
}

// Area `orders` routes (owner: server-orders): every endpoint of the contract's `ordersEndpoints`,
// wired to `services/orders.ts`. Auth, headers and input validation come from the contract
// (see `http/route.ts`); handlers only implement the rules.
import type { Deps } from '../deps.ts'
import * as endpoints from '@coffee/contract'
import Router from '@koa/router'
import { route } from '../http/route.ts'
import * as service from '../services/orders.ts'

export function ordersRouter(deps: Deps): Router {
  const router = new Router()
  route(router, deps, endpoints.placeOrder, service.placeOrder)
  route(router, deps, endpoints.listOrders, service.listOrders)
  route(router, deps, endpoints.getOrder, service.getOrder)
  return router
}

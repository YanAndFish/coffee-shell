// Area `cart` routes (owner: server-shop): every endpoint of the contract's `cartEndpoints`,
// wired to `services/cart.ts`. Auth, headers and input validation come from the contract
// (see `http/route.ts`); handlers only implement the rules.
import type { Deps } from '../deps.ts'
import * as endpoints from '@coffee/contract'
import Router from '@koa/router'
import { route } from '../http/route.ts'
import * as service from '../services/cart.ts'

export function cartRouter(deps: Deps): Router {
  const router = new Router()
  route(router, deps, endpoints.getCart, service.getCart)
  route(router, deps, endpoints.addCartLine, service.addCartLine)
  route(router, deps, endpoints.updateCartLine, service.updateCartLine)
  route(router, deps, endpoints.removeCartLine, service.removeCartLine)
  route(router, deps, endpoints.setCartSelection, service.setCartSelection)
  route(router, deps, endpoints.applyCoupon, service.applyCoupon)
  route(router, deps, endpoints.removeCoupon, service.removeCoupon)
  route(router, deps, endpoints.setCartAddress, service.setCartAddress)
  route(router, deps, endpoints.quoteCart, service.quoteCart)
  route(router, deps, endpoints.mergeCart, service.mergeCart)
  return router
}

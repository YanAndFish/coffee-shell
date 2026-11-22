// Area `addresses` routes (owner: server-orders): every endpoint of the contract's `addressesEndpoints`,
// wired to `services/addresses.ts`. Auth, headers and input validation come from the contract
// (see `http/route.ts`); handlers only implement the rules.
import type { Deps } from '../deps.ts'
import * as endpoints from '@coffee/contract'
import Router from '@koa/router'
import { route } from '../http/route.ts'
import * as service from '../services/addresses.ts'

export function addressesRouter(deps: Deps): Router {
  const router = new Router()
  route(router, deps, endpoints.listAddresses, service.listAddresses)
  route(router, deps, endpoints.createAddress, service.createAddress)
  route(router, deps, endpoints.updateAddress, service.updateAddress)
  route(router, deps, endpoints.deleteAddress, service.deleteAddress)
  route(router, deps, endpoints.setDefaultAddress, service.setDefaultAddress)
  return router
}

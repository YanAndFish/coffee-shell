// Area `profile` routes (owner: server-orders): every endpoint of the contract's `profileEndpoints`,
// wired to `services/profile.ts`. Auth, headers and input validation come from the contract
// (see `http/route.ts`); handlers only implement the rules.
import type { Deps } from '../deps.ts'
import * as endpoints from '@coffee/contract'
import Router from '@koa/router'
import { route } from '../http/route.ts'
import * as service from '../services/profile.ts'

export function profileRouter(deps: Deps): Router {
  const router = new Router()
  route(router, deps, endpoints.getProfile, service.getProfile)
  route(router, deps, endpoints.getSettings, service.getSettings)
  route(router, deps, endpoints.updateSettings, service.updateSettings)
  return router
}

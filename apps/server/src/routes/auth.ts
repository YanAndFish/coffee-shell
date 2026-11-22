// Area `auth` routes (owner: server-auth): every endpoint of the contract's `authEndpoints`,
// wired to `services/auth.ts`. Auth, headers and input validation come from the contract
// (see `http/route.ts`); handlers only implement the rules.
import type { Deps } from '../deps.ts'
import * as endpoints from '@coffee/contract'
import Router from '@koa/router'
import { createAuthLimits } from '../auth/rate-limit.ts'
import { route } from '../http/route.ts'
import * as service from '../services/auth.ts'

export function authRouter(deps: Deps): Router {
  const router = new Router()
  // Rate-limit counters live with this app instance (P-auth-3: in memory, reset on restart).
  const limits = createAuthLimits()
  route(router, deps, endpoints.register, input => service.register(input, limits))
  route(router, deps, endpoints.login, input => service.login(input, limits))
  route(router, deps, endpoints.resume, service.resume)
  route(router, deps, endpoints.logout, service.logout)
  route(router, deps, endpoints.getMe, service.getMe)
  return router
}

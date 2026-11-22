// Area `auth` (owner: server-auth). One function per endpoint, typed from the contract; the
// rules are in each endpoint's `description` (packages/contract/src/auth.ts) and in
// docs/design/decisions.md. Until implemented, each answers `501 not_implemented`.
//
// Building blocks already there: `src/auth/password.ts` (argon2id hash/verify),
// `src/auth/tokens.ts` (token, hash, TTLs), `services/members.ts` (`toUser`, `usernameKey`,
// `loadRememberStatus`, `ensureMemberRows`), `services/shop-time.ts` (`rememberStatus`).
import type {
  AuthSession,
  getMe as getMeEndpoint,
  login as loginEndpoint,
  logout as logoutEndpoint,
  Me,
  Ok,
  register as registerEndpoint,
  resume as resumeEndpoint,
} from '@coffee/contract'
import type { HandlerInput } from '../http/route.ts'
import { notImplemented } from '../middleware/error-handler.ts'

/** `POST /v1/auth/register` → 201. */
export function register(_input: HandlerInput<typeof registerEndpoint>): Promise<AuthSession> {
  notImplemented()
}

/** `POST /v1/auth/login`. */
export function login(_input: HandlerInput<typeof loginEndpoint>): Promise<AuthSession> {
  notImplemented()
}

/** `POST /v1/auth/resume`. */
export function resume(_input: HandlerInput<typeof resumeEndpoint>): Promise<AuthSession> {
  notImplemented()
}

/** `POST /v1/auth/logout`. */
export function logout(_input: HandlerInput<typeof logoutEndpoint>): Promise<Ok> {
  notImplemented()
}

/** `GET /v1/auth/me`. */
export function getMe(_input: HandlerInput<typeof getMeEndpoint>): Promise<Me> {
  notImplemented()
}

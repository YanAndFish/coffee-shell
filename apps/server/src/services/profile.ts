// Area `profile` (owner: server-orders): the profile page and per-account settings. One
// function per endpoint, typed from the contract; the taste-profile rules are in
// docs/design/decisions.md (P-profile-*).
//
// Building blocks: `services/members.ts` (`toUser`, `loadRememberStatus`,
// `ensureMemberRows`), `services/products.ts` (`productFacts` for flavors).
import type {
  getProfile as getProfileEndpoint,
  getSettings as getSettingsEndpoint,
  Profile,
  Settings,
  updateSettings as updateSettingsEndpoint,
} from '@coffee/contract'
import type { HandlerInput } from '../http/route.ts'
import { notImplemented } from '../middleware/error-handler.ts'

/** `GET /v1/profile`. */
export function getProfile(_input: HandlerInput<typeof getProfileEndpoint>): Promise<Profile> {
  notImplemented()
}

/** `GET /v1/settings`. */
export function getSettings(_input: HandlerInput<typeof getSettingsEndpoint>): Promise<Settings> {
  notImplemented()
}

/** `PATCH /v1/settings`. */
export function updateSettings(_input: HandlerInput<typeof updateSettingsEndpoint>): Promise<Settings> {
  notImplemented()
}

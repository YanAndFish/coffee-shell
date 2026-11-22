// The whole API surface in one place. Each area module (health, auth, catalog, …) contributes
// its named schemas and its endpoint table; add a new area by appending to both lists.
import type { Endpoint } from './endpoint.ts'
import { addressComponents, addressEndpoints } from './addresses.ts'
import { authComponents, authEndpoints } from './auth.ts'
import { cartComponents, cartEndpoints } from './cart.ts'
import { catalogComponents, catalogEndpoints } from './catalog.ts'
import { commonComponents } from './common.ts'
import { healthComponents, healthEndpoints } from './health.ts'
import { orderComponents, orderEndpoints } from './orders.ts'
import { profileComponents, profileEndpoints } from './profile.ts'
import { skuComponents } from './sku.ts'

/** Named schemas, in the order they appear in `openapi.json`. */
export const components = {
  ...commonComponents,
  ...skuComponents,
  ...healthComponents,
  ...authComponents,
  ...catalogComponents,
  ...cartComponents,
  ...addressComponents,
  ...orderComponents,
  ...profileComponents,
}

/** Every endpoint, in the order they appear in `openapi.json`. */
export const endpoints: readonly Endpoint[] = [
  ...healthEndpoints,
  ...authEndpoints,
  ...catalogEndpoints,
  ...cartEndpoints,
  ...addressEndpoints,
  ...orderEndpoints,
  ...profileEndpoints,
]

/** The endpoint with `operationId`; throws when there is none. */
export function endpointById(operationId: string): Endpoint {
  const endpoint = endpoints.find(e => e.operationId === operationId)
  if (!endpoint)
    throw new Error(`No endpoint ${operationId}`)
  return endpoint
}

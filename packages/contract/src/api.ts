// The whole API surface in one place. Each area module (health, products, …) contributes
// its named schemas and its endpoint table; add a new area by appending to both lists.
import type { Endpoint } from './endpoint.ts'
import { commonComponents } from './common.ts'
import { healthComponents, healthEndpoints } from './health.ts'
import { productComponents, productEndpoints } from './products.ts'

/** Named schemas, in the order they appear in `openapi.json`. */
export const components = {
  ...commonComponents,
  ...healthComponents,
  ...productComponents,
}

/** Every endpoint, in the order they appear in `openapi.json`. */
export const endpoints: readonly Endpoint[] = [
  ...healthEndpoints,
  ...productEndpoints,
]

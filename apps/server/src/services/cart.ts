// Area `cart` (owner: server-shop): the member cart, the guest quote and the merge after
// login. One function per endpoint, typed from the contract; rules in each endpoint's
// `description` (packages/contract/src/cart.ts) and docs/design/decisions.md.
//
// Building blocks: `services/cart-view.ts` (`loadCart`, `priceLine`, `buildCart`),
// `services/pricing.ts` (`normalizeCouponCode`, `MAX_QUANTITY`), `services/products.ts`
// (`productFacts`, `weightAvailable`, `productSku`), `services/address-book.ts`.
import type {
  addCartLine as addCartLineEndpoint,
  applyCoupon as applyCouponEndpoint,
  Cart,
  CartMergeResult,
  getCart as getCartEndpoint,
  mergeCart as mergeCartEndpoint,
  quoteCart as quoteCartEndpoint,
  removeCartLine as removeCartLineEndpoint,
  removeCoupon as removeCouponEndpoint,
  setCartAddress as setCartAddressEndpoint,
  setCartSelection as setCartSelectionEndpoint,
  updateCartLine as updateCartLineEndpoint,
} from '@coffee/contract'
import type { HandlerInput } from '../http/route.ts'
import { notImplemented } from '../middleware/error-handler.ts'

/** `GET /v1/cart`. */
export function getCart(_input: HandlerInput<typeof getCartEndpoint>): Promise<Cart> {
  notImplemented()
}

/** `POST /v1/cart/lines`. */
export function addCartLine(_input: HandlerInput<typeof addCartLineEndpoint>): Promise<Cart> {
  notImplemented()
}

/** `PATCH /v1/cart/lines/{sku}`. */
export function updateCartLine(_input: HandlerInput<typeof updateCartLineEndpoint>): Promise<Cart> {
  notImplemented()
}

/** `DELETE /v1/cart/lines/{sku}`. */
export function removeCartLine(_input: HandlerInput<typeof removeCartLineEndpoint>): Promise<Cart> {
  notImplemented()
}

/** `PUT /v1/cart/selection`. */
export function setCartSelection(_input: HandlerInput<typeof setCartSelectionEndpoint>): Promise<Cart> {
  notImplemented()
}

/** `PUT /v1/cart/coupon`. */
export function applyCoupon(_input: HandlerInput<typeof applyCouponEndpoint>): Promise<Cart> {
  notImplemented()
}

/** `DELETE /v1/cart/coupon`. */
export function removeCoupon(_input: HandlerInput<typeof removeCouponEndpoint>): Promise<Cart> {
  notImplemented()
}

/** `PUT /v1/cart/address`. */
export function setCartAddress(_input: HandlerInput<typeof setCartAddressEndpoint>): Promise<Cart> {
  notImplemented()
}

/** `POST /v1/cart/quote` (guests; nothing stored). */
export function quoteCart(_input: HandlerInput<typeof quoteCartEndpoint>): Promise<Cart> {
  notImplemented()
}

/** `POST /v1/cart/merge`. */
export function mergeCart(_input: HandlerInput<typeof mergeCartEndpoint>): Promise<CartMergeResult> {
  notImplemented()
}

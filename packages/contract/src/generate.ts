// Pure rendering of the generated files; `scripts/generate.ts` writes them and the tests
// compare them with the committed copies.
import type { z } from 'zod'
import * as addresses from './addresses.ts'
import * as auth from './auth.ts'
import * as cart from './cart.ts'
import * as catalog from './catalog.ts'
import { ApiErrorSchema, ERRORS, OkSchema } from './common.ts'
import * as ex from './examples/index.ts'
import { HealthSchema } from './health.ts'
import { buildOpenApi } from './openapi.ts'
import * as orders from './orders.ts'
import * as profile from './profile.ts'

/** A fixture: an example value and the schema it must satisfy. */
export interface Fixture {
  /** Path relative to `fixtures/`. */
  path: string
  schema: z.ZodType
  value: unknown
  /** The endpoint answering it, or `null` for the generic error envelope. */
  operationId: string | null
  /** The HTTP status it comes with. */
  status: number
}

function ok(path: string, operationId: string, status: number, schema: z.ZodType, value: unknown): Fixture {
  return {
    path,
    schema,
    value,
    operationId,
    status,
  }
}

/**
 * Every fixture, keyed by its path under `fixtures/`. Rust tests (crates/coffee-api), the TUI
 * fixture transport and the SSH e2e stub read these files; the first three paths are frozen.
 */
export const fixtures: readonly Fixture[] = [
  ok('v1/health.json', 'getHealth', 200, HealthSchema, ex.healthExample),
  ok('v1/products.json', 'listProducts', 200, catalog.ProductListSchema, ex.productListExample),
  {
    path: 'v1/error.json',
    schema: ApiErrorSchema,
    value: ex.notFoundExample,
    operationId: null,
    status: 404,
  },
  // auth
  ok('v1/auth/register.json', 'register', 201, auth.AuthSessionSchema, ex.registerExample),
  ok('v1/auth/login.json', 'login', 200, auth.AuthSessionSchema, ex.loginExample),
  ok('v1/auth/login-no-key.json', 'login', 200, auth.AuthSessionSchema, ex.loginNoKeyExample),
  ok('v1/auth/resume.json', 'resume', 200, auth.AuthSessionSchema, ex.resumeExample),
  ok('v1/auth/logout.json', 'logout', 200, OkSchema, ex.logoutExample),
  ok('v1/auth/me.json', 'getMe', 200, auth.MeSchema, ex.meExample),
  // catalog
  ok('v1/catalog/products-light-by-price.json', 'listProducts', 200, catalog.ProductListSchema, ex.productListLightByPriceExample),
  ok('v1/catalog/products-search.json', 'listProducts', 200, catalog.ProductListSchema, ex.productListSearchExample),
  ok('v1/catalog/products-empty.json', 'listProducts', 200, catalog.ProductListSchema, ex.productListEmptyExample),
  // one detail per seeded bean: product-<id>.json
  ...ex.productDetailExamples.map(detail => ok(`v1/catalog/product-${detail.id}.json`, 'getProduct', 200, catalog.ProductDetailSchema, detail)),
  ok('v1/catalog/detail-no-stock.json', 'getProduct', 200, catalog.ProductDetailSchema, ex.productDetailNoStockExample),
  // cart
  ok('v1/cart/cart.json', 'getCart', 200, cart.CartSchema, ex.cartExample),
  ok('v1/cart/added.json', 'addCartLine', 200, cart.CartSchema, ex.cartExample),
  ok('v1/cart/cart-partly-selected.json', 'updateCartLine', 200, cart.CartSchema, ex.cartPartlySelectedExample),
  ok('v1/cart/cart-coupon.json', 'applyCoupon', 200, cart.CartSchema, ex.cartCouponExample),
  ok('v1/cart/cart-empty.json', 'getCart', 200, cart.CartSchema, ex.cartEmptyExample),
  ok('v1/cart/cart-problem.json', 'getCart', 200, cart.CartSchema, ex.cartProblemExample),
  ok('v1/cart/cart-no-address.json', 'getCart', 200, cart.CartSchema, ex.cartNoAddressExample),
  ok('v1/cart/removed.json', 'removeCartLine', 200, cart.CartSchema, ex.cartRemovedExample),
  ok('v1/cart/none-selected.json', 'setCartSelection', 200, cart.CartSchema, ex.cartNoneSelectedExample),
  ok('v1/cart/coupon-removed.json', 'removeCoupon', 200, cart.CartSchema, ex.cartCouponRemovedExample),
  ok('v1/cart/address-set.json', 'setCartAddress', 200, cart.CartSchema, ex.cartAddressSetExample),
  ok('v1/cart/guest-quote.json', 'quoteCart', 200, cart.CartSchema, ex.guestQuoteExample),
  ok('v1/cart/merge.json', 'mergeCart', 200, cart.CartMergeResultSchema, ex.cartMergeExample),
  // addresses
  ok('v1/addresses/list.json', 'listAddresses', 200, addresses.AddressListSchema, ex.addressListExample),
  ok('v1/addresses/list-order-elsewhere.json', 'listAddresses', 200, addresses.AddressListSchema, ex.addressListOrderElsewhereExample),
  ok('v1/addresses/list-empty.json', 'listAddresses', 200, addresses.AddressListSchema, ex.addressListEmptyExample),
  ok('v1/addresses/saved-first.json', 'createAddress', 201, addresses.AddressSavedSchema, ex.addressSavedFirstExample),
  ok('v1/addresses/saved-no-phone.json', 'createAddress', 201, addresses.AddressSavedSchema, ex.addressSavedNoPhoneExample),
  ok('v1/addresses/updated.json', 'updateAddress', 200, addresses.AddressSavedSchema, ex.addressUpdatedExample),
  ok('v1/addresses/deleted-default.json', 'deleteAddress', 200, addresses.AddressListSchema, ex.addressDeletedDefaultExample),
  ok('v1/addresses/default-moved.json', 'setDefaultAddress', 200, addresses.AddressListSchema, ex.addressDefaultMovedExample),
  // orders
  ok('v1/orders/placed.json', 'placeOrder', 201, orders.PlacedOrderSchema, ex.placedOrderExample),
  ok('v1/orders/list.json', 'listOrders', 200, orders.OrderListSchema, ex.orderListExample),
  ok('v1/orders/list-empty.json', 'listOrders', 200, orders.OrderListSchema, ex.orderListEmptyExample),
  ok('v1/orders/order.json', 'getOrder', 200, orders.OrderSchema, ex.orderDeliveredExample),
  ok('v1/orders/order-coupon.json', 'getOrder', 200, orders.OrderSchema, ex.orderWithCouponExample),
  // profile
  ok('v1/profile/profile.json', 'getProfile', 200, profile.ProfileSchema, ex.profileExample),
  ok('v1/profile/profile-new.json', 'getProfile', 200, profile.ProfileSchema, ex.profileNewExample),
  ok('v1/profile/profile-no-key.json', 'getProfile', 200, profile.ProfileSchema, ex.profileNoKeyExample),
  ok('v1/profile/profile-orders-no-key.json', 'getProfile', 200, profile.ProfileSchema, ex.profileOrdersNoKeyExample),
  ok('v1/profile/settings.json', 'getSettings', 200, profile.SettingsSchema, ex.settingsExample),
  ok('v1/profile/settings-animations-off.json', 'updateSettings', 200, profile.SettingsSchema, ex.settingsAnimationsOffExample),
  // one envelope per error code
  ...ex.errorExamples.map(([code, value]): Fixture => ({
    path: `v1/errors/${code}.json`,
    schema: ApiErrorSchema,
    value,
    operationId: null,
    status: ERRORS[code].status,
  })),
]

/** The canonical JSON text of the generated files: 2-space indent and a final newline. */
export function toJsonText(value: unknown): string {
  return `${JSON.stringify(value, null, 2)}\n`
}

/** Every generated file, keyed by its path relative to the package root. */
export function renderGeneratedFiles(): Map<string, string> {
  const files = new Map<string, string>()
  files.set('openapi.json', toJsonText(buildOpenApi()))
  for (const fixture of fixtures)
    files.set(`fixtures/${fixture.path}`, toJsonText(fixture.schema.parse(fixture.value)))
  return files
}

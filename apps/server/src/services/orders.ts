// Area `orders` (owner: server-orders): checkout, the order list, one order. One function per
// endpoint, typed from the contract; rules in packages/contract/src/orders.ts (placeOrder's
// `description` is the checkout transaction step by step) and docs/design/decisions.md.
//
// Building blocks: `services/pricing.ts` (re-price), `services/cart-view.ts` (`loadCart`,
// `priceLine`), `services/products.ts`, `services/address-book.ts` (`loadOrderAddress`,
// `toOrderAddress`), `services/shop-time.ts` (`orderNumber`, `etaOf`, `orderStatus`,
// `shopDate`, `instant`).
import type {
  getOrder as getOrderEndpoint,
  listOrders as listOrdersEndpoint,
  Order,
  OrderList,
  PlacedOrder,
  placeOrder as placeOrderEndpoint,
} from '@coffee/contract'
import type { HandlerInput } from '../http/route.ts'
import { notImplemented } from '../middleware/error-handler.ts'

/** `POST /v1/orders` → 201. */
export function placeOrder(_input: HandlerInput<typeof placeOrderEndpoint>): Promise<PlacedOrder> {
  notImplemented()
}

/** `GET /v1/orders`. */
export function listOrders(_input: HandlerInput<typeof listOrdersEndpoint>): Promise<OrderList> {
  notImplemented()
}

/** `GET /v1/orders/{number}`. */
export function getOrder(_input: HandlerInput<typeof getOrderEndpoint>): Promise<Order> {
  notImplemented()
}

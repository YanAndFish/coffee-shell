// Area `orders` (owner: server-orders): checkout, the order list, one order. One function per
// endpoint, typed from the contract; rules in packages/contract/src/orders.ts (placeOrder's
// `description` is the checkout transaction step by step) and docs/design/decisions.md.
//
// Checkout itself is `services/orders-checkout.ts`; row → contract mapping (status derived at
// read time from the injected clock) is `services/orders-view.ts`.
import type {
  getOrder as getOrderEndpoint,
  listOrders as listOrdersEndpoint,
  Order,
  OrderList,
  PlacedOrder,
  placeOrder as placeOrderEndpoint,
} from '@coffee/contract'
import type { HandlerInput } from '../http/route.ts'
import { fail } from '../middleware/error-handler.ts'
import { placeOrderNow } from './orders-checkout.ts'
import { findOrder, ORDER_ITEMS, toOrder, toOrderSummary } from './orders-view.ts'

/** `GET /v1/orders` without `limit`. */
export const DEFAULT_ORDER_LIMIT = 20

/** `POST /v1/orders` → 201. */
export function placeOrder({ deps, user, body }: HandlerInput<typeof placeOrderEndpoint>): Promise<PlacedOrder> {
  return placeOrderNow(deps.prisma, {
    userId: user.id,
    idempotencyKey: body.idempotencyKey,
    expectedTotal: body.expectedTotal,
    now: deps.now(),
  })
}

/** `GET /v1/orders`: newest first, at most `limit`, with the member's total. */
export async function listOrders({ deps, user, query }: HandlerInput<typeof listOrdersEndpoint>): Promise<OrderList> {
  const now = deps.now()
  const [rows, total] = await Promise.all([
    deps.prisma.order.findMany({
      where: { userId: user.id },
      orderBy: [
        { createdAt: 'desc' },
        { id: 'desc' },
      ],
      take: query.limit ?? DEFAULT_ORDER_LIMIT,
      include: ORDER_ITEMS,
    }),
    deps.prisma.order.count({
      where: { userId: user.id },
    }),
  ])
  return {
    items: rows.map(row => toOrderSummary(row, now)),
    total,
  }
}

/** `GET /v1/orders/{number}`: the member's own orders only. */
export async function getOrder({ deps, user, params }: HandlerInput<typeof getOrderEndpoint>): Promise<Order> {
  const row = (await findOrder(deps.prisma, user.id, params.number)) ?? fail('not_found')
  return toOrder(row, deps.now())
}

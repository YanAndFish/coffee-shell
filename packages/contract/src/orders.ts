// Orders: checkout (place order), the order list and one order (the receipt).
// Owner in phase 2 wave 2: server-orders.
import { z } from 'zod'
import { CartSchema } from './cart.ts'
import { API_PREFIX, InstantSchema, MoneySchema, ShopDateSchema } from './common.ts'
import { defineEndpoint, defineEndpoints } from './endpoint.ts'
import { GrindSchema, PackSchema, SkuSchema, WeightSchema } from './sku.ts'

/**
 * Derived from the time of reading, never stored: `roasting` (● 烘焙中) for the first
 * 24 hours, then `shipping` (● 运输中) until the ETA, then `delivered` (● 已签收).
 */
export const OrderStatusSchema = z.enum(['roasting', 'shipping', 'delivered'])
export type OrderStatus = z.infer<typeof OrderStatusSchema>

/** Display label of each status. */
export const ORDER_STATUS_LABELS: Record<OrderStatus, string> = { roasting: '烘焙中', shipping: '运输中', delivered: '已签收' }

/** Order number: `RT` + yymmdd (shop date) + 4 random digits (0000–9999), e.g. `RT2611220417`. */
export const OrderNumberSchema = z.string().regex(/^RT\d{10}$/)
export type OrderNumber = z.infer<typeof OrderNumberSchema>

/** One purchased line, a snapshot taken at checkout. */
export const OrderItemSchema = z.strictObject({
  sku: SkuSchema,
  productId: z.string().min(1),
  /** Name at checkout. */
  name: z.string().min(1),
  weight: WeightSchema,
  grind: GrindSchema,
  pack: PackSchema,
  quantity: z.int().min(1).max(9),
  unitPrice: MoneySchema,
  lineTotal: MoneySchema,
})
export type OrderItem = z.infer<typeof OrderItemSchema>

/** The shipping address, a snapshot taken at checkout. */
export const OrderAddressSchema = z.strictObject({
  recipient: z.string().min(1),
  phone: z.string().regex(/^\d{11}$/).nullable(),
  phoneMasked: z.string().regex(/^\d{3} \*{4} \d{4}$/).nullable(),
  region: z.string().min(1),
  detail: z.string().min(1),
})
export type OrderAddress = z.infer<typeof OrderAddressSchema>

/** One order: the receipt (`订单回执`). */
export const OrderSchema = z.strictObject({
  number: OrderNumberSchema,
  createdAt: InstantSchema,
  /** Shop-local date of `createdAt` (`11-18` in the order list). */
  orderDate: ShopDateSchema,
  /** createdAt + 3 days. */
  etaAt: InstantSchema,
  /** Shop-local date of `etaAt`: `预计 {M} 月 {D} 日 送达`. */
  etaDate: ShopDateSchema,
  status: OrderStatusSchema,
  /** In cart order. */
  items: z.array(OrderItemSchema).min(1),
  subtotal: MoneySchema,
  discount: MoneySchema,
  shipping: MoneySchema,
  /** 实付. */
  total: MoneySchema,
  /** The coupon used, e.g. `TTY10`, or `null`. */
  couponCode: z.string().min(1).nullable(),
  /** `寄往 {姓名} · {地区} {详细地址}`. */
  address: OrderAddressSchema,
})
export type Order = z.infer<typeof OrderSchema>

/** A line of an order-list row: `{name} {weight} ×{quantity}`. */
export const OrderSummaryLineSchema = z.strictObject({
  productId: z.string().min(1),
  name: z.string().min(1),
  weight: WeightSchema,
  quantity: z.int().min(1).max(9),
})
export type OrderSummaryLine = z.infer<typeof OrderSummaryLineSchema>

/** One row of `最近订单 ORDERS`. */
export const OrderSummarySchema = z.strictObject({
  number: OrderNumberSchema,
  createdAt: InstantSchema,
  orderDate: ShopDateSchema,
  status: OrderStatusSchema,
  total: MoneySchema,
  /** Σ quantity. */
  itemCount: z.int().positive(),
  lines: z.array(OrderSummaryLineSchema).min(1),
})
export type OrderSummary = z.infer<typeof OrderSummarySchema>

/** `GET /v1/orders`. */
export const OrderListSchema = z.strictObject({
  /** Newest first, at most `limit`. */
  items: z.array(OrderSummarySchema),
  /** How many orders the member has in all (the profile's `订单` stat). */
  total: z.int().nonnegative(),
})
export type OrderList = z.infer<typeof OrderListSchema>

/** Query of `GET /v1/orders`. */
export const OrderListQuerySchema = z.strictObject({
  /** 1–50, default 20. */
  limit: z.coerce.number<string>().int().min(1).max(50).optional(),
})
export type OrderListQuery = z.infer<typeof OrderListQuerySchema>

/** Body of `POST /v1/orders`. */
export const PlaceOrderRequestSchema = z.strictObject({
  /**
   * Chosen by the TUI once per checkout attempt (a retry reuses it). A key the member already
   * used answers that order again (201, same body, the cart untouched) whatever else is sent.
   */
  idempotencyKey: z.string().regex(/^[\w-]{8,64}$/),
  /**
   * The `totals.total` the member was shown. When given and the server's total differs,
   * the order is refused with `price_changed` and nothing changes.
   */
  expectedTotal: MoneySchema.optional(),
})
export type PlaceOrderRequest = z.input<typeof PlaceOrderRequestSchema>

/** Result of `POST /v1/orders`. */
export const PlacedOrderSchema = z.strictObject({
  order: OrderSchema,
  /** The cart afterwards: the purchased lines removed, the coupon cleared. */
  cart: CartSchema,
})
export type PlacedOrder = z.infer<typeof PlacedOrderSchema>

/** Path of `GET /v1/orders/{number}`. */
export const OrderParamsSchema = z.strictObject({
  number: OrderNumberSchema,
})
export type OrderParams = z.infer<typeof OrderParamsSchema>

export const orderComponents = {
  OrderStatus: OrderStatusSchema,
  OrderNumber: OrderNumberSchema,
  OrderItem: OrderItemSchema,
  OrderAddress: OrderAddressSchema,
  Order: OrderSchema,
  OrderSummaryLine: OrderSummaryLineSchema,
  OrderSummary: OrderSummarySchema,
  OrderList: OrderListSchema,
  PlaceOrderRequest: PlaceOrderRequestSchema,
  PlacedOrder: PlacedOrderSchema,
}

const member = {
  access: 'internal',
  auth: 'user',
  fingerprint: 'none',
  tag: 'orders',
} as const

export const placeOrder = defineEndpoint({
  ...member,
  method: 'post',
  path: `${API_PREFIX}/orders`,
  operationId: 'placeOrder',
  summary: '结算下单（演示：不扣款，下单即完成）',
  description: [
    'One transaction: take the selected lines (none: `empty_selection`) and the order address (none: `address_required`);',
    're-price them (`services/pricing.ts`) with the cart coupon; check each weight (`sold_out`) and that each product has stock for the sum of its lines (`out_of_stock`, details);',
    'compare `expectedTotal` (`price_changed`); decrement stock; create the order with a unique number (retry on collision), name/price/address snapshots and ETA = now + 3 days;',
    'delete the purchased lines and clear the coupon. On any error nothing changes.',
  ].join(' '),
  request: { body: PlaceOrderRequestSchema },
  success: { status: 201, schema: PlacedOrderSchema },
  errors: [
    'empty_selection',
    'address_required',
    'sold_out',
    'out_of_stock',
    'price_changed',
  ],
})

export const listOrders = defineEndpoint({
  ...member,
  method: 'get',
  path: `${API_PREFIX}/orders`,
  operationId: 'listOrders',
  summary: '订单列表（最新在前，状态按时间推进）',
  request: { query: OrderListQuerySchema },
  success: { status: 200, schema: OrderListSchema },
  errors: [],
})

export const getOrder = defineEndpoint({
  ...member,
  method: 'get',
  path: `${API_PREFIX}/orders/{number}`,
  operationId: 'getOrder',
  summary: '订单详情（回执）',
  request: { params: OrderParamsSchema },
  success: { status: 200, schema: OrderSchema },
  errors: ['not_found'],
})

export const orderEndpoints = defineEndpoints([placeOrder, listOrders, getOrder])

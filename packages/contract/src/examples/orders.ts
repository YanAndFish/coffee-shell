// Example orders of 林晓, read at 2026-11-22T13:04Z (21:04 shop time).
import type { Order, OrderItem, OrderList, OrderSummary, PlacedOrder } from '../orders.ts'
import type { Grind, Pack, Weight } from '../sku.ts'
import { homeAddress } from './addresses.ts'
import { cartEmptyExample, line } from './cart.ts'

function item(productId: string, weight: Weight, grind: Grind, pack: Pack, quantity: number): OrderItem {
  const { sku, name, unitPrice, lineTotal } = line(productId, weight, grind, pack, quantity)
  return {
    sku,
    productId,
    name,
    weight,
    grind,
    pack,
    quantity,
    unitPrice,
    lineTotal,
  }
}

const shippedTo: Order['address'] = {
  recipient: homeAddress.recipient,
  phone: homeAddress.phone,
  phoneMasked: homeAddress.phoneMasked,
  region: homeAddress.region,
  detail: homeAddress.detail,
}

/** The receipt of the design (`RT2611220417`, 426, 包邮), just placed: 烘焙中. */
export const orderJustPlacedExample: Order = {
  number: 'RT2611220417',
  createdAt: '2026-11-22T13:04:00.000Z',
  orderDate: '2026-11-22',
  etaAt: '2026-11-25T13:04:00.000Z',
  etaDate: '2026-11-25',
  status: 'roasting',
  items: [
    item('pinkb', '250g', 'PO', 'STD', 1),
    item('night', '500g', 'ES', 'STD', 1),
    item('geisha', '100g', 'WB', 'GIFT', 1),
  ],
  subtotal: 426,
  discount: 0,
  shipping: 0,
  total: 426,
  couponCode: null,
  address: shippedTo,
}

/** 118 + 167 = 285 (包邮), placed two days ago: 运输中. */
const orderShipping: Order = {
  number: 'RT2611200031',
  createdAt: '2026-11-20T12:10:00.000Z',
  orderDate: '2026-11-20',
  etaAt: '2026-11-23T12:10:00.000Z',
  etaDate: '2026-11-23',
  status: 'shipping',
  items: [
    item('pinkb', '250g', 'PO', 'STD', 1),
    item('night', '500g', 'ES', 'STD', 1),
  ],
  subtotal: 285,
  discount: 0,
  shipping: 0,
  total: 285,
  couponCode: null,
  address: shippedTo,
}

/** data.md 例 3: 98 × 2 = 196 < 199 → +12 = 208, delivered. */
export const orderDeliveredExample: Order = {
  number: 'RT2611060117',
  createdAt: '2026-11-06T09:30:00.000Z',
  orderDate: '2026-11-06',
  etaAt: '2026-11-09T09:30:00.000Z',
  etaDate: '2026-11-09',
  status: 'delivered',
  items: [
    item('yirga', '250g', 'PO', 'STD', 2),
  ],
  subtotal: 196,
  discount: 0,
  shipping: 12,
  total: 208,
  couponCode: null,
  address: shippedTo,
}

/** 121 < 199 → +12 = 133. */
const orderOld: Order = {
  number: 'RT2610210088',
  createdAt: '2026-10-21T11:00:00.000Z',
  orderDate: '2026-10-21',
  etaAt: '2026-10-24T11:00:00.000Z',
  etaDate: '2026-10-24',
  status: 'delivered',
  items: [
    item('geisha', '100g', 'PO', 'STD', 1),
  ],
  subtotal: 121,
  discount: 0,
  shipping: 12,
  total: 133,
  couponCode: null,
  address: shippedTo,
}

/** A TTY10 order: 186 − round(18.6) = 167 < 199 → +12 = 179. */
export const orderWithCouponExample: Order = {
  number: 'RT2611150203',
  createdAt: '2026-11-15T04:00:00.000Z',
  orderDate: '2026-11-15',
  etaAt: '2026-11-18T04:00:00.000Z',
  etaDate: '2026-11-18',
  status: 'delivered',
  items: [
    item('yirga', '500g', 'WB', 'STD', 1),
  ],
  subtotal: 186,
  discount: 19,
  shipping: 12,
  total: 179,
  couponCode: 'TTY10',
  address: shippedTo,
}

function summarize(order: Order): OrderSummary {
  return {
    number: order.number,
    createdAt: order.createdAt,
    orderDate: order.orderDate,
    status: order.status,
    total: order.total,
    itemCount: order.items.reduce((n, i) => n + i.quantity, 0),
    lines: order.items.map(i => ({
      productId: i.productId,
      name: i.name,
      weight: i.weight,
      quantity: i.quantity,
    })),
  }
}

/** The orders the examples above use, newest first (the profile's taste is computed from them). */
export const exampleOrders: readonly Order[] = [
  orderJustPlacedExample,
  orderShipping,
  orderDeliveredExample,
  orderOld,
]

/** `GET /v1/orders`: all three statuses. */
export const orderListExample: OrderList = {
  items: exampleOrders.map(summarize),
  total: exampleOrders.length,
}

/** `GET /v1/orders` of a new member. */
export const orderListEmptyExample: OrderList = {
  items: [],
  total: 0,
}

/** `POST /v1/orders`: the receipt and the cart afterwards (lines gone, address kept). */
export const placedOrderExample: PlacedOrder = {
  order: orderJustPlacedExample,
  cart: { ...cartEmptyExample, address: homeAddress },
}

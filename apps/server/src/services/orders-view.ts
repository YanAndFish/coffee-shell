// Order rows → the contract's `Order` (receipt) and `OrderSummary` (list row). The status is
// derived from the time of reading (P-order-3), never stored.
import type { Order, OrderItem, OrderSummary } from '@coffee/contract'
import type { Db } from '../db.ts'
import type { OrderItem as OrderItemRow, Order as OrderRow } from '../generated/prisma/client.ts'
import { GrindSchema, PackSchema, SkuSchema, WeightSchema } from '@coffee/contract'
import { toOrderAddress } from './address-book.ts'
import { instant, orderStatus, shopDate } from './shop-time.ts'

/** An order row with its lines (in `position` order). */
export type OrderWithItems = OrderRow & { items: OrderItemRow[] }

/** Include clause that loads the lines in their order. */
export const ORDER_ITEMS = {
  items: {
    orderBy: { position: 'asc' },
  },
} as const

function toOrderItem(row: OrderItemRow): OrderItem {
  return {
    sku: SkuSchema.parse(row.sku),
    productId: row.productId,
    name: row.productName,
    weight: WeightSchema.parse(row.weight),
    grind: GrindSchema.parse(row.grind),
    pack: PackSchema.parse(row.pack),
    quantity: row.quantity,
    unitPrice: row.unitPrice,
    lineTotal: row.lineTotal,
  }
}

/** The receipt of `row`, its status at `now`. */
export function toOrder(row: OrderWithItems, now: Date): Order {
  return {
    number: row.number,
    createdAt: instant(row.createdAt),
    orderDate: shopDate(row.createdAt),
    etaAt: instant(row.etaAt),
    etaDate: shopDate(row.etaAt),
    status: orderStatus(row.createdAt, row.etaAt, now),
    items: row.items.map(toOrderItem),
    subtotal: row.subtotal,
    discount: row.discount,
    shipping: row.shipping,
    total: row.total,
    couponCode: row.couponCode,
    address: toOrderAddress(row),
  }
}

/** The order-list row of `row`, its status at `now`. */
export function toOrderSummary(row: OrderWithItems, now: Date): OrderSummary {
  return {
    number: row.number,
    createdAt: instant(row.createdAt),
    orderDate: shopDate(row.createdAt),
    status: orderStatus(row.createdAt, row.etaAt, now),
    total: row.total,
    itemCount: row.items.reduce((n, item) => n + item.quantity, 0),
    lines: row.items.map(item => ({
      productId: item.productId,
      name: item.productName,
      weight: WeightSchema.parse(item.weight),
      quantity: item.quantity,
    })),
  }
}

/** The member's order `number` with its lines, or `null` (unknown or someone else's). */
export function findOrder(db: Db, userId: number, number: string): Promise<OrderWithItems | null> {
  return db.order.findFirst({
    where: { userId, number },
    include: ORDER_ITEMS,
  })
}

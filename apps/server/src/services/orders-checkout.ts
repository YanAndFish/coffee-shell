// Checkout: the demo's "payment" (P-order-1…5, U7). One transaction takes the selected cart
// lines and the order address, re-prices them with the shared rules (`loadCart` →
// `services/pricing.ts`), checks stock, decrements it, creates the order with snapshots and a
// unique number, and empties the purchased part of the cart. Any error rolls everything back.
import type { Cart, CartLine, PlacedOrder } from '@coffee/contract'
import type { Db, PrismaClient } from '../db.ts'
import { randomInt } from 'node:crypto'
import { Prisma } from '../generated/prisma/client.ts'
import { fail } from '../middleware/error-handler.ts'
import { toOrderAddress } from './address-book.ts'
import { loadCart } from './cart-view.ts'
import { ORDER_ITEMS, toOrder } from './orders-view.ts'
import { etaOf, orderNumber } from './shop-time.ts'

/** Draws the 4 random digits of an order number: an integer 0–9999. */
export type DrawOrderDigits = () => number

/** The real draw: uniform over 0000–9999 (P-order-1). */
export const randomOrderDigits: DrawOrderDigits = () => randomInt(0, 10_000)

/** Draws before giving up on finding a free number for the day (a 500). */
export const MAX_NUMBER_DRAWS = 100

export interface CheckoutRequest {
  userId: number
  idempotencyKey: string
  /** The total the member was shown; a different server total is `price_changed`. */
  expectedTotal?: number | undefined
  /** The clock reading the order is placed at. */
  now: Date
  /** The order-number digits (tests stub it); default {@link randomOrderDigits}. */
  draw?: DrawOrderDigits
}

/** The member's order placed with `key`, if any. */
function orderByKey(db: Db, userId: number, idempotencyKey: string) {
  return db.order.findUnique({
    where: {
      userId_idempotencyKey: { userId, idempotencyKey },
    },
    include: ORDER_ITEMS,
  })
}

/** A number for `now` that no order has yet: redraw while taken. */
async function freeOrderNumber(db: Db, now: Date, draw: DrawOrderDigits): Promise<string> {
  for (let i = 0; i < MAX_NUMBER_DRAWS; i++) {
    const number = orderNumber(now, draw())
    const taken = await db.order.findUnique({
      where: { number },
      select: { id: true },
    })
    if (!taken)
      return number
  }
  throw new Error(`no free order number after ${MAX_NUMBER_DRAWS} draws`)
}

/**
 * Units wanted per product, in cart order of first appearance (stock is shared by all
 * weights, so several lines of one product are checked together).
 */
function unitsByProduct(lines: readonly CartLine[]): Map<string, { name: string, quantity: number }> {
  const wanted = new Map<string, { name: string, quantity: number }>()
  for (const line of lines) {
    const entry = wanted.get(line.productId)
    if (entry)
      entry.quantity += line.quantity
    else
      wanted.set(line.productId, { name: line.name, quantity: line.quantity })
  }
  return wanted
}

/** Refuses a sold-out weight among the selected lines, then a product short of stock. */
async function checkStock(db: Db, selected: readonly CartLine[]): Promise<Map<string, { name: string, quantity: number }>> {
  for (const line of selected) {
    if (line.problem === 'sold_out')
      fail('sold_out', { productId: line.productId, productName: line.name, weight: line.weight })
  }
  const wanted = unitsByProduct(selected)
  const products = await db.product.findMany({
    where: {
      id: {
        in: [
          ...wanted.keys(),
        ],
      },
    },
    select: { id: true, stock: true },
  })
  for (const [productId, { name, quantity }] of wanted) {
    const available = products.find(p => p.id === productId)?.stock ?? 0
    if (quantity > available) {
      fail('out_of_stock', {
        productId,
        productName: name,
        available: Math.max(0, available),
      })
    }
  }
  return wanted
}

/** The checkout steps inside the transaction `tx`. */
async function placeIn(tx: Db, request: CheckoutRequest): Promise<PlacedOrder> {
  const { userId, now } = request
  const previous = await orderByKey(tx, userId, request.idempotencyKey)
  if (previous) {
    return {
      order: toOrder(previous, now),
      cart: await loadCart(tx, userId),
    }
  }

  const cart: Cart = await loadCart(tx, userId)
  const selected = cart.lines.filter(line => line.selected)
  if (selected.length === 0)
    fail('empty_selection')
  if (!cart.address)
    fail('address_required')
  const wanted = await checkStock(tx, selected)
  if (request.expectedTotal !== undefined && request.expectedTotal !== cart.totals.total)
    fail('price_changed')

  for (const [productId, { name, quantity }] of wanted) {
    // Conditional decrement: never below zero even if something raced past the check.
    const { count } = await tx.product.updateMany({
      where: {
        id: productId,
        stock: { gte: quantity },
      },
      data: {
        stock: { decrement: quantity },
      },
    })
    if (count !== 1) {
      const row = await tx.product.findUnique({
        where: { id: productId },
        select: { stock: true },
      })
      fail('out_of_stock', {
        productId,
        productName: name,
        available: Math.max(0, row?.stock ?? 0),
      })
    }
  }

  const number = await freeOrderNumber(tx, now, request.draw ?? randomOrderDigits)
  const { totals } = cart
  const order = await tx.order.create({
    data: {
      number,
      userId,
      idempotencyKey: request.idempotencyKey,
      createdAt: now,
      etaAt: etaOf(now),
      subtotal: totals.subtotal,
      discount: totals.discount,
      shipping: totals.shipping,
      total: totals.total,
      couponCode: cart.coupon?.code ?? null,
      ...toOrderAddressColumns(cart.address),
      items: {
        create: selected.map((line, position) => ({
          position,
          productId: line.productId,
          sku: line.sku,
          productName: line.name,
          weight: line.weight,
          grind: line.grind,
          pack: line.pack,
          quantity: line.quantity,
          unitPrice: line.unitPrice,
          lineTotal: line.lineTotal,
        })),
      },
    },
    include: ORDER_ITEMS,
  })

  await tx.cartItem.deleteMany({
    where: {
      userId,
      OR: selected.map(line => ({
        productId: line.productId,
        weight: line.weight,
        grind: line.grind,
        pack: line.pack,
      })),
    },
  })
  await tx.cart.updateMany({
    where: { userId },
    data: { couponCode: null },
  })
  return {
    order: toOrder(order, now),
    cart: await loadCart(tx, userId),
  }
}

/** The address columns of an order: the snapshot of the order address. */
function toOrderAddressColumns(address: NonNullable<Cart['address']>) {
  const { recipient, phone, region, detail } = toOrderAddress(address)
  return { recipient, phone, region, detail }
}

function isUniqueViolation(error: unknown): boolean {
  return error instanceof Prisma.PrismaClientKnownRequestError && error.code === 'P2002'
}

/**
 * Places the order (see the contract's `placeOrder`). A key the member already used answers
 * that order again with the cart as it is now; a concurrent request with the same key that
 * loses the race on the unique key does the same.
 */
export async function placeOrderNow(prisma: PrismaClient, request: CheckoutRequest): Promise<PlacedOrder> {
  try {
    return await prisma.$transaction(tx => placeIn(tx, request))
  }
  catch (error) {
    if (!isUniqueViolation(error))
      throw error
    const previous = await orderByKey(prisma, request.userId, request.idempotencyKey)
    if (!previous)
      throw error
    return {
      order: toOrder(previous, request.now),
      cart: await loadCart(prisma, request.userId),
    }
  }
}

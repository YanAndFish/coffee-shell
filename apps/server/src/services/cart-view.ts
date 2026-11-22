// Priced cart read model: rows → the contract's `Cart`. Shared foundation: the cart routes
// (server-shop) answer it, the guest quote prices client lines with it, and checkout
// (server-orders) answers the cart after placing an order.
import type { AppliedCoupon, Cart, CartLine, Grind, Pack, Weight } from '@coffee/contract'
import type { Db } from '../db.ts'
import type { Address as AddressRow } from '../generated/prisma/client.ts'
import type { ProductFacts } from './products.ts'
import { GrindSchema, PackSchema, WeightSchema } from '@coffee/contract'
import { addressRows, pickOrderAddress, toAddress } from './address-book.ts'
import { cartTotals, lineTotal, unitPrice } from './pricing.ts'
import { productFacts, productMaxQuantity, productSku, weightAvailable } from './products.ts'

/** A cart line before pricing (a `CartItem` row or a guest line). */
export interface LineChoice {
  productId: string
  weight: Weight
  grind: Grind
  pack: Pack
  quantity: number
  selected: boolean
}

/** A coupon row as the cart needs it. */
export interface CouponFacts {
  code: string
  description: string
  percentOff: number
}

/**
 * Prices one line: unit price from the product's base price, the line total, the stepper
 * bound, and its problem now (`sold_out` weight first, then quantity above stock).
 */
export function priceLine(facts: ProductFacts, choice: LineChoice): CartLine {
  const { row } = facts
  const unit = unitPrice(row.basePrice, choice.weight, choice.pack)
  return {
    sku: productSku(row, choice.weight, choice.grind, choice.pack),
    productId: row.id,
    name: row.name,
    origin: row.origin,
    roast: row.roast,
    weight: choice.weight,
    grind: choice.grind,
    pack: choice.pack,
    quantity: choice.quantity,
    selected: choice.selected,
    unitPrice: unit,
    lineTotal: lineTotal(unit, choice.quantity),
    maxQuantity: productMaxQuantity(row),
    problem: !weightAvailable(facts, choice.weight)
      ? 'sold_out'
      : choice.quantity > row.stock
        ? 'out_of_stock'
        : null,
  }
}

/** Assembles a `Cart` from priced lines, the coupon in use and the order address. */
export function buildCart(lines: CartLine[], coupon: CouponFacts | null, address: AddressRow | null): Cart {
  const applied: AppliedCoupon | null = coupon
    ? { code: coupon.code, description: coupon.description }
    : null
  return {
    lines,
    coupon: applied,
    address: address
      ? toAddress(address)
      : null,
    totals: cartTotals(lines, coupon),
  }
}

/** The member's cart, priced now (lines by position, then id; inactive coupons ignored). */
export async function loadCart(prisma: Db, userId: number): Promise<Cart> {
  const [cart, items, addresses] = await Promise.all([
    prisma.cart.findUnique({
      where: { userId },
      include: { coupon: true },
    }),
    prisma.cartItem.findMany({
      where: { userId },
      orderBy: [
        { position: 'asc' },
        { id: 'asc' },
      ],
      include: { product: true },
    }),
    addressRows(prisma, userId),
  ])
  const lines = items.map(item => priceLine(productFacts(item.product), {
    productId: item.productId,
    weight: WeightSchema.parse(item.weight),
    grind: GrindSchema.parse(item.grind),
    pack: PackSchema.parse(item.pack),
    quantity: item.quantity,
    selected: item.selected,
  }))
  const coupon = cart?.coupon?.active
    ? cart.coupon
    : null
  return buildCart(lines, coupon, pickOrderAddress(addresses, cart?.addressId ?? null))
}

// Area `cart` (owner: server-shop): the member cart, the guest quote and the merge after
// login. One function per endpoint, typed from the contract; rules in each endpoint's
// `description` (packages/contract/src/cart.ts) and docs/design/decisions.md (P-cart-*).
//
// Every mutation runs in one transaction and answers the cart read back through the shared
// read model (`cart-view.ts` → `pricing.ts`): no amount is computed here. Building blocks:
// `services/cart-lines.ts` (SKU lookup, stock rules, line order), `services/cart-guest.ts`
// (quote and merge).
import type {
  addCartLine as addCartLineEndpoint,
  applyCoupon as applyCouponEndpoint,
  Cart,
  getCart as getCartEndpoint,
  removeCartLine as removeCartLineEndpoint,
  removeCoupon as removeCouponEndpoint,
  setCartAddress as setCartAddressEndpoint,
  setCartSelection as setCartSelectionEndpoint,
  updateCartLine as updateCartLineEndpoint,
} from '@coffee/contract'
import type { Db } from '../db.ts'
import type { Deps } from '../deps.ts'
import type { HandlerInput } from '../http/route.ts'
import { MAX_CART_LINES } from '@coffee/contract'
import { fail } from '../middleware/error-handler.ts'
import {
  assertInStock,
  assertWeightAvailable,
  cartItems,
  ensureCart,
  findActiveCoupon,
  insertLine,
  mergedQuantity,
  requireLine,
  requireProduct,
} from './cart-lines.ts'
import { loadCart } from './cart-view.ts'
import { normalizeCouponCode } from './pricing.ts'

export { mergeCart, quoteCart } from './cart-guest.ts'

/** Runs `change` on the member's cart in one transaction, then answers the cart priced now. */
function mutateCart(deps: Deps, userId: number, change: (tx: Db) => Promise<void>): Promise<Cart> {
  return deps.prisma.$transaction(async (tx) => {
    await ensureCart(tx, userId)
    await change(tx)
    return loadCart(tx, userId)
  })
}

/** `GET /v1/cart`. */
export function getCart({ deps, user }: HandlerInput<typeof getCartEndpoint>): Promise<Cart> {
  return loadCart(deps.prisma, user.id)
}

/**
 * `POST /v1/cart/lines`. Same SKU: quantity = min(9, old + new), re-selected; else a new
 * selected line at `index` (default last). Checks in order: product (`not_found`), weight
 * (`sold_out`), resulting quantity ≤ stock (`out_of_stock`), a 51st line (`cart_full`).
 */
export function addCartLine({ deps, user, body }: HandlerInput<typeof addCartLineEndpoint>): Promise<Cart> {
  return mutateCart(deps, user.id, async (tx) => {
    const facts = await requireProduct(tx, body.productId)
    assertWeightAvailable(facts, body.weight)
    const choice = {
      productId: facts.row.id,
      weight: body.weight,
      grind: body.grind,
      pack: body.pack,
    }
    const existing = await tx.cartItem.findUnique({
      where: {
        userId_productId_weight_grind_pack: { userId: user.id, ...choice },
      },
    })
    if (existing) {
      const quantity = mergedQuantity(existing.quantity, body.quantity)
      assertInStock(facts, quantity)
      await tx.cartItem.update({
        where: { id: existing.id },
        data: { quantity, selected: true },
      })
      return
    }
    assertInStock(facts, body.quantity)
    const items = await cartItems(tx, user.id)
    if (items.length >= MAX_CART_LINES)
      fail('cart_full')
    await insertLine(tx, user.id, items, { ...choice, quantity: body.quantity, selected: true }, body.index)
  })
}

/**
 * `PATCH /v1/cart/lines/{sku}`. A new quantity must not exceed the stock (`out_of_stock`);
 * selection changes are always allowed.
 */
export function updateCartLine({ deps, user, params, body }: HandlerInput<typeof updateCartLineEndpoint>): Promise<Cart> {
  return mutateCart(deps, user.id, async (tx) => {
    const { item, facts } = await requireLine(tx, user.id, params.sku)
    if (body.quantity !== undefined)
      assertInStock(facts, body.quantity)
    await tx.cartItem.update({
      where: { id: item.id },
      data: {
        ...(body.quantity !== undefined
          ? { quantity: body.quantity }
          : {}),
        ...(body.selected !== undefined
          ? { selected: body.selected }
          : {}),
      },
    })
  })
}

/** `DELETE /v1/cart/lines/{sku}` (undo = `POST /v1/cart/lines` with the old `index`). */
export function removeCartLine({ deps, user, params }: HandlerInput<typeof removeCartLineEndpoint>): Promise<Cart> {
  return mutateCart(deps, user.id, async (tx) => {
    const { item } = await requireLine(tx, user.id, params.sku)
    await tx.cartItem.delete({
      where: { id: item.id },
    })
  })
}

/** `PUT /v1/cart/selection`: select or unselect every line. */
export function setCartSelection({ deps, user, body }: HandlerInput<typeof setCartSelectionEndpoint>): Promise<Cart> {
  return mutateCart(deps, user.id, async (tx) => {
    await tx.cartItem.updateMany({
      where: { userId: user.id },
      data: { selected: body.selected },
    })
  })
}

/** `PUT /v1/cart/coupon`: trimmed, case-insensitive; empty = remove; unknown = `coupon_invalid`, cart unchanged. */
export function applyCoupon({ deps, user, body }: HandlerInput<typeof applyCouponEndpoint>): Promise<Cart> {
  return mutateCart(deps, user.id, async (tx) => {
    const code = normalizeCouponCode(body.code)
    const coupon = code === null
      ? null
      : (await findActiveCoupon(tx, code)) ?? fail('coupon_invalid')
    await tx.cart.update({
      where: { userId: user.id },
      data: { couponCode: coupon?.code ?? null },
    })
  })
}

/** `DELETE /v1/cart/coupon`. */
export function removeCoupon({ deps, user }: HandlerInput<typeof removeCouponEndpoint>): Promise<Cart> {
  return mutateCart(deps, user.id, async (tx) => {
    await tx.cart.update({
      where: { userId: user.id },
      data: { couponCode: null },
    })
  })
}

/** `PUT /v1/cart/address`: one of the member's own addresses, else `not_found`. */
export function setCartAddress({ deps, user, body }: HandlerInput<typeof setCartAddressEndpoint>): Promise<Cart> {
  return mutateCart(deps, user.id, async (tx) => {
    const address = await tx.address.findFirst({
      where: { id: body.addressId, userId: user.id },
      select: { id: true },
    }) ?? fail('not_found')
    await tx.cart.update({
      where: { userId: user.id },
      data: { addressId: address.id },
    })
  })
}

// The guest cart (owner: server-shop): the TUI holds it until login; the server prices it
// (`POST /v1/cart/quote`, nothing stored) and merges it into the member cart right after
// login (`POST /v1/cart/merge`). Decisions P-cart-6, P-cart-7, P-cart-8.
import type {
  Cart,
  CartLine,
  CartMergeResult,
  GuestCartLine,
  mergeCart as mergeCartEndpoint,
  quoteCart as quoteCartEndpoint,
} from '@coffee/contract'
import type { CartItem } from '../generated/prisma/client.ts'
import type { HandlerInput } from '../http/route.ts'
import type { SkuChoice } from './cart-lines.ts'
import type { LineChoice } from './cart-view.ts'
import type { ProductFacts } from './products.ts'
import { createHash } from 'node:crypto'
import { MAX_CART_LINES } from '@coffee/contract'
import { Prisma } from '../generated/prisma/client.ts'
import { fail } from '../middleware/error-handler.ts'
import {
  cartItems,
  choiceKey,
  ensureCart,
  findActiveCoupon,
  itemChoice,
  mergedQuantity,
  nextPosition,
  productsById,
} from './cart-lines.ts'
import { buildCart, loadCart, priceLine } from './cart-view.ts'
import { MAX_QUANTITY, normalizeCouponCode } from './pricing.ts'
import { weightAvailable } from './products.ts'

/** A guest line after the contract's defaults (`selected` defaults to `true`). */
type GuestLine = Omit<GuestCartLine, 'selected'> & { selected: boolean }

/**
 * Folds guest lines of the same SKU into the first one: quantity = min(9, sum), selected
 * when any of them is. Order = first appearance.
 */
export function foldGuestLines(lines: readonly GuestLine[]): LineChoice[] {
  const folded = new Map<string, LineChoice>()
  for (const line of lines) {
    const key = choiceKey(line)
    const seen = folded.get(key)
    if (seen) {
      seen.quantity = mergedQuantity(seen.quantity, line.quantity)
      seen.selected ||= line.selected
    }
    else {
      folded.set(key, { ...line })
    }
  }
  return [
    ...folded.values(),
  ]
}

/**
 * `POST /v1/cart/quote`: prices the guest lines as the member cart would. Unknown products
 * are `not_found`; sold-out weights and quantities above stock are priced and flagged; a
 * non-empty unknown coupon is `coupon_invalid`. `address` is always `null`.
 */
export async function quoteCart({ deps, body }: HandlerInput<typeof quoteCartEndpoint>): Promise<Cart> {
  const choices = foldGuestLines(body.lines)
  const products = await productsById(deps.prisma, choices.map(c => c.productId))
  const lines: CartLine[] = choices.map(choice => priceLine(products.get(choice.productId) ?? fail('not_found'), choice))
  const code = normalizeCouponCode(body.couponCode)
  const coupon = code === null
    ? null
    : (await findActiveCoupon(deps.prisma, code)) ?? fail('coupon_invalid')
  return buildCart(lines, coupon, null)
}

/** What a merge does to the member cart, before it is written. */
interface MergePlan {
  updates: Map<number, { quantity: number, selected: boolean }>
  creates: (SkuChoice & { quantity: number, selected: boolean })[]
  mergedCount: number
  skippedCount: number
}

/** 返回访客商品和优惠码的摘要。字段顺序和优惠码写法不影响摘要。 */
function mergeRequestHash(body: HandlerInput<typeof mergeCartEndpoint>['body']): string {
  return createHash('sha256').update(JSON.stringify({
    lines: body.lines.map(({ productId, weight, grind, pack, quantity, selected }) => ({
      productId,
      weight,
      grind,
      pack,
      quantity,
      selected,
    })),
    couponCode: normalizeCouponCode(body.couponCode),
  })).digest('hex')
}

/**
 * 把访客商品合并到会员购物车，返回购物车和实际合并、跳过的数量。
 * 同一账户的相同请求标识只执行一次。重试返回首次数量和当前购物车。
 * 有效优惠码替换会员优惠码，无法购买的商品行会被跳过。
 */
export async function mergeCart({ deps, user, body }: HandlerInput<typeof mergeCartEndpoint>): Promise<CartMergeResult> {
  const key = body.idempotencyKey
  const requestHash = mergeRequestHash(body)
  try {
    return await deps.prisma.$transaction(async (tx) => {
      if (key !== undefined) {
        // 先写入请求记录，让 SQLite 在读取购物车前取得写锁。合并失败时记录也会回滚。
        await tx.cartMerge.create({
          data: {
            userId: user.id,
            idempotencyKey: key,
            requestHash,
            mergedCount: 0,
            skippedCount: 0,
          },
        })
      }
      await ensureCart(tx, user.id)
      const items = await cartItems(tx, user.id)
      const products = await productsById(tx, body.lines.map(line => line.productId))
      const plan = planMerge(items, body.lines, products)
      for (const [id, data] of plan.updates) {
        await tx.cartItem.update({
          where: { id },
          data,
        })
      }
      let position = nextPosition(items)
      for (const line of plan.creates) {
        await tx.cartItem.create({
          data: { userId: user.id, ...line, position },
        })
        position++
      }
      const code = normalizeCouponCode(body.couponCode)
      const coupon = code === null
        ? null
        : await findActiveCoupon(tx, code)
      if (coupon) {
        await tx.cart.update({
          where: { userId: user.id },
          data: { couponCode: coupon.code },
        })
      }
      if (key !== undefined) {
        await tx.cartMerge.update({
          where: {
            userId_idempotencyKey: { userId: user.id, idempotencyKey: key },
          },
          data: { mergedCount: plan.mergedCount, skippedCount: plan.skippedCount },
        })
      }
      return {
        cart: await loadCart(tx, user.id),
        mergedCount: plan.mergedCount,
        skippedCount: plan.skippedCount,
      }
    })
  }
  catch (error) {
    if (key === undefined || !(error instanceof Prisma.PrismaClientKnownRequestError) || error.code !== 'P2002')
      throw error
    const previous = await deps.prisma.cartMerge.findUnique({
      where: {
        userId_idempotencyKey: { userId: user.id, idempotencyKey: key },
      },
    })
    if (!previous)
      throw error
    if (previous.requestHash !== requestHash)
      fail('validation_failed', { field: 'body.idempotencyKey' })
    return {
      cart: await loadCart(deps.prisma, user.id),
      mergedCount: previous.mergedCount,
      skippedCount: previous.skippedCount,
    }
  }
}

/** Plans a merge of `guest` lines into the member's `items`, given their `products` (pure). */
export function planMerge(
  items: readonly CartItem[],
  guest: readonly GuestLine[],
  products: ReadonlyMap<string, ProductFacts>,
): MergePlan {
  const plan: MergePlan = {
    updates: new Map(),
    creates: [],
    mergedCount: 0,
    skippedCount: 0,
  }
  const existing = new Map(items.map(item => [
    choiceKey(itemChoice(item)),
    { id: item.id, quantity: item.quantity, selected: item.selected },
  ]))
  const created = new Map<string, MergePlan['creates'][number]>()
  for (const line of guest) {
    const facts = products.get(line.productId)
    if (!facts || !weightAvailable(facts, line.weight) || facts.row.stock <= 0) {
      plan.skippedCount++
      continue
    }
    const cap = Math.min(MAX_QUANTITY, facts.row.stock)
    const key = choiceKey(line)
    const target = existing.get(key) ?? created.get(key)
    if (target) {
      const quantity = Math.max(target.quantity, Math.min(cap, target.quantity + line.quantity))
      plan.mergedCount += quantity - target.quantity
      target.quantity = quantity
      target.selected ||= line.selected
      if ('id' in target)
        plan.updates.set(target.id, { quantity: target.quantity, selected: target.selected })
      continue
    }
    if (existing.size + created.size >= MAX_CART_LINES) {
      plan.skippedCount++
      continue
    }
    const quantity = Math.min(cap, line.quantity)
    const row = {
      productId: line.productId,
      weight: line.weight,
      grind: line.grind,
      pack: line.pack,
      quantity,
      selected: line.selected,
    }
    created.set(key, row)
    plan.creates.push(row)
    plan.mergedCount += quantity
  }
  return plan
}

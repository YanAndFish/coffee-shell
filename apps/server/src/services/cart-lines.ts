// Building blocks of the cart endpoints (owner: server-shop): finding products and lines by
// SKU, the per-line stock rules, and writing line order. Money stays in `pricing.ts` /
// `cart-view.ts`; this file only decides which rows exist and with which quantities.
import type { Grind, Pack, Weight } from '@coffee/contract'
import type { Db } from '../db.ts'
import type { CartItem } from '../generated/prisma/client.ts'
import type { CouponFacts } from './cart-view.ts'
import type { ProductFacts } from './products.ts'
import { GrindSchema, PackSchema, WEIGHTS, WeightSchema } from '@coffee/contract'
import { fail } from '../middleware/error-handler.ts'
import { MAX_QUANTITY } from './pricing.ts'
import { productFacts, weightAvailable } from './products.ts'

/** A line's choice of weight, grind and pack (with its product: the SKU). */
export interface SkuChoice {
  productId: string
  weight: Weight
  grind: Grind
  pack: Pack
}

/** The key of a line within one member's cart (product × weight × grind × pack). */
export function choiceKey(choice: SkuChoice): string {
  return `${choice.productId}\u0000${choice.weight}\u0000${choice.grind}\u0000${choice.pack}`
}

/** A stored line as a {@link SkuChoice} (columns validated with the contract's enums). */
export function itemChoice(item: CartItem): SkuChoice {
  return {
    productId: item.productId,
    weight: WeightSchema.parse(item.weight),
    grind: GrindSchema.parse(item.grind),
    pack: PackSchema.parse(item.pack),
  }
}

const SKU_PATTERN = /^RT-([A-Z]{3})-(100G|250G|500G|1KG)-([A-Z]{2})-([A-Z]+)$/

/** Splits a SKU (already checked by the contract's `SkuSchema`) into product code and choice. */
export function parseSku(sku: string): { code: string, weight: Weight, grind: Grind, pack: Pack } | null {
  const match = SKU_PATTERN.exec(sku)
  if (!match)
    return null
  const [, code, weightCode, grind, pack] = match
  const weight = WEIGHTS.find(w => w.toUpperCase() === weightCode)
  const parsedGrind = GrindSchema.safeParse(grind)
  const parsedPack = PackSchema.safeParse(pack)
  if (!code || !weight || !parsedGrind.success || !parsedPack.success)
    return null
  return {
    code,
    weight,
    grind: parsedGrind.data,
    pack: parsedPack.data,
  }
}

/** The product `productId` with its JSON columns parsed, or `404 not_found`. */
export async function requireProduct(db: Db, productId: string): Promise<ProductFacts> {
  const row = await db.product.findUnique({
    where: { id: productId },
  })
  return productFacts(row ?? fail('not_found'))
}

/** The products among `ids`, keyed by id (missing ids are absent). */
export async function productsById(db: Db, ids: readonly string[]): Promise<Map<string, ProductFacts>> {
  const rows = await db.product.findMany({
    where: {
      id: {
        in: [
          ...new Set(ids),
        ],
      },
    },
  })
  return new Map(rows.map(row => [
    row.id,
    productFacts(row),
  ]))
}

/** The member's line with this SKU and its product, or `404 not_found`. */
export async function requireLine(db: Db, userId: number, sku: string): Promise<{ item: CartItem, facts: ProductFacts }> {
  const parsed = parseSku(sku) ?? fail('not_found')
  const product = await db.product.findUnique({
    where: { code: parsed.code },
  }) ?? fail('not_found')
  const item = await db.cartItem.findUnique({
    where: {
      userId_productId_weight_grind_pack: {
        userId,
        productId: product.id,
        weight: parsed.weight,
        grind: parsed.grind,
        pack: parsed.pack,
      },
    },
  }) ?? fail('not_found')
  return {
    item,
    facts: productFacts(product),
  }
}

/** The active coupon with this canonical code, or `null`. */
export function findActiveCoupon(db: Db, code: string): Promise<CouponFacts | null> {
  return db.coupon.findFirst({
    where: { code, active: true },
    select: { code: true, description: true, percentOff: true },
  })
}

/** `409 sold_out` when the weight cannot be bought. */
export function assertWeightAvailable(facts: ProductFacts, weight: Weight): void {
  if (!weightAvailable(facts, weight)) {
    fail('sold_out', {
      productId: facts.row.id,
      productName: facts.row.name,
      weight,
    })
  }
}

/** `409 out_of_stock` when one line of `quantity` units is above the product's stock. */
export function assertInStock(facts: ProductFacts, quantity: number): void {
  if (quantity > facts.row.stock) {
    fail('out_of_stock', {
      productId: facts.row.id,
      productName: facts.row.name,
      available: facts.row.stock,
    })
  }
}

/** Adding `added` units to a line of `current`: never above {@link MAX_QUANTITY}. */
export function mergedQuantity(current: number, added: number): number {
  return Math.min(MAX_QUANTITY, current + added)
}

/** Creates the member's cart row when missing (lines reference it). */
export async function ensureCart(db: Db, userId: number): Promise<void> {
  await db.cart.upsert({
    where: { userId },
    create: { userId },
    update: {},
  })
}

/** The member's lines in cart order (position, then id: the order `loadCart` shows). */
export function cartItems(db: Db, userId: number): Promise<CartItem[]> {
  return db.cartItem.findMany({
    where: { userId },
    orderBy: [
      { position: 'asc' },
      { id: 'asc' },
    ],
  })
}

/** The position after every line of the member's cart. */
export function nextPosition(items: readonly CartItem[]): number {
  return items.reduce((max, item) => Math.max(max, item.position + 1), 0)
}

/**
 * Creates a line at 0-based `index` of the cart order (`undefined` or past the end = last):
 * the lines from `index` on move one place down, renumbered 0, 1, 2, … where needed.
 */
export async function insertLine(
  db: Db,
  userId: number,
  items: readonly CartItem[],
  line: SkuChoice & { quantity: number, selected: boolean },
  index: number | undefined,
): Promise<void> {
  if (index === undefined || index >= items.length) {
    await db.cartItem.create({
      data: {
        userId,
        ...line,
        position: nextPosition(items),
      },
    })
    return
  }
  for (const [i, item] of items.entries()) {
    const position = i < index
      ? i
      : i + 1
    if (item.position !== position) {
      await db.cartItem.update({
        where: { id: item.id },
        data: { position },
      })
    }
  }
  await db.cartItem.create({
    data: { userId, ...line, position: index },
  })
}

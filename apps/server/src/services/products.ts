// Product rows → contract values, and the per-weight rules every area shares (catalog,
// cart, checkout, profile). Shared foundation: change only with every area in mind.
import type { Flavor, Grind, Pack, ProductSummary, Weight } from '@coffee/contract'
import type { Product } from '../generated/prisma/client.ts'
import { FlavorSchema, skuCode, WEIGHTS, WeightSchema } from '@coffee/contract'
import { z } from 'zod'
import { parseJsonColumn } from './json-columns.ts'
import { maxQuantity } from './pricing.ts'

const Notes = z.array(z.string().min(1))
const SoldOut = z.array(WeightSchema)

/** A product row with its JSON columns parsed (corrupt data throws → 500). */
export interface ProductFacts {
  row: Product
  notes: string[]
  flavor: Flavor
  soldOutWeights: Weight[]
}

export function productFacts(row: Product): ProductFacts {
  return {
    row,
    notes: parseJsonColumn(Notes, row.notes),
    flavor: parseJsonColumn(FlavorSchema, row.flavor),
    soldOutWeights: parseJsonColumn(SoldOut, row.soldOutWeights),
  }
}

/** Whether `weight` can be bought (not in the sold-out list). */
export function weightAvailable(facts: ProductFacts, weight: Weight): boolean {
  return !facts.soldOutWeights.includes(weight)
}

/** 250g, else the first weight in stock (`firstAvail`); 250g when everything is sold out. */
export function defaultWeight(soldOutWeights: readonly Weight[]): Weight {
  if (!soldOutWeights.includes('250g'))
    return '250g'
  return WEIGHTS.find(w => !soldOutWeights.includes(w)) ?? '250g'
}

/** The stepper's upper bound of this product: min(9, stock). */
export function productMaxQuantity(row: Pick<Product, 'stock'>): number {
  return maxQuantity(row.stock)
}

/** `RT-{code}-{WEIGHT}-{grind}-{pack}`. */
export function productSku(row: Pick<Product, 'code'>, weight: Weight, grind: Grind, pack: Pack): string {
  return skuCode(row.code, weight, grind, pack)
}

/** Maps a database row to the list DTO. */
export function toProductSummary(row: Product): ProductSummary {
  const facts = productFacts(row)
  return {
    id: row.id,
    name: row.name,
    subtitle: row.subtitle,
    origin: row.origin,
    region: row.region,
    process: row.process,
    roast: row.roast,
    tag: row.tag,
    notes: facts.notes,
    flavor: facts.flavor,
    score: row.score,
    description: row.description,
    price250: row.basePrice,
    stock: row.stock,
    defaultWeight: defaultWeight(facts.soldOutWeights),
  }
}

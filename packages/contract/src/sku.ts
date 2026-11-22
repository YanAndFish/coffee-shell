// The SKU dimensions (docs/design/data.md#sku-维度): weight × grind × pack. Codes travel in
// the API; the labels are what the UI shows. Prices are not here: they are server-side only.
import { z } from 'zod'

/** Weight options, in display order. */
export const WEIGHTS = ['100g', '250g', '500g', '1kg'] as const
export const WeightSchema = z.enum(WEIGHTS)
export type Weight = z.infer<typeof WeightSchema>

/** Grind codes, in display order: 整豆 手冲 意式 法压 冷萃. */
export const GRINDS = ['WB', 'PO', 'ES', 'FP', 'CB'] as const
export const GrindSchema = z.enum(GRINDS)
export type Grind = z.infer<typeof GrindSchema>

/** Pack codes, in display order: 标准袋 礼盒装. */
export const PACKS = ['STD', 'GIFT'] as const
export const PackSchema = z.enum(PACKS)
export type Pack = z.infer<typeof PackSchema>

/** Display label of each weight (the key itself). */
export const WEIGHT_LABELS: Record<Weight, string> = {
  '100g': '100g',
  '250g': '250g',
  '500g': '500g',
  '1kg': '1kg',
}

/** Display label of each grind. */
export const GRIND_LABELS: Record<Grind, string> = {
  WB: '整豆',
  PO: '手冲',
  ES: '意式',
  FP: '法压',
  CB: '冷萃',
}

/** Display label of each pack (the gift surcharge, `+¥20`, is copy, not data). */
export const PACK_LABELS: Record<Pack, string> = { STD: '标准袋', GIFT: '礼盒装' }

/**
 * A SKU code, `RT-{product code}-{WEIGHT}-{grind}-{pack}`, e.g. `RT-PNK-250G-WB-STD`. It
 * identifies a cart line (one per user and SKU) and is the `{sku}` path parameter.
 */
export const SkuSchema = z.string().regex(/^RT-[A-Z]{3}-(?:100G|250G|500G|1KG)-(?:WB|PO|ES|FP|CB)-(?:STD|GIFT)$/)
export type Sku = z.infer<typeof SkuSchema>

/** The SKU code of a product code (`PNK`) and a choice. */
export function skuCode(productCode: string, weight: Weight, grind: Grind, pack: Pack): Sku {
  return `RT-${productCode}-${weight.toUpperCase()}-${grind}-${pack}`
}

/** Roast level, 1 (light) to 5 (dark). */
export const RoastLevelSchema = z.int().min(1).max(5)
export type RoastLevel = z.infer<typeof RoastLevelSchema>

/** Roast level names, index = level − 1. */
export const ROAST_NAMES = ['浅烘', '中浅烘', '中烘', '中深烘', '深烘'] as const

export const skuComponents = {
  Weight: WeightSchema,
  Grind: GrindSchema,
  Pack: PackSchema,
  Sku: SkuSchema,
  RoastLevel: RoastLevelSchema,
}

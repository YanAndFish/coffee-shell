import { z } from 'zod'
import { API_PREFIX } from './common.ts'
import { defineEndpoints } from './endpoint.ts'

/** Flavor axes, in the order of {@link Flavor}. */
export const FLAVOR_AXES = ['酸度', '甜感', '醇厚', '苦度', '余韵', '香气'] as const

/** Flavor profile: six scores 0..=10 in the order of {@link FLAVOR_AXES}. */
export const FlavorSchema = z.array(z.int().min(0).max(10)).length(FLAVOR_AXES.length)
export type Flavor = z.infer<typeof FlavorSchema>

/** Roast level, 1 (light) to 5 (dark). */
export const RoastLevelSchema = z.int().min(1).max(5)
export type RoastLevel = z.infer<typeof RoastLevelSchema>

/** One product in a list (`GET /v1/products`). */
export const ProductSummarySchema = z.strictObject({
  /** Stable id, e.g. `yirga`. */
  id: z.string().min(1),
  /** Display name, e.g. `耶加雪菲 孔加`. */
  name: z.string().min(1),
  /** Latin subtitle, e.g. `Ethiopia · Yirgacheffe Konga G1`. */
  subtitle: z.string(),
  /** Origin country or `拼配`. */
  origin: z.string().min(1),
  /** Processing method, e.g. `水洗`. */
  process: z.string().min(1),
  roast: RoastLevelSchema,
  /** Badge such as `新产季`, or `null`. */
  tag: z.string().min(1).nullable(),
  /** Tasting notes. */
  notes: z.array(z.string().min(1)),
  flavor: FlavorSchema,
  /** Cupping score, e.g. `87.5`. */
  score: z.number().min(0).max(100),
  /** Base price of 250 g, integer yuan. */
  price250: z.int().nonnegative(),
  /** Units in stock. */
  stock: z.int().nonnegative(),
})
export type ProductSummary = z.infer<typeof ProductSummarySchema>

/** `GET /v1/products`. */
export const ProductListSchema = z.strictObject({
  /** The products, in display order. */
  items: z.array(ProductSummarySchema),
})
export type ProductList = z.infer<typeof ProductListSchema>

export const productComponents = {
  Flavor: FlavorSchema,
  RoastLevel: RoastLevelSchema,
  ProductSummary: ProductSummarySchema,
  ProductList: ProductListSchema,
}

export const productEndpoints = defineEndpoints([
  {
    method: 'get',
    path: `${API_PREFIX}/products`,
    operationId: 'listProducts',
    tag: 'products',
    summary: '商品列表（按陈列顺序）',
    access: 'internal',
    success: { status: 200, schema: ProductListSchema },
    errors: [401],
  },
])

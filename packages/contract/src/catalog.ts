// Catalog: the product list (filters, sorts, search, facets) and the product detail.
// Owner in phase 2 wave 2: server-shop.
import { z } from 'zod'
import { API_PREFIX, MoneySchema } from './common.ts'
import { defineEndpoint, defineEndpoints } from './endpoint.ts'
import { PackSchema, RoastLevelSchema, WeightSchema } from './sku.ts'

/** Flavor axes, in the order of {@link Flavor}. */
export const FLAVOR_AXES = ['酸度', '甜感', '醇厚', '苦度', '余韵', '香气'] as const

/** Flavor profile: six scores 0..=10 in the order of {@link FLAVOR_AXES}. */
export const FlavorSchema = z.array(z.int().min(0).max(10)).length(FLAVOR_AXES.length)
export type Flavor = z.infer<typeof FlavorSchema>

/** Roast band filter: `light` = levels 1–2 (浅), `medium` = 3 (中), `dark` = 4–5 (深). */
export const RoastBandSchema = z.enum(['light', 'medium', 'dark'])
export type RoastBand = z.infer<typeof RoastBandSchema>

/**
 * List order: `recommended` = display order; `price_asc` / `price_desc` = by the 250 g base
 * price; `score` = cupping score high to low. Ties keep display order.
 */
export const ProductSortSchema = z.enum([
  'recommended',
  'price_asc',
  'price_desc',
  'score',
])
export type ProductSort = z.infer<typeof ProductSortSchema>

/** One product in a list (`GET /v1/products`). */
export const ProductSummarySchema = z.strictObject({
  /** Stable id, e.g. `yirga`. */
  id: z.string().min(1),
  /** Display name, e.g. `耶加雪菲 孔加`. */
  name: z.string().min(1),
  /** Latin subtitle, e.g. `Ethiopia · Yirgacheffe Konga G1`. */
  subtitle: z.string(),
  /** Origin country or `拼配`; the origin filter matches it exactly. */
  origin: z.string().min(1),
  /** Region, e.g. `耶加雪菲 · 孔加`. */
  region: z.string().min(1),
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
  /** The preview's description. */
  description: z.string().min(1),
  /** Base price of 250 g, integer yuan (the list price and the preview's `¥98 / 250g 起`). */
  price250: MoneySchema,
  /** Units in stock (all weights share it). */
  stock: z.int().nonnegative(),
  /** What quick add (`a`) and the detail page start with: 250g, else the first weight in stock. */
  defaultWeight: WeightSchema,
})
export type ProductSummary = z.infer<typeof ProductSummarySchema>

/** One origin of the filter sidebar with its product count. */
export const OriginFacetSchema = z.strictObject({
  origin: z.string().min(1),
  count: z.int().positive(),
})
export type OriginFacet = z.infer<typeof OriginFacetSchema>

/** The filter sidebar's counts: always over the whole catalog, whatever the filters. */
export const ProductFacetsSchema = z.strictObject({
  /** All products: the `全部` count. */
  total: z.int().nonnegative(),
  /** Each origin in order of first appearance in display order. */
  origins: z.array(OriginFacetSchema),
})
export type ProductFacets = z.infer<typeof ProductFacetsSchema>

/** `GET /v1/products`. */
export const ProductListSchema = z.strictObject({
  /** The matching products, in the requested order. */
  items: z.array(ProductSummarySchema),
  facets: ProductFacetsSchema,
})
export type ProductList = z.infer<typeof ProductListSchema>

/** Query of `GET /v1/products`; every parameter is optional. */
export const ProductListQuerySchema = z.strictObject({
  /** Exact origin (`埃塞俄比亚`); absent or empty = all. An unknown origin matches nothing. */
  origin: z.string().max(32).optional(),
  roast: RoastBandSchema.optional(),
  sort: ProductSortSchema.optional(),
  /**
   * Search text: trimmed and lower-cased, then a substring of name + subtitle + origin +
   * region + process + notes (lower-cased, joined by spaces). Empty = no search.
   */
  q: z.string().max(64).optional(),
})
export type ProductListQuery = z.infer<typeof ProductListQuerySchema>

/** Whether one weight can be bought. */
export const WeightOptionSchema = z.strictObject({
  weight: WeightSchema,
  /** `false` for the sold-out weights (pinkb 1kg; geisha 500g, 1kg). */
  available: z.boolean(),
})
export type WeightOption = z.infer<typeof WeightOptionSchema>

/** The price of one weight × pack, for the SKU panel (the grind does not change prices). */
export const PriceCellSchema = z.strictObject({
  weight: WeightSchema,
  pack: PackSchema,
  /** round(price250 × weight factor) + pack surcharge, integer yuan. */
  unitPrice: MoneySchema,
  /**
   * `totals[n − 1]` = the line total for quantity `n`, for n = 1 … `maxQuantity` (the SKU
   * panel's `¥{合计}`); empty when nothing can be bought.
   */
  totals: z.array(MoneySchema),
})
export type PriceCell = z.infer<typeof PriceCellSchema>

/** `GET /v1/products/{id}`: everything the detail page shows. */
export const ProductDetailSchema = z.strictObject({
  ...ProductSummarySchema.shape,
  /** Three-letter SKU code, e.g. `PNK` (`RT-PNK-250G-WB-STD`). */
  code: z.string().regex(/^[A-Z]{3}$/),
  /** `庄园`. */
  farm: z.string().min(1),
  /** `海拔` as displayed, e.g. `1,950–2,200 m` (`—` for the blend). */
  altitude: z.string().min(1),
  /** `品种`. */
  variety: z.string().min(1),
  /** `产季`. */
  harvest: z.string().min(1),
  /** `冲煮建议`, e.g. `1:15 · 92°C · 2′30″`. */
  brew: z.string().min(1),
  /** min(9, stock): the quantity stepper's upper bound (0 = 暂时缺货, buying disabled). */
  maxQuantity: z.int().min(0).max(9),
  /** The four weights in display order. */
  weights: z.array(WeightOptionSchema).length(4),
  /** Eight cells: each weight (display order) × STD, GIFT. Sold-out weights are priced too. */
  prices: z.array(PriceCellSchema).length(8),
})
export type ProductDetail = z.infer<typeof ProductDetailSchema>

/** Path of `GET /v1/products/{id}`. */
export const ProductParamsSchema = z.strictObject({
  id: z.string().min(1).max(32),
})
export type ProductParams = z.infer<typeof ProductParamsSchema>

export const catalogComponents = {
  Flavor: FlavorSchema,
  RoastBand: RoastBandSchema,
  ProductSort: ProductSortSchema,
  ProductSummary: ProductSummarySchema,
  OriginFacet: OriginFacetSchema,
  ProductFacets: ProductFacetsSchema,
  ProductList: ProductListSchema,
  WeightOption: WeightOptionSchema,
  PriceCell: PriceCellSchema,
  ProductDetail: ProductDetailSchema,
}

export const listProducts = defineEndpoint({
  method: 'get',
  path: `${API_PREFIX}/products`,
  operationId: 'listProducts',
  tag: 'catalog',
  summary: '商品列表：筛选、排序、搜索与筛选栏计数',
  description: 'Filters combine (AND). `facets` always count the whole catalog. Each product keeps the same fields whatever the filters.',
  access: 'internal',
  auth: 'none',
  fingerprint: 'none',
  request: { query: ProductListQuerySchema },
  success: { status: 200, schema: ProductListSchema },
  errors: [],
})

export const getProduct = defineEndpoint({
  method: 'get',
  path: `${API_PREFIX}/products/{id}`,
  operationId: 'getProduct',
  tag: 'catalog',
  summary: '商品详情：档案、风味、冲煮建议、各重量价格与库存',
  access: 'internal',
  auth: 'none',
  fingerprint: 'none',
  request: { params: ProductParamsSchema },
  success: { status: 200, schema: ProductDetailSchema },
  errors: ['not_found'],
})

export const catalogEndpoints = defineEndpoints([listProducts, getProduct])

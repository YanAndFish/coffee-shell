// Area `catalog` (owner: server-shop). One function per endpoint, typed from the contract;
// rules in packages/contract/src/catalog.ts and docs/design/data.md#筛选与排序.
//
// Building blocks: `services/products.ts` (`productFacts`, `toProductSummary`,
// `defaultWeight`, `productMaxQuantity`), `services/pricing.ts` (`unitPrice`,
// `quantityTotals`).
import type {
  getProduct as getProductEndpoint,
  listProducts as listProductsEndpoint,
  ProductDetail,
  ProductFacets,
  ProductList,
  ProductListQuery,
  ProductSummary,
  RoastBand,
} from '@coffee/contract'
import type { Product } from '../generated/prisma/client.ts'
import type { HandlerInput } from '../http/route.ts'
import { notImplemented } from '../middleware/error-handler.ts'
import { productFacts, toProductSummary } from './products.ts'

/** Whether roast `level` is in `band`: light 1–2, medium 3, dark 4–5. */
export function inRoastBand(level: number, band: RoastBand): boolean {
  switch (band) {
    case 'light': return level <= 2
    case 'medium': return level === 3
    case 'dark': return level >= 4
  }
}

/** The searchable text of a product: name, subtitle, origin, region, process, notes; lower-case. */
function haystack(row: Product): string {
  const { notes } = productFacts(row)
  return [
    row.name,
    row.subtitle,
    row.origin,
    row.region,
    row.process,
    ...notes,
  ].join(' ').toLowerCase()
}

/** Facets over the whole catalog: total, and each origin in order of first appearance. */
export function productFacets(rows: readonly Product[]): ProductFacets {
  const origins = new Map<string, number>()
  for (const row of rows)
    origins.set(row.origin, (origins.get(row.origin) ?? 0) + 1)
  return {
    total: rows.length,
    origins: [...origins].map(([origin, count]) => ({ origin, count })),
  }
}

/** Filters and sorts rows already in display order (`position`); pure. */
export function selectProducts(rows: readonly Product[], query: ProductListQuery): ProductSummary[] {
  const origin = query.origin?.trim() || undefined
  const needle = query.q?.trim().toLowerCase() || undefined
  const matched = rows.filter(row => (origin === undefined || row.origin === origin)
    && (query.roast === undefined || inRoastBand(row.roast, query.roast))
    && (needle === undefined || haystack(row).includes(needle)))
  const sorted = [...matched]
  switch (query.sort ?? 'recommended') {
    case 'price_asc':
      sorted.sort((a, b) => a.basePrice - b.basePrice || a.position - b.position)
      break
    case 'price_desc':
      sorted.sort((a, b) => b.basePrice - a.basePrice || a.position - b.position)
      break
    case 'score':
      sorted.sort((a, b) => b.score - a.score || a.position - b.position)
      break
    case 'recommended':
      break
  }
  return sorted.map(toProductSummary)
}

/** `GET /v1/products`. */
export async function listProducts({ deps, query }: HandlerInput<typeof listProductsEndpoint>): Promise<ProductList> {
  const rows = await deps.prisma.product.findMany({
    orderBy: { position: 'asc' },
  })
  return {
    items: selectProducts(rows, query),
    facets: productFacets(rows),
  }
}

/** `GET /v1/products/{id}`. */
export function getProduct(_input: HandlerInput<typeof getProductEndpoint>): Promise<ProductDetail> {
  notImplemented()
}

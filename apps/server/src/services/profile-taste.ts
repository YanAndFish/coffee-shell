// The taste profile (我的口味画像 TASTE) and the recommendations, pure (P-profile-1…4).
import type { Flavor, Grind, ProductRef, RoastBand, Taste, Weight } from '@coffee/contract'
import { FLAVOR_AXES, GRIND_LABELS, GRINDS, WEIGHTS } from '@coffee/contract'
import { divRound } from './pricing.ts'

/** One purchased order line with what the profile needs of its product. */
export interface PurchasedLine {
  productId: string
  /** The product's flavor (0–10 per axis). */
  flavor: Flavor
  /** The product's roast level, 1–5. */
  roast: number
  grind: Grind
  weight: Weight
  quantity: number
}

/** A product as the recommendations see it (in display order). */
export interface CandidateProduct {
  id: string
  name: string
  flavor: Flavor
  stock: number
}

/** `偏好` phrase of each axis (index = axis). */
export const PREFER_PHRASES = [
  '明亮果酸',
  '干净甜感',
  '厚实醇度',
  '可可苦甜',
  '悠长余韵',
  '花香',
] as const

/** `少碰` phrase of each axis (index = axis). */
export const AVOID_PHRASES = [
  '浅烘 · 明亮果酸',
  '日晒 · 浓郁果甜',
  '深烘 · 厚重醇度',
  '深烘 · 烟熏与草本调',
  '湿刨 · 悠长余韵',
  '浅烘 · 花香调',
] as const

/** Roast bands in display order, with their `常买` phrase. */
const ROAST_BANDS: readonly [{ band: RoastBand, label: string }, ...{ band: RoastBand, label: string }[]] = [
  { band: 'light', label: '浅烘 / 中浅烘' },
  { band: 'medium', label: '中烘' },
  { band: 'dark', label: '中深烘 / 深烘' },
]

/** light = levels 1–2, medium = 3, dark = 4–5. */
export function roastBand(level: number): RoastBand {
  return level <= 2
    ? 'light'
    : level === 3
      ? 'medium'
      : 'dark'
}

/** Quantity-weighted average flavor, each axis rounded half up; `null` without lines. */
export function averageFlavor(lines: readonly PurchasedLine[]): Flavor | null {
  const units = lines.reduce((n, line) => n + line.quantity, 0)
  if (units === 0)
    return null
  return FLAVOR_AXES.map((_, axis) => divRound(
    lines.reduce((sum, line) => sum + (line.flavor[axis] ?? 0) * line.quantity, 0),
    units,
  ))
}

/** The key with the most units; ties go to the earlier key of `order`. */
function mostBought<K>(order: readonly [K, ...K[]], units: (key: K) => number): K {
  let best = order[0]
  for (const key of order) {
    if (units(key) > units(best))
      best = key
  }
  return best
}

/** Axis indexes sorted by value, high to low (`desc`) or low to high; ties by axis order. */
function axesBy(flavor: Flavor, desc: boolean): number[] {
  return flavor
    .map((value, axis) => ({ value, axis }))
    .sort((a, b) => (desc
      ? b.value - a.value
      : a.value - b.value) || a.axis - b.axis)
    .map(entry => entry.axis)
}

/** The taste profile of everything bought, or `null` before the first order. */
export function tasteOf(lines: readonly PurchasedLine[]): Taste | null {
  const flavor = averageFlavor(lines)
  if (!flavor)
    return null
  const unitsWhere = (match: (line: PurchasedLine) => boolean): number =>
    lines.reduce((n, line) => n + (match(line)
      ? line.quantity
      : 0), 0)
  const band = mostBought(ROAST_BANDS, b => unitsWhere(line => roastBand(line.roast) === b.band))
  const grind = mostBought(GRINDS, g => unitsWhere(line => line.grind === g))
  const weight = mostBought(WEIGHTS, w => unitsWhere(line => line.weight === w))
  const grindPhrase = grind === 'WB'
    ? GRIND_LABELS.WB
    : `${GRIND_LABELS[grind]}研磨`
  return {
    flavor,
    prefer: axesBy(flavor, true).slice(0, 3).map(axis => PREFER_PHRASES[axis]).join(' · '),
    often: [band.label, grindPhrase, weight].join(' · '),
    avoid: AVOID_PHRASES[axesBy(flavor, false)[0]!]!,
  }
}

/** Squared Euclidean distance between two flavors (same order as the distance). */
export function flavorDistance2(a: Flavor, b: Flavor): number {
  return a.reduce((sum, value, axis) => sum + (value - (b[axis] ?? 0)) ** 2, 0)
}

/**
 * Up to `count` products closest to `flavor`: products not bought yet and in stock first
 * (nearest first, ties in display order), then bought ones the same way when too few.
 */
export function recommend(
  products: readonly CandidateProduct[],
  bought: ReadonlySet<string>,
  flavor: Flavor,
  count = 2,
): ProductRef[] {
  const byDistance = (list: readonly CandidateProduct[]): CandidateProduct[] => list
    .map((product, index) => ({
      product,
      index,
      d: flavorDistance2(product.flavor, flavor),
    }))
    .sort((a, b) => a.d - b.d || a.index - b.index)
    .map(entry => entry.product)
  const fresh = byDistance(products.filter(p => !bought.has(p.id) && p.stock > 0))
  const again = byDistance(products.filter(p => bought.has(p.id)))
  return [
    ...fresh,
    ...again,
  ]
    .slice(0, count)
    .map(p => ({ id: p.id, name: p.name }))
}

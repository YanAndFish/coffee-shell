// Example profile pages. The taste is derived from `exampleOrders` by the rules in
// docs/design/decisions.md (P-profile-*); the server tests recompute it.
import type { Profile, Settings } from '../profile.ts'
import { noKeyExample, notRememberedExample, rememberedExample, userExample } from './auth.ts'

/**
 * 林晓 bought (by quantity) yirga ×2, pinkb ×2, night ×2, geisha ×2: average flavor
 * [6.5, 7.75, 4.75, 2.25, 7.75, 8.25] → [7, 8, 5, 2, 8, 8]. Prefer: the three highest axes
 * (ties in axis order) 甜感 余韵 香气; often: light roasts (6 of 8), 手冲 (5), 250g (4);
 * avoid: the lowest axis, 苦度. Recommended: the closest unbought beans in stock, nyeri and
 * yunnan.
 */
export const profileExample: Profile = {
  user: userExample,
  remembered: rememberedExample,
  stats: { orders: 4, addresses: 3, coupons: 1 },
  taste: {
    flavor: [7, 8, 5, 2, 8, 8],
    prefer: '干净甜感 · 悠长余韵 · 花香',
    often: '浅烘 / 中浅烘 · 手冲研磨 · 250g',
    avoid: '深烘 · 烟熏与草本调',
  },
  recommendations: [
    { id: 'nyeri', name: '涅里 AA 双洗' },
    { id: 'yunnan', name: '保山 小粒 日晒' },
  ],
}

/**
 * 林晓 before her first order (no orders, no addresses, no taste profile) on a connection
 * with a key that is not remembered: the `fixture:empty` profile.
 */
export const profileNewExample: Profile = {
  user: userExample,
  remembered: notRememberedExample,
  stats: { orders: 0, addresses: 0, coupons: 1 },
  taste: null,
  recommendations: [],
}

/** The same, on a connection without a key: the `fixture:empty` profile without a key. */
export const profileNoKeyExample: Profile = {
  ...profileNewExample,
  remembered: noKeyExample,
}

/**
 * 林晓's profile (her orders, addresses and taste) on a connection without a key
 * (记住我 unavailable): what a login without a key shows next to the cart and order fixtures.
 */
export const profileOrdersNoKeyExample: Profile = {
  ...profileExample,
  remembered: noKeyExample,
}

/** `GET /v1/settings` of a new account. */
export const settingsExample: Settings = { animations: true, mouseTracking: true }

/** `PATCH /v1/settings {"animations": false}`. */
export const settingsAnimationsOffExample: Settings = { animations: false, mouseTracking: true }

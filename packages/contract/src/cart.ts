// The cart: the member cart (server-side), the guest quote (a client-held cart priced by the
// server) and the merge after login. Owner in phase 2 wave 2: server-shop.
import { z } from 'zod'
import { AddressSchema } from './addresses.ts'
import { API_PREFIX, MoneySchema } from './common.ts'
import { defineEndpoint, defineEndpoints } from './endpoint.ts'
import { GrindSchema, PackSchema, RoastLevelSchema, SkuSchema, WeightSchema } from './sku.ts'

/** At most this many lines (distinct SKUs) per cart. */
export const MAX_CART_LINES = 50

/** Quantity of one line: 1–9 (and never above the product's stock). */
export const QuantitySchema = z.int({ error: 'quantity_invalid' })
  .min(1, { error: 'quantity_invalid' })
  .max(9, { error: 'quantity_invalid' })

/** Why a line cannot be checked out as it is. */
export const CartLineProblemSchema = z.enum([
  /** Its weight is sold out now. */
  'sold_out',
  /** Its quantity is above the product's stock now (`maxQuantity` says how many are left). */
  'out_of_stock',
])
export type CartLineProblem = z.infer<typeof CartLineProblemSchema>

/** One cart line, priced by the server. */
export const CartLineSchema = z.strictObject({
  /** The line's identity (one line per SKU) and the `{sku}` of the line routes. */
  sku: SkuSchema,
  productId: z.string().min(1),
  name: z.string().min(1),
  /** Row subtitle `{产区} · {烘焙度名}`. */
  origin: z.string().min(1),
  roast: RoastLevelSchema,
  weight: WeightSchema,
  grind: GrindSchema,
  pack: PackSchema,
  quantity: z.int().min(1).max(9),
  /** Only selected lines count in the totals and are checked out. */
  selected: z.boolean(),
  /** `¥{单价}`. */
  unitPrice: MoneySchema,
  /** unitPrice × quantity: `¥{小计}`. */
  lineTotal: MoneySchema,
  /** min(9, stock): the stepper's upper bound (0 when the product is out of stock). */
  maxQuantity: z.int().min(0).max(9),
  problem: CartLineProblemSchema.nullable(),
})
export type CartLine = z.infer<typeof CartLineSchema>

/** A coupon in use. */
export const AppliedCouponSchema = z.strictObject({
  /** Canonical (upper-case) code: `TTY10`. */
  code: z.string().min(1),
  /** `全单 9 折` (the UI shows `✓ 已使用 {code} · {description}`). */
  description: z.string().min(1),
})
export type AppliedCoupon = z.infer<typeof AppliedCouponSchema>

/** Free shipping progress (`满 ¥199 包邮`). */
export const FreeShippingSchema = z.strictObject({
  /** `199`. */
  threshold: MoneySchema,
  /** subtotal − discount ≥ threshold: `已满 ¥199 · 本单包邮`. */
  reached: z.boolean(),
  /** min(100, round((subtotal − discount) ÷ threshold × 100)). */
  progress: z.int().min(0).max(100),
  /** max(0, threshold − (subtotal − discount)): `再买 ¥{gap} 即可包邮`. */
  gap: MoneySchema,
})
export type FreeShipping = z.infer<typeof FreeShippingSchema>

/** The cart's amounts (docs/design/data.md#计价). */
export const CartTotalsSchema = z.strictObject({
  /** Σ quantity of all lines: `购物车 · {n} 件`, the nav badge. */
  itemCount: z.int().nonnegative(),
  /** Σ quantity of selected lines: `已选 {n} 件`, `c 结算 {n} 件`. */
  selectedCount: z.int().nonnegative(),
  /** Σ lineTotal of selected lines. */
  subtotal: MoneySchema,
  /** round(subtotal × 10 %) with TTY10, else 0. */
  discount: MoneySchema,
  /** 0 (包邮) when subtotal is 0 or subtotal − discount ≥ 199, else 12. */
  shipping: MoneySchema,
  /** subtotal − discount + shipping. */
  total: MoneySchema,
  freeShipping: FreeShippingSchema,
})
export type CartTotals = z.infer<typeof CartTotalsSchema>

/** A priced cart: the member cart, or a guest quote (then `address` is `null`). */
export const CartSchema = z.strictObject({
  /** Oldest first; `index` of `POST /v1/cart/lines` inserts elsewhere. */
  lines: z.array(CartLineSchema),
  coupon: AppliedCouponSchema.nullable(),
  /** 本单寄往: the chosen address, else the default; `null` when none (or for guests). */
  address: AddressSchema.nullable(),
  totals: CartTotalsSchema,
})
export type Cart = z.infer<typeof CartSchema>

/** A line of a guest cart, held by the TUI until login. */
export const GuestCartLineSchema = z.strictObject({
  productId: z.string().min(1).max(32),
  weight: WeightSchema,
  grind: GrindSchema,
  pack: PackSchema,
  quantity: QuantitySchema,
  selected: z.boolean().default(true),
})
export type GuestCartLine = z.input<typeof GuestCartLineSchema>

/** Body of `POST /v1/cart/quote`. */
export const CartQuoteRequestSchema = z.strictObject({
  lines: z.array(GuestCartLineSchema).max(MAX_CART_LINES, { error: 'cart_full' }),
  /** As typed; trimmed, case-insensitive; empty or absent = none. */
  couponCode: z.string().max(32).nullable().optional(),
})
export type CartQuoteRequest = z.input<typeof CartQuoteRequestSchema>

/** Body of `POST /v1/cart/merge`. */
export const CartMergeRequestSchema = z.strictObject({
  lines: z.array(GuestCartLineSchema).max(MAX_CART_LINES, { error: 'cart_full' }),
  /** The guest's coupon: applied when valid, ignored otherwise. */
  couponCode: z.string().max(32).nullable().optional(),
})
export type CartMergeRequest = z.input<typeof CartMergeRequestSchema>

/** Result of `POST /v1/cart/merge`. */
export const CartMergeResultSchema = z.strictObject({
  cart: CartSchema,
  /** Σ quantity actually added (after capping): `已合并访客购物车 {n} 件`; 0 = no merge toast. */
  mergedCount: z.int().nonnegative(),
  /** Guest lines dropped (unknown product, sold-out weight, no stock, cart full). */
  skippedCount: z.int().nonnegative(),
})
export type CartMergeResult = z.infer<typeof CartMergeResultSchema>

/** Body of `POST /v1/cart/lines`. */
export const AddCartLineRequestSchema = z.strictObject({
  productId: z.string().min(1).max(32),
  weight: WeightSchema,
  grind: GrindSchema,
  pack: PackSchema,
  quantity: QuantitySchema,
  /**
   * 0-based position for a new line (undo of a delete puts it back where it was); absent =
   * at the end. Ignored when the SKU is already in the cart.
   */
  index: z.int().nonnegative().optional(),
})
export type AddCartLineRequest = z.input<typeof AddCartLineRequestSchema>

/** Body of `PATCH /v1/cart/lines/{sku}`: at least one field. */
export const UpdateCartLineRequestSchema = z.strictObject({
  quantity: QuantitySchema.optional(),
  selected: z.boolean().optional(),
}).refine(body => body.quantity !== undefined || body.selected !== undefined, { error: 'validation_failed' })
export type UpdateCartLineRequest = z.input<typeof UpdateCartLineRequestSchema>

/** Body of `PUT /v1/cart/selection`: select or unselect every line. */
export const CartSelectionRequestSchema = z.strictObject({
  selected: z.boolean(),
})
export type CartSelectionRequest = z.input<typeof CartSelectionRequestSchema>

/** Body of `PUT /v1/cart/coupon`. */
export const ApplyCouponRequestSchema = z.strictObject({
  /** Trimmed, case-insensitive; empty = remove the coupon (no error). */
  code: z.string().max(32),
})
export type ApplyCouponRequest = z.input<typeof ApplyCouponRequestSchema>

/** Body of `PUT /v1/cart/address`. */
export const SetCartAddressRequestSchema = z.strictObject({
  /** One of the member's addresses (`⏎ 用于本单`). */
  addressId: z.int().positive(),
})
export type SetCartAddressRequest = z.input<typeof SetCartAddressRequestSchema>

/** Path of the `/v1/cart/lines/{sku}` routes. */
export const CartLineParamsSchema = z.strictObject({
  sku: SkuSchema,
})
export type CartLineParams = z.infer<typeof CartLineParamsSchema>

export const cartComponents = {
  CartLineProblem: CartLineProblemSchema,
  CartLine: CartLineSchema,
  AppliedCoupon: AppliedCouponSchema,
  FreeShipping: FreeShippingSchema,
  CartTotals: CartTotalsSchema,
  Cart: CartSchema,
  GuestCartLine: GuestCartLineSchema,
  CartQuoteRequest: CartQuoteRequestSchema,
  CartMergeRequest: CartMergeRequestSchema,
  CartMergeResult: CartMergeResultSchema,
  AddCartLineRequest: AddCartLineRequestSchema,
  UpdateCartLineRequest: UpdateCartLineRequestSchema,
  CartSelectionRequest: CartSelectionRequestSchema,
  ApplyCouponRequest: ApplyCouponRequestSchema,
  SetCartAddressRequest: SetCartAddressRequestSchema,
}

const member = {
  access: 'internal',
  auth: 'user',
  fingerprint: 'none',
  tag: 'cart',
} as const

export const getCart = defineEndpoint({
  ...member,
  method: 'get',
  path: `${API_PREFIX}/cart`,
  operationId: 'getCart',
  summary: '会员购物车（服务端计价）',
  success: { status: 200, schema: CartSchema },
  errors: [],
})

export const addCartLine = defineEndpoint({
  ...member,
  method: 'post',
  path: `${API_PREFIX}/cart/lines`,
  operationId: 'addCartLine',
  summary: '加入购物车（同一 SKU 数量相加，上限 9，并重新勾选）',
  description: [
    'Same SKU: quantity = min(9, old + new) and selected = true; otherwise a new selected line at `index` (default: last).',
    'Rejects a sold-out weight (`sold_out`), a resulting quantity above stock (`out_of_stock`, details.available) and a 51st line (`cart_full`).',
  ].join(' '),
  request: { body: AddCartLineRequestSchema },
  success: { status: 200, schema: CartSchema },
  errors: [
    'not_found',
    'quantity_invalid',
    'sold_out',
    'out_of_stock',
    'cart_full',
  ],
})

export const updateCartLine = defineEndpoint({
  ...member,
  method: 'patch',
  path: `${API_PREFIX}/cart/lines/{sku}`,
  operationId: 'updateCartLine',
  summary: '改数量或勾选',
  description: 'A quantity above stock is `out_of_stock` (details.available). Unselecting is always allowed.',
  request: { params: CartLineParamsSchema, body: UpdateCartLineRequestSchema },
  success: { status: 200, schema: CartSchema },
  errors: ['not_found', 'quantity_invalid', 'out_of_stock'],
})

export const removeCartLine = defineEndpoint({
  ...member,
  method: 'delete',
  path: `${API_PREFIX}/cart/lines/{sku}`,
  operationId: 'removeCartLine',
  summary: '删除一行（撤销 = 带 index 重新加入）',
  request: { params: CartLineParamsSchema },
  success: { status: 200, schema: CartSchema },
  errors: ['not_found'],
})

export const setCartSelection = defineEndpoint({
  ...member,
  method: 'put',
  path: `${API_PREFIX}/cart/selection`,
  operationId: 'setCartSelection',
  summary: '全选 / 全不选',
  request: { body: CartSelectionRequestSchema },
  success: { status: 200, schema: CartSchema },
  errors: [],
})

export const applyCoupon = defineEndpoint({
  ...member,
  method: 'put',
  path: `${API_PREFIX}/cart/coupon`,
  operationId: 'applyCoupon',
  summary: '使用优惠码（空码 = 取消）',
  description: 'An invalid code leaves the cart (and its current coupon) unchanged.',
  request: { body: ApplyCouponRequestSchema },
  success: { status: 200, schema: CartSchema },
  errors: ['coupon_invalid'],
})

export const removeCoupon = defineEndpoint({
  ...member,
  method: 'delete',
  path: `${API_PREFIX}/cart/coupon`,
  operationId: 'removeCoupon',
  summary: '取消优惠码',
  success: { status: 200, schema: CartSchema },
  errors: [],
})

export const setCartAddress = defineEndpoint({
  ...member,
  method: 'put',
  path: `${API_PREFIX}/cart/address`,
  operationId: 'setCartAddress',
  summary: '指定本单收货地址（用于本单）',
  request: { body: SetCartAddressRequestSchema },
  success: { status: 200, schema: CartSchema },
  errors: ['not_found'],
})

export const quoteCart = defineEndpoint({
  method: 'post',
  path: `${API_PREFIX}/cart/quote`,
  operationId: 'quoteCart',
  tag: 'cart',
  summary: '访客购物车报价：为客户端持有的购物车计价',
  description: [
    'Prices the lines as the member cart would, in the given order; lines of the same SKU are summed (capped at 9).',
    'Unknown products are `not_found`; sold-out weights and quantities above stock are priced and flagged in `problem`.',
    'An invalid non-empty coupon is `coupon_invalid` (re-quote without it). `address` is always `null`. Nothing is stored.',
  ].join(' '),
  access: 'internal',
  auth: 'none',
  fingerprint: 'none',
  request: { body: CartQuoteRequestSchema },
  success: { status: 200, schema: CartSchema },
  errors: [
    'not_found',
    'quantity_invalid',
    'cart_full',
    'coupon_invalid',
  ],
})

export const mergeCart = defineEndpoint({
  ...member,
  method: 'post',
  path: `${API_PREFIX}/cart/merge`,
  operationId: 'mergeCart',
  summary: '登录后把访客购物车合并进会员购物车',
  description: [
    'In guest order, each line is added like `addCartLine` but never fails: quantities are capped at min(9, stock);',
    'unknown products, sold-out weights, products without stock and lines beyond 50 are skipped (`skippedCount`).',
    'A new line keeps the guest `selected` flag; a line merged into an existing one is selected if either was.',
    'A valid `couponCode` replaces the member coupon; an invalid one is ignored.',
  ].join(' '),
  request: { body: CartMergeRequestSchema },
  success: { status: 200, schema: CartMergeResultSchema },
  errors: [],
})

export const cartEndpoints = defineEndpoints([
  getCart,
  addCartLine,
  updateCartLine,
  removeCartLine,
  setCartSelection,
  applyCoupon,
  removeCoupon,
  setCartAddress,
  quoteCart,
  mergeCart,
])

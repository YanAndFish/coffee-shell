// The profile page (stats, tier, taste profile, recommendations) and per-account settings.
// Owner in phase 2 wave 2: server-orders.
import { z } from 'zod'
import { RememberStatusSchema, UserSchema } from './auth.ts'
import { FlavorSchema } from './catalog.ts'
import { API_PREFIX } from './common.ts'
import { defineEndpoint, defineEndpoints } from './endpoint.ts'

/** The three stat tiles. */
export const ProfileStatsSchema = z.strictObject({
  /** 订单: how many orders. */
  orders: z.int().nonnegative(),
  /** 地址: how many addresses. */
  addresses: z.int().nonnegative(),
  /** 优惠券: how many active coupons exist (TTY10 → 1). */
  coupons: z.int().nonnegative(),
})
export type ProfileStats = z.infer<typeof ProfileStatsSchema>

/** 我的口味画像 TASTE, derived from what the member bought (rules: docs/design/decisions.md). */
export const TasteSchema = z.strictObject({
  /** Quantity-weighted average flavor of the purchased beans, rounded. */
  flavor: FlavorSchema,
  /** `偏好`: e.g. `明亮果酸 · 花香 · 干净甜感`. */
  prefer: z.string().min(1),
  /** `常买`: e.g. `浅烘 / 中浅烘 · 手冲研磨 · 250g`. */
  often: z.string().min(1),
  /** `少碰`: e.g. `深烘 · 烟熏与草本调`. */
  avoid: z.string().min(1),
})
export type Taste = z.infer<typeof TasteSchema>

/** A product the profile links to (`{商品名} →`). */
export const ProductRefSchema = z.strictObject({
  id: z.string().min(1),
  name: z.string().min(1),
})
export type ProductRef = z.infer<typeof ProductRefSchema>

/** `GET /v1/profile`. */
export const ProfileSchema = z.strictObject({
  user: UserSchema,
  remembered: RememberStatusSchema,
  stats: ProfileStatsSchema,
  /** `null` before the first order (`下单后生成你的口味画像`). */
  taste: TasteSchema.nullable(),
  /** 按画像推荐: 0–2 products (empty when `taste` is `null`). */
  recommendations: z.array(ProductRefSchema).max(2),
})
export type Profile = z.infer<typeof ProfileSchema>

/** Per-account UI settings (both on for a new account). */
export const SettingsSchema = z.strictObject({
  /** 界面动画 (`m`). */
  animations: z.boolean(),
  /** 鼠标追踪 (`o`). */
  mouseTracking: z.boolean(),
})
export type Settings = z.infer<typeof SettingsSchema>

/** Body of `PATCH /v1/settings`: at least one field. */
export const SettingsPatchSchema = z.strictObject({
  animations: z.boolean().optional(),
  mouseTracking: z.boolean().optional(),
}).refine(body => body.animations !== undefined || body.mouseTracking !== undefined, { error: 'validation_failed' })
export type SettingsPatch = z.input<typeof SettingsPatchSchema>

export const profileComponents = {
  ProfileStats: ProfileStatsSchema,
  Taste: TasteSchema,
  ProductRef: ProductRefSchema,
  Profile: ProfileSchema,
  Settings: SettingsSchema,
  SettingsPatch: SettingsPatchSchema,
}

const member = { access: 'internal', auth: 'user', tag: 'profile' } as const

export const getProfile = defineEndpoint({
  ...member,
  method: 'get',
  path: `${API_PREFIX}/profile`,
  operationId: 'getProfile',
  summary: '我的：会员、统计、口味画像与推荐',
  fingerprint: 'optional',
  success: { status: 200, schema: ProfileSchema },
  errors: [],
})

export const getSettings = defineEndpoint({
  ...member,
  method: 'get',
  path: `${API_PREFIX}/settings`,
  operationId: 'getSettings',
  summary: '界面设置（随账号保存）',
  fingerprint: 'none',
  success: { status: 200, schema: SettingsSchema },
  errors: [],
})

export const updateSettings = defineEndpoint({
  ...member,
  method: 'patch',
  path: `${API_PREFIX}/settings`,
  operationId: 'updateSettings',
  summary: '修改界面设置',
  fingerprint: 'none',
  request: { body: SettingsPatchSchema },
  success: { status: 200, schema: SettingsSchema },
  errors: [],
})

export const profileEndpoints = defineEndpoints([getProfile, getSettings, updateSettings])

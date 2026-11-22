// Area `profile` (owner: server-orders): the profile page and per-account settings. One
// function per endpoint, typed from the contract; the taste-profile rules are in
// docs/design/decisions.md (P-profile-*), implemented purely in `services/profile-taste.ts`.
import type {
  getProfile as getProfileEndpoint,
  getSettings as getSettingsEndpoint,
  Profile,
  Settings,
  updateSettings as updateSettingsEndpoint,
} from '@coffee/contract'
import type { Db } from '../db.ts'
import type { HandlerInput } from '../http/route.ts'
import type { PurchasedLine } from './profile-taste.ts'
import { GrindSchema, WeightSchema } from '@coffee/contract'
import { loadRememberStatus, toUser } from './members.ts'
import { productFacts } from './products.ts'
import { recommend, tasteOf } from './profile-taste.ts'

/** Every line the member bought, with its product's current flavor and roast. */
async function purchasedLines(db: Db, userId: number): Promise<PurchasedLine[]> {
  const items = await db.orderItem.findMany({
    where: {
      order: { userId },
    },
    include: { product: true },
  })
  return items.map(item => ({
    productId: item.productId,
    flavor: productFacts(item.product).flavor,
    roast: item.product.roast,
    grind: GrindSchema.parse(item.grind),
    weight: WeightSchema.parse(item.weight),
    quantity: item.quantity,
  }))
}

/** `GET /v1/profile`. */
export async function getProfile({ deps, user, fingerprint }: HandlerInput<typeof getProfileEndpoint>): Promise<Profile> {
  const { prisma } = deps
  const now = deps.now()
  const [row, remembered, orders, addresses, coupons, lines] = await Promise.all([
    prisma.user.findUniqueOrThrow({
      where: { id: user.id },
      select: { id: true, username: true, createdAt: true },
    }),
    loadRememberStatus(prisma, user.id, fingerprint, now),
    prisma.order.count({
      where: { userId: user.id },
    }),
    prisma.address.count({
      where: { userId: user.id },
    }),
    prisma.coupon.count({
      where: { active: true },
    }),
    purchasedLines(prisma, user.id),
  ])
  const taste = tasteOf(lines)
  let recommendations: Profile['recommendations'] = []
  if (taste) {
    const products = await prisma.product.findMany({
      orderBy: { position: 'asc' },
    })
    recommendations = recommend(
      products.map(p => ({
        id: p.id,
        name: p.name,
        flavor: productFacts(p).flavor,
        stock: p.stock,
      })),
      new Set(lines.map(line => line.productId)),
      taste.flavor,
    )
  }
  return {
    user: toUser(row),
    remembered,
    stats: { orders, addresses, coupons },
    taste,
    recommendations,
  }
}

/** The member's settings row (created with the defaults, both on, when missing). */
function settingsRow(db: Db, userId: number, patch: Partial<Settings> = {}): Promise<Settings> {
  return db.setting.upsert({
    where: { userId },
    create: { userId, ...patch },
    update: patch,
    select: { animations: true, mouseTracking: true },
  })
}

/** `GET /v1/settings`. */
export function getSettings({ deps, user }: HandlerInput<typeof getSettingsEndpoint>): Promise<Settings> {
  return settingsRow(deps.prisma, user.id)
}

/** `PATCH /v1/settings`: only the fields sent change. */
export function updateSettings({ deps, user, body }: HandlerInput<typeof updateSettingsEndpoint>): Promise<Settings> {
  const patch: Partial<Settings> = {}
  if (body.animations !== undefined)
    patch.animations = body.animations
  if (body.mouseTracking !== undefined)
    patch.mouseTracking = body.mouseTracking
  return settingsRow(deps.prisma, user.id, patch)
}

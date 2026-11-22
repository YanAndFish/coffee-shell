// Direct database writers for tests: members with a session, cart lines, addresses and
// orders, without going through the routes (so one area can be tested before another is
// implemented). Rows follow the same rules the services use.
import type { Grind, Pack, Weight } from '@coffee/contract'
import type { PrismaClient } from '../../src/db.ts'
import { randomInt } from 'node:crypto'
import { hashPassword } from '../../src/auth/password.ts'
import { hashSessionToken, newSessionToken, REMEMBER_TTL_MS, SESSION_TTL_MS } from '../../src/auth/tokens.ts'
import { ensureMemberRows, usernameKey } from '../../src/services/members.ts'
import { lineTotal, unitPrice } from '../../src/services/pricing.ts'
import { productSku } from '../../src/services/products.ts'
import { etaOf, orderNumber } from '../../src/services/shop-time.ts'

export interface TestMember {
  id: number
  username: string
  /** Its password in clear (tests only). */
  password: string
  /** A valid session token: send as `Authorization: Bearer`. */
  token: string
}

let serial = 0

/**
 * A member with a cart, settings and one session valid from `now` for 7 days. The default
 * username is unique per call (`member_1`, …); the password is `espresso42`.
 */
export async function createMember(
  prisma: PrismaClient,
  options: { username?: string, password?: string, now?: Date } = {},
): Promise<TestMember> {
  serial++
  const username = options.username ?? `member_${serial}`
  const password = options.password ?? 'espresso42'
  const now = options.now ?? new Date()
  const user = await prisma.user.create({
    data: {
      username,
      usernameKey: usernameKey(username),
      passwordHash: await hashPassword(password),
      createdAt: now,
    },
  })
  await ensureMemberRows(prisma, user.id)
  const token = await createSession(prisma, user.id, now)
  return {
    id: user.id,
    username,
    password,
    token,
  }
}

/** A new session of `userId` starting at `now`; returns its token. */
export async function createSession(prisma: PrismaClient, userId: number, now = new Date(), ttlMs = SESSION_TTL_MS): Promise<string> {
  const token = newSessionToken()
  await prisma.session.create({
    data: {
      tokenHash: hashSessionToken(token),
      userId,
      createdAt: now,
      expiresAt: new Date(now.getTime() + ttlMs),
    },
  })
  return token
}

/** Binds `fingerprint` to `userId` (记住我) from `now` for 30 days. */
export async function rememberKey(prisma: PrismaClient, userId: number, fingerprint: string, now = new Date()): Promise<void> {
  await prisma.rememberedKey.upsert({
    where: { fingerprint },
    create: {
      fingerprint,
      userId,
      createdAt: now,
      expiresAt: new Date(now.getTime() + REMEMBER_TTL_MS),
    },
    update: {
      userId,
      createdAt: now,
      expiresAt: new Date(now.getTime() + REMEMBER_TTL_MS),
    },
  })
}

export interface TestLine {
  productId: string
  weight: Weight
  grind: Grind
  pack: Pack
  quantity: number
  selected?: boolean
}

/** Appends a cart line (positions 0, 1, 2, … in call order). */
export async function addCartRow(prisma: PrismaClient, userId: number, line: TestLine): Promise<void> {
  const position = await prisma.cartItem.count({
    where: { userId },
  })
  await prisma.cartItem.create({
    data: {
      userId,
      ...line,
      selected: line.selected ?? true,
      position,
    },
  })
}

export interface TestAddress {
  recipient?: string
  phone?: string | null
  region?: string
  detail?: string
  tag?: 'home' | 'work' | 'school'
  isDefault?: boolean
}

/** Adds an address; the member's first one becomes the default. Returns its id. */
export async function addAddressRow(prisma: PrismaClient, userId: number, address: TestAddress = {}): Promise<number> {
  const first = (await prisma.address.count({
    where: { userId },
  })) === 0
  const isDefault = first || (address.isDefault ?? false)
  if (isDefault) {
    await prisma.address.updateMany({
      where: { userId },
      data: { isDefault: false },
    })
  }
  const row = await prisma.address.create({
    data: {
      userId,
      recipient: address.recipient ?? '林晓',
      phone: address.phone === undefined
        ? '13800005521'
        : address.phone,
      region: address.region ?? '上海市 徐汇区',
      detail: address.detail ?? '天平路 88 弄 3 号 602 室',
      tag: address.tag ?? 'home',
      isDefault,
    },
  })
  return row.id
}

/**
 * Inserts an order placed at `createdAt` with these lines (priced by the real rules, no
 * coupon, shipped to a fixed address); stock and cart are left alone. Returns its number.
 */
export async function addOrderRow(
  prisma: PrismaClient,
  userId: number,
  lines: readonly TestLine[],
  createdAt = new Date(),
): Promise<string> {
  const products = await prisma.product.findMany({
    where: {
      id: {
        in: lines.map(l => l.productId),
      },
    },
  })
  const items = lines.map((line, position) => {
    const product = products.find(p => p.id === line.productId)!
    const unit = unitPrice(product.basePrice, line.weight, line.pack)
    return {
      position,
      productId: product.id,
      sku: productSku(product, line.weight, line.grind, line.pack),
      productName: product.name,
      weight: line.weight,
      grind: line.grind,
      pack: line.pack,
      quantity: line.quantity,
      unitPrice: unit,
      lineTotal: lineTotal(unit, line.quantity),
    }
  })
  const subtotal = items.reduce((sum, item) => sum + item.lineTotal, 0)
  const shipping = subtotal >= 199
    ? 0
    : 12
  for (;;) {
    const number = orderNumber(createdAt, randomInt(0, 10_000))
    if (await prisma.order.findUnique({
      where: { number },
      select: { id: true },
    })) {
      continue
    }
    await prisma.order.create({
      data: {
        number,
        userId,
        idempotencyKey: `test-${number}`,
        createdAt,
        etaAt: etaOf(createdAt),
        subtotal,
        discount: 0,
        shipping,
        total: subtotal + shipping,
        couponCode: null,
        recipient: '林晓',
        phone: '13800005521',
        region: '上海市 徐汇区',
        detail: '天平路 88 弄 3 号 602 室',
        items: { create: items },
      },
    })
    return number
  }
}

// Member rows → contract values (User, RememberStatus) and the rows every member owns.
// Shared foundation (auth, profile, test helpers).
import type { RememberStatus, Tier, User } from '@coffee/contract'
import type { PrismaClient } from '../db.ts'
import { instant, rememberStatus } from './shop-time.ts'

/** The tier every member shows for now: static display values (data.md#会员与个人统计). */
export const STATIC_TIER: Tier = {
  name: '熟豆会员',
  level: 3,
  points: 1280,
  nextLevel: 4,
  nextLevelPoints: 2000,
  pointsToNext: 720,
  progress: 64,
}

/**
 * The uniqueness / login key of a username: ASCII letters lower-cased (Chinese characters,
 * digits and `_` are unchanged), so `Lin` and `lin` are the same account.
 */
export function usernameKey(username: string): string {
  return username.replace(/[A-Z]/g, c => c.toLowerCase())
}

/** Maps a user row to the contract's `User`. */
export function toUser(row: { id: number, username: string, createdAt: Date }): User {
  return {
    id: row.id,
    username: row.username,
    createdAt: instant(row.createdAt),
    tier: STATIC_TIER,
  }
}

/** The 记住我 status of this connection (`fingerprint`) for `userId`, read from the database. */
export async function loadRememberStatus(prisma: PrismaClient, userId: number, fingerprint: string | null, now: Date): Promise<RememberStatus> {
  if (!fingerprint)
    return rememberStatus(null, null, userId, now)
  const binding = await prisma.rememberedKey.findUnique({
    where: { fingerprint },
    select: { userId: true, expiresAt: true },
  })
  return rememberStatus(fingerprint, binding, userId, now)
}

/**
 * Creates the rows every member has (an empty cart, default settings) when missing.
 * Register calls it; it is idempotent.
 */
export async function ensureMemberRows(prisma: PrismaClient, userId: number): Promise<void> {
  await prisma.cart.upsert({
    where: { userId },
    create: { userId },
    update: {},
  })
  await prisma.setting.upsert({
    where: { userId },
    create: { userId },
    update: {},
  })
}

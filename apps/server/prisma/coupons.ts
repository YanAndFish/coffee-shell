// The coupons of the design: only `TTY10` (全单 9 折). `seedCoupons` inserts the missing ones
// and leaves existing rows alone.
import type { PrismaClient } from '../src/db.ts'

export interface CatalogCoupon {
  code: string
  description: string
  percentOff: number
}

export const COUPONS: readonly CatalogCoupon[] = [
  { code: 'TTY10', description: '全单 9 折', percentOff: 10 },
]

/** Inserts the coupons that are missing; returns how many were created. */
export async function seedCoupons(prisma: PrismaClient): Promise<number> {
  let created = 0
  for (const coupon of COUPONS) {
    const existing = await prisma.coupon.findUnique({
      where: { code: coupon.code },
      select: { code: true },
    })
    if (existing)
      continue
    await prisma.coupon.create({ data: coupon })
    created++
  }
  return created
}

// `prisma db seed` entry (see prisma.config.ts). Idempotent: only missing rows are added;
// stock and everything members created are left alone. No users are seeded.
import process from 'node:process'
import { createPrisma } from '../src/db.ts'
import { loadEnv } from '../src/env.ts'
import { logInfo } from '../src/log.ts'
import { CATALOG, seedCatalog } from './catalog.ts'
import { COUPONS, seedCoupons } from './coupons.ts'

async function main(): Promise<void> {
  const prisma = createPrisma(loadEnv().databaseUrl)
  try {
    const products = await seedCatalog(prisma)
    logInfo('seed', `catalog: ${products} created, ${CATALOG.length - products} already present`)
    const coupons = await seedCoupons(prisma)
    logInfo('seed', `coupons: ${coupons} created, ${COUPONS.length - coupons} already present`)
  }
  finally {
    await prisma.$disconnect()
  }
}

main().catch((error: unknown) => {
  console.error('[seed] failed:', error)
  process.exitCode = 1
})

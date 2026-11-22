// `prisma db seed` entry (see prisma.config.ts). Idempotent: only missing products are added.
import process from 'node:process'
import { createPrisma } from '../src/db.ts'
import { loadEnv } from '../src/env.ts'
import { logInfo } from '../src/log.ts'
import { CATALOG, seedCatalog } from './catalog.ts'

async function main(): Promise<void> {
  const prisma = createPrisma(loadEnv().databaseUrl)
  try {
    const created = await seedCatalog(prisma)
    logInfo('seed', `catalog: ${created} created, ${CATALOG.length - created} already present`)
  }
  finally {
    await prisma.$disconnect()
  }
}

main().catch((error: unknown) => {
  console.error('[seed] failed:', error)
  process.exitCode = 1
})

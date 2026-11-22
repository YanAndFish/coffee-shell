import type { Prisma } from './generated/prisma/client.ts'
import { PrismaBetterSqlite3 } from '@prisma/adapter-better-sqlite3'
import { ensureDatabaseDir, resolveDatabaseUrl } from './env.ts'
import { PrismaClient } from './generated/prisma/client.ts'

export type { PrismaClient }

/**
 * The client or a transaction's client (`prisma.$transaction(async tx => …)`): shared read
 * helpers take a `Db` so checkout can call them inside its transaction.
 */
export type Db = PrismaClient | Prisma.TransactionClient

/** Opens the SQLite database at `databaseUrl` (a `file:` URL) through the better-sqlite3 adapter. */
export function createPrisma(databaseUrl: string): PrismaClient {
  const url = resolveDatabaseUrl(databaseUrl)
  ensureDatabaseDir(url)
  return new PrismaClient({
    adapter: new PrismaBetterSqlite3({ url }),
  })
}

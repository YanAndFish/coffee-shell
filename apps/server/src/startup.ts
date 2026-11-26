// The checks `main.ts` runs before it listens: a backend that cannot use its database says
// why, in one line, and exits non-zero, instead of answering /v1/health and failing every
// other request with 500.
import type { PrismaClient } from './db.ts'
import { existsSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { SERVER_ROOT, sqlitePath } from './env.ts'

const MIGRATE = 'run `pnpm --filter @coffee/server db:migrate` (pnpm dev and the image\'s start.sh do it)'

/** Why the database file cannot be used before opening it, or `null`. */
export function missingDatabase(databaseUrl: string): string | null {
  const path = sqlitePath(databaseUrl)
  if (!existsSync(path))
    return `the database ${path} does not exist: ${MIGRATE}, then db:seed`
  return null
}

/** The migrations of this version: `prisma/migrations` next to the sources or the bundle (the image copies it). */
export const MIGRATIONS_DIR = join(SERVER_ROOT, 'prisma', 'migrations')

/** The names of the migrations in `dir` (one directory each), in order. */
export function migrationsIn(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true })
    .filter(entry => entry.isDirectory())
    .map(entry => entry.name)
    .sort()
}

/**
 * Why the opened database cannot serve, or `null`: every migration of this version (in
 * `migrationsDir`) must be recorded as applied — a database migrated by an older version
 * has a catalogue but no accounts or orders — and the catalogue must be readable.
 */
export async function databaseProblem(
  prisma: PrismaClient,
  databaseUrl: string,
  migrationsDir: string = MIGRATIONS_DIR,
): Promise<string | null> {
  const path = sqlitePath(databaseUrl)
  let required: string[]
  try {
    required = migrationsIn(migrationsDir)
  }
  catch (error) {
    return `the migrations ${migrationsDir} cannot be read: ${describe(error)}`
  }
  try {
    const rows = await prisma.$queryRaw<{ name: string }[]>`
      SELECT migration_name AS name FROM _prisma_migrations
      WHERE finished_at IS NOT NULL AND rolled_back_at IS NULL`
    const applied = new Set(rows.map(row => row.name))
    const missing = required.filter(name => !applied.has(name))
    if (missing.length > 0)
      return `the database ${path} lacks ${missing.join(', ')} (migrations of this version): ${MIGRATE}`
    await prisma.product.count()
    return null
  }
  catch (error) {
    const message = describe(error)
    if (/no such table/i.test(message))
      return `the database ${path} is not migrated: ${MIGRATE}`
    if (/locked|SQLITE_BUSY/i.test(message))
      return `the database ${path} is locked by another process (a second backend, a migration, a sqlite3 shell?): stop it, then start again`
    if (/unable to open|SQLITE_CANTOPEN|readonly|permission/i.test(message))
      return `the database ${path} cannot be opened or written: check its directory and permissions`
    return `the database ${path} cannot be read: ${message}`
  }
}

/** The driver's own words (Prisma wraps them in `meta.driverAdapterError.cause`), else the message. */
function describe(error: unknown): string {
  const cause = (error as { meta?: { driverAdapterError?: { cause?: { originalCode?: string, originalMessage?: string } } } })
    ?.meta
    ?.driverAdapterError
    ?.cause
  const message = error instanceof Error
    ? error.message
    : String(error)
  const line = message.split('\n').map(part => part.trim()).find(Boolean) ?? ''
  return [cause?.originalCode, cause?.originalMessage, line].filter(Boolean).join(': ')
}

/** A catalogue with no products is served (every list is empty) but is almost always a missed seed. */
export async function catalogIsEmpty(prisma: PrismaClient): Promise<boolean> {
  return (await prisma.product.count()) === 0
}

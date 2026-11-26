import type { TestDatabase } from './helpers.ts'
import { spawn, spawnSync } from 'node:child_process'
import { once } from 'node:events'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import process from 'node:process'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { createPrisma } from '../src/db.ts'
import { SERVER_ROOT, sqlitePath } from '../src/env.ts'
import { catalogIsEmpty, databaseProblem, MIGRATIONS_DIR, migrationsIn, missingDatabase } from '../src/startup.ts'
import { createTestDatabase } from './helpers.ts'

/** A child that holds an exclusive lock on the file in argv[1] until its stdin closes. */
const HOLD_LOCK = [
  'const { DatabaseSync } = require("node:sqlite")',
  'const db = new DatabaseSync(process.argv[1])',
  'db.exec("BEGIN EXCLUSIVE")',
  'console.log("locked")',
  'process.stdin.on("end", () => process.exit(0)).resume()',
].join('\n')

describe('the database checks before listening', () => {
  let dir: string
  let db: TestDatabase

  beforeAll(async () => {
    dir = mkdtempSync(join(tmpdir(), 'coffee-startup-'))
    db = await createTestDatabase()
  }, 60_000)

  afterAll(async () => {
    await db.cleanup()
    rmSync(dir, { recursive: true, force: true })
  })

  it('a missing file is named, with the command that makes it', () => {
    const url = `file:${join(dir, 'none.db')}`
    expect(missingDatabase(url)).toMatch(/none\.db does not exist: run `pnpm --filter @coffee\/server db:migrate`/)
    expect(missingDatabase(db.url)).toBeNull()
  })

  it('an unmigrated file says so', async () => {
    const url = `file:${join(dir, 'empty.db')}`
    writeFileSync(sqlitePath(url), '')
    const prisma = createPrisma(url)
    try {
      expect(await databaseProblem(prisma, url)).toMatch(/empty\.db is not migrated/)
    }
    finally {
      await prisma.$disconnect()
    }
  })

  it('a database without this version\'s later migrations says which (it has a catalogue but no accounts)', async () => {
    const partial = await createTestDatabase()
    try {
      const [first, ...later] = migrationsIn(MIGRATIONS_DIR)
      expect(first).toMatch(/_init$/)
      expect(later.length).toBeGreaterThan(0)
      // As if only the first migration had been deployed (an older image's database).
      for (const name of later)
        await partial.prisma.$executeRaw`DELETE FROM _prisma_migrations WHERE migration_name = ${name}`
      const problem = await databaseProblem(partial.prisma, partial.url)
      expect(problem).toMatch(/test\.db lacks .*accounts_cart_orders.*: run `pnpm --filter @coffee\/server db:migrate`/)
      expect(await partial.prisma.product.count()).toBeGreaterThan(0)
    }
    finally {
      await partial.cleanup()
    }
  }, 60_000)

  it('a migration newer than the database, or one that failed half way, is missing', async () => {
    const migrations = join(dir, 'migrations')
    for (const name of [
      ...migrationsIn(MIGRATIONS_DIR),
      '20991231000000_later',
    ])
      mkdirSync(join(migrations, name), { recursive: true })
    expect(await databaseProblem(db.prisma, db.url, migrations)).toMatch(/lacks 20991231000000_later \(/)
    expect(await databaseProblem(db.prisma, db.url, join(dir, 'nowhere'))).toMatch(/the migrations .*nowhere cannot be read/)
  })

  it('a ready database passes, with its catalogue', async () => {
    expect(await databaseProblem(db.prisma, db.url)).toBeNull()
    expect(await catalogIsEmpty(db.prisma)).toBe(false)
  })

  it('a database another process holds locked says so (after the driver\'s busy wait)', async () => {
    // Another process: SQLite's locks are per process (and node:sqlite is another SQLite).
    const holder = spawn(process.execPath, [
      '-e',
      HOLD_LOCK,
      sqlitePath(db.url),
    ], {
      stdio: ['pipe', 'pipe', 'inherit'],
    })
    await once(holder.stdout, 'data')
    const prisma = createPrisma(db.url)
    try {
      expect(await databaseProblem(prisma, db.url)).toMatch(/is locked by another process/)
    }
    finally {
      holder.stdin.end()
      await once(holder, 'exit')
      await prisma.$disconnect()
    }
  }, 30_000)

  it('the server exits 1 with one line on stderr and listens on nothing', () => {
    const run = spawnSync(process.execPath, ['src/main.ts'], {
      cwd: SERVER_ROOT,
      env: { ...process.env, DATABASE_URL: `file:${join(dir, 'nowhere', 'none.db')}`, PORT: '0' },
      encoding: 'utf8',
      timeout: 20_000,
    })
    expect(run.status).toBe(1)
    expect(run.stderr).toMatch(/^\[server\] ✗ the database .*none\.db does not exist/m)
    expect(run.stdout).not.toMatch(/listening/)
  }, 30_000)
})

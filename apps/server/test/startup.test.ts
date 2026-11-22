import type { TestDatabase } from './helpers.ts'
import { spawn, spawnSync } from 'node:child_process'
import { once } from 'node:events'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import process from 'node:process'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { createPrisma } from '../src/db.ts'
import { SERVER_ROOT, sqlitePath } from '../src/env.ts'
import { catalogIsEmpty, databaseProblem, missingDatabase } from '../src/startup.ts'
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

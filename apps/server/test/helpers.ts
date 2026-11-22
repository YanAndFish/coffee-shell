import type Koa from 'koa'
import type { AddressInfo } from 'node:net'
import type { AppOptions } from '../src/app.ts'
import type { PrismaClient } from '../src/db.ts'
import { execFileSync } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import process from 'node:process'
import { seedCatalog } from '../prisma/catalog.ts'
import { seedCoupons } from '../prisma/coupons.ts'
import { createApp } from '../src/app.ts'
import { createPrisma } from '../src/db.ts'
import { SERVER_ROOT } from '../src/env.ts'

const require = createRequire(import.meta.url)

/** Reads a committed contract fixture, e.g. `v1/products.json`. */
export function readFixture(path: string): unknown {
  return JSON.parse(readFileSync(require.resolve(`@coffee/contract/fixtures/${path}`), 'utf8'))
}

export interface TestDatabase {
  url: string
  prisma: PrismaClient
  cleanup: () => Promise<void>
}

/**
 * A fresh SQLite file in a temp dir, migrated with the real `prisma migrate deploy` and
 * seeded (catalog + coupons). Call it in `beforeAll` (allow ~60 s).
 */
export async function createTestDatabase(): Promise<TestDatabase> {
  const dir = mkdtempSync(join(tmpdir(), 'coffee-server-test-'))
  const url = `file:${join(dir, 'test.db')}`
  const prismaBin = join(SERVER_ROOT, 'node_modules', '.bin', 'prisma')
  execFileSync(prismaBin, ['migrate', 'deploy'], {
    cwd: SERVER_ROOT,
    env: { ...process.env, DATABASE_URL: url, PRISMA_HIDE_UPDATE_MESSAGE: '1' },
    stdio: 'pipe',
  })
  const prisma = createPrisma(url)
  await seedCatalog(prisma)
  await seedCoupons(prisma)
  return {
    url,
    prisma,
    cleanup: async () => {
      await prisma.$disconnect()
      rmSync(dir, { recursive: true, force: true })
    },
  }
}

export interface TestServer {
  /** `http://127.0.0.1:<port>` */
  origin: string
  close: () => Promise<void>
}

/** Listens with `app` on a free loopback port. */
export async function listen(app: Koa): Promise<TestServer> {
  const server = app.listen(0, '127.0.0.1')
  await new Promise<void>(resolve => server.once('listening', resolve))
  const { port } = server.address() as AddressInfo
  return {
    origin: `http://127.0.0.1:${port}`,
    close: () => new Promise<void>((resolve, reject) => {
      server.closeAllConnections()
      server.close(error => (error
        ? reject(error)
        : resolve()))
    }),
  }
}

/** The app over `prisma` with response validation on, listening on a free port. */
export function listenApp(prisma: PrismaClient, options: Omit<AppOptions, 'prisma'> = {}): Promise<TestServer> {
  return listen(createApp({
    validateResponses: true,
    logError: () => {},
    ...options,
    prisma,
  }))
}

export interface JsonResponse {
  status: number
  headers: Headers
  body: unknown
}

/** `fetch` with JSON in and out; `token` becomes `Authorization: Bearer`. */
export async function call(
  origin: string,
  method: string,
  path: string,
  options: { body?: unknown, token?: string, headers?: Record<string, string> } = {},
): Promise<JsonResponse> {
  const headers: Record<string, string> = { Accept: 'application/json', ...options.headers }
  if (options.token)
    headers.Authorization = `Bearer ${options.token}`
  if (options.body !== undefined)
    headers['Content-Type'] = 'application/json'
  const response = await fetch(`${origin}${path}`, {
    method,
    headers,
    ...(options.body !== undefined
      ? {
          body: JSON.stringify(options.body),
        }
      : {}),
  })
  const text = await response.text()
  return {
    status: response.status,
    headers: response.headers,
    body: text
      ? JSON.parse(text)
      : undefined,
  }
}

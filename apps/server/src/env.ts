import { mkdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'
import { z } from 'zod'

/** `apps/server/`, from both `src/` and the built `dist/`. */
export const SERVER_ROOT = resolve(import.meta.dirname, '..')

/** The default SQLite file, `apps/server/.data/coffee.db` (git-ignored). */
export const DEFAULT_DATABASE_FILE = join(SERVER_ROOT, '.data', 'coffee.db')

/** Treats an empty variable as unset. */
const optionalString = z.string().optional().transform(value => (value || undefined))

const EnvSchema = z.object({
  HOST: optionalString.transform(value => value ?? '127.0.0.1'),
  PORT: optionalString.transform(value => value ?? '3000').pipe(z.coerce.number<string>().int().min(0).max(65535)),
  DATABASE_URL: optionalString
    .refine(value => value === undefined || value.startsWith('file:'), 'DATABASE_URL 必须是 SQLite 的 file: 地址')
    .transform(value => resolveDatabaseUrl(value)),
  COFFEE_INTERNAL_KEY: optionalString,
})

export interface Env {
  host: string
  port: number
  /** Always `file:<absolute path>`. */
  databaseUrl: string
  /** When set, every route except `/v1/health` requires `X-Internal-Key`. */
  internalKey: string | undefined
}

/** Reads and validates the backend's environment variables. */
export function loadEnv(source: NodeJS.ProcessEnv = process.env): Env {
  const env = EnvSchema.parse(source)
  return {
    host: env.HOST,
    port: env.PORT,
    databaseUrl: env.DATABASE_URL,
    internalKey: env.COFFEE_INTERNAL_KEY,
  }
}

/**
 * Normalizes a SQLite `file:` URL (`file:./x.db`, `file:/abs/x.db`, `file:///abs/x.db`) to
 * `file:<absolute path>`, resolving relative paths against the working directory. The Prisma
 * CLI (`prisma.config.ts`) and the runtime adapter both use this, so they open the same file.
 */
export function resolveDatabaseUrl(databaseUrl: string | undefined): string {
  return `file:${sqlitePath(databaseUrl ?? `file:${DEFAULT_DATABASE_FILE}`)}`
}

/** The absolute filesystem path of a SQLite `file:` URL. */
export function sqlitePath(databaseUrl: string): string {
  if (databaseUrl.startsWith('file://'))
    return fileURLToPath(databaseUrl)
  return resolve(databaseUrl.slice('file:'.length))
}

/** Creates the directory of the SQLite file; SQLite creates the file but not its directory. */
export function ensureDatabaseDir(databaseUrl: string): void {
  mkdirSync(dirname(sqlitePath(databaseUrl)), { recursive: true })
}

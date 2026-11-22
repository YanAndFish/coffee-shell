import type { Context } from 'koa'
import type { PrismaClient } from './db.ts'
import type { Deps } from './deps.ts'
import Koa from 'koa'
import { errorHandler } from './middleware/error-handler.ts'
import { internalKey } from './middleware/internal-key.ts'
import { apiRouter, HEALTH_PATH } from './routes/index.ts'

export interface AppOptions {
  prisma: PrismaClient
  /** When set, every route except `/v1/health` requires a matching `X-Internal-Key`. */
  internalKey?: string | undefined
  /** Receives unexpected (5xx) errors; defaults to `console.error`. */
  logError?: (error: unknown, ctx: Context) => void
  /** The clock; defaults to the system clock (tests pass a fixed one). */
  now?: () => Date
  /** Check every success body against the contract (tests); default off. */
  validateResponses?: boolean
}

function defaultLogError(error: unknown, ctx: Context): void {
  console.error(`[server] ${ctx.method} ${ctx.path} failed:`, error)
}

/** Builds the Koa app; `main.ts` listens with it and tests drive it directly. */
export function createApp(options: AppOptions): Koa {
  const app = new Koa()
  // errorHandler answers and logs every error; keep Koa's default stderr logger quiet.
  app.silent = true

  const deps: Deps = {
    prisma: options.prisma,
    now: options.now ?? (() => new Date()),
    validateResponses: options.validateResponses ?? false,
  }
  const router = apiRouter(deps)
  app.use(errorHandler(options.logError ?? defaultLogError))
  app.use(internalKey(options.internalKey, new Set([HEALTH_PATH])))
  app.use(router.routes())
  // Sets 405/501 with an `Allow` header; errorHandler adds the envelope.
  app.use(router.allowedMethods())
  return app
}

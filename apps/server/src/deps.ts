import type { PrismaClient } from './db.ts'

/** What every route and service gets: the database and the clock (tests move the clock). */
export interface Deps {
  prisma: PrismaClient
  /** The current time; status, expiry and order dates all read it. */
  now: () => Date
  /**
   * Parse every success body with its contract schema before answering (a mismatch is a
   * 500, logged). On in tests (`test/helpers.ts`), off in production.
   */
  validateResponses: boolean
}

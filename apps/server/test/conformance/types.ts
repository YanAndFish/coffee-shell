// Shapes of the contract-conformance cases (`test/conformance.test.ts`). Each area owns its
// case file (`test/conformance/<area>.ts`); the harness runs them against the real app.
import type { ErrorCode } from '@coffee/contract'
import type { PrismaClient } from '../../src/db.ts'
import type { TestMember } from '../support/members.ts'
import { createHash } from 'node:crypto'

/** The fixed clock every case runs at: 2026-11-22 21:04 shop time. */
export const NOW = new Date('2026-11-22T13:04:00.000Z')

/** A well-formed fingerprint for cases that need one. */
export const FINGERPRINT = 'SHA256:KpdRbDVLaISM29j1SiJqClWyHtE44getbFy7nACqWuo'

/** A well-formed fingerprint derived from `seed` (`SHA256:` + 43 base64 characters). */
export function fakeFingerprint(seed: string): string {
  return `SHA256:${createHash('sha256').update(seed).digest('base64').replace(/=+$/, '')}`
}

/** What a case can use: the area's own database (fresh per area) and helpers. */
export interface CaseContext {
  prisma: PrismaClient
  /** A new member with a session at {@link NOW}. */
  member: (options?: { username?: string, password?: string }) => Promise<TestMember>
}

/** The request a case sends. */
export interface CaseRequest {
  /** The concrete path, e.g. `/v1/cart/lines/RT-PNK-250G-PO-STD`. */
  path: string
  query?: Record<string, string>
  body?: unknown
  /** Sent as `Authorization: Bearer`. */
  token?: string
  /** Sent as `X-SSH-Fingerprint`. */
  fingerprint?: string
}

export interface ConformanceCase {
  /** The contract endpoint (`operationId`). */
  operationId: string
  name: string
  /** Prepares data (directly in the database) and builds the request. */
  request: (ctx: CaseContext) => Promise<CaseRequest>
  /** Expect this error envelope instead of the success response. */
  expectError?: ErrorCode
  /** More checks on the success body (already parsed with the endpoint's success schema). */
  check?: (body: unknown, ctx: CaseContext) => void | Promise<void>
}

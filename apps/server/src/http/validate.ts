// Request validation wired to the contract: zod issues become the contract's error codes.
// Schemas carry the code as their message (`z.string().min(1, { error: 'username_required' })`);
// any other failure is `validation_failed` with the offending field in `details.field`.
import type { Context } from 'koa'
import type { z } from 'zod'
import { Buffer } from 'node:buffer'
import { CLIENT_IP_HEADER, isErrorCode, SSH_FINGERPRINT_HEADER, SshFingerprintSchema } from '@coffee/contract'
import { fail } from '../middleware/error-handler.ts'

/** Largest JSON body accepted (bytes). */
export const MAX_BODY_BYTES = 64 * 1024

/**
 * Validates request input with `schema`. The first issue decides the answer: its message
 * when that is an error code (status and text from the contract), else `400
 * validation_failed` naming the field. Any ZodError thrown elsewhere stays a 500.
 */
export function parseInput<T extends z.ZodType>(schema: T, value: unknown, where?: string): z.infer<T> {
  const result = schema.safeParse(value)
  if (result.success)
    return result.data
  const issue = result.error.issues[0]
  const path = [
    where,
    ...(issue?.path ?? []).map(String),
  ].filter(Boolean).join('.')
  if (issue && isErrorCode(issue.message) && issue.message !== 'validation_failed')
    fail(issue.message)
  fail('validation_failed', path
    ? { field: path }
    : undefined)
}

/**
 * Reads the JSON body (once; Koa has no body parser here). An empty body is `undefined`;
 * a non-JSON content type, bad JSON or more than {@link MAX_BODY_BYTES} is `400 bad_request`.
 */
export async function readJsonBody(ctx: Context): Promise<unknown> {
  const state = ctx.state as { jsonBody?: { value: unknown } }
  if (state.jsonBody)
    return state.jsonBody.value
  const chunks: Buffer[] = []
  let size = 0
  for await (const chunk of ctx.req as AsyncIterable<Buffer>) {
    size += chunk.length
    if (size > MAX_BODY_BYTES)
      fail('bad_request')
    chunks.push(chunk)
  }
  let value: unknown
  const text = Buffer.concat(chunks).toString('utf8')
  if (text.trim() !== '') {
    if (!ctx.is('application/json'))
      fail('bad_request')
    try {
      value = JSON.parse(text)
    }
    catch {
      fail('bad_request')
    }
  }
  state.jsonBody = { value }
  return value
}

/**
 * The validated `X-SSH-Fingerprint`, or `null` without one. A malformed value is
 * `400 validation_failed` (field `header.x-ssh-fingerprint`): the TUI forwards what sshd set.
 */
export function sshFingerprint(ctx: Context): string | null {
  const value = ctx.get(SSH_FINGERPRINT_HEADER)
  if (!value)
    return null
  if (!SshFingerprintSchema.safeParse(value).success)
    fail('validation_failed', { field: `header.${SSH_FINGERPRINT_HEADER.toLowerCase()}` })
  return value
}

/** `X-Client-IP` (at most 64 characters), else the socket's address; for rate limits and logs. */
export function clientIp(ctx: Context): string | null {
  const value = ctx.get(CLIENT_IP_HEADER).trim()
  if (value && value.length <= 64)
    return value
  return ctx.ip || null
}

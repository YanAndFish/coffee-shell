import type { Endpoint } from '@coffee/contract'
// Registers a contract endpoint on a router: auth per `endpoint.auth`, `X-SSH-Fingerprint`
// per `endpoint.fingerprint`, params / query / body parsed with the endpoint's schemas, the
// success status set, and (in tests) the response checked against the success schema.
import type Router from '@koa/router'
import type { Context } from 'koa'
import type { z } from 'zod'
import type { Deps } from '../deps.ts'
import type { SessionUser } from '../middleware/auth.ts'
import { currentUser, resolveUser } from '../middleware/auth.ts'
import { fail } from '../middleware/error-handler.ts'
import { clientIp, parseInput, readJsonBody, sshFingerprint } from './validate.ts'

type RequestPart<E extends Endpoint, K extends 'params' | 'query' | 'body'>
  = E['request'] extends infer R
    ? R extends Record<K, infer S>
      ? S extends z.ZodType ? z.infer<S> : undefined
      : undefined
    : undefined

type UserOf<E extends Endpoint> = E['auth'] extends 'user'
  ? SessionUser
  : E['auth'] extends 'optional'
    ? SessionUser | null
    : null

type FingerprintOf<E extends Endpoint> = E['fingerprint'] extends 'required' ? string : string | null

/** What a handler receives, typed from its endpoint. */
export interface HandlerInput<E extends Endpoint> {
  ctx: Context
  deps: Deps
  params: RequestPart<E, 'params'>
  query: RequestPart<E, 'query'>
  body: RequestPart<E, 'body'>
  /** The member (per `endpoint.auth`). */
  user: UserOf<E>
  /** `X-SSH-Fingerprint` (validated; always `null` when `endpoint.fingerprint` is `none`). */
  fingerprint: FingerprintOf<E>
  /** `X-Client-IP`, else the socket address (rate limits, logs). */
  clientIp: string | null
}

/** What a handler returns: the success body (its schema's input type). */
export type HandlerOutput<E extends Endpoint> = E['success']['schema'] extends z.ZodType
  ? z.input<E['success']['schema']>
  : void

export type Handler<E extends Endpoint> = (input: HandlerInput<E>) => Promise<HandlerOutput<E>> | HandlerOutput<E>

/** `/v1/cart/lines/{sku}` → `/v1/cart/lines/:sku`. */
export function koaPath(path: string): string {
  return path.replace(/\{(\w+)\}/g, ':$1')
}

/** Registers `endpoint` on `router` with `handler`. */
export function route<E extends Endpoint>(router: Router, deps: Deps, endpoint: E, handler: Handler<E>): void {
  router.register(koaPath(endpoint.path), [
    endpoint.method.toUpperCase(),
  ], async (ctx) => {
    const user = endpoint.auth === 'user'
      ? await currentUser(ctx, deps)
      : endpoint.auth === 'optional'
        ? await resolveUser(ctx, deps)
        : null
    let fingerprint: string | null = null
    if (endpoint.fingerprint !== 'none') {
      fingerprint = sshFingerprint(ctx)
      if (fingerprint === null && endpoint.fingerprint === 'required')
        fail('fingerprint_required')
    }
    const { request } = endpoint
    const params = request?.params
      ? parseInput(request.params, ctx.params, 'params')
      : undefined
    const query = request?.query
      ? parseInput(request.query, ctx.query, 'query')
      : undefined
    const body = request?.body
      ? parseInput(request.body, await readJsonBody(ctx), 'body')
      : undefined
    const input = {
      ctx,
      deps,
      params,
      query,
      body,
      user,
      fingerprint,
      clientIp: clientIp(ctx),
    } as unknown as HandlerInput<E>
    const result = await handler(input)
    const { status, schema } = endpoint.success
    if (schema && deps.validateResponses)
      schema.parse(result)
    ctx.status = status
    if (schema)
      ctx.body = result
  }, { name: endpoint.operationId })
}

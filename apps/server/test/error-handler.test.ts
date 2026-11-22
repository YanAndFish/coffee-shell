import type { ApiError } from '@coffee/contract'
import type { TestServer } from './helpers.ts'
import { ApiErrorSchema } from '@coffee/contract'
import Router from '@koa/router'
import Koa from 'koa'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { z } from 'zod'
import { parseInput } from '../src/http/validate.ts'
import { errorHandler } from '../src/middleware/error-handler.ts'
import { listen } from './helpers.ts'

let server: TestServer

beforeAll(async () => {
  const app = new Koa()
  app.silent = true
  const router = new Router()
  router.get('/validate', (ctx) => {
    const query = parseInput(z.object({
      qty: z.coerce.number().int().min(1).max(9),
    }), ctx.query)
    ctx.body = { qty: query.qty }
  })
  router.get('/coded', (ctx) => {
    const query = parseInput(z.object({
      qty: z.coerce.number().int().max(9, { error: 'quantity_invalid' }),
    }), ctx.query)
    ctx.body = { qty: query.qty }
  })
  router.get('/teapot', ctx => ctx.throw(418, 'I am a teapot'))
  router.get('/zod', () => z.number().parse('stored garbage'))
  app.use(errorHandler(() => {}))
  app.use(router.routes())
  server = await listen(app)
})

afterAll(async () => {
  await server?.close()
})

async function errorOf(path: string): Promise<{ status: number, error: ApiError['error'] }> {
  const response = await fetch(`${server.origin}${path}`)
  return { status: response.status, error: ApiErrorSchema.parse(await response.json()).error }
}

describe('errorHandler', () => {
  it('passes valid input through', async () => {
    const response = await fetch(`${server.origin}/validate?qty=3`)
    expect(await response.json()).toEqual({ qty: 3 })
  })

  it('answers invalid input with 400 validation_failed naming the field', async () => {
    expect(await errorOf('/validate?qty=10')).toEqual({
      status: 400,
      error: {
        code: 'validation_failed',
        message: '请求参数不合法',
        details: { field: 'qty' },
      },
    })
  })

  it('answers the error code a schema carries, with its status and message', async () => {
    expect(await errorOf('/coded?qty=10')).toEqual({
      status: 422,
      error: { code: 'quantity_invalid', message: '数量应为 1–9' },
    })
  })

  it('maps Koa 4xx errors to a Chinese envelope', async () => {
    expect(await errorOf('/teapot')).toEqual({
      status: 418,
      error: { code: 'bad_request', message: '请求格式错误' },
    })
  })

  it('treats a ZodError outside parseInput as a 500', async () => {
    expect(await errorOf('/zod')).toEqual({
      status: 500,
      error: { code: 'internal_error', message: '服务器内部错误' },
    })
  })
})

// Contract conformance, table-driven: for every endpoint of the contract, run its cases
// (test/conformance/<area>.ts) against the real app with response validation on, and parse
// each answer with the endpoint's schema (or check the error envelope). An endpoint still
// answering `501 not_implemented` is skipped and listed at the end.
import type { Endpoint, EndpointArea } from '@coffee/contract'
import type { CaseContext, ConformanceCase } from './conformance/types.ts'
import type { TestDatabase, TestServer } from './helpers.ts'
import process from 'node:process'
import { ApiErrorSchema, endpoints, ERRORS, SSH_FINGERPRINT_HEADER } from '@coffee/contract'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { addressCases } from './conformance/addresses.ts'
import { authCases } from './conformance/auth.ts'
import { cartCases } from './conformance/cart.ts'
import { catalogCases } from './conformance/catalog.ts'
import { orderCases } from './conformance/orders.ts'
import { profileCases } from './conformance/profile.ts'
import { NOW } from './conformance/types.ts'
import { call, createTestDatabase, listenApp } from './helpers.ts'
import { createMember } from './support/members.ts'

const healthCases: ConformanceCase[] = [
  {
    operationId: 'getHealth',
    name: 'answers',
    request: async () => ({ path: '/v1/health' }),
  },
]

const CASES: Record<EndpointArea, readonly ConformanceCase[]> = {
  health: healthCases,
  auth: authCases,
  catalog: catalogCases,
  cart: cartCases,
  addresses: addressCases,
  orders: orderCases,
  profile: profileCases,
}

const stubbed = new Set<string>()

afterAll(() => {
  const implemented = endpoints.filter(e => !stubbed.has(e.operationId)).length
  const lines = [`[conformance] ${implemented}/${endpoints.length} endpoints implemented`]
  if (stubbed.size > 0)
    lines.push(`[conformance] still 501 not_implemented: ${[...stubbed].join(', ')}`)
  process.stdout.write(`${lines.join('\n')}\n`)
})

function areaEndpoints(area: EndpointArea): Endpoint[] {
  return endpoints.filter(e => e.tag === area)
}

for (const area of Object.keys(CASES) as EndpointArea[]) {
  describe(`${area}`, () => {
    let db: TestDatabase
    let server: TestServer
    const ctx: CaseContext = {
      get prisma() {
        return db.prisma
      },
      member: options => createMember(db.prisma, { ...options, now: NOW }),
    }

    beforeAll(async () => {
      db = await createTestDatabase()
      server = await listenApp(db.prisma, {
        now: () => NOW,
      })
    }, 60_000)

    afterAll(async () => {
      await server?.close()
      await db?.cleanup()
    })

    for (const endpoint of areaEndpoints(area)) {
      const cases = CASES[area].filter(c => c.operationId === endpoint.operationId)
      describe(`${endpoint.method.toUpperCase()} ${endpoint.path} (${endpoint.operationId})`, () => {
        it('has at least one case', () => {
          expect(cases.length, `add a case for ${endpoint.operationId} to test/conformance/${area}.ts`).toBeGreaterThan(0)
        })

        for (const testCase of cases) {
          it(testCase.name, async (t) => {
            const request = await testCase.request(ctx)
            const query = request.query
              ? `?${new URLSearchParams(request.query)}`
              : ''
            const response = await call(server.origin, endpoint.method.toUpperCase(), `${request.path}${query}`, {
              ...(request.body !== undefined
                ? { body: request.body }
                : {}),
              ...(request.token
                ? { token: request.token }
                : {}),
              headers: request.fingerprint
                ? { [SSH_FINGERPRINT_HEADER]: request.fingerprint }
                : {},
            })
            const envelope = ApiErrorSchema.safeParse(response.body)
            if (response.status === 501 && envelope.success && envelope.data.error.code === 'not_implemented') {
              stubbed.add(endpoint.operationId)
              t.skip()
              return
            }
            if (testCase.expectError) {
              expect(envelope.success, JSON.stringify(response.body)).toBe(true)
              expect(envelope.data?.error.code).toBe(testCase.expectError)
              expect(envelope.data?.error.message).toBe(ERRORS[testCase.expectError].message)
              expect(response.status).toBe(ERRORS[testCase.expectError].status)
              return
            }
            expect(response.status, JSON.stringify(response.body)).toBe(endpoint.success.status)
            if (endpoint.success.schema) {
              const body = endpoint.success.schema.parse(response.body)
              await testCase.check?.(body, ctx)
            }
          })
        }
      })
    }
  })
}

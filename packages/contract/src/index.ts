// @coffee/contract: zod schemas are the single source of truth of the `/v1` API.
// Each `FooSchema` validates; the type `Foo` is what it parses to.

export { components, endpoints } from './api.ts'
export * from './common.ts'
export * from './endpoint.ts'
export * from './health.ts'
export * from './products.ts'

// @coffee/contract: zod schemas are the single source of truth of the `/v1` API.
// Each `FooSchema` validates; the type `Foo` is what it parses to (request types are the
// schema's input, so fields with defaults are optional).

export * from './addresses.ts'
export { components, endpointById, endpoints } from './api.ts'
export * from './auth.ts'
export * from './cart.ts'
export * from './catalog.ts'
export * from './common.ts'
export * from './endpoint.ts'
export * from './health.ts'
export * from './orders.ts'
export * from './profile.ts'
export * from './sku.ts'

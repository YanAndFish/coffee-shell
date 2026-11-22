// Pure rendering of the generated files; `scripts/generate.ts` writes them and the tests
// compare them with the committed copies.
import type { z } from 'zod'
import { ApiErrorSchema } from './common.ts'
import { notFoundExample } from './examples/error.ts'
import { healthExample } from './examples/health.ts'
import { productListExample } from './examples/products.ts'
import { HealthSchema } from './health.ts'
import { buildOpenApi } from './openapi.ts'
import { ProductListSchema } from './products.ts'

/** A fixture: an example value and the schema it must satisfy. */
export interface Fixture {
  /** Path relative to `fixtures/`. */
  path: string
  schema: z.ZodType
  value: unknown
}

/** Every fixture, keyed by its path under `fixtures/`. Rust tests read these files too. */
export const fixtures: readonly Fixture[] = [
  { path: 'v1/health.json', schema: HealthSchema, value: healthExample },
  { path: 'v1/products.json', schema: ProductListSchema, value: productListExample },
  { path: 'v1/error.json', schema: ApiErrorSchema, value: notFoundExample },
]

/** The canonical JSON text of the generated files: 2-space indent and a final newline. */
export function toJsonText(value: unknown): string {
  return `${JSON.stringify(value, null, 2)}\n`
}

/** Every generated file, keyed by its path relative to the package root. */
export function renderGeneratedFiles(): Map<string, string> {
  const files = new Map<string, string>()
  files.set('openapi.json', toJsonText(buildOpenApi()))
  for (const fixture of fixtures)
    files.set(`fixtures/${fixture.path}`, toJsonText(fixture.schema.parse(fixture.value)))
  return files
}

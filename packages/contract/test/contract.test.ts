import { readdirSync, readFileSync } from 'node:fs'
import { join, relative } from 'node:path'
import { describe, expect, it } from 'vitest'
import { fixtures, renderGeneratedFiles } from '../src/generate.ts'
import { ApiErrorSchema, endpoints, HealthSchema, ProductListSchema } from '../src/index.ts'

const root = join(import.meta.dirname, '..')

function readJson(path: string): unknown {
  return JSON.parse(readFileSync(join(root, path), 'utf8'))
}

function listFiles(dir: string): string[] {
  return readdirSync(join(root, dir), { recursive: true, withFileTypes: true })
    .filter(entry => entry.isFile())
    .map(entry => relative(root, join(entry.parentPath, entry.name)))
    .sort()
}

describe('generated files', () => {
  const generated = renderGeneratedFiles()

  it.each([
    ...generated.keys(),
  ])('%s is up to date (run `pnpm -F @coffee/contract generate`)', (path) => {
    expect(readFileSync(join(root, path), 'utf8')).toBe(generated.get(path))
  })

  it('fixtures/ holds exactly the generated fixtures', () => {
    const expected = [
      ...generated.keys(),
    ].filter(path => path.startsWith('fixtures/')).sort()
    expect(listFiles('fixtures')).toEqual(expected)
  })
})

describe('frozen fixtures parse with the schemas', () => {
  it('health.json', () => {
    expect(HealthSchema.parse(readJson('fixtures/v1/health.json'))).toEqual({
      ok: true,
      service: 'roast-tty',
      version: '0.1.0',
      demo: true,
    })
  })

  it('products.json', () => {
    const list = ProductListSchema.parse(readJson('fixtures/v1/products.json'))
    expect(list.items).toHaveLength(8)
    expect(list.items.map(p => p.id)).toEqual([
      'yirga',
      'pinkb',
      'nyeri',
      'geisha',
      'yunnan',
      'antigua',
      'mandh',
      'night',
    ])
  })

  it('error.json', () => {
    expect(ApiErrorSchema.parse(readJson('fixtures/v1/error.json')).error.code).toBe('not_found')
  })

  it.each(fixtures.map(f => [f.path, f] as const))('%s matches its schema', (_path, fixture) => {
    expect(fixture.schema.safeParse(readJson(`fixtures/${fixture.path}`)).success).toBe(true)
  })
})

describe('schemas', () => {
  it('reject extra keys and out-of-range values', () => {
    const item = ProductListSchema.parse(readJson('fixtures/v1/products.json')).items[0]!
    expect(ProductListSchema.safeParse({
      items: [
        { ...item, extra: 1 },
      ],
    }).success).toBe(false)
    expect(ProductListSchema.safeParse({
      items: [
        {
          ...item,
          flavor: [1, 2, 3],
        },
      ],
    }).success).toBe(false)
    expect(ProductListSchema.safeParse({
      items: [
        { ...item, roast: 6 },
      ],
    }).success).toBe(false)
    expect(ProductListSchema.safeParse({
      items: [
        { ...item, price250: 9.5 },
      ],
    }).success).toBe(false)
  })
})

describe('openapi.json', () => {
  const doc = readJson('openapi.json') as { paths: Record<string, Record<string, { operationId: string }>> }

  it('documents every endpoint', () => {
    for (const endpoint of endpoints)
      expect(doc.paths[endpoint.path]?.[endpoint.method]?.operationId).toBe(endpoint.operationId)
  })

  it('has unique operation ids', () => {
    const ids = endpoints.map(e => e.operationId)
    expect(new Set(ids).size).toBe(ids.length)
  })
})

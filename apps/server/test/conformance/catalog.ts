// Conformance cases of area `catalog` (owner: server-shop). The seeded catalog answers
// exactly the committed fixtures.
import type { ProductList } from '@coffee/contract'
import type { ConformanceCase } from './types.ts'
import { expect } from 'vitest'
import { readFixture } from '../helpers.ts'

export const catalogCases: ConformanceCase[] = [
  {
    operationId: 'listProducts',
    name: 'no filters: the fixture',
    request: async () => ({ path: '/v1/products' }),
    check: body => expect(body).toEqual(readFixture('v1/products.json')),
  },
  {
    operationId: 'listProducts',
    name: 'light roasts by price',
    request: async () => ({
      path: '/v1/products',
      query: { roast: 'light', sort: 'price_asc' },
    }),
    check: body => expect(body).toEqual(readFixture('v1/catalog/products-light-by-price.json')),
  },
  {
    operationId: 'listProducts',
    name: 'search by note, by score',
    request: async () => ({
      path: '/v1/products',
      query: { q: ' 茉莉 ', sort: 'score' },
    }),
    check: body => expect(body).toEqual(readFixture('v1/catalog/products-search.json')),
  },
  {
    operationId: 'listProducts',
    name: 'no match keeps the facets',
    request: async () => ({
      path: '/v1/products',
      query: { q: 'xyz', origin: '埃塞俄比亚' },
    }),
    check: body => expect(body).toEqual(readFixture('v1/catalog/products-empty.json')),
  },
  {
    operationId: 'listProducts',
    name: 'an unknown sort is refused',
    request: async () => ({
      path: '/v1/products',
      query: { sort: 'cheapest' },
    }),
    expectError: 'validation_failed',
  },
  {
    operationId: 'getProduct',
    name: 'pinkb: the fixture',
    request: async () => ({ path: '/v1/products/pinkb' }),
    check: body => expect(body).toEqual(readFixture('v1/catalog/product-pinkb.json')),
  },
  {
    operationId: 'getProduct',
    name: 'geisha: the fixture',
    request: async () => ({ path: '/v1/products/geisha' }),
    check: body => expect(body).toEqual(readFixture('v1/catalog/product-geisha.json')),
  },
  {
    operationId: 'listProducts',
    name: 'dark roasts by price high to low, facets unchanged',
    request: async () => ({
      path: '/v1/products',
      query: { roast: 'dark', sort: 'price_desc' },
    }),
    check: (body) => {
      const list = body as ProductList
      expect(list.items.map(p => p.id)).toEqual(['night', 'antigua', 'mandh'])
      expect(list.facets).toEqual((readFixture('v1/products.json') as ProductList).facets)
    },
  },
  {
    operationId: 'getProduct',
    name: 'unknown product',
    request: async () => ({ path: '/v1/products/nope' }),
    expectError: 'not_found',
  },
]

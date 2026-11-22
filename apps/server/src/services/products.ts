import type { ProductList, ProductSummary } from '@coffee/contract'
import type { PrismaClient } from '../db.ts'
import type { Product } from '../generated/prisma/client.ts'
import { FlavorSchema } from '@coffee/contract'
import { z } from 'zod'
import { parseJsonColumn } from './json-columns.ts'

const Notes = z.array(z.string().min(1))

/** Maps a database row to the list DTO. */
export function toProductSummary(row: Product): ProductSummary {
  return {
    id: row.id,
    name: row.name,
    subtitle: row.subtitle,
    origin: row.origin,
    process: row.process,
    roast: row.roast,
    tag: row.tag,
    notes: parseJsonColumn(Notes, row.notes),
    flavor: parseJsonColumn(FlavorSchema, row.flavor),
    score: row.score,
    price250: row.basePrice,
    stock: row.stock,
  }
}

/** All products in display order. */
export async function listProducts(prisma: PrismaClient): Promise<ProductList> {
  const rows = await prisma.product.findMany({
    orderBy: { position: 'asc' },
  })
  return {
    items: rows.map(toProductSummary),
  }
}

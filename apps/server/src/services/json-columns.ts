// SQLite has no arrays: list columns are JSON text, validated on the way in and out.
import { z } from 'zod'

/** Weight options, in display order; prices scale from the 250 g base price. */
export const WeightKeySchema = z.enum(['100g', '250g', '500g', '1kg'])
export type WeightKey = z.infer<typeof WeightKeySchema>

/** Parses a JSON text column with `schema`; throws (→ 500) when the stored data is corrupt. */
export function parseJsonColumn<T extends z.ZodType>(schema: T, text: string): z.infer<T> {
  return schema.parse(JSON.parse(text))
}

/** Serializes a value for a JSON text column after validating it with `schema`. */
export function toJsonColumn<T extends z.ZodType>(schema: T, value: z.input<T>): string {
  return JSON.stringify(schema.parse(value))
}

// SQLite has no arrays: list columns are JSON text, validated on the way in and out.
import type { z } from 'zod'

/** Parses a JSON text column with `schema`; throws (→ 500) when the stored data is corrupt. */
export function parseJsonColumn<T extends z.ZodType>(schema: T, text: string): z.infer<T> {
  return schema.parse(JSON.parse(text))
}

/** Serializes a value for a JSON text column after validating it with `schema`. */
export function toJsonColumn<T extends z.ZodType>(schema: T, value: z.input<T>): string {
  return JSON.stringify(schema.parse(value))
}

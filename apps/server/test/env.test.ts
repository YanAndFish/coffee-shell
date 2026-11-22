import { join, resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { DEFAULT_DATABASE_FILE, loadEnv, resolveDatabaseUrl, SERVER_ROOT } from '../src/env.ts'

describe('loadEnv', () => {
  it('defaults to loopback:3000, the .data SQLite file and no internal key', () => {
    expect(loadEnv({})).toEqual({
      host: '127.0.0.1',
      port: 3000,
      databaseUrl: `file:${DEFAULT_DATABASE_FILE}`,
      internalKey: undefined,
    })
    expect(DEFAULT_DATABASE_FILE).toBe(join(SERVER_ROOT, '.data', 'coffee.db'))
  })

  it('reads HOST, PORT, DATABASE_URL and COFFEE_INTERNAL_KEY; empty means unset', () => {
    expect(loadEnv({
      HOST: '0.0.0.0',
      PORT: '8080',
      DATABASE_URL: 'file:/tmp/x.db',
      COFFEE_INTERNAL_KEY: 'k',
    }))
      .toEqual({
        host: '0.0.0.0',
        port: 8080,
        databaseUrl: 'file:/tmp/x.db',
        internalKey: 'k',
      })
    expect(loadEnv({ HOST: '', PORT: '', COFFEE_INTERNAL_KEY: '' }).internalKey).toBeUndefined()
  })

  it('rejects a bad port or a non-SQLite database URL', () => {
    expect(() => loadEnv({ PORT: 'abc' })).toThrow()
    expect(() => loadEnv({ PORT: '70000' })).toThrow()
    expect(() => loadEnv({ DATABASE_URL: 'postgres://x' })).toThrow()
  })
})

describe('resolveDatabaseUrl', () => {
  it('makes paths absolute', () => {
    expect(resolveDatabaseUrl('file:./a.db')).toBe(`file:${resolve('a.db')}`)
    expect(resolveDatabaseUrl('file:///tmp/a.db')).toBe('file:/tmp/a.db')
    expect(resolveDatabaseUrl(undefined)).toBe(`file:${DEFAULT_DATABASE_FILE}`)
  })
})

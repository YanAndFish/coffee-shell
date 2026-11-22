// Password hashing with node:crypto's argon2id (Node ≥ 24.7); scrypt only where argon2 is
// missing. Hashes are self-describing strings, so either kind verifies later.
import { Buffer } from 'node:buffer'
import crypto, { randomBytes, timingSafeEqual } from 'node:crypto'
import { promisify } from 'node:util'

/** argon2id parameters (OWASP 2024 minimum: m = 19 MiB, t = 2, p = 1). */
const ARGON2 = {
  memory: 19_456,
  passes: 2,
  parallelism: 1,
  tagLength: 32,
} as const
/** scrypt parameters (N = 2^15, r = 8, p = 1). */
const SCRYPT = {
  N: 32_768,
  r: 8,
  p: 1,
  maxmem: 64 * 1024 * 1024,
  keylen: 32,
} as const

interface Argon2Parameters {
  message: string
  nonce: Buffer
  memory: number
  passes: number
  parallelism: number
  tagLength: number
}

type Argon2 = (
  algorithm: 'argon2id',
  parameters: Argon2Parameters,
  callback: (error: Error | null, derived: Buffer) => void,
) => void

const argon2 = (crypto as unknown as { argon2?: Argon2 }).argon2

function argon2id(parameters: Argon2Parameters): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    argon2!('argon2id', parameters, (error, derived) => (error
      ? reject(error)
      : resolve(derived)))
  })
}

const scrypt = promisify(crypto.scrypt) as (password: string, salt: Buffer, keylen: number, options: crypto.ScryptOptions) => Promise<Buffer>

/** Which algorithm new hashes use: argon2id unless this Node lacks `crypto.argon2`. */
export const PASSWORD_ALGORITHM: 'argon2id' | 'scrypt' = argon2
  ? 'argon2id'
  : 'scrypt'

/** `argon2id$m=…,t=…,p=…$<salt b64>$<hash b64>` (or `scrypt$N=…,r=…,p=…$…$…`). Never log it. */
export async function hashPassword(password: string): Promise<string> {
  const salt = randomBytes(16)
  if (argon2) {
    const hash = await argon2id({ message: password, nonce: salt, ...ARGON2 })
    return [
      'argon2id',
      `m=${ARGON2.memory},t=${ARGON2.passes},p=${ARGON2.parallelism}`,
      salt.toString('base64'),
      hash.toString('base64'),
    ].join('$')
  }
  const hash = await scrypt(password, salt, SCRYPT.keylen, {
    N: SCRYPT.N,
    r: SCRYPT.r,
    p: SCRYPT.p,
    maxmem: SCRYPT.maxmem,
  })
  return [
    'scrypt',
    `N=${SCRYPT.N},r=${SCRYPT.r},p=${SCRYPT.p}`,
    salt.toString('base64'),
    hash.toString('base64'),
  ].join('$')
}

function parameters(text: string): Record<string, number> {
  return Object.fromEntries(text.split(',').map((pair) => {
    const [key, value] = pair.split('=')
    return [
      key ?? '',
      Number(value),
    ]
  }))
}

/** Whether `password` matches `stored` (constant-time compare); malformed hashes never match. */
export async function verifyPassword(password: string, stored: string): Promise<boolean> {
  const [algorithm, parameterText, saltText, hashText] = stored.split('$')
  if (!parameterText || !saltText || !hashText)
    return false
  const salt = Buffer.from(saltText, 'base64')
  const expected = Buffer.from(hashText, 'base64')
  const p = parameters(parameterText)
  let actual: Buffer
  if (algorithm === 'argon2id' && argon2) {
    actual = await argon2id({
      message: password,
      nonce: salt,
      memory: p.m ?? 0,
      passes: p.t ?? 0,
      parallelism: p.p ?? 0,
      tagLength: expected.length,
    })
  }
  else if (algorithm === 'scrypt') {
    actual = await scrypt(password, salt, expected.length, {
      N: p.N ?? 0,
      r: p.r ?? 0,
      p: p.p ?? 0,
      maxmem: SCRYPT.maxmem,
    })
  }
  else {
    return false
  }
  return actual.length === expected.length && timingSafeEqual(actual, expected)
}

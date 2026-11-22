// Accounts and sessions: register, login, resume by SSH key, logout, me.
// Owner in phase 2 wave 2: server-auth.
import { z } from 'zod'
import { API_PREFIX, InstantSchema, OkSchema, ShopDateSchema } from './common.ts'
import { defineEndpoint, defineEndpoints } from './endpoint.ts'

/**
 * Username rule: 2–16 characters (code points), each a Chinese character (Han script), an
 * ASCII letter, a digit or `_`. Unique ignoring the case of ASCII letters.
 */
export const USERNAME_PATTERN = /^[\p{Script=Han}\w]{2,16}$/u

/** Password length bounds (UTF-16 code units, as JavaScript counts). */
export const PASSWORD_MIN = 6
export const PASSWORD_MAX = 128

/** A username in a register request. The error codes travel as zod messages. */
export const NewUsernameSchema = z.string({ error: 'username_required' })
  .min(1, { error: 'username_required' })
  .regex(USERNAME_PATTERN, { error: 'username_invalid' })

/** A username in a login request: only required (a wrong one is `invalid_credentials`). */
export const LoginUsernameSchema = z.string({ error: 'username_required' })
  .min(1, { error: 'username_required' })
  .max(64, { error: 'invalid_credentials' })

export const PasswordSchema = z.string({ error: 'password_too_short' })
  .min(PASSWORD_MIN, { error: 'password_too_short' })
  .max(PASSWORD_MAX, { error: 'password_too_long' })

/** `POST /v1/auth/register` body. */
export const RegisterRequestSchema = z.strictObject({
  username: NewUsernameSchema,
  password: PasswordSchema,
  /**
   * 记住我: bind the session's SSH key (`X-SSH-Fingerprint`) to the account for 30 days.
   * Ignored without the header (then `remembered.state` is `no_key`).
   */
  remember: z.boolean().default(false),
})
export type RegisterRequest = z.input<typeof RegisterRequestSchema>

/** `POST /v1/auth/login` body. */
export const LoginRequestSchema = z.strictObject({
  username: LoginUsernameSchema,
  password: PasswordSchema,
  /** As in {@link RegisterRequestSchema}. */
  remember: z.boolean().default(false),
})
export type LoginRequest = z.input<typeof LoginRequestSchema>

/** Member tier: static display values for now (docs/design/data.md#会员与个人统计). */
export const TierSchema = z.strictObject({
  /** `熟豆会员`. */
  name: z.string().min(1),
  /** `3` → `Lv.3`. */
  level: z.int().positive(),
  /** `1280` → `1,280 / 2,000 豆`. */
  points: z.int().nonnegative(),
  nextLevel: z.int().positive(),
  nextLevelPoints: z.int().positive(),
  /** nextLevelPoints − points: `再积 720 豆升级 Lv.4`. */
  pointsToNext: z.int().nonnegative(),
  /** round(points ÷ nextLevelPoints × 100), the meter's percentage. */
  progress: z.int().min(0).max(100),
})
export type Tier = z.infer<typeof TierSchema>

/** A member as the UI shows them. */
export const UserSchema = z.strictObject({
  id: z.int().positive(),
  /** As registered (case kept). The avatar is its first character, Latin letters upper-cased. */
  username: z.string().min(1),
  createdAt: InstantSchema,
  tier: TierSchema,
})
export type User = z.infer<typeof UserSchema>

/**
 * 记住我 for the current connection and user:
 * - `remembered`: this session's key is bound to this user until `expiresAt`
 *   (`● 已记住当前公钥 · 至 {MM-DD}` from `expiresOn`);
 * - `not_remembered`: the connection has a key, not bound to this user (`○ 未记住当前公钥`);
 * - `no_key`: no `X-SSH-Fingerprint` (`○ 本次连接没有公钥`).
 */
export const RememberStatusSchema = z.strictObject({
  state: z.enum(['remembered', 'not_remembered', 'no_key']),
  /** Set only when `remembered`. */
  expiresAt: InstantSchema.nullable(),
  /** Shop-local date of `expiresAt`; set only when `remembered`. */
  expiresOn: ShopDateSchema.nullable(),
})
export type RememberStatus = z.infer<typeof RememberStatusSchema>

/** `GET /v1/auth/me`. */
export const MeSchema = z.strictObject({
  user: UserSchema,
  remembered: RememberStatusSchema,
})
export type Me = z.infer<typeof MeSchema>

/** A new session (register, login, resume): send `token` as `Authorization: Bearer <token>`. */
export const AuthSessionSchema = z.strictObject({
  /** Opaque, 43 base64url characters; the server stores only its SHA-256. */
  token: z.string().regex(/^[\w-]{43}$/),
  /** The session ends then (7 days after it started); afterwards `401 session_expired`. */
  expiresAt: InstantSchema,
  user: UserSchema,
  remembered: RememberStatusSchema,
})
export type AuthSession = z.infer<typeof AuthSessionSchema>

export const authComponents = {
  RegisterRequest: RegisterRequestSchema,
  LoginRequest: LoginRequestSchema,
  Tier: TierSchema,
  User: UserSchema,
  RememberStatus: RememberStatusSchema,
  Me: MeSchema,
  AuthSession: AuthSessionSchema,
}

export const register = defineEndpoint({
  method: 'post',
  path: `${API_PREFIX}/auth/register`,
  operationId: 'register',
  tag: 'auth',
  summary: '注册（用户名 + 密码）并登录',
  description: [
    'Creates the user (password hashed with argon2id), its empty cart and default settings, then a session, as login does.',
    'With `remember` and `X-SSH-Fingerprint`, binds the key to the new user for 30 days (replacing any binding of that key).',
    'Rate limit: see `docs/design/decisions.md` (P-auth-3).',
  ].join(' '),
  access: 'internal',
  auth: 'none',
  fingerprint: 'optional',
  request: { body: RegisterRequestSchema },
  success: { status: 201, schema: AuthSessionSchema },
  errors: [
    'username_required',
    'username_invalid',
    'password_too_short',
    'password_too_long',
    'username_taken',
    'rate_limited',
  ],
})

export const login = defineEndpoint({
  method: 'post',
  path: `${API_PREFIX}/auth/login`,
  operationId: 'login',
  tag: 'auth',
  summary: '登录（用户名 + 密码），可选记住当前 SSH 公钥 30 天',
  description: [
    'Username matching ignores the case of ASCII letters. A wrong username and a wrong password are the same `invalid_credentials`.',
    'With `remember` and `X-SSH-Fingerprint`, (re)binds the key to this user for 30 days from now; without `remember` an existing binding is left alone.',
    'The guest cart is merged afterwards by `POST /v1/cart/merge`.',
  ].join(' '),
  access: 'internal',
  auth: 'none',
  fingerprint: 'optional',
  request: { body: LoginRequestSchema },
  success: { status: 200, schema: AuthSessionSchema },
  errors: [
    'username_required',
    'password_too_short',
    'password_too_long',
    'invalid_credentials',
    'rate_limited',
  ],
})

export const resume = defineEndpoint({
  method: 'post',
  path: `${API_PREFIX}/auth/resume`,
  operationId: 'resume',
  tag: 'auth',
  summary: '用已记住的 SSH 公钥恢复登录（无需密码）',
  description: 'No body. A binding of `X-SSH-Fingerprint` that has not expired yields a new session; it does not extend the binding.',
  access: 'internal',
  auth: 'none',
  fingerprint: 'required',
  success: { status: 200, schema: AuthSessionSchema },
  errors: ['fingerprint_required', 'not_remembered'],
})

export const logout = defineEndpoint({
  method: 'post',
  path: `${API_PREFIX}/auth/logout`,
  operationId: 'logout',
  tag: 'auth',
  summary: '退出登录：结束会话，并删除当前公钥的“记住我”',
  description: 'No body. Deletes the session and, when `X-SSH-Fingerprint` is bound to this user, that binding.',
  access: 'internal',
  auth: 'user',
  fingerprint: 'optional',
  success: { status: 200, schema: OkSchema },
  errors: [],
})

export const getMe = defineEndpoint({
  method: 'get',
  path: `${API_PREFIX}/auth/me`,
  operationId: 'getMe',
  tag: 'auth',
  summary: '当前会员与“记住我”状态',
  access: 'internal',
  auth: 'user',
  fingerprint: 'optional',
  success: { status: 200, schema: MeSchema },
  errors: [],
})

export const authEndpoints = defineEndpoints([register, login, resume, logout, getMe])

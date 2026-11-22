import { z } from 'zod'
import { WeightSchema } from './sku.ts'

/** The API version prefix every backend route lives under. */
export const API_PREFIX = '/v1'

/** The header the backend checks when it runs with an internal key (gateway ↔ backend). */
export const INTERNAL_KEY_HEADER = 'X-Internal-Key'

/**
 * The SSH key fingerprint of the session (`SHA256:…`, as OpenSSH prints it). The TUI copies
 * it from its session environment; the backend trusts it only because every route that
 * reads it also requires {@link INTERNAL_KEY_HEADER}.
 */
export const SSH_FINGERPRINT_HEADER = 'X-SSH-Fingerprint'

/** The SSH client's IP address (from `COFFEE_CLIENT_IP`), for rate limits and logs only. */
export const CLIENT_IP_HEADER = 'X-Client-IP'

/** The user session: `Authorization: Bearer <token>`. */
export const AUTHORIZATION_HEADER = 'Authorization'

/** OpenSSH SHA-256 fingerprint: `SHA256:` + 43 base64 characters (no padding). */
export const SshFingerprintSchema = z.string().regex(/^SHA256:[A-Za-z0-9+/]{43}$/)
export type SshFingerprint = z.infer<typeof SshFingerprintSchema>

/** An instant, ISO 8601 in UTC with milliseconds: `2026-11-22T13:04:00.000Z`. */
export const InstantSchema = z.iso.datetime()
export type Instant = z.infer<typeof InstantSchema>

/**
 * A calendar date in the shop's time zone (Asia/Shanghai, UTC+8): `2026-11-25`. Everything
 * the UI shows as a date (`11-18`, `11 月 25 日`, `至 12-22`) is formatted from one of these,
 * so the TUI never converts time zones.
 */
export const ShopDateSchema = z.iso.date()
export type ShopDate = z.infer<typeof ShopDateSchema>

/** Integer yuan, never negative. */
export const MoneySchema = z.int().nonnegative()
export type Money = z.infer<typeof MoneySchema>

/** `{ "ok": true }`: the body of a success that has nothing else to say. */
export const OkSchema = z.strictObject({
  ok: z.literal(true),
})
export type Ok = z.infer<typeof OkSchema>

/**
 * Every error the API answers, with its HTTP status and its one Chinese message. The message
 * is shown to people as is (the TUI prefixes `✗ ` in forms); texts come from
 * `docs/design/copy.md` (those marked 新增 there were added for the API).
 */
export const ERRORS = {
  // Transport and generic
  bad_request: { status: 400, message: '请求格式错误' },
  validation_failed: { status: 400, message: '请求参数不合法' },
  unauthorized: { status: 401, message: '缺少或错误的内部密钥' },
  not_found: { status: 404, message: '资源不存在' },
  method_not_allowed: { status: 405, message: '不支持该请求方法' },
  rate_limited: { status: 429, message: '操作太频繁，请稍后再试' },
  internal_error: { status: 500, message: '服务器内部错误' },
  not_implemented: { status: 501, message: '功能尚未实现' },
  // Sessions
  login_required: { status: 401, message: '请先登录' },
  session_expired: { status: 401, message: '登录已失效' },
  // Register / login / remember me
  username_required: { status: 422, message: '请输入用户名' },
  username_invalid: { status: 422, message: '用户名需为 2–16 个字符，可用中文、字母、数字和下划线' },
  password_too_short: { status: 422, message: '密码至少 6 位' },
  password_too_long: { status: 422, message: '密码最多 128 位' },
  username_taken: { status: 409, message: '这个用户名已被注册' },
  invalid_credentials: { status: 401, message: '用户名或密码不正确' },
  fingerprint_required: { status: 400, message: '当前连接没有 SSH 公钥，无法记住登录' },
  not_remembered: { status: 404, message: '当前公钥没有记住的登录' },
  // Cart
  quantity_invalid: { status: 422, message: '数量应为 1–9' },
  sold_out: { status: 409, message: '该重量暂时缺货' },
  out_of_stock: { status: 409, message: '库存不足' },
  cart_full: { status: 409, message: '购物车最多放 50 种规格' },
  coupon_invalid: { status: 422, message: '优惠码无效或已过期' },
  // Checkout
  empty_selection: { status: 409, message: '还没有勾选商品' },
  address_required: { status: 409, message: '请先选择收货地址' },
  price_changed: { status: 409, message: '价格有变化，请确认后再结算' },
  // Addresses
  recipient_required: { status: 422, message: '请填写收货人' },
  region_required: { status: 422, message: '请填写所在地区' },
  detail_required: { status: 422, message: '请填写详细地址' },
  phone_invalid: { status: 422, message: '手机号应为 11 位数字' },
  text_too_long: { status: 422, message: '内容太长了' },
  address_limit: { status: 409, message: '地址最多保存 20 条' },
} as const satisfies Record<string, { status: number, message: string }>

/** Machine-readable error codes (the keys of {@link ERRORS}). */
export type ErrorCode = keyof typeof ERRORS
const ERROR_CODES = Object.keys(ERRORS) as [ErrorCode, ...ErrorCode[]]
export const ErrorCodeSchema = z.enum(ERROR_CODES)

/** Whether `value` is one of the {@link ErrorCode}s. */
export function isErrorCode(value: unknown): value is ErrorCode {
  return typeof value === 'string' && Object.hasOwn(ERRORS, value)
}

/**
 * Structured facts some errors carry so the UI can build its sentence without parsing the
 * message: `validation_failed` → `field`; `out_of_stock` → `productId`, `productName`,
 * `available` (units left); `sold_out` → `productId`, `productName`, `weight`;
 * `rate_limited` → `retryAfter` (seconds).
 */
export const ApiErrorDetailsSchema = z.strictObject({
  /** Dotted path of the offending input field, e.g. `lines.0.quantity`. */
  field: z.string().min(1).optional(),
  productId: z.string().min(1).optional(),
  productName: z.string().min(1).optional(),
  weight: WeightSchema.optional(),
  /** Units in stock. */
  available: z.int().nonnegative().optional(),
  /** Seconds until a rate-limited action may be retried. */
  retryAfter: z.int().positive().optional(),
})
export type ApiErrorDetails = z.infer<typeof ApiErrorDetailsSchema>

/** The body of an {@link ApiError}. */
export const ApiErrorBodySchema = z.strictObject({
  code: ErrorCodeSchema,
  /** {@link ERRORS}`[code].message`, exactly. */
  message: z.string().min(1),
  details: ApiErrorDetailsSchema.optional(),
})
export type ApiErrorBody = z.infer<typeof ApiErrorBodySchema>

/** The envelope of every non-2xx response. */
export const ApiErrorSchema = z.strictObject({
  error: ApiErrorBodySchema,
})
export type ApiError = z.infer<typeof ApiErrorSchema>

/** The error envelope of `code`, with its standard message. */
export function apiError(code: ErrorCode, details?: ApiErrorDetails): ApiError {
  return {
    error: {
      code,
      message: ERRORS[code].message,
      ...(details
        ? { details }
        : {}),
    },
  }
}

/** Named schemas this module contributes to `components.schemas`. */
export const commonComponents = {
  ErrorCode: ErrorCodeSchema,
  ApiErrorDetails: ApiErrorDetailsSchema,
  ApiErrorBody: ApiErrorBodySchema,
  ApiError: ApiErrorSchema,
  Ok: OkSchema,
}

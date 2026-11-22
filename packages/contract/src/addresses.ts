// The address book. Owner in phase 2 wave 2: server-orders.
import { z } from 'zod'
import { API_PREFIX } from './common.ts'
import { defineEndpoint, defineEndpoints } from './endpoint.ts'

/** Address label: 家 / 公司 / 学校. */
export const AddressTagSchema = z.enum(['home', 'work', 'school'])
export type AddressTag = z.infer<typeof AddressTagSchema>

/** Display label of each tag. */
export const ADDRESS_TAG_LABELS: Record<AddressTag, string> = { home: '家', work: '公司', school: '学校' }

/** At most this many addresses per member. */
export const MAX_ADDRESSES = 20

/** A mobile number as stored: exactly 11 digits. */
export const PhoneSchema = z.string().regex(/^\d{11}$/)

/** A saved address. */
export const AddressSchema = z.strictObject({
  id: z.int().positive(),
  /** 收货人. */
  recipient: z.string().min(1),
  /** The full number (the edit dialog shows it), or `null` when none was given. */
  phone: PhoneSchema.nullable(),
  /** `138 **** 5521` (cards, checkout), or `null` (the UI shows `—`). */
  phoneMasked: z.string().regex(/^\d{3} \*{4} \d{4}$/).nullable(),
  /** 所在地区, e.g. `上海市 徐汇区`. */
  region: z.string().min(1),
  /** 详细地址, e.g. `天平路 88 弄 3 号 602 室`. */
  detail: z.string().min(1),
  tag: AddressTagSchema,
  /** Exactly one address is the default whenever the book is not empty. */
  isDefault: z.boolean(),
})
export type Address = z.infer<typeof AddressSchema>

/** `GET /v1/addresses` and the result of every address change. */
export const AddressListSchema = z.strictObject({
  /** Oldest first (creation order). */
  items: z.array(AddressSchema),
  /** The default address, `null` when the book is empty. */
  defaultAddressId: z.int().positive().nullable(),
  /**
   * 本单寄往: the address checkout uses — the one chosen with `PUT /v1/cart/address`, else the
   * default; `null` (`未指定`) when the book is empty.
   */
  orderAddressId: z.int().positive().nullable(),
})
export type AddressList = z.infer<typeof AddressListSchema>

/**
 * Body of `POST /v1/addresses` and `PUT /v1/addresses/{id}` (a full replacement). Text fields
 * are trimmed; fields are checked in the order 收货人 → 所在地区 → 详细地址 → 手机号, and the
 * first failure answers its code.
 */
export const AddressInputSchema = z.strictObject({
  recipient: z.string({ error: 'recipient_required' })
    .trim()
    .min(1, { error: 'recipient_required' })
    .max(20, { error: 'text_too_long' }),
  region: z.string({ error: 'region_required' })
    .trim()
    .min(1, { error: 'region_required' })
    .max(40, { error: 'text_too_long' }),
  detail: z.string({ error: 'detail_required' })
    .trim()
    .min(1, { error: 'detail_required' })
    .max(80, { error: 'text_too_long' }),
  /** 11 digits, or `""` / `null` / absent for none. */
  phone: z.string({ error: 'phone_invalid' }).regex(/^(?:\d{11})?$/, { error: 'phone_invalid' }).nullable().optional(),
  tag: AddressTagSchema.default('home'),
  /**
   * `true` makes it the only default. `false` on the current default keeps it the default
   * (the book always has one). The first address is always the default.
   */
  isDefault: z.boolean().default(false),
})
export type AddressInput = z.input<typeof AddressInputSchema>

/** Result of creating or updating an address. */
export const AddressSavedSchema = z.strictObject({
  /** The saved address (the toast shows `{地区} {详细地址}`; the cursor moves to it). */
  address: AddressSchema,
  /** The whole book after the change (the default may have moved). */
  book: AddressListSchema,
})
export type AddressSaved = z.infer<typeof AddressSavedSchema>

/** Path of the `/v1/addresses/{id}` routes. */
export const AddressParamsSchema = z.strictObject({
  id: z.coerce.number<string>().int().positive(),
})
export type AddressParams = z.infer<typeof AddressParamsSchema>

export const addressComponents = {
  AddressTag: AddressTagSchema,
  Address: AddressSchema,
  AddressList: AddressListSchema,
  AddressInput: AddressInputSchema,
  AddressSaved: AddressSavedSchema,
}

export const listAddresses = defineEndpoint({
  method: 'get',
  path: `${API_PREFIX}/addresses`,
  operationId: 'listAddresses',
  tag: 'addresses',
  summary: '地址本',
  access: 'internal',
  auth: 'user',
  fingerprint: 'none',
  success: { status: 200, schema: AddressListSchema },
  errors: [],
})

export const createAddress = defineEndpoint({
  method: 'post',
  path: `${API_PREFIX}/addresses`,
  operationId: 'createAddress',
  tag: 'addresses',
  summary: '新增地址',
  description: 'The first address becomes the default (and so the order address). `isDefault: true` moves the default to it.',
  access: 'internal',
  auth: 'user',
  fingerprint: 'none',
  request: { body: AddressInputSchema },
  success: { status: 201, schema: AddressSavedSchema },
  errors: [
    'recipient_required',
    'region_required',
    'detail_required',
    'phone_invalid',
    'text_too_long',
    'address_limit',
  ],
})

export const updateAddress = defineEndpoint({
  method: 'put',
  path: `${API_PREFIX}/addresses/{id}`,
  operationId: 'updateAddress',
  tag: 'addresses',
  summary: '编辑地址（整体替换）',
  access: 'internal',
  auth: 'user',
  fingerprint: 'none',
  request: { params: AddressParamsSchema, body: AddressInputSchema },
  success: { status: 200, schema: AddressSavedSchema },
  errors: [
    'not_found',
    'recipient_required',
    'region_required',
    'detail_required',
    'phone_invalid',
    'text_too_long',
  ],
})

export const deleteAddress = defineEndpoint({
  method: 'delete',
  path: `${API_PREFIX}/addresses/{id}`,
  operationId: 'deleteAddress',
  tag: 'addresses',
  summary: '删除地址（无确认）',
  description: 'Deleting the default makes the oldest remaining address the default. Deleting the order address falls back to the default (`orderAddressId`).',
  access: 'internal',
  auth: 'user',
  fingerprint: 'none',
  request: { params: AddressParamsSchema },
  success: { status: 200, schema: AddressListSchema },
  errors: ['not_found'],
})

export const setDefaultAddress = defineEndpoint({
  method: 'post',
  path: `${API_PREFIX}/addresses/{id}/default`,
  operationId: 'setDefaultAddress',
  tag: 'addresses',
  summary: '设为默认地址（已是默认时不变）',
  access: 'internal',
  auth: 'user',
  fingerprint: 'none',
  request: { params: AddressParamsSchema },
  success: { status: 200, schema: AddressListSchema },
  errors: ['not_found'],
})

export const addressEndpoints = defineEndpoints([
  listAddresses,
  createAddress,
  updateAddress,
  deleteAddress,
  setDefaultAddress,
])

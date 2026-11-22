// Area `addresses` (owner: server-orders). One function per endpoint, typed from the
// contract; rules in packages/contract/src/addresses.ts and docs/design/decisions.md
// (P-addr-1…4).
//
// Every change runs in one transaction and keeps the book's invariant: exactly one default
// whenever it is not empty (the first address becomes it; deleting it promotes the oldest
// remaining one). Field validation (order, lengths, phone) comes from the contract schema.
import type {
  AddressList,
  AddressSaved,
  createAddress as createAddressEndpoint,
  deleteAddress as deleteAddressEndpoint,
  listAddresses as listAddressesEndpoint,
  setDefaultAddress as setDefaultAddressEndpoint,
  updateAddress as updateAddressEndpoint,
} from '@coffee/contract'
import type { Db } from '../db.ts'
import type { Address as AddressRow } from '../generated/prisma/client.ts'
import type { HandlerInput } from '../http/route.ts'
import { MAX_ADDRESSES } from '@coffee/contract'
import { fail } from '../middleware/error-handler.ts'
import { loadAddressList, toAddress } from './address-book.ts'

/** The validated body (`AddressInput` after the schema's trims and defaults). */
type ParsedAddressInput = HandlerInput<typeof createAddressEndpoint>['body']

/** The stored columns of a validated input: `""`, `null` or no phone is `null`. */
function addressColumns(input: ParsedAddressInput): Pick<AddressRow, 'recipient' | 'phone' | 'region' | 'detail' | 'tag'> {
  return {
    recipient: input.recipient,
    phone: input.phone || null,
    region: input.region,
    detail: input.detail,
    tag: input.tag,
  }
}

/** The member's address `id`, or `404 not_found` (unknown or someone else's). */
async function ownAddress(db: Db, userId: number, id: number): Promise<AddressRow> {
  return (await db.address.findFirst({
    where: { id, userId },
  })) ?? fail('not_found')
}

/** Makes `id` the member's only default. */
async function makeDefault(db: Db, userId: number, id: number): Promise<void> {
  await db.address.updateMany({
    where: {
      userId,
      isDefault: true,
      id: { not: id },
    },
    data: { isDefault: false },
  })
  await db.address.update({
    where: { id },
    data: { isDefault: true },
  })
}

/** `GET /v1/addresses`. */
export function listAddresses({ deps, user }: HandlerInput<typeof listAddressesEndpoint>): Promise<AddressList> {
  return loadAddressList(deps.prisma, user.id)
}

/** `POST /v1/addresses` → 201. */
export function createAddress({ deps, user, body }: HandlerInput<typeof createAddressEndpoint>): Promise<AddressSaved> {
  return deps.prisma.$transaction(async (tx) => {
    const count = await tx.address.count({
      where: { userId: user.id },
    })
    if (count >= MAX_ADDRESSES)
      fail('address_limit')
    const isDefault = count === 0 || body.isDefault
    const row = await tx.address.create({
      data: {
        userId: user.id,
        ...addressColumns(body),
        isDefault: false,
      },
    })
    if (isDefault)
      await makeDefault(tx, user.id, row.id)
    return saved(tx, user.id, row.id)
  })
}

/** `PUT /v1/addresses/{id}`: a full replacement; `isDefault: false` never removes the default. */
export function updateAddress({ deps, user, params, body }: HandlerInput<typeof updateAddressEndpoint>): Promise<AddressSaved> {
  return deps.prisma.$transaction(async (tx) => {
    const row = await ownAddress(tx, user.id, params.id)
    await tx.address.update({
      where: { id: row.id },
      data: addressColumns(body),
    })
    if (body.isDefault && !row.isDefault)
      await makeDefault(tx, user.id, row.id)
    return saved(tx, user.id, row.id)
  })
}

/**
 * `DELETE /v1/addresses/{id}`: the cart stops pointing at it (本单寄往 falls back to the
 * default), and deleting the default makes the oldest remaining address the default.
 */
export function deleteAddress({ deps, user, params }: HandlerInput<typeof deleteAddressEndpoint>): Promise<AddressList> {
  return deps.prisma.$transaction(async (tx) => {
    const row = await ownAddress(tx, user.id, params.id)
    await tx.cart.updateMany({
      where: { addressId: row.id },
      data: { addressId: null },
    })
    await tx.address.delete({
      where: { id: row.id },
    })
    if (row.isDefault) {
      const oldest = await tx.address.findFirst({
        where: { userId: user.id },
        orderBy: { id: 'asc' },
        select: { id: true },
      })
      if (oldest)
        await makeDefault(tx, user.id, oldest.id)
    }
    return loadAddressList(tx, user.id)
  })
}

/** `POST /v1/addresses/{id}/default` (already the default: unchanged). */
export function setDefaultAddress({ deps, user, params }: HandlerInput<typeof setDefaultAddressEndpoint>): Promise<AddressList> {
  return deps.prisma.$transaction(async (tx) => {
    const row = await ownAddress(tx, user.id, params.id)
    if (!row.isDefault)
      await makeDefault(tx, user.id, row.id)
    return loadAddressList(tx, user.id)
  })
}

/** The saved address and the whole book after the change. */
async function saved(db: Db, userId: number, id: number): Promise<AddressSaved> {
  const [row, book] = await Promise.all([
    db.address.findUniqueOrThrow({
      where: { id },
    }),
    loadAddressList(db, userId),
  ])
  return {
    address: toAddress(row),
    book,
  }
}

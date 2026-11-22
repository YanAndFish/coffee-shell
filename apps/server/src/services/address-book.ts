// Address rows → contract values, and the order-address rule (本单寄往). Shared foundation:
// the cart shows the order address, checkout snapshots it, the address book lists it.
import type { Address, AddressList, AddressTag, OrderAddress } from '@coffee/contract'
import type { PrismaClient } from '../db.ts'
import type { Address as AddressRow } from '../generated/prisma/client.ts'
import { AddressTagSchema } from '@coffee/contract'

/** `13800005521` → `138 **** 5521`; `null` for no phone (the UI shows `—`). */
export function maskPhone(phone: string | null): string | null {
  if (!phone)
    return null
  return `${phone.slice(0, 3)} **** ${phone.slice(-4)}`
}

/** Maps a row to the contract's `Address`. */
export function toAddress(row: AddressRow): Address {
  return {
    id: row.id,
    recipient: row.recipient,
    phone: row.phone,
    phoneMasked: maskPhone(row.phone),
    region: row.region,
    detail: row.detail,
    tag: AddressTagSchema.parse(row.tag) satisfies AddressTag,
    isDefault: row.isDefault,
  }
}

/** The snapshot an order keeps of its address. */
export function toOrderAddress(row: Pick<AddressRow, 'recipient' | 'phone' | 'region' | 'detail'>): OrderAddress {
  return {
    recipient: row.recipient,
    phone: row.phone,
    phoneMasked: maskPhone(row.phone),
    region: row.region,
    detail: row.detail,
  }
}

/** A member's addresses, oldest first. */
export function addressRows(prisma: PrismaClient, userId: number): Promise<AddressRow[]> {
  return prisma.address.findMany({
    where: { userId },
    orderBy: { id: 'asc' },
  })
}

/**
 * 本单寄往: the cart's chosen address (when it still exists and is the member's), else the
 * default address, else `null`.
 */
export function pickOrderAddress(rows: readonly AddressRow[], chosenId: number | null): AddressRow | null {
  return rows.find(row => row.id === chosenId)
    ?? rows.find(row => row.isDefault)
    ?? null
}

/** The order address of `userId` (see {@link pickOrderAddress}). */
export async function loadOrderAddress(prisma: PrismaClient, userId: number): Promise<AddressRow | null> {
  const [rows, cart] = await Promise.all([
    addressRows(prisma, userId),
    prisma.cart.findUnique({
      where: { userId },
      select: { addressId: true },
    }),
  ])
  return pickOrderAddress(rows, cart?.addressId ?? null)
}

/** The contract's `AddressList` of `userId`. */
export async function loadAddressList(prisma: PrismaClient, userId: number): Promise<AddressList> {
  const [rows, cart] = await Promise.all([
    addressRows(prisma, userId),
    prisma.cart.findUnique({
      where: { userId },
      select: { addressId: true },
    }),
  ])
  return {
    items: rows.map(toAddress),
    defaultAddressId: rows.find(row => row.isDefault)?.id ?? null,
    orderAddressId: pickOrderAddress(rows, cart?.addressId ?? null)?.id ?? null,
  }
}

// Area `addresses` (owner: server-orders). One function per endpoint, typed from the
// contract; rules in packages/contract/src/addresses.ts and docs/design/decisions.md.
//
// Building blocks: `services/address-book.ts` (`toAddress`, `loadAddressList`,
// `pickOrderAddress`, `maskPhone`); `MAX_ADDRESSES` from the contract.
import type {
  AddressList,
  AddressSaved,
  createAddress as createAddressEndpoint,
  deleteAddress as deleteAddressEndpoint,
  listAddresses as listAddressesEndpoint,
  setDefaultAddress as setDefaultAddressEndpoint,
  updateAddress as updateAddressEndpoint,
} from '@coffee/contract'
import type { HandlerInput } from '../http/route.ts'
import { notImplemented } from '../middleware/error-handler.ts'

/** `GET /v1/addresses`. */
export function listAddresses(_input: HandlerInput<typeof listAddressesEndpoint>): Promise<AddressList> {
  notImplemented()
}

/** `POST /v1/addresses` → 201. */
export function createAddress(_input: HandlerInput<typeof createAddressEndpoint>): Promise<AddressSaved> {
  notImplemented()
}

/** `PUT /v1/addresses/{id}`. */
export function updateAddress(_input: HandlerInput<typeof updateAddressEndpoint>): Promise<AddressSaved> {
  notImplemented()
}

/** `DELETE /v1/addresses/{id}`. */
export function deleteAddress(_input: HandlerInput<typeof deleteAddressEndpoint>): Promise<AddressList> {
  notImplemented()
}

/** `POST /v1/addresses/{id}/default`. */
export function setDefaultAddress(_input: HandlerInput<typeof setDefaultAddressEndpoint>): Promise<AddressList> {
  notImplemented()
}

// Example address book (data.md#地址（mock only）); phone numbers are fictional.
import type { Address, AddressList, AddressSaved } from '../addresses.ts'

export const homeAddress: Address = {
  id: 1,
  recipient: '林晓',
  phone: '13800005521',
  phoneMasked: '138 **** 5521',
  region: '上海市 徐汇区',
  detail: '天平路 88 弄 3 号 602 室',
  tag: 'home',
  isDefault: true,
}

export const workAddress: Address = {
  id: 2,
  recipient: '林晓',
  phone: '13800005521',
  phoneMasked: '138 **** 5521',
  region: '上海市 静安区',
  detail: '江宁路 300 号 12F（前台代收）',
  tag: 'work',
  isDefault: false,
}

export const friendAddress: Address = {
  id: 3,
  recipient: '陈默',
  phone: '13900000817',
  phoneMasked: '139 **** 0817',
  region: '浙江省 杭州市 西湖区',
  detail: '文三路 128 号 5 幢 1 单元 301',
  tag: 'home',
  isDefault: false,
}

/** `fixtures/v1/addresses/list.json`: the default is also the order address. */
export const addressListExample: AddressList = {
  items: [homeAddress, workAddress, friendAddress],
  defaultAddressId: 1,
  orderAddressId: 1,
}

/** 本单寄往 ≠ 默认: `PUT /v1/cart/address {addressId: 2}` was used. */
export const addressListOrderElsewhereExample: AddressList = {
  ...addressListExample,
  orderAddressId: 2,
}

/** A new member. */
export const addressListEmptyExample: AddressList = {
  items: [],
  defaultAddressId: null,
  orderAddressId: null,
}

/** `POST /v1/addresses` of a first address: it becomes the default and the order address. */
export const addressSavedFirstExample: AddressSaved = {
  address: homeAddress,
  book: {
    items: [homeAddress],
    defaultAddressId: 1,
    orderAddressId: 1,
  },
}

const schoolAddress: Address = {
  id: 4,
  recipient: '周一',
  phone: null,
  phoneMasked: null,
  region: '北京市 海淀区',
  detail: '学院路 37 号 2 号楼 415',
  tag: 'school',
  isDefault: false,
}

/** `POST /v1/addresses` without a phone, tagged 学校. */
export const addressSavedNoPhoneExample: AddressSaved = {
  address: schoolAddress,
  book: {
    items: [
      homeAddress,
      workAddress,
      friendAddress,
      schoolAddress,
    ],
    defaultAddressId: 1,
    orderAddressId: 1,
  },
}

/** `PUT /v1/addresses/2`: a new detail line. */
export const addressUpdatedExample: AddressSaved = {
  address: { ...workAddress, detail: '江宁路 300 号 15F（前台代收）' },
  book: {
    items: [
      homeAddress,
      { ...workAddress, detail: '江宁路 300 号 15F（前台代收）' },
      friendAddress,
    ],
    defaultAddressId: 1,
    orderAddressId: 1,
  },
}

/** `DELETE /v1/addresses/1` (the default): the oldest remaining one becomes the default. */
export const addressDeletedDefaultExample: AddressList = {
  items: [
    { ...workAddress, isDefault: true },
    friendAddress,
  ],
  defaultAddressId: 2,
  orderAddressId: 2,
}

/** `POST /v1/addresses/3/default`; no order address was chosen, so it follows the default. */
export const addressDefaultMovedExample: AddressList = {
  items: [
    { ...homeAddress, isDefault: false },
    workAddress,
    { ...friendAddress, isDefault: true },
  ],
  defaultAddressId: 3,
  orderAddressId: 3,
}

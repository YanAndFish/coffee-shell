# @coffee/contract

The single source of the ROAST/TTY `/v1` API: zod schemas (`src/*.ts`) generate `openapi.json` and
`fixtures/v1/**/*.json`. The server validates requests and (in tests) responses with these schemas; the Rust DTOs
(`crates/coffee-api`) deserialize every fixture; the TUI's fixture transport serves them. **Money is integer yuan and
is computed only by the server.**

```sh
pnpm --filter @coffee/contract generate          # write openapi.json and fixtures/
pnpm --filter @coffee/contract check:generated   # fail when they are stale (CI)
```

## Conventions

| Topic                  | Rule                                                                                                                                                                                                                                                                  |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Modules                | One per area: `health`, `auth`, `catalog`, `cart`, `addresses`, `orders`, `profile` (+ `common`, `sku`, `endpoint`). Each exports `<area>Components` and `<area>Endpoints`; `src/api.ts` lists them.                                                                  |
| Names                  | `FooSchema` + `type Foo = z.infer<…>`; request types are `z.input<…>` (fields with defaults are optional). Objects are `z.strictObject` (unknown keys are errors).                                                                                                    |
| Endpoints              | `defineEndpoint({ method, path, operationId, tag, summary, description?, access, auth, fingerprint, request?, success, errors })`, exported by name (the server types handlers from it).                                                                              |
| JSON                   | camelCase keys; `null` for “none” (never omitted) in responses; optional fields only in requests.                                                                                                                                                                     |
| Money                  | Integer yuan (`MoneySchema`). The TUI formats (`¥426.00`, `−¥10.00`) but never adds or multiplies: line totals, cart totals, the SKU panel's per-quantity totals (`PriceCell.totals`) and free-shipping progress all come from the server.                            |
| Time                   | `*At` = UTC instant (ISO 8601 with ms, `2026-11-22T13:04:00.000Z`); `*Date` / `*On` = calendar date in the shop's time zone, Asia/Shanghai (`2026-11-25`). The TUI only reformats them.                                                                               |
| Enums                  | Codes in the API (`WB`, `GIFT`, `home`, `roasting`, `light`); display labels are exported tables (`GRIND_LABELS`, `PACK_LABELS`, `ADDRESS_TAG_LABELS`, `ORDER_STATUS_LABELS`, `ROAST_NAMES`). SQLite stores the same strings.                                         |
| SKU                    | `RT-{product code}-{WEIGHT}-{grind}-{pack}` (`skuCode()`), e.g. `RT-PNK-250G-WB-STD`; a cart line's identity and `{sku}` path parameter.                                                                                                                              |
| Errors                 | Always `{ "error": { "code", "message", "details"? } }` (`ApiErrorSchema`). One table, `ERRORS` in `src/common.ts`: code → HTTP status + the one Chinese message (from `docs/design/copy.md`). `details` carries facts (below).                                       |
| Error codes in schemas | A request schema carries the error code as its zod message (`.min(1, { error: 'username_required' })`); the server answers the first issue's code, else `validation_failed` with `details.field` (`body.lines.0.weight`). Field order in a schema is the check order. |

### Transport (headers)

| Header                           | Who sets it                                   | Meaning                                                                                                                                                                                   |
| -------------------------------- | --------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `X-Internal-Key`                 | the TUI, from `COFFEE_INTERNAL_KEY`           | Gateway ↔ backend secret. When the backend has one, every route except `/v1/health` requires it (`401 unauthorized`). This is what makes the two headers below trustworthy.               |
| `Authorization: Bearer <token>`  | the TUI, after register / login / resume      | The member session: 43 base64url characters, 7 days, stored hashed. `auth: user` endpoints answer `401 login_required` without it and `401 session_expired` for a bad or old one.         |
| `X-SSH-Fingerprint`              | the TUI, from `COFFEE_SSH_FINGERPRINT`        | `SHA256:` + 43 base64 characters, as sshd computed it. Used by 记住我 (register/login), resume, logout, me and profile. Malformed → `400 validation_failed` (`header.x-ssh-fingerprint`). |
| `X-Client-IP`                    | the TUI, from `COFFEE_CLIENT_IP` (auth calls) | The SSH client's address, only for rate limits and logs.                                                                                                                                  |
| `Content-Type: application/json` | every request with a body                     | Bodies are JSON (≤ 64 KiB); anything else is `400 bad_request`.                                                                                                                           |

There is no pagination: lists are small. `GET /v1/orders` takes `limit` (1–50, default 20) and returns `total`.

### Error details

| Code                | `details`                                                                 |
| ------------------- | ------------------------------------------------------------------------- |
| `validation_failed` | `field`: dotted path, prefixed `body.` / `query.` / `params.` / `header.` |
| `out_of_stock`      | `productId`, `productName`, `available` (units in stock)                  |
| `sold_out`          | `productId`, `productName`, `weight`                                      |
| `rate_limited`      | `retryAfter` (seconds; also the `Retry-After` header)                     |

The full code table (status and message) is in `openapi.json` (`info.description`) and `fixtures/v1/errors/<code>.json`.

## Endpoints

“Implied” errors are not listed: `unauthorized` (internal endpoints), `login_required` + `session_expired` (`auth:
user`), `session_expired` (`auth: optional`), `validation_failed` (any input), `bad_request` (a body), and
`internal_error`. Owners are the phase 2 wave 2 server workers.

| Method | Path                         | operationId         | Area (owner)              | Auth | Fingerprint | Request                           | Success             | Errors (besides implied)                                                                                        |
| ------ | ---------------------------- | ------------------- | ------------------------- | ---- | ----------- | --------------------------------- | ------------------- | --------------------------------------------------------------------------------------------------------------- |
| GET    | `/v1/health`                 | `getHealth`         | health (—)                | none | none        | —                                 | 200 Health          | —                                                                                                               |
| POST   | `/v1/auth/register`          | `register`          | auth (server-auth)        | none | optional    | RegisterRequest                   | 201 AuthSession     | `username_required` `username_invalid` `password_too_short` `password_too_long` `username_taken` `rate_limited` |
| POST   | `/v1/auth/login`             | `login`             | auth (server-auth)        | none | optional    | LoginRequest                      | 200 AuthSession     | `username_required` `password_too_short` `password_too_long` `invalid_credentials` `rate_limited`               |
| POST   | `/v1/auth/resume`            | `resume`            | auth (server-auth)        | none | required    | —                                 | 200 AuthSession     | `fingerprint_required` `not_remembered`                                                                         |
| POST   | `/v1/auth/logout`            | `logout`            | auth (server-auth)        | user | optional    | —                                 | 200 Ok              | —                                                                                                               |
| GET    | `/v1/auth/me`                | `getMe`             | auth (server-auth)        | user | optional    | —                                 | 200 Me              | —                                                                                                               |
| GET    | `/v1/products`               | `listProducts`      | catalog (server-shop)     | none | none        | query origin,roast,sort,q         | 200 ProductList     | —                                                                                                               |
| GET    | `/v1/products/{id}`          | `getProduct`        | catalog (server-shop)     | none | none        | params id                         | 200 ProductDetail   | `not_found`                                                                                                     |
| GET    | `/v1/cart`                   | `getCart`           | cart (server-shop)        | user | none        | —                                 | 200 Cart            | —                                                                                                               |
| POST   | `/v1/cart/lines`             | `addCartLine`       | cart (server-shop)        | user | none        | AddCartLineRequest                | 200 Cart            | `not_found` `quantity_invalid` `sold_out` `out_of_stock` `cart_full`                                            |
| PATCH  | `/v1/cart/lines/{sku}`       | `updateCartLine`    | cart (server-shop)        | user | none        | params sku; UpdateCartLineRequest | 200 Cart            | `not_found` `quantity_invalid` `out_of_stock`                                                                   |
| DELETE | `/v1/cart/lines/{sku}`       | `removeCartLine`    | cart (server-shop)        | user | none        | params sku                        | 200 Cart            | `not_found`                                                                                                     |
| PUT    | `/v1/cart/selection`         | `setCartSelection`  | cart (server-shop)        | user | none        | CartSelectionRequest              | 200 Cart            | —                                                                                                               |
| PUT    | `/v1/cart/coupon`            | `applyCoupon`       | cart (server-shop)        | user | none        | ApplyCouponRequest                | 200 Cart            | `coupon_invalid`                                                                                                |
| DELETE | `/v1/cart/coupon`            | `removeCoupon`      | cart (server-shop)        | user | none        | —                                 | 200 Cart            | —                                                                                                               |
| PUT    | `/v1/cart/address`           | `setCartAddress`    | cart (server-shop)        | user | none        | SetCartAddressRequest             | 200 Cart            | `not_found`                                                                                                     |
| POST   | `/v1/cart/quote`             | `quoteCart`         | cart (server-shop)        | none | none        | CartQuoteRequest                  | 200 Cart            | `not_found` `quantity_invalid` `cart_full` `coupon_invalid`                                                     |
| POST   | `/v1/cart/merge`             | `mergeCart`         | cart (server-shop)        | user | none        | CartMergeRequest                  | 200 CartMergeResult | —                                                                                                               |
| GET    | `/v1/addresses`              | `listAddresses`     | addresses (server-orders) | user | none        | —                                 | 200 AddressList     | —                                                                                                               |
| POST   | `/v1/addresses`              | `createAddress`     | addresses (server-orders) | user | none        | AddressInput                      | 201 AddressSaved    | `recipient_required` `region_required` `detail_required` `phone_invalid` `text_too_long` `address_limit`        |
| PUT    | `/v1/addresses/{id}`         | `updateAddress`     | addresses (server-orders) | user | none        | params id; AddressInput           | 200 AddressSaved    | `not_found` `recipient_required` `region_required` `detail_required` `phone_invalid` `text_too_long`            |
| DELETE | `/v1/addresses/{id}`         | `deleteAddress`     | addresses (server-orders) | user | none        | params id                         | 200 AddressList     | `not_found`                                                                                                     |
| POST   | `/v1/addresses/{id}/default` | `setDefaultAddress` | addresses (server-orders) | user | none        | params id                         | 200 AddressList     | `not_found`                                                                                                     |
| POST   | `/v1/orders`                 | `placeOrder`        | orders (server-orders)    | user | none        | PlaceOrderRequest                 | 201 PlacedOrder     | `empty_selection` `address_required` `sold_out` `out_of_stock` `price_changed`                                  |
| GET    | `/v1/orders`                 | `listOrders`        | orders (server-orders)    | user | none        | query limit                       | 200 OrderList       | —                                                                                                               |
| GET    | `/v1/orders/{number}`        | `getOrder`          | orders (server-orders)    | user | none        | params number                     | 200 Order           | `not_found`                                                                                                     |
| GET    | `/v1/profile`                | `getProfile`        | profile (server-orders)   | user | optional    | —                                 | 200 Profile         | —                                                                                                               |
| GET    | `/v1/settings`               | `getSettings`       | profile (server-orders)   | user | none        | —                                 | 200 Settings        | —                                                                                                               |
| PATCH  | `/v1/settings`               | `updateSettings`    | profile (server-orders)   | user | none        | SettingsPatch                     | 200 Settings        | —                                                                                                               |

## Fixtures

`fixtures/v1/` holds at least one success example per endpoint (`src/generate.ts` lists each with its endpoint and
status) and one envelope per error code (`errors/<code>.json`). The first three paths are frozen (the TUI and the sshd
e2e stub read them): `health.json`, `products.json` (the unfiltered list), `error.json` (404). State variants the UI
renders: empty / partly selected / coupon / problem carts, the guest quote, the merge result, empty and non-empty
address books (order address ≠ default), the receipt (`orders/placed.json`), empty and full order lists (all three
statuses), the profile with and without a taste profile, settings.

## Adding or changing an endpoint

Follow `.claude/skills/add-api-endpoint/SKILL.md`. In short: schema + `defineEndpoint` in the area module (register
new schemas in `<area>Components`), an example in `src/examples/<area>.ts` and its entry in `src/generate.ts`, then
`generate`; mirror the DTO in `crates/coffee-api` (its test fails on a fixture without a DTO); the server route is one
`route(router, deps, endpoint, service.fn)` line in `apps/server/src/routes/<area>.ts`; add a conformance case in
`apps/server/test/conformance/<area>.ts`. Every endpoint must appear in the table above (a test checks it).

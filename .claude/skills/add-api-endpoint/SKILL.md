---
name: add-api-endpoint
description: Add or change a backend endpoint of ROAST/TTY contract-first - zod schema in packages/contract, generated openapi.json and fixtures, the Rust DTO in crates/coffee-api, the Koa route in apps/server, and the TUI client. Use for any /v1 API change.
---

# Add an API endpoint (contract first)

The zod schemas in `@coffee/contract` (`packages/contract`) are the single source of the API. Both sides test
against the fixtures they generate, so drift fails a test on one side or the other. Conventions (headers, time,
money, error details) and the endpoint table: `packages/contract/README.md`. Decisions behind the rules:
`docs/design/decisions.md`.

## 1. Contract (`packages/contract`)

1. One module per area: `src/{auth,catalog,cart,addresses,orders,profile}.ts` (shared: `common.ts`, `sku.ts`).
   Schemas are `FooSchema` with `export type Foo = z.infer<typeof FooSchema>` (request types: `z.input`), objects
   are `z.strictObject`. Money is **integer yuan** (`MoneySchema`); the server computes all amounts. Responses use
   `null` for “none”, never an absent key.
2. Declare the endpoint with `export const fooBar = defineEndpoint({ method, path, operationId, tag, summary,
   description, access, auth: 'none' | 'optional' | 'user', fingerprint: 'none' | 'optional' | 'required',
   request: { params?, query?, body? }, success: { status, schema }, errors: [...] })`, add it to
   `<area>Endpoints` and every new named schema to `<area>Components` (request bodies too: OpenAPI `$ref`s them).
   `description` states the rules a handler must follow, so implementing it needs no further design.
3. Errors: one table, `ERRORS` in `src/common.ts` (code → HTTP status + the one Chinese message, also listed in
   `docs/design/copy.md#服务端错误`). A new code needs both. Request schemas carry codes as their zod message
   (`.min(1, { error: 'recipient_required' })`); field order is the check order. Implied codes (`unauthorized`,
   `login_required`, `session_expired`, `validation_failed`, `bad_request`, `internal_error`) are not listed.
4. Add example values in `src/examples/<area>.ts` and an entry in `src/generate.ts` (`path` under
   `fixtures/v1/<area>/`, `operationId`, `status`, schema); every endpoint needs at least one success fixture, and
   state variants the UI renders get their own. Then `pnpm --filter @coffee/contract generate` (writes `openapi.json`
   and the fixtures) and commit both; `check:generated` fails when they are stale. Add the endpoint's row to the
   README table (a test checks every `operationId` is there). pnpm 12 runs one script per
   `pnpm --filter <pkg> <script>`: chain separate commands with `&&`.

## 2. Rust DTO (`crates/coffee-api`)

1. Mirror the types in the area module (`src/<area>.rs`, re-exported at the root) with
   `#[serde(rename_all = "camelCase")]`; nullable → `Option<T>`; optional request fields →
   `#[serde(default, skip_serializing_if = "Option::is_none")]`; enums with `rename` / `snake_case` and a `label()`
   when people see them. A new error code goes into the `error_codes!` table in `src/lib.rs`.
2. Map the new fixture path in `check()` (`src/tests.rs`): every file in `packages/contract/fixtures/v1/` must
   round-trip through its DTO, and an unmapped fixture fails.
3. `cargo test -p coffee-api`.

## 3. Server (`apps/server`)

1. The route is already one line in `src/routes/<area>.ts`: `route(router, deps, endpoints.fooBar, service.fooBar)`.
   `src/http/route.ts` applies the session (`endpoint.auth`), `X-SSH-Fingerprint` (`endpoint.fingerprint`), parses
   params / query / body with the contract schemas (their codes become the answers), sets the success status and,
   in tests, checks the response against the success schema. Do not validate input by hand.
2. Implement `fooBar(input: HandlerInput<typeof fooBarEndpoint>)` in `src/services/<area>.ts`: `input.user`,
   `input.body`, `input.deps.prisma`, `input.deps.now()` (never `new Date()`, tests move the clock). Fail with
   `fail('code', details?)` (status and message come from the contract). Shared rules live in
   `services/pricing.ts` (all money), `shop-time.ts` (dates, order numbers, status), `products.ts`,
   `cart-view.ts` (priced cart), `address-book.ts`, `members.ts`; use them, do not re-derive.
3. Keep the internal-key check: when `COFFEE_INTERNAL_KEY` is set, requests without the matching `X-Internal-Key`
   are refused. The server listens on `HOST`/`PORT` (127.0.0.1:3000) only.
4. Prisma (7.9.1) changes: edit `prisma/schema.prisma`, then create the migration against a scratch database
   (`DATABASE_URL=file:/tmp/x.db npx prisma migrate dev --name …`, then `npx prisma generate`), update the seed
   (`prisma/seed.ts`, idempotent). `pnpm --filter @coffee/server db:migrate` (= `prisma migrate deploy`) and
   `db:seed` apply them, and `pnpm dev` runs both on every start. The database is `apps/server/.data/coffee.db`
   (git-ignored; `DATABASE_URL=file:...` overrides). **`db:reset` is for humans only**: Prisma refuses
   `migrate reset` when an AI agent runs it - never work around that guard; ask the user to run it.
5. Tests: add a case to `test/conformance/<area>.ts` (the harness calls the real app, parses the answer with the
   contract schema, and lists endpoints still answering `501 not_implemented`); build data with
   `test/support/members.ts` (`createMember`, `addCartRow`, `addAddressRow`, `addOrderRow`, `rememberKey`) instead
   of other areas' routes. Unit-test pure rules next to `test/pricing.test.ts`.

## 4. TUI client (`apps/tui/src/api`, `apps/tui/src/stores`)

1. Add a request through `api::ApiClient` (it sends `X-Internal-Key` from `session_env()` and decodes into the
   `coffee_api` DTO with `api::decode`); member calls also send `Authorization: Bearer <token>` and, where the
   contract says so, `X-SSH-Fingerprint` (`coffee_api::SSH_FINGERPRINT_HEADER`). The store's `SessionStore` holds the
   client and the token.
2. Call it from a store method; expose results as signals with explicit loading/empty/error states. Errors carry
   `ApiFailure::Status { code, message }`: compare `code` with `coffee_api::ErrorCode::*.as_str()`.
3. Add the fixture to `apps/tui/src/api/fixtures.rs` so the fixture transport (`COFFEE_API_URL=fixture:`, and the
   tests' default) answers it; tests then run against the committed fixtures.

## 5. Verify

```sh
pnpm --filter @coffee/contract generate && git status --short packages/contract   # fixtures committed?
pnpm check && pnpm test
cd apps/tui && nsh check && nsh test
```

With `pnpm dev` running, `curl -H "X-Internal-Key: $KEY" http://127.0.0.1:3000/v1/<path>` cannot work because the
key is random per run - test the route through vitest, or start the server alone with your own key
(`COFFEE_INTERNAL_KEY=dev pnpm --filter @coffee/server dev`).

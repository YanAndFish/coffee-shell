---
name: add-api-endpoint
description: Add or change a backend endpoint of ROAST/TTY contract-first - zod schema in packages/contract, generated openapi.json and fixtures, the Rust DTO in crates/coffee-api, the Koa route in apps/server, and the TUI client. Use for any /v1 API change.
---

# Add an API endpoint (contract first)

The zod schemas in `@coffee/contract` (`packages/contract`) are the single source of the API. Both sides test
against the fixtures they generate, so drift fails a test on one side or the other.

## 1. Contract (`packages/contract`)

1. One module per area: `packages/contract/src/<area>.ts` (see `products.ts`). Schemas are `FooSchema` with
   `export type Foo = z.infer<typeof FooSchema>`, objects are `z.strictObject`. Money is **integer yuan**
   (`price250: z.int().nonnegative()`); the server computes all amounts.
2. Export `<area>Components` (named schemas) and `<area>Endpoints = defineEndpoints([...])` (method, path under
   `/v1`, request/success schemas), and append both to the lists in `src/api.ts` so they reach `openapi.json`.
3. Add example values in `src/examples/<area>.ts` and regenerate: `pnpm --filter @coffee/contract generate` writes
   `openapi.json` and `fixtures/v1/<name>.json`. Commit both; `pnpm --filter @coffee/contract check:generated`
   fails when they are stale. (pnpm 12 runs one script per `pnpm --filter <pkg> <script>`: chain separate
   commands with `&&`.)
4. Errors use the shared error shape (`fixtures/v1/error.json`).

## 2. Rust DTO (`crates/coffee-api`)

1. Mirror the types with serde (`#[serde(rename_all = "camelCase")]` if the JSON is camelCase, as existing DTOs do).
2. Add the fixture to the test that deserializes every file in `packages/contract/fixtures/v1/` - a new fixture
   without a DTO must fail that test.
3. `cargo test -p coffee-api`.

## 3. Server (`apps/server`)

1. Add the Koa route under `/v1` and validate input with the contract's zod schema; respond with the contract's
   response schema.
2. Keep the internal-key check: when `COFFEE_INTERNAL_KEY` is set, requests without the matching `X-Internal-Key`
   are refused. The server listens on `HOST`/`PORT` (127.0.0.1:3000) only.
3. Money rules live here only: unit price = round(base × {0.45, 1, 1.9, 3.6}) + packaging (0/20); quantity 1–9 and
   ≤ stock; `TTY10` = 10 % off; shipping ¥12, free from ¥199 after discount; checkout is one transaction.
4. Prisma (7.9.1) changes: edit `prisma/schema.prisma`, add a migration under `prisma/migrations/`, update the
   seed (`prisma/seed.ts`); `pnpm --filter @coffee/server db:migrate` (= `prisma migrate deploy`) and `db:seed`
   apply them, and `pnpm dev` runs both on every start. The database is `apps/server/.data/coffee.db`
   (git-ignored; `DATABASE_URL=file:...` overrides). **`db:reset` is for humans only**: Prisma refuses
   `migrate reset` when an AI agent runs it - never work around that guard; ask the user to run it.
5. A vitest test (`apps/server/test/routes.test.ts`, helpers in `test/helpers.ts`) calls the route and parses
   the real response with the contract schema.

## 4. TUI client (`apps/tui/src/api`, `apps/tui/src/stores`)

1. Add a request through `api::ApiClient` (it sends `X-Internal-Key` from `session_env()` and decodes into the
   `coffee_api` DTO with `api::decode`); the store's `SessionStore` holds the client.
2. Call it from a store method; expose results as signals with explicit loading/empty/error states.
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

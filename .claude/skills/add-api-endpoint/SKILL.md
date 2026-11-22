---
name: add-api-endpoint
description: Add or change a backend endpoint of ROAST/TTY contract-first - zod schema in packages/contract, generated openapi.json and fixtures, the Rust DTO in crates/coffee-api, the Koa route in apps/server, and the TUI client. Use for any /v1 API change.
---

# Add an API endpoint (contract first)

The zod schemas in `@coffee/contract` (`packages/contract`) are the single source of the API. Both sides test
against the fixtures they generate, so drift fails a test on one side or the other.

## 1. Contract (`packages/contract`)

1. Add/extend the zod schemas for the request and response in `packages/contract/src/`. Money is integer fen or a
   decimal string as the existing schemas do - follow them; the server computes all amounts.
   <!-- TODO(integration): confirm money representation and schema file layout -->
2. Register the route (method, path under `/v1`, schemas) so it appears in `openapi.json`.
3. Add example values and regenerate: `pnpm --filter @coffee/contract generate` writes `openapi.json` and
   `fixtures/v1/<name>.json`. Commit both. <!-- TODO(integration): confirm the script name -->
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
4. Prisma changes: edit `prisma/schema.prisma`, then `pnpm --filter @coffee/server db:migrate` and update the seed
   (`db:seed`). `db:reset` rebuilds from scratch. Runtime data lives in `.data/` (git-ignored).
5. A vitest test calls the route and parses the real response with the contract schema.

## 4. TUI client (`apps/tui/src/api`, `apps/tui/src/stores`)

1. Add a client function returning the `coffee_api` DTO; it sends `X-Internal-Key` from `session_env()`.
2. Call it from a store method; expose results as signals with explicit loading/error states.
3. Tests use a stubbed network answering with the fixture.

## 5. Verify

```sh
pnpm --filter @coffee/contract generate && git status --short packages/contract   # fixtures committed?
pnpm check && pnpm test
cd apps/tui && nsh check && nsh test
```

With `pnpm dev` running, `curl -H "X-Internal-Key: $KEY" http://127.0.0.1:3000/v1/<path>` cannot work because the
key is random per run - test the route through vitest, or start the server alone with your own key
(`COFFEE_INTERNAL_KEY=dev pnpm --filter @coffee/server dev`).

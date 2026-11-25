# Phase 2 acceptance

How to re-run the acceptance of ROAST/TTY phase 2 (the shop works end to end over a stock `ssh`). Part A is the
automated checks; part B is the same flows by hand, for a person at a terminal. Everything runs on a throwaway
database: neither part touches `apps/server/.data/coffee.db`.

## A. Automated

From the repository root, after `pnpm install`:

| #   | Command                                                    | Passes when                                                       |
| --- | ---------------------------------------------------------- | ----------------------------------------------------------------- |
| A1  | `pnpm check`                                               | eslint, mock sizes, `tsc`, `cargo fmt --check`, clippy all clean  |
| A2  | `pnpm test`                                                | Rust tests (SSH end-to-end in `crates/coffee-sshd/tests`), vitest |
| A3  | `pnpm check:tui` and `pnpm test:tui` (needs the nsh CLI)   | `nsh check`, every `apps/tui/*.test.tsx` headless and on the TUI  |
| A4  | `pnpm test:e2e:real`                                       | every flow of `scripts/e2e` passes (about 2 minutes)              |
| A5  | `python3 scripts/e2e/run.py --repeat 5`                    | stability: 5 × 5 flows, zero failures                             |
| A6  | `deploy/smoke-test.sh` (Docker; CI's `docker` job runs it) | the compose pair answers, serves the catalogue and sells one bag  |

A4 builds `coffee-tui` and `coffee-sshd`, migrates and seeds one template database, and runs each flow on its own
copy with its own backend and `coffee-sshd` on free ports, through stock `ssh` clients on pseudo-terminals. Text
screenshots and logs land in `.data/e2e/<flow>/` (`NN-<session>-<step>.txt`, `backend.log`, `sshd.log`, `FAIL.txt`
with the last screens on a failure). `python3 scripts/e2e/run.py main money` runs some flows; `E2E_WAIT=30` gives
slow machines more time per step (default 20 s).

What the flows check (`scripts/e2e/flows.py`):

- **main** — a client with a key: the login page shows `[✓] 记住我 30 天 · 绑定当前 SSH 公钥`; register; filter by origin
  (`f`), clear (`Esc`), search (`/瑰夏`); open 波奎特 瑰夏, choose 100g · 手冲, add. A second client **without a key**
  browses as a guest (`g`), adds 保山 小粒 日晒; `c` on the cart is gated to login (`结算前请先登录`); logging in merges
  the guest cart (`已合并访客购物车 1 件`) and returns to the cart. A new address: the empty form is refused
  (`✗ 请填写收货人`), then saved. `TTY10` applied; the amounts equal the rules (below). `c`: the receipt has an order
  number `RT` + the shop's date (Asia/Shanghai, yymmdd) + 4 digits, the amount paid, `! 演示订单 · 没有真实扣款，也不会发货`;
  the database holds that order with those amounts. `4`: the order is listed as `烘焙中`. Disconnect; reconnect with
  the key: `已通过 SSH 公钥恢复登录`. `q` asks `确定退出登录吗？` / `同时解除当前公钥的免密登录。`; `q` again logs out. A
  new connection with that key opens on the login page and no key is remembered.
- **money** — the cart's 商品小计 / 优惠 / 运费 / 合计 against an independent implementation of the rules: unit price =
  round(base × {100g 0.45, 250g 1, 500g 1.9, 1kg 3.6}) + pack (标准袋 0, 礼盒装 20); `TTY10` = round(subtotal × 0.1);
  shipping ¥12, free when subtotal − discount ≥ ¥199. Cases: 2 × 耶加雪菲 250g = ¥196 (+¥12), with `TTY10` (−¥20, still
  +¥12); 耶加雪菲 + 慧兰 = ¥216 with `TTY10` (−¥22 → ¥194: shipping again) and without (free); 保山 250g + 3 × 保山
  100g 礼盒装 (¥51 each) = ¥221, with `TTY10` exactly ¥199 (free). The last cart is checked out: receipt and database
  agree.
- **stock** — 波奎特 瑰夏: 500g and 1kg show `缺货` and `→` cannot select them; the quantity stops at the stock
  (`仅剩 6 份`). Two members each put 4 in their carts and press `c` at the same moment: one receipt, one
  `下单失败` / `波奎特 瑰夏 库存不足，购物车没有变化`, one order, stock 2, the loser's cart unchanged. Also: a taken
  username (`✗ 这个用户名已被注册`), an invalid coupon (`✗ 优惠码无效或已过期`), a wrong password
  (`✗ 用户名或密码不正确`) and, on the sixth try within 10 minutes, `✗ 操作太频繁，请稍后再试`.
- **session** — a keyless client: no prompt, `[ ] 记住我` with `当前连接没有 SSH 公钥，无法记住登录`; after
  registering, the profile says `○ 本次连接没有公钥` and nothing is remembered; the next keyless connection opens on
  the login page. A client killed (SIGKILL to `ssh`) right after `c`: its `coffee-tui` is gone within 5 s and the
  order is either whole (order + item + stock − 1 + line gone) or absent (cart and stock unchanged). The backend
  stopped mid-session: the profile shows `✗ 个人信息加载失败` with `⏎ 重试`; restarted, `⏎` loads it (the session
  survives). A detail page with 500g chosen, resized to 60×20 (`终端太小`, `当前 60×20`) and back: same page, same
  choice.
- **gateway** — the backend down when a client connects: logging in says `✗ 连接服务器失败，按 ⏎ 重试`; the guest
  list says `✗ 商品加载失败` with `⏎ 重试`, which works once the backend is back. `coffee-sshd` started with a missing
  `COFFEE_TUI_BIN`: a warning at start, and a client sees `ROAST/TTY 暂时无法启动，请稍后再试。` with exit status 1,
  logged as `could not start the session program`.
- After every flow: no `coffee-tui` left under that flow's `coffee-sshd`; neither log contains a password, the
  internal key or a bearer token; the backend's log has no full key fingerprint (the gateway logs them by design,
  ADR 0001).

## B. By hand

Two terminals, a scratch database, ports that are free:

```sh
pnpm install
export DATABASE_URL="file:$PWD/.data/acceptance.db"    # throwaway; .data/ is git-ignored
pnpm dev                                               # terminal 1; wait for “✓ ready”
pnpm ssh                                               # terminal 2: with your default key
ssh -p 2222 -o PubkeyAuthentication=no -o IdentitiesOnly=yes \
  -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null localhost   # without a key
```

Do not use a real password anywhere. Tick each line:

1. [ ] The banner says `演示环境 · 不是真实商店`; no password prompt with or without a key.
2. [ ] With a key: login page, `[✓] 记住我`. `Shift+Tab` `⏎` switches to 注册; type a name (2–16 characters) and a
       password (≥ 6), `⏎`: `注册成功，欢迎 …`, then the catalogue.
3. [ ] `f` cycles origins, `r` roasts, `s` sorting, `Esc` clears; `/` searches (`⏎` leaves the box).
4. [ ] `⏎` opens a product; `↑↓` rows, `←→` choices (sold-out weights are skipped), `+`/`-` quantity (stops at the
       stock); `a` adds (`已加入购物车`).
5. [ ] Without a key: `Esc`, `g` browses as a guest; add something; `2`, `c`: `结算前请先登录`; log in: the guest cart
       merges, back on the cart.
6. [ ] `3`, `n`: `⏎` on the empty form shows `✗ 请填写收货人`; fill it, `⏎`: `已新增地址`.
7. [ ] `2`, `Tab` to the coupon, `tty10` `⏎`: `✓ 已使用 TTY10`; the amounts follow the rules in part A; `Esc`.
8. [ ] `c`: the receipt, order number `RT` + yymmdd + 4 digits, the demo notice; `4`: the order, `烘焙中`.
9. [ ] Ctrl+C twice leaves (`再按一次 Ctrl+C 退出` in between). Reconnect with the key: `已通过 SSH 公钥恢复登录`.
10. [ ] `q`: the confirmation; `q` again: `已退出登录`. Reconnect with the key: the login page.
11. [ ] Resize the window below 80×24: `终端太小`; back: the same page.
12. [ ] Stop `pnpm dev` with Ctrl+C. `pgrep -fl "scripts/dev.mjs|coffee-sshd|coffee-tui|src/main.ts"` prints nothing;
        `lsof -nP -iTCP:3000 -iTCP:2222 -sTCP:LISTEN` too. `rm .data/acceptance.db`.

## Not covered

- Payment, shipping and e-mail: the shop has none (a demo).
- A real network between client and gateway (latency, packet loss): everything runs on loopback; ADR 0001 covers
  keepalives for vanished clients.
- Terminals other than the emulator in `scripts/pty-run.py` (xterm-256color answers): check real terminals by hand
  (colours: ADR 0001).

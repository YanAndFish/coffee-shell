# coffee-app — the ROAST/TTY terminal UI

The nsh 0.6.1 (Rust mode) UI of the terminal coffee shop, as a library: `coffee-host` (bin `coffee-tui`) runs
`coffee_app::App` in a terminal, one process per SSH session. Rules: `CLAUDE.md` and
`.claude/skills/nsh-tui-conventions`; the design: `docs/design/`.

```sh
nsh check                 # cargo check + the TS check of App.test.tsx
nsh test                  # crate tests + App.test.tsx headless
nsh test --target tui     # App.test.tsx on an in-memory terminal
python3 ../../scripts/pty-run.py --size 120x36 --env COFFEE_API_URL=fixture: -- ../../target/debug/coffee-tui
```

## Module map

```
src/
  lib.rs                 frozen host interface: App, registry, SessionEnv, configure, session_env, APP_ID, THEME_JSON, api
  api/                   the backend client
    mod.rs               ApiClient, Transport, Call/Endpoint, ApiFailure, decode, now_or_never, username_is_valid
    endpoints.rs         one Endpoint const + one typed async method per /v1 operation
    fixtures.rs          the fixture transport: FixtureSet scenarios over the committed contract fixtures
    tests.rs             every endpoint decoded from its fixtures, error mapping, headers, scenarios
  stores/                Copy structs of private signals, provided once by App (Stores::provide)
    mod.rs               Stores (all stores + viewport), spawn, busy; testing::with_stores for unit tests
    remote.rs            Remote<T>: idle / loading / ready / empty / failed, stale answers dropped
    session.rs           SessionStore: guest/member, token, login form, resume, return-to route, task spawner
    catalog.rs           CatalogStore: list query (filters, sort, search), facets, cursor, filter dialog
    product.rs           ProductStore: one product's detail and the SKU selection rules
    cart.rs              CartStore: member cart / guest quote, merge, undo, coupon, checkout, receipt
    addresses.rs         AddressStore: book, grid cursor, dialog draft with the server's field errors
    orders.rs            OrdersStore     profile.rs  ProfileStore     settings.rs  SettingsStore
    toast.rs             ToastStore: queue, kinds, durations
    ui.rs                UiStore: overlays (help, logout, address dialog, filter dialog), focused field
    actions.rs           cross-store flows: login/register/resume/logout, add to cart, checkout, addresses…
  router.rs              Route, Page, Router (login first; tabs 1–4; detail over the catalog)
  commands.rs            the command model (Command, Key, Scope, Typing, Footer, KeyContext), global and
                         overlay commands, resolve / footer / bindings / press, mode()
  pages/
    mod.rs               PageModule registry (PAGES), page(route), dialog(overlay)
    format.rs            money / dates / quantities as copy.md writes them (formatting only)
    placeholder.rs       the plain bodies pages ship with: Line, screen, panel, lines, field
    login/ catalog/ detail/ cart/ addresses/ profile/
      mod.rs             the page: MODULE (mode, commands, view, dialogs) and its view (placeholder today)
      commands.rs        the page's COMMANDS: keys, footer ranks, help entries
  shell/                 header (badge, account, key indicator), status bar, outlets (toast, dialog),
                         overlays (help, logout confirmation placeholders), too-small guard, key bindings
  ui/                    the design-system components (the only module touching nsh_base and theme tokens)
```

Dependencies point down: `shell → pages → stores → api → coffee_api`, and everything visual through `ui`.

## How a key reaches its command

The shell binds every keystroke of the table once (`commands::bindings`). A key press builds the
`KeyContext` (page on screen, overlay open, a text field focused, something submitting) and runs the first command that
is active there and enabled (`commands::resolve`), in one `batch`. The order is keymap.md's:

1. help open → only `Scope::Overlay(Help)` (`?` `Esc` `q` close it);
2. a dialog open → only that dialog's commands (no `?`, K24);
3. a text field focused → only commands with `Typing::Only` / `Typing::Also` (`⏎` submit, `Esc` leave); letters go
   into the field (nsh does not fire `When::Auto` bindings there);
4. something submitting (`Stores::busy`) → only `while_busy()` commands (`?`, a dialog's `Esc`);
5. `Scope::Pages` (`?`), then on app pages `Scope::App` (`1`–`4` `/` `q`), then `Scope::Page(page)`.

The status bar shows the active, enabled commands with a footer rank for the width class: `footer(rank)` (all
widths), `footer_full(rank)` (M/L/XL), `footer_s(rank)` (S), `short("f 筛选")` for the S label; at most
`Bp::footer_hints()` (S 4, M 6, L/XL 7) and what the row holds. A press on a hint runs `commands::press`. A command
with `Key::hint(..)` (no strokes) is a hint only: its key is handled elsewhere (Tab by nsh, `4` by the global
command) and a press runs it.

## Fixture scenarios (no backend needed)

`COFFEE_API_URL=fixture:<scenario>` (the host) or storage `coffee.transport` (tests, `nsh run`) picks one; storage
`coffee.fingerprint` (or `COFFEE_SSH_FINGERPRINT`) gives the connection an SSH key. Answers take one engine turn.

| Scenario                     | What it is                                                                                   |
| ---------------------------- | -------------------------------------------------------------------------------------------- |
| `fixture:` / `fixture:guest` | a new visitor: the key (if any) is not remembered; logging in gives `林晓` and every fixture |
| `fixture:member`             | the key is remembered for `林晓`: resume signs in at once (needs a key)                      |
| `fixture:empty`              | no products, empty cart, no addresses, no orders, a new profile                              |
| `fixture:down`               | every request fails as a refused connection                                                  |
| `fixture:error`              | every request answers `500 internal_error`                                                   |

Rules in every scenario (details on `api::FixtureSet`): member calls without a token are `401 login_required`; login
with password `wrongpass` is `invalid_credentials`, registering `林晓` is `username_taken`; validation codes as the
server sends them; the product list is filtered/sorted/searched from `products.json` (equal to the committed list
fixtures); only `pinkb` and `geisha` have detail fixtures; cart, address and order changes answer their static fixtures
(an add does not change the cart's lines).

In Rust unit tests, `stores::testing::with_stores(set, fingerprint, (cols, rows), |stores| …)` runs every task in
place (`SessionStore::inline`), so whole flows run synchronously.

## How to build a screen

A screen worker edits **only its page's directory** `src/pages/<name>/`:

- `mod.rs`: replace the placeholder body (`render`) with the real layout, composed of `ui::*` (new looks go to `ui/`
  first, through the ui owner). Keep `MODULE`, `MODE` and the public constants (field names) other code uses. More
  files beside `mod.rs` as the page grows (`pages/<name>/*.rs`).
- `commands.rs`: the page's `COMMANDS` (already registered with the keymap's keys, footer ranks and labels — adjust,
  add, remove). Commands call store functions and `stores::actions`.
- Text fields report their focus to `UiStore` (`on_focus` → `field_focused(name, Some(ref))` then
  `focus_taken(name)`, `on_blur` → `field_blurred(name)`, and `cleanup(|| field_blurred(name))` for a field that goes
  away focused); see `pages::placeholder::field`. Then letters type, `⏎`/`Esc` reach the page's `Typing` commands and
  the footer shows the field's hints. A field asked to focus (`UiStore::focus_requested`) focuses itself.
- Loads start from the page's entry: `effect(move || untracked(|| store.ensure_loaded()))`.
- Tests: add `App.test.tsx` cases for the screen (they open the app with `platform: 'linux'` and explicit sizes).

What each screen has today (the files are `pages/<name>/*`; `stores.x` is a field of `Stores`):

### 登录 `login`

- Store: `stores.session` — `draft`, `mode`, `set_username`, `set_password`, `toggle_remember`, `set_mode`,
  `set_error`, `step` (`AuthStep::{Connect, Verify, Sync}` with `label`/`percent`), `submitting`, `resuming`,
  `has_key`, `return_to`. Actions: `actions::submit_auth`, `actions::continue_as_guest`.
- Keys: `Tab 切换` (hint), `⏎ 登录` / `⏎ 注册` (also while typing), `Esc 离开输入` (typing), `Esc 返回登录`
  (register), `g 访客浏览`; hints `→ 注册新账号`, `space 记住我`. Fields: `USERNAME_FIELD`, `PASSWORD_FIELD`.
- Placeholder: two bare fields, 记住我, the error line, the step, the buttons as lines; mode `登录 LOGIN` /
  `注册 SIGN UP`.

### 选购 `catalog`

- Store: `stores.catalog` — `phase`, `failure`, `products`, `count`, `product`, `facets`, `origins`, `query`,
  `cursor`, `current`, `is_cursor`, `move_cursor`, `select`, `set_origin` / `set_roast` / `set_sort` /
  `set_search`, `cycle_origin` / `cycle_roast` / `cycle_sort`, `clear_filters`, `ensure_loaded`, `load`; the S
  filter dialog: `open_filter`, `filter_options`, `filter_selected`, `filter_focus`, `filter_move`,
  `filter_next_group`, `filter_jump`, `filter_choose`. Actions: `actions::open_product`, `quick_add`, `search`.
- Keys: `↑↓ 移动`, `⏎ 详情` (`l` `→`), `a 加入`, `f 产区` / `f 筛选` (S: the dialog), `r 烘焙`, `s 排序`,
  `/ 搜索` (hint), `Esc 清除筛选`, `⏎ 重试`; in the search box `Esc` / `⏎` leave it; the dialog `↑↓` `Tab` `⏎` /
  `space` `f r s` `Esc`. Field: `actions::SEARCH_FIELD`.
- Placeholder: the query line, rows with `▶`, phase lines, the search box when asked for, the dialog's lines.

### 详情 `detail`

- Store: `stores.product` — `id`, `phase`, `failure`, `detail`, `selection`, `move_focus`, `focus`, `step`,
  `step_quantity`, `select_weight` / `select_grind` / `select_pack`, `set_quantity`, `can_buy`, `unit_price`,
  `total` (the server's `PriceCell.totals`), `sku`, `line`, `open`, `load`. Actions: `actions::add_selection`
  (`buy_now`), `actions::back`.
- Keys: `↑↓ 规格行`, `←→ 选择` (`h` `l`), `+ 数量` (`+` `=` `-` `_`), `a 加入`, `b 购买`, `Esc 返回`
  (`Backspace`), `⏎ 重试`.
- Placeholder: the profile line, stock, the SKU rows with `›` and `[chosen]`, the server's total, the SKU code.

### 购物车 `cart`

- Store: `stores.cart` — `phase`, `failure`, `cart` (every amount), `lines`, `item_count`, `selected_count`,
  `total`, `guest`, `cursor`, `cursor_line`, `is_cursor`, `move_cursor`, `select_line`, `undo`, `coupon_input`,
  `set_coupon_input`, `coupon_feedback`, `checkout_step`, `submitting`, `receipt`, `has_receipt`,
  `checkout_check`. Actions: `actions::toggle_line`, `toggle_all`, `step_line`, `remove_line`, `undo_remove`,
  `apply_coupon`, `checkout`, `continue_shopping`.
- Keys: `↑↓ 移动`, `space 勾选`, `←→ 数量` (`+ = - l h`), `x 删除` (`d` `Delete`), `u 撤销`, `a 全选`,
  `c 结算` (`⏎`), `Esc`; the coupon field `⏎ 应用`, `Esc 离开输入`; the receipt `⏎ 继续选购` (`Esc`),
  `4 查看订单`, `1–4 切换页面`, `/ 搜索`. Field: `COUPON_FIELD`.
- Placeholder: lines, the undo bar, the address, coupon feedback, the server's amounts, the checkout step, the
  receipt.

### 地址本 `addresses`

- Store: `stores.addresses` — `phase`, `failure`, `items`, `count`, `address`, `order_address`,
  `default_address`, `cursor` (`AddressCursor::{Card(id), New}`), `cursor_address`, `move_cursor`, `select`,
  `dialog` (`AddressDraft`: `editing`, `input`, `error: FieldError { field, message }`, `saving`), `edit`,
  `saving`, `ensure_loaded`, `load`. Actions: `actions::use_address`, `set_default_address`, `delete_address`,
  `new_address`, `edit_address`, `save_address`, `cancel_address`, `go_login`.
- Keys: member `←↑↓→ 移动` (`↑↓` by the grid's columns), `⏎ 用于本单` (on `+`: new), `n 新增`, `e 编辑`,
  `space 默认`, `d 删除` (`x`), `Esc` (to the cart), `⏎ 重试`; guest `⏎ 去登录`, `1–4`, `/`; the dialog
  `Tab 下一项`, `←→ 标签`, `space 设为默认`, `⏎ 保存`, `Esc 取消` (also while saving). Fields: `FIELDS`.
- Placeholder: cards with their tags and `▶`, `+ 新增收货地址`, the guest card; the dialog's four bare fields,
  tag, error line, saving.

### 我的 `profile`

- Stores: `stores.profile` (`profile`, `phase`, `failure`, `ensure_loaded`), `stores.orders` (`items`, `total`,
  `phase`, `ensure_loaded`), `stores.settings` (`animations`, `mouse_tracking`), `stores.session` (`member`,
  `remembered`, `has_key`). Actions: `actions::toggle_setting`, `ask_logout`, `go_login`, `open_product`.
- Keys: `m 动画开关`, `o 鼠标追踪`, `q 退出登录` (hint; the key is the global `q`), `1–4 切换页面`, `/ 搜索`;
  guest `⏎ 去登录`; `⏎ 重试`.
- Placeholder: account, the 记住我 line, stats, taste, recommendations, settings, orders; the guest card.

### Shell overlays (next wave)

- Files: `shell/overlays.rs` (help, logout confirmation), `shell/outlets.rs` (toast and dialog outlets),
  `shell/header.rs`, `shell/footer.rs`. State: `stores.ui` (`overlay`, `logout_choice`), `stores.session`
  (`key_remembered`, `logging_out`), `stores.toasts` (`current`, `queue`, `duration`); the help reads
  `commands::all()`.
- Keys: help `Esc` / `q` / `?` close; logout `q` / `⏎` confirm, `Tab` `←` `→` switch, `Esc` cancel.

Money is never computed here: every amount is a field of the server's answer (`Cart.totals`, `CartLine.line_total`,
`PriceCell.totals`), formatted with `pages::format`.

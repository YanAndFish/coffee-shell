"""What a shopper does in ROAST/TTY, as steps over a `Term` (scripts/e2e/harness.py).

Each step waits for what the screen shows, never for a fixed time or a fixed number of key
presses: a step that moves the focus or a selection presses its key until the screen says it
got there (with a bound), so a layout or focus-order change fails loudly instead of typing into
the wrong place.

Money is computed here independently of the server (docs/design/decisions.md U7), so the flows
compare what the UI shows with the rules, not with the server's own arithmetic.
"""

from __future__ import annotations

import datetime
import re
import zoneinfo

from harness import Failure, Term

PASSWORD = "e2e-pass-1"  # throwaway: the demo's accounts are not real

# --- Money (U7), half up in integers like apps/server/src/services/pricing.ts ----------------

WEIGHT_PERCENT = {"100g": 45, "250g": 100, "500g": 190, "1kg": 360}
PACK_SURCHARGE = {"STD": 0, "GIFT": 20}
SHIPPING_FEE, FREE_SHIPPING = 12, 199


def div_round(numerator: int, denominator: int) -> int:
  return (2 * numerator + denominator) // (2 * denominator)


def unit_price(base: int, weight: str, pack: str) -> int:
  return div_round(base * WEIGHT_PERCENT[weight], 100) + PACK_SURCHARGE[pack]


def quote(lines: list[tuple[int, str, str, int]], coupon: bool) -> dict[str, int]:
  """lines: (base price, weight, pack, quantity) → subtotal, discount, shipping, total."""
  subtotal = sum(unit_price(base, weight, pack) * qty for base, weight, pack, qty in lines)
  discount = min(subtotal, div_round(subtotal * 10, 100)) if coupon else 0
  shipping = 0 if subtotal == 0 or subtotal - discount >= FREE_SHIPPING else SHIPPING_FEE
  return {"subtotal": subtotal, "discount": discount, "shipping": shipping,
          "total": subtotal - discount + shipping}


def shop_today() -> datetime.date:
  return datetime.datetime.now(zoneinfo.ZoneInfo("Asia/Shanghai")).date()


# --- Login page ------------------------------------------------------------------------------

def on_login_page(t: Term) -> None:
  """The login card, ready: the page opens (and returns) with the focus in 用户名 (keymap K1),
  so the status bar says 离开输入. Keys sent before that land elsewhere."""
  t.wait("用户名", "密码", "演示环境 · 请勿使用真实密码")
  t.wait_footer("离开输入")


def login_field(t: Term) -> str | None:
  """The login card's text field holding the focus (`用户名` / `密码`), from the status bar
  (`离开输入` while typing) and where the cursor sits (the field below its label)."""
  if "离开输入" not in t.footer():
    return None
  _, row = t.cursor()
  for label in ("用户名", "密码"):
    at = find_label(t, label)
    if at and at[0] < row <= at[0] + 3:
      return label
  return None


def focus_login_field(t: Term, label: str) -> None:
  for _ in range(8):
    if login_field(t) == label:
      return
    before = (t.footer(), t.cursor())
    t.key("tab")
    t.wait_for(lambda: (t.footer(), t.cursor()) != before, "Tab moving the focus", 5)
  raise t.fail(f"Tab never reached the {label} field")


def find_label(t: Term, label: str) -> tuple[int, int] | None:
  """A field's label: alone on its line in a box, or in the field's top border at compact
  sizes (`┏━ 用户名 ━━┓`); not the same word inside a sentence."""
  return (t.find_re(rf"[│║┃]\s+({label})(?:\s{{2,}}|\s*[│║┃]|$)")
          or t.find_re(rf"[┏╭][━─] ({label}) [━─]"))


def field_value(t: Term, label: str) -> str:
  """The text in the field under `label` (`› value`; a password shows as dots)."""
  at = find_label(t, label)
  if not at:
    raise t.fail(f"no field labelled {label}")
  for row in range(at[0] + 1, min(at[0] + 4, t.screen.rows)):
    cells = t.columns(at[1])[row]
    match = re.search(r"›\s?(.*?)\s*[┃│]", cells)
    if match:
      return match.group(1).strip()
  raise t.fail(f"cannot read the {label} field")


def set_login_field(t: Term, label: str, value: str) -> None:
  focus_login_field(t, label)
  for _ in range(len(field_value(t, label)) + 2):
    t.key("backspace")
  t.type(value)
  shown = value if label != "密码" else "•" * len(value)
  t.wait_for(lambda: field_value(t, label) == shown, f"{label} showing what was typed", 5)


def auth_error(t: Term, message: str) -> None:
  """A refused login or registration: the error shows, then the card gives the focus back to
  a field (a frame later). Keys sent in between are undone by that focus: wait for it."""
  t.wait(message)
  t.wait_for(lambda: login_field(t) is not None, "the focus back in a field after the error")


def leave_field(t: Term) -> None:
  if "离开输入" in t.footer():
    t.key("esc")
    t.wait_for(lambda: "离开输入" not in t.footer(), "Esc leaving the field", 5)


def to_register_mode(t: Term) -> None:
  """From the login card: the switch button is the last control of the Tab cycle (keymap.md),
  so Shift+Tab from 用户名 reaches it; ⏎ turns the card to 注册."""
  on_login_page(t)
  if "注册 SIGN UP" in t.text():
    return
  focus_login_field(t, "用户名")
  t.key("backtab")
  t.key("enter")
  seen = t.wait_any("创建账号", "商品列表")
  if seen != "创建账号":
    raise t.fail("⏎ on the control before 用户名 did not open 注册")


def register(t: Term, username: str, password: str = PASSWORD) -> None:
  to_register_mode(t)
  set_login_field(t, "用户名", username)
  set_login_field(t, "密码", password)
  t.key("enter")


def login(t: Term, username: str, password: str = PASSWORD) -> None:
  on_login_page(t)
  if "注册 SIGN UP" in t.text():
    leave_field(t)
    t.key("esc")
    t.wait("登录 · SIGN IN")
  set_login_field(t, "用户名", username)
  set_login_field(t, "密码", password)
  t.key("enter")


def browse_as_guest(t: Term) -> None:
  on_login_page(t)
  leave_field(t)
  t.wait_footer("访客")
  t.type("g")
  t.wait("商品列表")


# --- Catalog and detail ----------------------------------------------------------------------

def to_catalog(t: Term) -> None:
  t.type("1")
  t.wait("选购 BROWSE", "商品列表")


def search(t: Term, query: str) -> None:
  """`/`, the query, ⏎: the list shows only what matches. Esc first clears an earlier search
  (and the filters)."""
  t.key("esc")
  t.wait("商品列表 · 8 款")
  t.type("/")
  t.wait_footer("离开搜索")
  t.type(query)
  t.key("enter")
  t.wait_for(lambda: "离开搜索" not in t.footer() and re.search(r"商品列表 · \d+ 款", t.text()),
             f"the list after searching {query!r}")


def clear_filters(t: Term) -> None:
  t.key("esc")
  t.wait("商品列表 · 8 款")


def open_product(t: Term, query: str, name: str) -> None:
  to_catalog(t)
  search(t, query)
  t.wait("商品列表 · 1 款", name)
  t.key("enter")
  # The SKU code shows once the detail has loaded: keys before that do nothing.
  t.wait("详情 DETAIL", name, "规格 SKU", "SKU RT-")


SKU_GROUPS = ("重量", "研磨", "包装", "数量")


def sku_panel(t: Term) -> list[str]:
  at = t.find("规格 SKU")
  if not at:
    raise t.fail("no 规格 SKU panel")
  return t.columns(max(at[1] - 3, 0))


def sku_group(t: Term) -> str | None:
  for line in sku_panel(t):
    for group in SKU_GROUPS:
      if f"› {group}" in line:
        return group
  return None


def sku_choice(t: Term, group: str) -> str | None:
  """The highlighted option (`▐ 250g ▌`) of a group, or the quantity of 数量."""
  panel = sku_panel(t)
  for i, line in enumerate(panel):
    if re.search(rf"(›|\s)\s{group}\b", line) and i + 1 < len(panel):
      below = panel[i + 1]
      if group == "数量":
        match = re.search(r"│\s*(\d+)\s*│", below)
      else:
        # 右边缘在真彩色和 256 色下用 ▐ 绘制，在 16 色和单色下用 ▌ 绘制。
        match = re.search(r"▐\s*([^▐▌]+?)\s*[▐▌]", below)
      return match.group(1) if match else None
  return None


def to_sku_group(t: Term, group: str) -> None:
  for _ in range(len(SKU_GROUPS) + 1):
    now = sku_group(t)
    if now == group:
      return
    order = SKU_GROUPS.index
    key = "down" if now is None or order(now) < order(group) else "up"
    t.key(key)
    t.wait_for(lambda: sku_group(t) != now, f"{key} moving to another SKU row", 5)
  raise t.fail(f"never reached the {group} row")


def choose(t: Term, group: str, option: str) -> None:
  """Selects `option` in `group` with ← →; fails when the row will not take it."""
  to_sku_group(t, group)
  for key in ("right", "left"):
    for _ in range(6):
      if sku_choice(t, group) == option:
        return
      before = sku_choice(t, group)
      t.key(key)
      t.pause(0.2)
      if sku_choice(t, group) == before:
        break
  if sku_choice(t, group) != option:
    raise t.fail(f"{group} cannot be set to {option} (it shows {sku_choice(t, group)})")


def set_quantity(t: Term, quantity: int) -> int:
  """Presses + / - towards `quantity`; returns where it stopped (the stock caps it)."""
  to_sku_group(t, "数量")
  for _ in range(12):
    now = int(sku_choice(t, "数量") or 0)
    if now == quantity:
      return now
    t.type("+" if now < quantity else "-")
    t.pause(0.2)
    if int(sku_choice(t, "数量") or 0) == now:
      return now
  return int(sku_choice(t, "数量") or 0)


def add_to_cart(t: Term) -> None:
  t.type("a")
  t.wait("已加入购物车")


# --- Cart ------------------------------------------------------------------------------------

MONEY = r"¥([\d,]+)\.(\d\d)"


def yuan(text: str) -> int:
  match = re.fullmatch(MONEY, text)
  if not match:
    raise ValueError(text)
  return int(match.group(1).replace(",", ""))


def to_cart(t: Term) -> None:
  t.type("2")
  t.wait("购物车 CART")


def cart_amounts(t: Term) -> dict[str, int] | None:
  text = t.text()
  found = {}
  for key, pattern in (("subtotal", rf"商品小计\s+({MONEY})"), ("discount", rf"优惠\s+−({MONEY})"),
                       ("shipping", rf"运费\s+(包邮|{MONEY})"), ("total", rf"合计\s+({MONEY})")):
    match = re.search(pattern, text)
    if not match:
      return None
    found[key] = 0 if match.group(1) == "包邮" else yuan(match.group(1))
  return found


def expect_amounts(t: Term, expected: dict[str, int], label: str) -> None:
  try:
    t.wait_for(lambda: cart_amounts(t) == expected, f"cart amounts {expected}", 10)
  except Failure as failure:
    raise t.fail(f"{label}: expected {expected}, the cart shows {cart_amounts(t)}") from failure
  t.snap(label)


def cart_cursor_to(t: Term, name: str) -> None:
  for key in ("down", "up"):
    for _ in range(12):
      row = next((line for line in t.text().splitlines() if "▶" in line), "")
      if name in row:
        return
      t.key(key)
      t.pause(0.15)
  raise t.fail(f"the cart cursor never reached {name}")


def remove_line(t: Term, name: str) -> None:
  cart_cursor_to(t, name)
  t.type("x")
  t.wait("已从购物车移除")


def focus_coupon(t: Term) -> None:
  for _ in range(8):
    if "应用" in t.footer() and "离开输入" in t.footer():
      return
    before = t.footer()
    t.key("tab")
    t.wait_for(lambda: t.footer() != before, "Tab moving the focus", 5)
  raise t.fail("Tab never reached the coupon field")


def enter_coupon(t: Term, code: str) -> None:
  """Types `code` into the coupon field and applies it (empty: removes the coupon)."""
  focus_coupon(t)
  for _ in range(16):
    t.key("backspace")
  t.type(code)
  t.key("enter")


def checkout(t: Term) -> dict[str, str]:
  """`c` on the cart → the receipt; returns its order number and amount."""
  t.type("c")
  text = t.wait("订单回执", "订单已提交")
  number = re.search(r"订单号 (RT\d+)", text)
  paid = re.search(rf"实付\s+({MONEY})", text)
  if not number or not paid:
    raise t.fail("the receipt lacks an order number or the amount paid")
  return {"number": number.group(1), "paid": paid.group(1)}


def check_order_number(t: Term, number: str) -> None:
  expected = "RT" + shop_today().strftime("%y%m%d")
  if not re.fullmatch(r"RT\d{10}", number) or not number.startswith(expected):
    raise t.fail(f"order number {number} is not RT + {expected[2:]} (shop date) + 4 digits")


# --- Addresses -------------------------------------------------------------------------------

def open_new_address(t: Term) -> None:
  t.type("3")
  t.wait("地址本 ADDRESS")
  t.type("n")
  t.wait("新增地址 NEW", "收货人")


def fill_address(t: Term, recipient: str, phone: str, region: str, detail: str) -> None:
  """The dialog's fields in Tab order (收货人 → 手机号 → 所在地区 → 详细地址), each checked."""
  for value in (recipient, phone, region, detail):
    t.type(value)
    t.wait(value)
    t.key("tab")


def add_address(t: Term, recipient: str = "测试员", phone: str = "13800001234",
                region: str = "上海市 黄浦区", detail: str = "南京东路 1 号") -> None:
  open_new_address(t)
  fill_address(t, recipient, phone, region, detail)
  t.key("enter")
  t.wait("已新增地址")

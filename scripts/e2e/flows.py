"""The Phase 2 acceptance flows (docs/acceptance.md), each on a fresh database.

A flow gets a running `Stack` (backend + coffee-sshd) and drives stock `ssh` clients through
it. It fails by raising `Failure` (a wait that timed out carries its screen); `snap(label)`
keeps a named text screenshot next to the flow's logs.
"""

from __future__ import annotations

import os
import time

from harness import Failure, Stack, Term, log, wait_until
from shop import (add_address, add_to_cart, auth_error, browse_as_guest, cart_cursor_to,
                  check_order_number, checkout, choose, clear_filters, enter_coupon, expect_amounts,
                  fill_address, leave_field, login, on_login_page, open_new_address, open_product,
                  quote, register, remove_line, search, set_login_field, set_quantity, sku_choice,
                  to_cart, to_catalog, unit_price, yuan)

SIZE = (160, 50)


def products(stack: Stack) -> dict[str, tuple[int, int]]:
  """id → (base price per 250 g, stock), straight from the database."""
  return {row[0]: (row[1], row[2]) for row in stack.sql("SELECT id, basePrice, stock FROM Product")}


def orders(stack: Stack) -> list[tuple]:
  return stack.sql("SELECT number, subtotal, discount, shipping, total FROM \"Order\" ORDER BY id")


def check(condition: bool, message: str, t: Term | None = None) -> None:
  if not condition:
    raise t.fail(message) if t else Failure(message)


# --- Main flow -------------------------------------------------------------------------------

def flow_main(stack: Stack) -> None:
  """Register with a key, browse, buy as a guest who logs in, check out, resume, log out."""
  user = "小明_e2e"
  base = products(stack)

  # Connect with a key and register (记住我 is ticked by default when there is a key).
  member = stack.connect("member", key="main", size=SIZE)
  on_login_page(member)
  member.wait("● 公钥已识别", "[✓] 记住我")
  member.snap("login")
  register(member, user)
  member.wait("注册成功，欢迎 " + user, "选购 BROWSE")
  member.snap("registered")

  # Browse: filter by origin, then search.
  member.type("f")
  member.wait("商品列表 · 1 款", "◉ 埃塞俄比亚", "耶加雪菲 孔加")
  member.snap("filtered")
  clear_filters(member)
  search(member, "瑰夏")
  member.wait("商品列表 · 1 款", "波奎特 瑰夏")
  member.snap("searched")

  # A product: 100g, 手冲, added.
  member.key("enter")
  member.wait("详情 DETAIL", "规格 SKU", "SKU RT-")
  choose(member, "重量", "100g")
  choose(member, "研磨", "手冲")
  member.wait(f"1 × ¥{unit_price(base['geisha'][0], '100g', 'STD')}", "已选 100g · 手冲 · 标准袋")
  add_to_cart(member)
  member.snap("added")
  member.quit()

  # A guest in another connection, without a key: add, checkout is gated to login.
  guest = stack.connect("guest", key=None, size=SIZE)
  browse_as_guest(guest)
  guest.wait("○ 未提供公钥", "以访客身份浏览")
  to_catalog(guest)
  search(guest, "保山")
  guest.wait("商品列表 · 1 款", "保山 小粒 日晒")
  add_to_cart(guest)
  to_cart(guest)
  guest.type("c")
  guest.wait("结算前请先登录")
  on_login_page(guest)
  guest.snap("checkout-gated")

  # Log in: the guest cart merges into the account, back on the cart.
  login(guest, user)
  guest.wait("欢迎回来，" + user, "已合并访客购物车 1 件", "购物车 CART")
  guest.wait("波奎特 瑰夏", "保山 小粒 日晒")
  guest.snap("merged")

  # A new address: the empty form is refused first, then saved.
  open_new_address(guest)
  guest.key("enter")
  guest.wait("✗ 请填写收货人")
  guest.snap("address-error")
  fill_address(guest, "测试员", "13800001234", "上海市 黄浦区", "南京东路 1 号")
  guest.key("enter")
  guest.wait("已新增地址", "地址本 · 1 条", "测试员  138 **** 1234")
  guest.snap("address-saved")

  # TTY10, then the amounts the rules give.
  to_cart(guest)
  enter_coupon(guest, "tty10")
  guest.wait("✓ 已使用 TTY10")
  leave_field(guest)
  lines = [(base["geisha"][0], "100g", "STD", 1), (base["yunnan"][0], "250g", "STD", 1)]
  expected = quote(lines, coupon=True)
  expect_amounts(guest, expected, "cart-with-coupon")

  # Checkout → the receipt.
  receipt = checkout(guest)
  check_order_number(guest, receipt["number"])
  check(yuan(receipt["paid"]) == expected["total"], f"paid {receipt['paid']}, expected "
        f"¥{expected['total']}", guest)
  guest.wait("! 演示订单 · 没有真实扣款，也不会发货", "寄往 测试员")
  guest.snap("receipt")
  saved = orders(stack)
  check(len(saved) == 1 and saved[0][0] == receipt["number"]
        and saved[0][1:] == (expected["subtotal"], expected["discount"], expected["shipping"],
                             expected["total"]), f"the database holds {saved}", guest)

  # The profile lists the order with its status.
  guest.type("4")
  guest.wait("最近订单", receipt["number"])
  row = next(line for line in guest.text().splitlines() if receipt["number"] in line)
  check("烘焙中" in row, f"the order row has no status 烘焙中: {row.strip()}", guest)
  guest.snap("profile")
  guest.quit()

  # The same key again: resumed without a password.
  again = stack.connect("again", key="main", size=SIZE)
  again.wait("已通过 SSH 公钥恢复登录", user)
  again.snap("resumed")

  # q asks first; confirming logs out and forgets the key.
  again.type("q")
  again.wait("确定退出登录吗？", "同时解除当前公钥的免密登录。")
  again.snap("logout-confirm")
  again.type("q")
  again.wait("已退出登录")
  on_login_page(again)
  again.quit()

  # Reconnecting with that key shows the login page.
  after = stack.connect("after-logout", key="main", size=SIZE)
  on_login_page(after)
  after.wait_footer("离开输入")
  after.pause(1.5)  # a resume would land within this
  check("恢复登录" not in after.text() and user not in after.text(),
        "the key still resumed the account after logging out", after)
  check(not stack.sql("SELECT 1 FROM RememberedKey"), "a remembered key is left in the database")
  after.snap("login-again")
  after.quit()


# --- Money rules -----------------------------------------------------------------------------

def flow_money(stack: Stack) -> None:
  """The cart's amounts against the rules: under and over ¥199, with and without TTY10,
  exactly at the threshold, a gift pack and a 100 g bag."""
  base = products(stack)
  yirga, pinkb, yunnan = base["yirga"][0], base["pinkb"][0], base["yunnan"][0]
  t = stack.connect("buyer", key=None, size=SIZE)
  register(t, "算账_e2e")
  t.wait("注册成功")
  add_address(t)

  # Just under: 2 × 耶加雪菲 250g (quick add twice).
  to_catalog(t)
  search(t, "耶加")
  add_to_cart(t)
  t.gone("已加入购物车", timeout=10)
  add_to_cart(t)
  to_cart(t)
  t.wait("耶加雪菲 孔加")
  under = [(yirga, "250g", "STD", 2)]
  expect_amounts(t, quote(under, False), "under-199")
  t.wait("即可包邮")

  # TTY10 on it.
  enter_coupon(t, "TTY10")
  t.wait("✓ 已使用 TTY10")
  leave_field(t)
  expect_amounts(t, quote(under, True), "under-199-coupon")

  # Over ¥199 without the coupon, under it with: 耶加雪菲 ×1 + 慧兰 ×1.
  cart_cursor_to(t, "耶加雪菲 孔加")
  t.key("left")
  over = [(yirga, "250g", "STD", 1)]
  expect_amounts(t, quote(over, True), "one-yirga-coupon")
  to_catalog(t)
  clear_filters(t)
  search(t, "慧兰")
  add_to_cart(t)
  to_cart(t)
  over.append((pinkb, "250g", "STD", 1))
  expect_amounts(t, quote(over, True), "coupon-pulls-under-199")
  enter_coupon(t, "")
  t.wait_for(lambda: "已使用" not in t.text(), "the coupon removed")
  leave_field(t)
  expect_amounts(t, quote(over, False), "over-199")
  t.wait("已满 ¥199 · 本单包邮")

  # Exactly at the threshold after TTY10: 保山 250g + 3 × 保山 100g 礼盒装 = 221 − 22 = 199.
  remove_line(t, "耶加雪菲 孔加")
  remove_line(t, "慧兰 粉红波旁")
  t.wait("购物车是空的")
  open_product(t, "保山", "保山 小粒 日晒")
  add_to_cart(t)
  t.gone("已加入购物车", timeout=10)
  choose(t, "重量", "100g")
  choose(t, "包装", "礼盒装 +¥20")
  check(set_quantity(t, 3) == 3, "the quantity did not reach 3", t)
  gift = (yunnan, "100g", "GIFT", 3)
  unit = unit_price(yunnan, "100g", "GIFT")
  t.wait(f"3 × ¥{unit}", "已选 100g · 整豆 · 礼盒装")
  add_to_cart(t)
  to_cart(t)
  edge = [(yunnan, "250g", "STD", 1), gift]
  t.wait(f"100g · 整豆 · 礼盒装 ¥{unit}", f"¥{unit * 3}.00")
  expect_amounts(t, quote(edge, False), "gift-pack")
  enter_coupon(t, "tty10")
  t.wait("✓ 已使用 TTY10")
  leave_field(t)
  at_edge = quote(edge, True)
  check(at_edge["subtotal"] - at_edge["discount"] == 199 and at_edge["shipping"] == 0,
        f"the edge case is not at ¥199: {at_edge}")
  expect_amounts(t, at_edge, "exactly-199-free")

  receipt = checkout(t)
  check(yuan(receipt["paid"]) == at_edge["total"], f"paid {receipt['paid']}", t)
  saved = orders(stack)
  check(saved[-1][1:] == (at_edge["subtotal"], at_edge["discount"], at_edge["shipping"],
                          at_edge["total"]), f"the database holds {saved}", t)
  t.snap("receipt")
  t.quit()


# --- Stock and errors ------------------------------------------------------------------------

def prepare_geisha(t: Term, quantity: int) -> None:
  open_product(t, "瑰夏", "波奎特 瑰夏")
  check(set_quantity(t, quantity) == quantity, f"the quantity did not reach {quantity}", t)
  add_to_cart(t)


def flow_stock(stack: Stack) -> None:
  """Sold-out weights, the stock cap, two buyers racing for the last bags, and the errors of
  coupon, password, duplicate name and the login rate limit."""
  stock = products(stack)["geisha"][1]
  a = stack.connect("buyer-a", key=None, size=SIZE)
  register(a, "抢购_a")
  a.wait("注册成功")

  # 瑰夏: 500g and 1kg are sold out and cannot be chosen; the quantity stops at the stock.
  open_product(a, "瑰夏", "波奎特 瑰夏")
  a.wait("缺货")
  choose(a, "重量", "250g")
  a.key("right")
  a.pause(0.3)
  check(sku_choice(a, "重量") == "250g", f"→ chose {sku_choice(a, '重量')}, a sold-out weight", a)
  check(set_quantity(a, 9) == stock, f"the quantity passed the stock of {stock}", a)
  a.wait(f"仅剩 {stock} 份")
  a.snap("sold-out-and-cap")
  half = stock // 2 + 1  # two of these are more than the stock
  check(set_quantity(a, half) == half, "the quantity did not come down", a)
  add_to_cart(a)
  add_address(a)
  to_cart(a)

  # A second buyer: the name is taken first, then a free one.
  b = stack.connect("buyer-b", key=None, size=SIZE)
  register(b, "抢购_a")
  auth_error(b, "✗ 这个用户名已被注册")
  b.snap("username-taken")
  set_login_field(b, "用户名", "抢购_b")
  b.key("enter")
  b.wait("注册成功")
  prepare_geisha(b, half)
  add_address(b, recipient="二号买家")
  to_cart(b)

  # Both check out at once: one receipt, one clear refusal, one order.
  for t in (a, b):
    t.wait("波奎特 瑰夏", f"结算 {half} 件")
  a.session.send(b"c")
  b.session.send(b"c")
  results: dict[str, str] = {}

  def settled() -> bool:
    for t in (a, b):
      text = t.text()
      if "订单回执" in text:
        results[t.name] = "receipt"
      elif "下单失败" in text:
        results[t.name] = "refused"
    return len(results) == 2

  a.wait_for(settled, "both checkouts answered")
  check(sorted(results.values()) == ["receipt", "refused"], f"race results {results}")
  loser = a if results[a.name] == "refused" else b
  loser.wait("库存不足")
  loser.snap("race-refused")
  (b if loser is a else a).snap("race-won")
  check(len(orders(stack)) == 1, f"orders after the race: {orders(stack)}")
  left = products(stack)["geisha"][1]
  check(left == stock - half, f"stock {left} after selling {half} of {stock}")
  check(stack.sql("SELECT quantity FROM CartItem WHERE productId = 'geisha'") == [(half,)],
        "the refused buyer's cart changed")

  # An invalid coupon.
  loser.gone("下单失败", timeout=15)
  enter_coupon(loser, "NOPE10")
  loser.wait("✗ 优惠码无效或已过期")
  loser.snap("coupon-invalid")
  leave_field(loser)
  a.quit()
  b.quit()

  # A wrong password, then the login rate limit (5 failures per name in 10 minutes).
  c = stack.connect("login", key=None, size=SIZE)
  on_login_page(c)
  failures = 0
  for attempt in range(1, 7):
    set_login_field(c, "用户名", "抢购_a")
    set_login_field(c, "密码", f"wrong-pass-{attempt}")
    c.key("enter")
    if attempt <= 5:
      c.wait_for(lambda: stack.backend.log.read_text().count("login failed") == attempt,
                 f"the backend's answer to wrong password {attempt}")
      auth_error(c, "✗ 用户名或密码不正确")
      failures += 1
      if attempt == 1:
        c.snap("wrong-password")
    else:
      auth_error(c, "✗ 操作太频繁，请稍后再试")
      c.snap("rate-limited")
  check(failures == 5, f"{failures} plain failures before the limit")
  c.quit()


# --- Session hygiene -------------------------------------------------------------------------

def flow_session(stack: Stack) -> None:
  """No key: no prompt, nothing to remember. A client killed mid-checkout leaves nothing behind;
  a backend restart is an error with a retry; resizing keeps the page."""
  user = "会话_e2e"
  t = stack.connect("keyless", key=None, size=SIZE)
  on_login_page(t)
  t.wait("○ 未提供公钥", "[ ] 记住我", "当前连接没有 SSH 公钥，无法记住登录")
  t.snap("keyless-login")
  register(t, user)
  t.wait("注册成功")
  t.type("4")
  t.wait("○ 本次连接没有公钥", user)
  t.snap("keyless-profile")
  t.quit()
  check(not stack.sql("SELECT 1 FROM RememberedKey"), "a keyless login remembered something")

  # No key, nothing remembered: the next connection opens on the login page.
  t = stack.connect("keyless-again", key=None, size=SIZE)
  on_login_page(t)
  t.wait_footer("离开输入")
  login(t, user)
  t.wait("欢迎回来，" + user)
  add_address(t)
  open_product(t, "曼特宁", "曼特宁 G1")
  add_to_cart(t)
  to_cart(t)
  t.wait("曼特宁 G1", "结算 1 件")
  before_stock = products(stack)["mandh"][1]

  # Kill the client as it checks out.
  t.session.send(b"c")
  time.sleep(0.02)
  t.kill()
  if not wait_until(lambda: not stack.tui_pids(), 5):
    raise Failure(f"coffee-tui still running 5 s after the client died: {stack.tui_pids()}")
  if not wait_until(lambda: "client left" in stack.sshd.log.read_text(), 5):
    raise Failure("sshd did not log the session's end")
  time.sleep(1)  # a checkout already in the backend finishes (or not) in its transaction
  placed = orders(stack)
  cart = stack.sql("SELECT quantity FROM CartItem WHERE productId = 'mandh'")
  after_stock = products(stack)["mandh"][1]
  log(f"e2e:   killed mid-checkout: {'the order was placed whole' if placed else 'no order'}")
  if placed:
    whole = len(placed) == 1 and not cart and after_stock == before_stock - 1
    items = stack.sql("SELECT COUNT(*) FROM OrderItem")[0][0]
    check(whole and items == 1, f"a half-made order: {placed}, cart {cart}, stock {after_stock}")
  else:
    check(cart == [(1,)] and after_stock == before_stock,
          f"no order, yet cart {cart} and stock {after_stock} changed")

  # The backend goes away mid-session: an error with a retry; back, the retry works.
  t = stack.connect("restart", key=None, size=SIZE)
  login(t, user)
  t.wait("欢迎回来，" + user)
  to_catalog(t)
  stack.stop_backend()
  t.type("4")
  # The retry is the page's ⏎ (the profile does not list it in the status bar: reported).
  t.wait("个人信息加载失败", "⏎ 重试")
  t.snap("backend-down")
  stack.start_backend()
  t.key("enter")
  t.wait("最近订单", user)
  t.wait_for(lambda: "加载失败" not in t.text(), "the profile without errors")
  t.snap("backend-back")

  # Resize to 60×20 and back: the page (and the SKU chosen on it) stays.
  open_product(t, "耶加", "耶加雪菲 孔加")
  choose(t, "重量", "500g")
  t.resize(60, 20)
  t.wait("终端太小", "当前 60×20")
  t.snap("too-small")
  t.resize(*SIZE)
  t.gone("终端太小")
  t.wait("详情 DETAIL", "耶加雪菲 孔加")
  check(sku_choice(t, "重量") == "500g", f"the weight is {sku_choice(t, '重量')} after resizing", t)
  t.snap("resized-back")
  t.quit()


# --- Gateway edges ---------------------------------------------------------------------------

def flow_gateway(stack: Stack) -> None:
  """The backend down when a client connects: the app's error states, with a retry that works
  once it is back. A missing session program: a clear message, a clear log line."""
  stack.stop_backend()
  t = stack.connect("backend-down", key=None, size=SIZE)
  login(t, "无人_e2e")
  auth_error(t, "✗ 连接服务器失败，按 ⏎ 重试")
  t.snap("login-backend-down")
  browse_as_guest_failing(t)
  t.snap("list-backend-down")
  stack.start_backend()
  t.key("enter")
  t.wait("商品列表 · 8 款", "耶加雪菲 孔加")
  t.snap("list-after-retry")
  t.quit()

  stack.stop_sshd()
  stack.start_sshd(COFFEE_TUI_BIN=str(stack.work / "no-such-coffee-tui"))
  check("COFFEE_TUI_BIN does not exist" in stack.sshd.log.read_text(),
        "sshd did not warn about the missing session program at start")
  t = stack.connect("no-program", key=None, size=SIZE)
  t.wait("ROAST/TTY 暂时无法启动，请稍后再试。")
  t.exited()
  check(t.session.describe_exit() == "exited with code 1", f"ssh {t.session.describe_exit()}")
  t.snap("no-program")
  t.close()
  check("could not start the session program" in stack.sshd.log.read_text(),
        "sshd did not log why the session failed")
  stack.stop_sshd()
  stack.start_sshd()


def browse_as_guest_failing(t: Term) -> None:
  """`g` with the backend gone: the list's error state and its retry."""
  leave_field(t)
  t.wait_footer("访客")
  t.type("g")
  t.wait("商品加载失败", "⏎ 重试")
  t.wait_footer("重试")


# --- Compose smoke ---------------------------------------------------------------------------

def flow_smoke(stack: Stack) -> None:
  """A short purchase, for a gateway started elsewhere (deploy/smoke-test.sh): register → add →
  address → checkout → receipt. Reads nothing but the screen."""
  t = stack.connect("smoke", key="smoke", size=SIZE)
  user = f"烟测_{os.getpid() % 10000:04d}"
  register(t, user)
  t.wait("注册成功，欢迎 " + user)
  search(t, "耶加")
  t.wait("商品列表 · 1 款", "耶加雪菲 孔加")
  add_to_cart(t)
  add_address(t)
  to_cart(t)
  t.wait("耶加雪菲 孔加", "测试员")
  receipt = checkout(t)
  check_order_number(t, receipt["number"])
  t.wait("! 演示订单 · 没有真实扣款，也不会发货")
  t.snap("receipt")
  if "样例数据" in t.text():
    raise t.fail("the session shows the built-in sample data, not the backend's")
  t.quit()


# Every flow but `smoke` runs by default; `smoke` alone runs against a gateway (--gateway).
FLOWS = {
  "main": flow_main,
  "money": flow_money,
  "stock": flow_stock,
  "session": flow_session,
  "gateway": flow_gateway,
  "smoke": flow_smoke,
}
DEFAULT = ["main", "money", "stock", "session", "gateway"]
EXTERNAL = ["smoke"]

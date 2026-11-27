// The cart / checkout screen (docs/design/screens/04-cart.md) at the four design sizes, on
// the fixture transport: storage `coffee.transport` picks the scenario and
// `coffee.fingerprint` gives the connection a key (`fixture:member` + a key = signed in at
// once). `nsh test` runs these headless (texts and behaviour); `nsh test --target tui` runs
// the same steps on the in-memory terminal, where the row checks also run.
//
// Fixture carts are static (an add or a change answers a fixed cart): what a key does is
// shown by the answer it gets (`cart-partly-selected.json`, `none-selected.json`,
// `removed.json`, `added.json`, `cart-coupon.json`, `orders/placed.json`).
import type { App } from './testkit'
import { expect, test } from 'nsh/test'
import { enterAsGuest, logInAsMember, logInOnClock, loginReady, runUntil, screenRows, shows, start, startOnClock, waitForPart } from './testkit'

/** Expects one screen row to hold every one of `parts` (on the terminal; headless draws no cells). */
async function expectRow(app: App, ...parts: string[]) {
  const rows = await screenRows(app)
  if (!rows)
    return
  const found = rows.find(row => parts.every(part => row.includes(part)))
  if (!found)
    throw new Error(`no row holds ${JSON.stringify(parts)}:\n${rows.join('\n')}`)
}

/** A remembered member, on the cart page. */
async function memberCart(cols: number, rows: number) {
  const app = await start(cols, rows, { transport: 'fixture:member', key: true })
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  await app.keyboard.press('2')
  await app.waitFor('text:购物车 · 3 件')
  await app.waitFor('text:结算 3 件')
  // The welcome toast (bottom right, 2.6 s) covers the checkout's last rows until it goes.
  await app.waitUntil(() => !shows(app, '欢迎回来，林晓'), { timeout: 5000 })
  return app
}

/** A guest, on the catalog. */
async function guest(cols: number, rows: number, transport?: string) {
  const app = await start(cols, rows, { transport })
  await enterAsGuest(app, transport === 'fixture:empty'
    ? '商品列表 · 0 款 · 推荐'
    : '商品列表 · 8 款 · 推荐')
  return app
}

/** What every member cart shows, whatever the size. */
function expectMemberCart(app: App) {
  const texts = app.texts()
  for (const text of [
    '购物车 · 3 件',
    '结算 CHECKOUT',
    '慧兰 粉红波旁',
    '「夜班」意式拼配',
    '波奎特 瑰夏',
    '¥118.00',
    '¥167.00',
    '¥141.00',
    '[✓]',
    '全选',
    '更换 →',
    '应用',
    '结算 3 件',
  ])
    expect(texts).toContain(text)
  expect(shows(app, '已选 3 件')).toBe(true)
  expect(shows(app, '已满 ¥199 · 本单包邮')).toBe(true)
  expect(shows(app, '¥426.00')).toBe(true)
}

test('160×50: the cart and the checkout column, as the mock', async () => {
  const app = await memberCart(160, 50)
  expectMemberCart(app)
  const texts = app.texts()
  for (const text of [
    '商品',
    '规格',
    '单价',
    '数量',
    '小计',
    '哥伦比亚 · 中浅烘',
    '250g · 手冲 · 标准袋',
    '¥118',
    '收货地址',
    '林晓',
    '上海市 徐汇区',
    '天平路 88 弄 3 号 602 室',
    '优惠码',
    '商品小计',
    '优惠',
    '运费',
    '包邮',
    '−¥0.00',
    '合计',
    '! 演示订单 · 没有真实扣款，也不会发货',
  ])
    expect(texts).toContain(text)
  expect(shows(app, '138 **** 5521')).toBe(true)
  // The cursor starts on the first line; one row holds the whole line.
  await expectRow(app, '▶ [✓]', '慧兰 粉红波旁', '250g · 手冲 · 标准袋', '¥118', '[ −   1   + ]', '¥118.00', '✕')
  const rows = await screenRows(app)
  if (rows) {
    expect(rows.find(row => row.includes('╭─ 购物车 · 3 件'))).toContain('╭─ 结算 CHECKOUT')
    expect(rows.some(row => row.includes('▐') && row.includes('c 结算 3 件') && row.includes('▌'))).toBe(true)
    expect(rows[rows.length - 1]).toContain('购物车 CART')
  }
})

test('130×40: L narrower; below 139 columns the name keeps 24 cells by dropping the unit price', async () => {
  const app = await memberCart(130, 40)
  expectMemberCart(app)
  expect(app.texts()).not.toContain('单价')
  expect(app.texts()).toContain('哥伦比亚 · 中浅烘 · 250g · 手冲 · 标准袋')
  await expectRow(app, '▶ [✓]', '慧兰 粉红波旁', '[ −   1   + ]', '¥118.00')
  await expectRow(app, '╭─ 购物车 · 3 件', '╭─ 结算 CHECKOUT')
  // The demo notice is never cut: two rows in the 34-cell column, broken after the comma.
  expect(app.texts()).toContain('! 演示订单 · 没有真实扣款，')
  expect(app.texts()).toContain('  也不会发货')
})

test('150×40: L with every column', async () => {
  const app = await memberCart(150, 40)
  expect(app.texts()).toContain('单价')
  expect(app.texts()).toContain('哥伦比亚 · 中浅烘')
  await expectRow(app, '慧兰 粉红波旁', '250g · 手冲 · 标准袋', '¥118', '[ −   1   + ]', '¥118.00')
})

test('100×30: M drops the unit price; the spec goes under the name', async () => {
  const app = await memberCart(100, 30)
  expectMemberCart(app)
  expect(app.texts()).not.toContain('单价')
  // The second row reaches past the narrow name column, so the SKU is never cut.
  expect(app.texts()).toContain('哥伦比亚 · 中浅烘 · 250g · 手冲 · 标准袋')
  await expectRow(app, '哥伦比亚 · 中浅烘 · 250g · 手冲 · 标准袋')
  // The address block: region and street on one row.
  expect(app.texts()).toContain('上海市 徐汇区 天平路 88 弄 3 号 602 室')
  await expectRow(app, '╭─ 购物车 · 3 件', '╭─ 结算 CHECKOUT')
  // Short as it is, the column keeps the demo notice over the button, on two rows.
  expect(app.texts()).toContain('! 演示订单 · 没有真实扣款，')
  expect(app.texts()).toContain('  也不会发货')
})

test('80×24: S stacks the cart over the compact checkout block', async () => {
  const app = await memberCart(80, 24)
  expectMemberCart(app)
  const texts = app.texts()
  expect(texts).toContain('250g · 手冲 · 标准袋 · ¥118')
  expect(texts).toContain('商品小计 ¥426.00 · 优惠 −¥0.00 · 运费 包邮')
  // The compact block keeps the demo notice on the row over 合计.
  expect(texts).toContain('! 演示订单 · 没有真实扣款，也不会发货')
  expect(shows(app, '寄往 林晓 138 **** 5521')).toBe(true)
  expect(texts).not.toContain('单价')
  const rows = await screenRows(app)
  if (rows) {
    expect(rows.length).toBe(24)
    const cart = rows.findIndex(row => row.includes('╭─ 购物车 · 3 件'))
    const checkout = rows.findIndex(row => row.includes('╭─ 结算 CHECKOUT'))
    expect(cart).toBeGreaterThan(0)
    expect(checkout).toBeGreaterThan(cart)
    await expectRow(app, '合计', '¥426.00', 'c 结算 3 件')
    expect(rows[23]).toContain('购物车 CART')
  }
})

test('↑↓ move the cursor, space ticks, a selects all / none, ←→ step the quantity', async () => {
  const app = await memberCart(160, 50)
  await app.keyboard.press('j')
  await expectRow(app, '▶ [✓]', '「夜班」意式拼配')
  await app.keyboard.press('k')
  await expectRow(app, '▶ [✓]', '慧兰 粉红波旁')
  // space unticks the line under the cursor: the server's answer selects 2 of 3.
  await app.keyboard.press('space')
  await waitForPart(app, '已选 2 件')
  expect(app.texts()).toContain('[ ]')
  expect(app.texts()).toContain('结算 2 件')
  await expectRow(app, '▶ [ ]', '慧兰 粉红波旁')
  expect(shows(app, '¥308.00')).toBe(true)
  // a: not all selected → all.
  await app.keyboard.press('a')
  await waitForPart(app, '已选 3 件')
  // a again: all selected → none; the button turns disabled, the amounts go to zero.
  await app.keyboard.press('a')
  await waitForPart(app, '已选 0 件')
  expect(app.texts()).toContain('结算 0 件')
  expect(shows(app, '满 ¥199 包邮')).toBe(true)
  await app.keyboard.press('a')
  await waitForPart(app, '已选 3 件')
  // → and ← ask the server for the line's new quantity (the fixture answers the same cart).
  await app.keyboard.press('right')
  await app.keyboard.press('left')
  await app.keyboard.press('shift+=')
  await app.waitFor('text:购物车 · 3 件')
  expect(shows(app, '操作没有完成')).toBe(false)
})

test('x removes the line with the undo bar; u puts it back', async () => {
  const app = await memberCart(130, 40)
  await app.keyboard.press('x')
  await app.waitFor('text:购物车 · 2 件')
  await app.waitFor('text:已从购物车移除')
  expect(shows(app, '慧兰 粉红波旁 · 按 u 撤销')).toBe(true)
  expect(app.texts()).toContain('已删除 慧兰 粉红波旁 · u 撤销')
  expect(app.texts()).toContain('撤销')
  // The cursor stays where it was: the next line.
  await expectRow(app, '▶ [✓]', '「夜班」意式拼配')
  await app.keyboard.press('u')
  await app.waitFor('text:购物车 · 3 件')
  await app.waitFor('text:已恢复')
  expect(app.texts()).not.toContain('已删除 慧兰 粉红波旁 · u 撤销')
})

test('a valid coupon: ✓ and the server\'s new amounts', async () => {
  const app = await memberCart(160, 50)
  await app.type('label:优惠码', 'tty10')
  expect(app.value('label:优惠码')).toBe('TTY10')
  // In the field: the field's keys.
  expect(app.texts()).toContain('离开输入')
  await app.keyboard.press('enter')
  await app.waitFor('text:✓ 已使用 TTY10 · 全单 9 折')
  const texts = app.texts()
  expect(texts).toContain('−¥10.00')
  expect(texts).toContain('¥12.00')
  expect(texts).toContain('¥100.00')
  expect(texts).toContain('再买 ¥111 即可包邮')
  expect(texts).toContain('购物车 · 1 件')
  // Esc leaves the field; the page's keys work again.
  await app.keyboard.press('escape')
  await app.waitFor('text:勾选')
})

test('an invalid coupon: ✗ under the field, the cart unchanged', async () => {
  const app = await memberCart(80, 24)
  await app.type('label:优惠码', 'nope')
  await app.keyboard.press('enter')
  await app.waitFor('text:✗ 优惠码无效或已过期')
  expect(shows(app, '¥426.00')).toBe(true)
  // Typing again clears the line.
  await app.type('label:优惠码', 'NOPE2')
  await app.waitUntil(() => !app.texts().includes('✗ 优惠码无效或已过期'))
})

test('checkout gate 1: a guest goes to log in, then back to the cart', async () => {
  const app = await guest(160, 50)
  await app.keyboard.press('a')
  await app.waitFor('text:已加入购物车')
  await app.keyboard.press('2')
  await app.waitFor('text:购物车 · 2 件')
  expect(app.texts()).toContain('登录后选择收货地址')
  expect(app.texts()).toContain('再买 ¥3 即可包邮')
  expect(app.texts()).toContain('¥208.00')
  await app.keyboard.press('c')
  await app.waitFor('text:登录 · SIGN IN')
  await app.waitFor('text:结算前请先登录')
  await logInAsMember(app)
  await app.waitFor('text:购物车 · 3 件')
  await app.waitFor('text:收货地址')
  expect(app.texts()).toContain('天平路 88 弄 3 号 602 室')
})

test('checkout gate 2: nothing selected says so and stays', async () => {
  const app = await memberCart(100, 30)
  await app.keyboard.press('a')
  await waitForPart(app, '已选 0 件')
  await app.keyboard.press('c')
  await app.waitFor('text:还没有勾选商品')
  expect(shows(app, '用 space 勾选要结算的豆子')).toBe(true)
  expect(app.texts()).toContain('购物车 · 3 件')
})

test('checkout gate 3 needs an address: a guest\'s empty cart sends nobody anywhere', async () => {
  // An empty guest cart (a member without an address: `fixture:new`, further down).
  const app = await guest(160, 50, 'fixture:empty')
  await app.keyboard.press('2')
  await app.waitFor('text:购物车 · 0 件')
  const texts = app.texts()
  expect(texts).toContain('购物车是空的')
  expect(texts).toContain('去挑豆子')
  expect(texts).toContain('满 ¥199 包邮')
  expect(texts).toContain('结算 0 件')
  expect(texts).toContain('登录后选择收货地址')
  await expectRow(app, '空  杯')
  // `1 去挑豆子`.
  await app.keyboard.press('1')
  await app.waitFor('text:商品列表 · 0 款 · 推荐')
})

test('checkout places the order, then the receipt; ⏎ leaves it for the catalog', async () => {
  const app = await memberCart(160, 50)
  await app.keyboard.press('c')
  // The receipt follows at once (the submitting state: `fixture:slow`, further down).
  await app.waitFor('text:订单回执 RECEIPT')
  const texts = app.texts()
  expect(texts).toContain('订单已提交')
  expect(texts).toContain('订单号 RT2611220417 · 预计 11 月 25 日 送达')
  expect(texts).toContain('实付')
  expect(texts).toContain('寄往 林晓 · 上海市 徐汇区 天平路 88 弄 3 号 602 室')
  expect(texts).toContain('! 演示订单 · 没有真实扣款，也不会发货')
  expect(texts).toContain('继续选购')
  expect(texts).toContain('查看订单')
  expect(shows(app, '慧兰 粉红波旁')).toBe(true)
  expect(shows(app, '250g · 手冲 · 标准袋 × 1')).toBe(true)
  // The toast (it queues behind the welcome one).
  await waitForPart(app, 'RT2611220417 · 下一个烘焙日出炉')
  await expectRow(app, '慧兰 粉红波旁', '┈', '¥118.00')
  // The receipt's footer.
  expect(texts).toContain('切换页面')
  expect(texts).not.toContain('勾选')
  await app.keyboard.press('enter')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  // Leaving the page drops the receipt.
  await app.keyboard.press('2')
  await app.waitFor('text:结算 CHECKOUT')
  expect(app.texts()).not.toContain('订单回执 RECEIPT')
})

test('the receipt at 80×24: Esc back to the beans, 4 to the orders', async () => {
  const app = await memberCart(80, 24)
  await app.keyboard.press('c')
  await app.waitFor('text:订单回执 RECEIPT')
  expect(app.texts()).toContain('! 演示订单 · 没有真实扣款，也不会发货')
  // The order's toast covers the card's buttons at 80 columns until it goes.
  await app.waitUntil(() => !shows(app, '下一个烘焙日出炉'), { timeout: 5000 })
  const rows = await screenRows(app)
  if (rows) {
    expect(rows.some(row => row.includes('╭─ 订单回执 RECEIPT'))).toBe(true)
    expect(rows.some(row => row.includes('! 演示订单 · 没有真实扣款，也不会发货'))).toBe(true)
    expect(rows.some(row => row.includes('⏎ 继续选购') && row.includes('4 查看订单'))).toBe(true)
  }
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  // The placed order left the cart as the server answered (the fixture's is empty).
  await app.keyboard.press('2')
  await app.waitFor('text:结算 CHECKOUT')
  expect(app.texts()).not.toContain('订单回执 RECEIPT')

  const again = await memberCart(80, 24)
  await again.keyboard.press('c')
  await again.waitFor('text:订单回执 RECEIPT')
  await again.keyboard.press('4')
  await again.waitFor('text:个人信息 ACCOUNT')
})

test('the receipt at 130×40 and 100×30', async () => {
  for (const [cols, rows] of [
    [130, 40],
    [100, 30],
  ]) {
    const app = await memberCart(cols, rows)
    await app.keyboard.press('c')
    await app.waitFor('text:订单回执 RECEIPT')
    expect(app.texts()).toContain('实付')
    await expectRow(app, '╭─ 订单回执 RECEIPT')
  }
})

test('a backend that fails: ✗ and ⏎ 重试', async () => {
  const app = await start(100, 30, { transport: 'fixture:error' })
  await enterAsGuest(app, '商品列表 · 加载失败')
  await app.keyboard.press('2')
  await app.waitFor('text:✗ 购物车加载失败')
  expect(app.texts()).toContain('重试')
  await app.keyboard.press('enter')
  await app.waitFor('text:✗ 购物车加载失败')
})

// --- States the fixture scenarios open ----------------------------------------------------

test('checkout gate 3: a member without an address is sent to the address book', async () => {
  // `fixture:new`: 林晓 as a new member, one cart line, no address.
  const app = await start(130, 40, { transport: 'fixture:new' })
  await loginReady(app)
  await logInAsMember(app)
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  await app.keyboard.press('2')
  await app.waitFor('text:结算 CHECKOUT')
  await waitForPart(app, '还没有收货地址，去地址本添加一个')
  await app.keyboard.press('c')
  await app.waitFor('text:请先选择收货地址')
  expect(shows(app, '在地址本里按 ⏎ 指定')).toBe(true)
  await waitForPart(app, '地址本 · 0 条')
  expect(shows(app, '新增收货地址')).toBe(true)
})

test('a problem line says why, and the checkout is refused with the server\'s reason', async () => {
  // `fixture:problem`: 3 Geisha in the cart, 2 left; checkout answers 409 out_of_stock.
  const app = await start(160, 50, { transport: 'fixture:problem' })
  await loginReady(app)
  await logInAsMember(app)
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  await app.keyboard.press('2')
  await app.waitFor('text:结算 CHECKOUT')
  await waitForPart(app, '! 库存不足 · 仅剩 2 份')
  // An error reads by its `!` and bold, not by its colour alone (monochrome, 16 colours).
  expect(app.spanStyle('text:! 库存不足 · 仅剩 2 份').fontWeight).toBe(700)
  await app.keyboard.press('c')
  await app.waitFor('text:下单失败')
  expect(shows(app, '波奎特 瑰夏 库存不足，购物车没有变化')).toBe(true)
  expect(app.texts()).not.toContain('订单回执 RECEIPT')
  // The button is back: the checkout can be tried again.
  await waitForPart(app, '结算')
})

test('submitting the order: the progress block, only ? works, then the receipt', async () => {
  const { app, clock } = await startOnClock(160, 50)
  await logInOnClock(app, clock)
  await app.keyboard.press('2')
  await runUntil(clock, () => shows(app, '结算 3 件'))
  await app.keyboard.press('c')
  await clock.runFor(200)
  // The block (4 steps, acc2) holds the button's place while the order is out (K6).
  expect(shows(app, '校验库存') || shows(app, '锁定烘焙批次') || shows(app, '生成订单')).toBe(true)
  expect(app.texts()).not.toContain('结算 3 件')
  for (const key of ['1', 'q', 'x', 'escape', 'c']) {
    await app.keyboard.press(key)
    expect(shows(app, '结算 CHECKOUT')).toBe(true)
    expect(shows(app, '退出登录 LOGOUT')).toBe(false)
  }
  // The coupon field takes no keys meanwhile.
  await app.keyboard.press('tab')
  await app.keyboard.press('t')
  expect(app.value('label:优惠码')).toBe('')
  await app.keyboard.press('shift+/')
  expect(app.texts()).toContain('快捷键 KEYMAP')
  await app.keyboard.press('escape')
  expect(await runUntil(clock, () => shows(app, '订单回执 RECEIPT'))).toBe(true)
})

test('鼠标追踪: the mouse moving over a row moves the cursor; off, it does not', async () => {
  const app = await memberCart(160, 50)
  await expectRow(app, '▶ [✓]', '慧兰 粉红波旁')
  await app.hover('text:「夜班」意式拼配')
  await expectRow(app, '▶ [✓]', '「夜班」意式拼配')
  // Off (profile `o`): hovering moves nothing.
  await app.keyboard.press('4')
  await app.waitFor('text:个人信息 ACCOUNT')
  await app.keyboard.press('o')
  await app.waitFor('text:鼠标追踪已关闭')
  await app.keyboard.press('2')
  await app.waitFor('text:结算 CHECKOUT')
  await app.hover('text:波奎特 瑰夏')
  await expectRow(app, '▶ [✓]', '「夜班」意式拼配')
})

test('a merge that fails at login: 购物车同步失败, the guest\'s lines not shown as the account\'s', async () => {
  // `fixture:member-error`: the quote and signing in work, every member call answers 500.
  const app = await guest(160, 50, 'fixture:member-error')
  await app.keyboard.press('a')
  await app.waitFor('text:已加入购物车')
  await app.keyboard.press('2')
  await app.waitFor('text:购物车 · 2 件')
  await app.keyboard.press('c')
  await app.waitFor('text:结算前请先登录')
  await logInAsMember(app)
  await app.waitFor('text:✗ 购物车同步失败')
  // The toast queues behind `结算前请先登录`.
  await app.waitUntil(() => shows(app, '购物车同步失败 · 在购物车按 ⏎ 重试'), { timeout: 8000 })
  expect(app.texts()).not.toContain('已合并访客购物车 1 件')
  expect(shows(app, '购物车 · 0 件')).toBe(true)
  expect(shows(app, '耶加雪菲 孔加')).toBe(false)
  // `c` checks out nothing; `⏎` tries the merge again (here it fails again).
  await app.keyboard.press('c')
  expect(app.texts()).not.toContain('请先选择收货地址')
  await app.keyboard.press('enter')
  await app.waitFor('text:✗ 购物车同步失败')
})

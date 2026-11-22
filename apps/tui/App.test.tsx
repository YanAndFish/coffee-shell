// The Rust root tested from TypeScript. `nsh test` runs this file headless (the default
// platform there is a phone, whose app shortcuts never fire, so every test opens the app as
// a Linux desktop); `nsh test --target tui` runs the same steps on the terminal host's
// in-memory terminal, where the viewport is cells: `cells(120, 36)` asks for 120×36 of them
// at nsh's 8×16 points per cell.
//
// No host configures the session here, so the app answers from the committed contract
// fixtures. Storage picks the scenario (`coffee.transport`: `fixture:` a new visitor,
// `fixture:member` a remembered key, `fixture:empty`, `fixture:down`, `fixture:error`) and
// gives the connection an SSH key (`coffee.fingerprint`). These tests prove the core (stores,
// router, commands) and the shell (frame, status bar, overlays, keys); each screen's looks
// are its own file's. Shared helpers (opening the app, the two ways in): `testkit.ts`.
import type { App } from './testkit'
import { expect, test } from 'nsh/test'
import {
  column,
  enterAsGuest,
  hint,
  logInAsMember,
  loginReady,
  screenCells,
  screenRows,
  shows,
  start,
  waitForPart,
} from './testkit'

/** The cart's count badge on the tabs (`购物车▐3▌`), read off the header rows; `undefined` headless. */
async function badge(app: App): Promise<string | null | undefined> {
  const rows = await screenRows(app)
  if (!rows)
    return undefined
  for (const row of rows.slice(1, 6)) {
    const match = row.match(/购物车 ?▐(\d+)▌/)
    if (match)
      return match[1]
  }
  return null
}

/** The badge says `count` (checked where the screen is drawn). */
async function expectBadge(app: App, count: string | null) {
  const shown = await badge(app)
  if (shown !== undefined)
    expect(shown).toBe(count)
}

test('a new connection starts on the login page, with the demo notices', async () => {
  const app = await start(120, 36)
  // The focus starts in 用户名 (K1).
  await loginReady(app)
  const texts = app.texts()
  expect(texts).toContain('! 演示环境 · 不是真实商店')
  expect(texts).toContain('! 演示环境 · 请勿使用真实密码')
  expect(texts).toContain('○ 未提供公钥')
  expect(shows(app, '登录 LOGIN')).toBe(true)
  // The compact card (36 rows at 120 columns): 记住我 with its note on one row.
  expect(texts).toContain('[ ]')
  expect(texts).toContain('记住我 30 天')
  expect(texts).toContain(' · 当前连接没有 SSH 公钥')
  // The login page has no header (no tabs, no account).
  expect(app.shows('label:主导航')).toBe(false)
  // The login page takes none of the app pages' keys, with or without the focus.
  await app.keyboard.press('escape')
  await app.waitUntil(() => !app.texts().includes('离开输入'))
  await app.keyboard.press('2')
  expect(app.texts()).toContain('登录 · SIGN IN')
  expect(app.shows('label:主导航')).toBe(false)
})

test('guest browse → add to cart → gated checkout → login → the merged cart', async () => {
  const app = await start(120, 36)
  await enterAsGuest(app)
  expect(app.texts()).toContain('以访客身份浏览')

  // Quick add: the toast, the tab's badge (the server's quote counts 2 units) and the
  // status bar's total.
  await app.keyboard.press('a')
  await app.waitFor('text:已加入购物车')
  await waitForPart(app, '¥208.00')
  await expectBadge(app, '2')
  expect(shows(app, '耶加雪菲 孔加 · 250g · 整豆 × 1')).toBe(true)

  await app.keyboard.press('2')
  await app.waitFor('text:购物车 · 2 件')
  expect(app.texts()).toContain('登录后选择收货地址')
  expect(app.texts()).toContain('¥208.00')

  // Checkout as a guest: the login gate.
  await app.keyboard.press('c')
  await app.waitFor('text:登录 · SIGN IN')
  // The toast queues behind `已加入购物车`, which then gives way within 0.8 s.
  await app.waitFor('text:结算前请先登录')

  await logInAsMember(app)
  // Back to the cart (K7), merged into the member's.
  await app.waitFor('text:购物车 · 3 件')
  await app.waitFor('text:欢迎回来，林晓')
  expect(shows(app, '已合并访客购物车 1 件')).toBe(true)
  expect(app.texts()).toContain('林晓')
  await expectBadge(app, '3')
  // The page's keys work again after the login form's fields went away.
  expect(app.texts()).toContain('结算')

  // Checkout as a member: the receipt.
  await app.keyboard.press('c')
  await app.waitFor('text:订单回执 RECEIPT')
  expect(shows(app, '订单号 RT2611220417 · 预计 11 月 25 日 送达')).toBe(true)
  expect(app.texts()).toContain('! 演示订单 · 没有真实扣款，也不会发货')
  expect(app.texts()).toContain('继续选购')
  await app.keyboard.press('enter')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
})

test('letters typed in a field stay in the field; Esc leaves it', async () => {
  const app = await start(120, 36)
  await loginReady(app)
  // The focus is in 用户名 from the start: letters type there.
  await app.keyboard.press('g')
  await app.keyboard.press('g')
  expect(app.value('label:用户名')).toBe('gg')
  expect(app.texts()).toContain('登录 · SIGN IN')
  // In the field: the field's hints, not the page's (the card's button stays: one press
  // target named `g 访客浏览`, two once the status bar offers it too).
  expect(app.texts()).toContain('离开输入')
  expect(app.count('label:g 访客浏览')).toBe(1)
  await app.keyboard.press('escape')
  await app.waitUntil(() => app.count('label:g 访客浏览') === 2)
  await app.keyboard.press('g')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
})

test('a wrong password shows the error and clears the password', async () => {
  const app = await start(120, 36)
  await loginReady(app)
  await logInAsMember(app, 'wrongpass')
  await app.waitFor('text:✗ 用户名或密码不正确')
  expect(app.value('label:密码')).toBe('')
  expect(shows(app, '登录 LOGIN')).toBe(true)
})

test('a remembered key resumes the session on connect', async () => {
  const app = await start(120, 36, { transport: 'fixture:member', key: true })
  await app.waitFor('text:欢迎回来，林晓')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  const texts = app.texts()
  expect(texts).toContain('林晓')
  expect(texts).toContain('● 公钥已识别')
  await expectBadge(app, '3')
  expect(shows(app, '已通过 SSH 公钥恢复登录')).toBe(true)
})

test('a key nobody remembered stays on the login page, 记住我 checked', async () => {
  const app = await start(120, 36, { key: true })
  await loginReady(app)
  expect(app.texts()).toContain('[✓]')
  expect(app.texts()).toContain('记住我 30 天')
  expect(app.texts()).toContain(' · 绑定当前 SSH 公钥')
  expect(app.texts()).toContain('● 公钥已识别')
})

test('q asks before logging out; Esc cancels, q q confirms', async () => {
  const app = await start(120, 36, { transport: 'fixture:member', key: true })
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  await app.keyboard.press('q')
  await app.waitFor('text:退出登录 LOGOUT')
  expect(shows(app, '确认 CONFIRM')).toBe(true)
  expect(app.texts()).toContain('同时解除当前公钥的免密登录。')
  expect(app.texts()).toContain('确认退出')
  // ?, 1–4 and the page's keys do nothing while it is open (K24).
  await app.keyboard.press('shift+/')
  await app.keyboard.press('2')
  expect(app.texts()).toContain('退出登录 LOGOUT')
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  expect(app.texts()).toContain('林晓')

  await app.keyboard.press('q')
  await app.waitFor('text:退出登录 LOGOUT')
  await app.keyboard.press('q')
  await app.waitFor('text:已退出登录')
  await app.waitFor('text:登录 · SIGN IN')
  expect(shows(app, '登录 LOGIN')).toBe(true)
})

test('a guest\'s q does nothing', async () => {
  const app = await start(120, 36)
  await enterAsGuest(app)
  await app.keyboard.press('q')
  expect(app.texts()).not.toContain('退出登录 LOGOUT')
})

test('tabs 1–4, the detail page and back', async () => {
  const app = await start(120, 36)
  await enterAsGuest(app)
  await app.keyboard.press('j')
  await app.keyboard.press('enter')
  await waitForPart(app, '选购 / 哥伦比亚 / 慧兰 粉红波旁')
  expect(shows(app, '详情 DETAIL')).toBe(true)
  expect(app.texts()).toContain('规格行')
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')

  await app.keyboard.press('2')
  await app.waitFor('text:购物车 · 0 件')
  expect(shows(app, '购物车是空的')).toBe(true)
  await app.keyboard.press('3')
  await app.waitFor('text:地址本 ADDRESS')
  expect(app.texts()).toContain('你正在以访客身份浏览')
  expect(app.texts()).toContain('去登录')
  await app.keyboard.press('4')
  await app.waitFor('text:我的 PROFILE')
  expect(app.texts()).toContain('登录后可以查看订单、管理地址本，并生成你的口味画像。')
  await app.keyboard.press('m')
  await app.waitFor('text:界面动画已关闭')
  await app.keyboard.press('1')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
})

test('/ from another tab goes to the list with the focus in the search box', async () => {
  const app = await start(120, 36)
  await enterAsGuest(app)
  await app.keyboard.press('2')
  await app.waitFor('text:购物车 · 0 件')
  await app.keyboard.press('/')
  await app.waitFor('text:离开搜索')
  await app.type('label:搜索豆子、产区或风味…', '茉莉')
  await app.waitFor('text:商品列表 · 2 款 · 推荐')
  // Esc leaves the box (the text stays); the list's keys work again.
  await app.keyboard.press('escape')
  await app.waitFor('text:移动')
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
})

test('a member\'s pages show their stores', async () => {
  const app = await start(120, 36, { transport: 'fixture:member', key: true })
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  await app.keyboard.press('3')
  await app.waitFor('text:地址本 · 3 条')
  expect(shows(app, '当前寄往：林晓')).toBe(true)
  await app.keyboard.press('n')
  await app.waitFor('text:新增地址 NEW')
  expect(shows(app, '编辑 INSERT')).toBe(true)
  await app.keyboard.press('escape')
  await app.waitFor('text:地址本 · 3 条')
  await app.keyboard.press('4')
  await app.waitFor('text:个人信息 ACCOUNT')
  await waitForPart(app, 'RT2611220417')
  expect(shows(app, '熟豆会员 Lv.3')).toBe(true)
  expect(shows(app, '1,280 / 2,000 豆')).toBe(true)
})

test('? opens the help from the table, Esc closes it', async () => {
  const app = await start(120, 36)
  await enterAsGuest(app)
  await app.keyboard.press('shift+/')
  await app.waitFor('text:快捷键 KEYMAP')
  expect(shows(app, '帮助 HELP')).toBe(true)
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
})

test('a backend that is down shows the error and ⏎ retries', async () => {
  const app = await start(100, 30, { transport: 'fixture:down' })
  await enterAsGuest(app, '商品列表 · 加载失败')
  expect(app.texts()).toContain('✗ 商品加载失败')
  expect(app.texts()).toContain('网络错误：fixture:down · connection refused')
  // The footer offers the retry; moving the cursor is not offered.
  expect(app.texts()).not.toContain('移动')
  await app.keyboard.press('enter')
  await app.waitFor('text:商品列表 · 加载失败')
})

test('an empty catalog says so', async () => {
  const app = await start(100, 30, { transport: 'fixture:empty' })
  await enterAsGuest(app, '商品列表 · 0 款 · 推荐')
  expect(app.texts()).toContain('∅')
  expect(app.texts()).toContain('没有匹配的豆子')
  expect(app.texts()).toContain('清除筛选')
})

// --- The shell: frame, status bar, overlays, keys, too-small page ---------------------

test('the status bar\'s mode block is a pill: ▐ 选购 BROWSE ▌, ink on amber', async () => {
  const app = await start(100, 30)
  await enterAsGuest(app)
  expect(shows(app, '选购 BROWSE')).toBe(true)
  const cells = await screenCells(app)
  if (!cells)
    return
  // nsh keeps an opaque accent fill as its exact colour and leaves the label in the
  // terminal's default text colour; coffee-host pins default text on that fill to `ink`.
  const bottom = cells[cells.length - 1]
  const block = bottom.slice(0, 15)
  expect(block.map(cell => cell.symbol).join('')).toBe('▐ 选 购  BROWSE ▌')
  for (const cell of block.slice(1, 14))
    expect(cell.bg).toBe('#f0b45a')
  expect(bottom[2].modifiers).toContain('bold')
})

test('the system row: the size, the demo notice, the key and the clock', async () => {
  const app = await start(160, 50, { transport: 'fixture:member', key: true })
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  const texts = app.texts()
  expect(texts).toContain('roast@tty: ~/shop — 160×50')
  expect(texts).toContain('! 演示环境 · 不是真实商店')
  expect(texts).toContain('● 公钥已识别')
  expect(texts.some(text => /^ · \d\d:\d\d$/.test(text))).toBe(true)
  const rows = await screenRows(app)
  if (rows)
    expect(rows[0]).toMatch(/^ roast@tty: ~\/shop — 160×50 +! 演示环境 · 不是真实商店 +● 公钥已识别 · \d\d:\d\d $/)
  const cells = await screenCells(app)
  // The notice centred, the odd half cell rounded down: the mock's column 67.
  if (cells)
    expect(column(cells[0], '!')).toBe(67)

  // Under 100 columns the path goes; without a key, `○ 未提供公钥`.
  const small = await start(80, 24)
  await small.waitFor('text:登录 · SIGN IN')
  expect(small.texts()).toContain('○ 未提供公钥')
  expect(shows(small, 'roast@tty')).toBe(false)
  const smallRows = await screenRows(small)
  if (smallRows)
    expect(smallRows[0]).toMatch(/! 演示环境 · 不是真实商店 +○ 未提供公钥 · \d\d:\d\d $/)
})

test('the header: tabs with the cart badge, the rule under the current tab, the account', async () => {
  const app = await start(160, 50, { transport: 'fixture:member', key: true })
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  expect(app.shows('label:主导航')).toBe(true)
  expect(app.texts()).toContain('林晓 Lv.3')
  expect(app.shows('label:搜索商品')).toBe(true)
  await expectBadge(app, '3')
  let rows = await screenRows(app)
  if (rows) {
    // XL: the 4-row header (rows 1–4), wide tabs from column 23, `━` under the current one.
    expect(rows[2]).toContain('◆ROAST/TTY')
    expect(rows[2]).toContain('搜索豆子、产区或风味…')
    expect(rows[1]).toContain('╭')
    expect(rows[4].indexOf('━')).toBe(23)
    expect(rows[4].lastIndexOf('━')).toBe(36)
  }
  await app.keyboard.press('2')
  await app.waitUntil(() => shows(app, '购物车 CART'))
  rows = await screenRows(app)
  if (rows)
    expect(rows[4].indexOf('━')).toBe(37)
  await app.keyboard.press('4')
  await app.waitUntil(() => shows(app, '我的 PROFILE'))
  const cells = await screenCells(app)
  if (cells) {
    // Each wide tab is 14 cells with its content centred, nothing between two tabs: tab 4
    // (65–78) holds `‹4› 我的` at 68–75, right over its underline.
    expect(column(cells[2], '4')).toBe(69)
    expect(column(cells[4], '━')).toBe(65)
    expect(column(cells[4], '━', true)).toBe(78)
  }

  // L: the wordmark two cells before the tabs, the search entry 40 cells at 130 columns.
  const l = await start(130, 40, { transport: 'fixture:member', key: true })
  await l.waitFor('text:商品列表 · 8 款 · 推荐')
  const lCells = await screenCells(l)
  if (lCells) {
    expect(column(lCells[4], '━')).toBe(13)
    expect(column(lCells[1], '╮') - column(lCells[1], '╭') + 1).toBe(40)
  }

  // S: two rows, tight tabs, `‹/› 搜索`, the avatar alone.
  const small = await start(80, 24, { transport: 'fixture:member', key: true })
  await small.waitFor('text:商品列表 · 8 款 · 推荐')
  expect(small.texts()).not.toContain('林晓 Lv.3')
  const smallRows = await screenRows(small)
  if (smallRows) {
    expect(smallRows[1]).toMatch(/^ ◆ROAST\/TTY {2}/)
    expect(smallRows[1]).toContain('购物车▐3▌')
    expect(smallRows[1]).toContain('搜索')
    expect(smallRows[2].indexOf('━')).toBeGreaterThan(12)
  }
  // S on the cart: the underline takes the cart tab with its badge whole (04-cart.md's S
  // mock); the search entry sits four cells before the avatar.
  await small.keyboard.press('2')
  await small.waitUntil(() => shows(small, '购物车 CART'))
  const smallCells = await screenCells(small)
  if (smallCells) {
    expect(column(smallCells[2], '━')).toBe(23)
    expect(column(smallCells[2], '━', true)).toBe(35)
    expect(column(smallCells[1], '搜')).toBe(66)
  }
})

test('status-bar hints per page: 7 at 160 columns, 4 and help at 80', async () => {
  const wide = await start(160, 50, { transport: 'fixture:member', key: true })
  await wide.waitFor('text:商品列表 · 8 款 · 推荐')
  for (const label of [
    '↑↓ 移动',
    '⏎ 详情',
    'a 加入',
    'f 产区',
    'r 烘焙',
    's 排序',
    '/ 搜索',
    '? 帮助',
  ])
    expect(hint(wide, label)).toBe(true)
  expect(shows(wide, '鼠标')).toBe(true)
  await wide.keyboard.press('2')
  await wide.waitUntil(() => shows(wide, '购物车 CART'))
  for (const label of [
    '↑↓ 移动',
    'space 勾选',
    '←→ 数量',
    'x 删除',
    'c 结算',
  ])
    expect(hint(wide, label)).toBe(true)
  expect(hint(wide, 'r 烘焙')).toBe(false)
  let rows = await screenRows(wide)
  if (rows)
    expect(rows[49]).toMatch(/● 鼠标 {3}购物车 ¥[\d,]+\.\d\d +\? +帮助 $/)

  const small = await start(80, 24, { transport: 'fixture:member', key: true })
  await small.waitFor('text:商品列表 · 8 款 · 推荐')
  for (const label of [
    '↑↓ 移动',
    '⏎ 详情',
    'a 加入',
    'f 筛选',
    '? 帮助',
  ])
    expect(hint(small, label)).toBe(true)
  for (const label of ['r 烘焙', 's 排序', '/ 搜索'])
    expect(hint(small, label)).toBe(false)
  rows = await screenRows(small)
  if (rows) {
    // The demo notice moved to the system row: the bar holds four hints, help and the total.
    expect(rows[23]).not.toContain('演示环境')
    expect(rows[23]).toMatch(/^▐ 选购 BROWSE ▌ .*移动 .*详情 .*加入 .*筛选 .*帮助 +¥[\d,]+\.\d\d $/)
  }
  await small.keyboard.press('4')
  await small.waitUntil(() => shows(small, '我的 PROFILE'))
  for (const label of [
    'm 动画',
    'o 鼠标',
    'q 退出',
    '1–4 切换',
    '? 帮助',
  ])
    expect(hint(small, label)).toBe(true)
})

test('? opens the help at 160×50: three groups from the command table; Esc, q, ? close it', async () => {
  const app = await start(160, 50)
  await enterAsGuest(app)
  await app.keyboard.press('shift+/')
  await app.waitFor('text:快捷键 KEYMAP')
  expect(shows(app, '帮助 HELP')).toBe(true)
  for (const part of [
    '── 全局',
    '── 选购 · 详情',
    '── 购物车 · 地址 · 设置',
    '退出登录（先确认）',
    '断开连接',
    '动画 / 鼠标追踪 开关',
    '底栏的快捷键提示可以直接点击',
  ])
    expect(shows(app, part)).toBe(true)
  expect(hint(app, 'Esc 关闭')).toBe(true)
  expect(hint(app, '↑↓ 移动')).toBe(false)
  const rows = await screenRows(app)
  if (rows) {
    expect(rows[0]).toContain('! 演示环境 · 不是真实商店')
    const top = rows.findIndex(row => row.includes('╔═ 快捷键 KEYMAP'))
    expect(top).toBe(13)
    const cells = await screenCells(app)
    expect(cells && column(cells[top], '╔')).toBe(24)
    expect(rows[top + 23]).toContain('╚')
    // The first entries of the three columns (33 cells, two apart).
    const first = rows[top + 5]
    expect(first.indexOf('1 – 4')).toBeLessThan(first.indexOf('切换页面 选购…我的'))
    expect(first.indexOf('选购…我的')).toBeLessThan(first.indexOf('↑↓ j k'))
    expect(first.indexOf('移动选择')).toBeLessThan(first.indexOf('space'))
    expect(first).toContain('勾选商品 / 设为默认')
  }
  // The page's keys do nothing under it; Esc, q and ? each close it.
  await app.keyboard.press('2')
  expect(shows(app, '快捷键 KEYMAP')).toBe(true)
  for (const key of ['escape', 'q', 'shift+/']) {
    await app.keyboard.press(key)
    await app.waitUntil(() => !shows(app, '快捷键 KEYMAP'))
    expect(shows(app, '选购 BROWSE')).toBe(true)
    await app.keyboard.press('shift+/')
    await app.waitFor('text:快捷键 KEYMAP')
  }
})

test('80×24: the help fits in two columns between the system row and the status bar', async () => {
  const app = await start(80, 24)
  await enterAsGuest(app)
  await app.keyboard.press('shift+/')
  await app.waitFor('text:快捷键 KEYMAP')
  for (const part of [
    '切换页面 选购…我的',
    '断开连接',
    '移动选择',
    '立即购买',
    '勾选商品 / 设为默认',
    '动画 / 鼠标追踪 开关',
    '底栏的快捷键提示可以直接点击',
  ])
    expect(shows(app, part)).toBe(true)
  const rows = await screenRows(app)
  if (rows) {
    expect(rows[0]).toContain('! 演示环境 · 不是真实商店')
    expect(rows[1]).toContain('╔═ 快捷键 KEYMAP')
    expect(rows[22]).toContain('╚')
    expect(rows[23]).toContain('HELP')
    const screen = rows.join('\n')
    for (const part of ['Ctrl+C ×2', 'm o', 'n e', 'Esc 关闭 ]'])
      expect(screen).toContain(part)
  }
  await app.keyboard.press('escape')
  await app.waitUntil(() => !shows(app, '快捷键 KEYMAP'))
})

test('the logout confirmation: ⏎ confirms, Esc cancels, Tab moves the focus ring', async () => {
  const app = await start(160, 50, { transport: 'fixture:member', key: true })
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  await app.keyboard.press('q')
  await app.waitFor('text:确定退出登录吗？')
  expect(shows(app, '确认 CONFIRM')).toBe(true)
  expect(app.texts()).toContain('同时解除当前公钥的免密登录。')
  expect(app.texts()).toContain('再按 q 也可以确认')
  for (const label of ['q 确认退出', '⏎ 确认', 'Tab 切换', 'Esc 取消'])
    expect(hint(app, label)).toBe(true)
  const buttons = async () => (await screenRows(app))?.find(row => row.includes('Esc 取消 ]'))
  // It opens on 退出登录; Tab moves the focus to 取消, where ⏎ cancels.
  let row = await buttons()
  if (row !== undefined)
    expect(row).toMatch(/Esc 取消 \] ›▐ ⏎ 退出登录 ▌/)
  await app.keyboard.press('tab')
  row = await buttons()
  if (row !== undefined)
    expect(row).toMatch(/›\[ Esc 取消 \] {2}▐ ⏎ 退出登录 ▌/)
  await app.keyboard.press('enter')
  await app.waitUntil(() => !shows(app, '确定退出登录吗？'))
  expect(app.texts()).toContain('林晓 Lv.3')

  // Esc cancels.
  await app.keyboard.press('q')
  await app.waitFor('text:确定退出登录吗？')
  await app.keyboard.press('escape')
  await app.waitUntil(() => !shows(app, '确定退出登录吗？'))
  expect(shows(app, '选购 BROWSE')).toBe(true)

  // It opens on 退出登录 again: ⏎ confirms.
  await app.keyboard.press('q')
  await app.waitFor('text:确定退出登录吗？')
  await app.keyboard.press('enter')
  await app.waitFor('text:已退出登录')
  await app.waitFor('text:登录 · SIGN IN')
  expect(shows(app, '登录 LOGIN')).toBe(true)
})

test('toasts: one at a time at the bottom right, the next one within 0.8 s', async () => {
  const app = await start(160, 50, { transport: 'fixture:member', key: true })
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  await app.waitFor('text:欢迎回来，林晓')
  const cells = await screenCells(app)
  if (cells) {
    // 45×4, its right edge at column 157, its bottom row above the blank row over the bar.
    expect(column(cells[47], '╯', true)).toBe(157)
    expect(column(cells[44], '╮', true)).toBe(157)
    expect(column(cells[44], '╭', true)).toBe(113)
  }
  await app.waitUntil(() => !shows(app, '欢迎回来，林晓'))
  await app.keyboard.press('4')
  await app.waitUntil(() => shows(app, '我的 PROFILE'))
  await app.keyboard.press('m')
  await app.keyboard.press('o')
  await waitForPart(app, '界面动画已')
  expect(shows(app, '鼠标追踪已')).toBe(false)
  await waitForPart(app, '鼠标追踪已')
  expect(shows(app, '界面动画已')).toBe(false)
})

test('Esc: the overlay first, then the field, then the page, then nothing', async () => {
  const app = await start(120, 36)
  await enterAsGuest(app)
  await app.keyboard.press('/')
  await app.waitFor('text:离开搜索')
  // Letters type while the field has the focus: q does not ask to log out.
  await app.type('label:搜索豆子、产区或风味…', '茉莉')
  await app.waitFor('text:商品列表 · 2 款 · 推荐')
  await app.keyboard.press('escape')
  await app.waitUntil(() => hint(app, '↑↓ 移动'))
  expect(app.texts()).toContain('商品列表 · 2 款 · 推荐')
  await app.keyboard.press('shift+/')
  await app.waitFor('text:快捷键 KEYMAP')
  await app.keyboard.press('escape')
  await app.waitUntil(() => !shows(app, '快捷键 KEYMAP'))
  expect(app.texts()).toContain('商品列表 · 2 款 · 推荐')
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  await app.keyboard.press('escape')
  expect(app.texts()).toContain('商品列表 · 8 款 · 推荐')
  expect(shows(app, '选购 BROWSE')).toBe(true)
})

test('a press on a status-bar hint or a tab runs its command', async () => {
  const app = await start(160, 50)
  await enterAsGuest(app)
  // The toast showing (`以访客身份浏览`) takes no presses.
  expect(shows(app, '以访客身份浏览')).toBe(true)
  await app.press('label:a 加入')
  await app.waitFor('text:已加入购物车')
  await app.press('label:? 帮助')
  await app.waitFor('text:快捷键 KEYMAP')
  await app.press('label:Esc 关闭 >> nth=1')
  await app.waitUntil(() => !shows(app, '快捷键 KEYMAP'))
  await app.press('text:地址本')
  await app.waitUntil(() => shows(app, '地址本 ADDRESS'))
  await app.press('label:ROAST/TTY 首页')
  await app.waitUntil(() => shows(app, '选购 BROWSE'))
  await app.press('label:搜索商品')
  await app.waitFor('text:离开搜索')
})

test('80×24: the frame of the S layout', async () => {
  const app = await start(80, 24)
  await enterAsGuest(app)
  const rows = await screenRows(app)
  if (rows) {
    expect(rows.length).toBe(24)
    expect(rows[0]).toContain('! 演示环境 · 不是真实商店')
    expect(rows[1]).toContain('◆ROAST/TTY')
    expect(rows[2]).toMatch(/^─+━+─+$/)
    expect(rows[23].startsWith('▐ 选购 BROWSE ▌')).toBe(true)
  }
  // `f` at S opens the filter dialog.
  await app.keyboard.press('f')
  await app.waitUntil(() => shows(app, '筛选 FILTER'))
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
})

test('60×20: a terminal opened too small shows only the notice with its size', async () => {
  const app = await start(60, 20)
  const texts = app.texts()
  expect(texts).toContain('终端太小')
  expect(texts).toContain('当前 60×20 · 需要至少 80×24')
  expect(texts).toContain('! 演示环境 · 不是真实商店')
  expect(texts).toContain('放大终端窗口后，会回到刚才的画面。')
  expect(texts).toContain('Ctrl+C 连按两次断开连接')
  expect(texts).not.toContain('登录 · SIGN IN')
  const rows = await screenRows(app)
  if (rows) {
    expect(rows[0]).toMatch(/^ ! 演示环境 · 不是真实商店/)
    expect(rows.join('\n')).toContain('◆ROAST/TTY')
  }
})

test('too small, then large again: the page comes back as it was', async () => {
  const app = await start(120, 36)
  await enterAsGuest(app)
  await app.keyboard.press('/')
  await app.waitFor('text:离开搜索')
  await app.type('label:搜索豆子、产区或风味…', '茉莉')
  await app.waitFor('text:商品列表 · 2 款 · 推荐')
  await app.keyboard.press('escape')
  await app.waitUntil(() => hint(app, '↑↓ 移动'))
  try {
    await app.setViewport(60 * 8, 20 * 16)
  }
  catch {
    return // `--target tui` cannot resize an open app.
  }
  await app.waitFor('text:终端太小')
  expect(app.texts()).toContain('当前 60×20 · 需要至少 80×24')
  // Keys do nothing meanwhile.
  await app.keyboard.press('2')
  await app.keyboard.press('escape')
  await app.setViewport(120 * 8, 36 * 16)
  await app.waitUntil(() => !app.texts().includes('终端太小'))
  expect(shows(app, '选购 BROWSE')).toBe(true)
  expect(app.texts()).toContain('商品列表 · 2 款 · 推荐')
})

test('a session that expires: back to the login page, 登录已失效', async () => {
  // `fixture:expired`: the key resumes at once, then every member call answers 401.
  const app = await start(130, 40, { transport: 'fixture:expired', key: true })
  await app.waitFor('text:登录 · SIGN IN')
  await app.waitFor('text:登录已失效')
  expect(shows(app, '请重新登录，购物车已保留')).toBe(true)
  expect(app.shows('label:主导航')).toBe(false)
  expect(shows(app, '林晓 Lv.3')).toBe(false)
})

test('a dialog\'s buttons take presses: the frames around them take none', async () => {
  const app = await start(160, 50, { transport: 'fixture:member', key: true })
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  // `Esc 取消` twice: the status bar's hint and the dialog's button; both cancel.
  for (const nth of [0, 1]) {
    await app.keyboard.press('q')
    await app.waitFor('text:确定退出登录吗？')
    await app.press(`label:Esc 取消 >> nth=${nth}`)
    await app.waitUntil(() => !shows(app, '确定退出登录吗？'))
    expect(app.texts()).toContain('林晓 Lv.3')
  }
  await app.keyboard.press('3')
  await app.waitFor('text:地址本 · 3 条')
  await app.keyboard.press('n')
  await app.waitFor('text:新增地址 NEW')
  await app.press('label:Esc 取消 >> nth=1')
  await app.waitUntil(() => !shows(app, '新增地址 NEW'))
  await app.keyboard.press('q')
  await app.waitFor('text:确定退出登录吗？')
  await app.press('label:⏎ 退出登录')
  await app.waitFor('text:已退出登录')
})

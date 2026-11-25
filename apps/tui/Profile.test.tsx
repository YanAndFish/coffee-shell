// The profile page (`docs/design/screens/06-profile.md`) at the four design sizes and in its
// states, against the fixture transport: storage `coffee.transport` picks the scenario and
// `coffee.fingerprint` gives the connection an SSH key. `nsh test` runs these headless
// (texts and behaviour); `nsh test --target tui` runs them on the terminal host's in-memory
// terminal, where the cell checks run too.
import type { App } from './testkit'
import { expect, open, test } from 'nsh/test'
import { cells, enterAsGuest, hint, isWide, KEY, logInAsMember, loginReady, screenRows, shows, start, waitForPart } from './testkit'

/** Cells of `line` up to (not including) the first `part` in it; -1 when absent. */
function cellOf(line: string, part: string) {
  const at = line.indexOf(part)
  if (at < 0)
    return -1
  let cells = 0
  for (const symbol of line.slice(0, at)) {
    cells += isWide(symbol)
      ? 2
      : 1
  }
  return cells
}

/** A remembered member (`林晓`, four orders), on the profile. */
async function memberProfile(cols: number, rows: number) {
  const app = await start(cols, rows, { transport: 'fixture:member', key: true })
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  await app.keyboard.press('4')
  await app.waitFor('text:个人信息 ACCOUNT')
  await waitForPart(app, 'RT2611220417')
  return app
}

/** Logs in from the login page with the form, then opens the profile. */
async function loginProfile(app: App) {
  await loginReady(app)
  await logInAsMember(app)
  await waitForPart(app, '商品列表 · ')
  await app.keyboard.press('4')
  await app.waitFor('text:个人信息 ACCOUNT')
}

// --- The member's profile at the four sizes ------------------------------------------

test('160×50: the design layout, every block from the server', async () => {
  const app = await memberProfile(160, 50)
  const texts = app.texts()
  for (const title of [
    '个人信息 ACCOUNT',
    '设置 SETTINGS',
    '我的口味画像 TASTE',
    '最近订单 ORDERS',
  ])
    expect(texts).toContain(title)
  for (const part of [
    '林晓',
    '● 已记住当前公钥 · 至 12-22',
    '熟豆会员 Lv.3',
    '1,280 / 2,000 豆',
    '再积 720 豆升级 Lv.4',
    '订单',
    '地址',
    '优惠券',
    '蒸汽、加载指示与进度条',
    '悬停即移动选择高亮',
    '按画像推荐',
    '涅里 AA 双洗 →',
    '保山 小粒 日晒 →',
    '订单号',
    '◔ 烘焙中',
    '◑ 运输中',
    '● 已签收',
    '¥426.00',
    '11-22',
  ])
    expect(shows(app, part)).toBe(true)
  expect(shows(app, '干净甜感 · 悠长余韵 · 花香')).toBe(true)
  expect(shows(app, '浅烘 / 中浅烘 · 手冲研磨 · 250g')).toBe(true)
  expect(shows(app, '深烘 · 烟熏与草本调')).toBe(true)
  // Member footer: m o q 1–4 /.
  for (const hint of ['动画开关', '鼠标追踪', '退出登录', '切换页面', '搜索'])
    expect(texts).toContain(hint)
  expect(shows(app, '我的 PROFILE')).toBe(true)

  const rows = await screenRows(app)
  if (rows) {
    expect(rows.length).toBe(50)
    // The design's columns: account 2–48, taste 51–157.
    const account = rows.findIndex(row => row.includes('个人信息 ACCOUNT'))
    // (The rows above it are the shell's header.)
    expect(cellOf(rows[account], '╭─ 个人信息')).toBe(2)
    expect(cellOf(rows[account], '╭─ 我的口味画像')).toBe(51)
    expect(rows[account].trimEnd().length).toBeGreaterThan(0)
    // The avatar block and the name.
    const avatar = rows.findIndex(row => row.includes('┃ 林 ┃'))
    expect(avatar).toBe(account + 3)
    expect(cellOf(rows[avatar], '┃ 林 ┃')).toBe(5)
    expect(rows[avatar - 1]).toContain('林晓')
    expect(rows[avatar]).toContain('● 已记住当前公钥 · 至 12-22')
    // The radar block (25 wide from column 55): its top axis and the taste text at 83.
    const prefer = rows.findIndex(row => row.includes('偏好'))
    expect(cellOf(rows[prefer], '偏好')).toBe(83)
    expect(rows[prefer - 1]).toContain('酸度')
    // The member meter: 26 of 41 cells (64 %).
    const meter = rows.find(row => row.includes('▉') && row.includes('░') && cellOf(row, '▉') === 5)
    expect(meter).toBeDefined()
    expect([...meter!].filter(c => c === '▉').length).toBe(26)
    // The order table: head, then the first order with its columns.
    const head = rows.findIndex(row => row.includes('订单号'))
    expect(cellOf(rows[head], '订单号')).toBe(54)
    expect(cellOf(rows[head], '日期')).toBe(73)
    expect(cellOf(rows[head], '商品')).toBe(81)
    const first = rows.findIndex(row => row.includes('RT2611220417'))
    expect(cellOf(rows[first], 'RT2611220417')).toBe(54)
    expect(cellOf(rows[first], '11-22')).toBe(73)
    expect(cellOf(rows[first], '¥426.00') + 7).toBe(144)
    expect(cellOf(rows[first], '◔ 烘焙中') + 8).toBe(155)
    expect(rows[first + 1]).toContain('┈┈┈┈')
    // The logout button fills the left column.
    const logout = rows.findIndex(row => row.includes('q 退出登录') && row.includes('['))
    expect(cellOf(rows[logout], '[')).toBe(2)
    expect(cellOf(rows[logout], ']')).toBe(48)
  }
})

test('130×40: two narrower columns, the radar kept', async () => {
  const app = await memberProfile(130, 40)
  expect(shows(app, '按画像推荐')).toBe(true)
  expect(shows(app, '● 已记住当前公钥 · 至 12-22')).toBe(true)
  const rows = await screenRows(app)
  if (rows) {
    const account = rows.findIndex(row => row.includes('个人信息 ACCOUNT'))
    expect(cellOf(rows[account], '╭─ 个人信息')).toBe(1)
    expect(cellOf(rows[account], '╭─ 我的口味画像')).toBe(42)
    expect(rows.some(row => row.includes('┃ 林 ┃'))).toBe(true)
    // The radar's braille, not the meters.
    expect(rows.some(row => [...row].some(c => c.codePointAt(0)! > 0x2800 && c.codePointAt(0)! <= 0x28FF))).toBe(true)
    expect(rows.some(row => row.includes('8/10'))).toBe(false)
  }
})

test('100×30: two columns, meters instead of the radar, the avatar a pill', async () => {
  const app = await memberProfile(100, 30)
  expect(shows(app, '甜感 ')).toBe(true)
  expect(shows(app, '8/10')).toBe(true)
  expect(shows(app, '◑ 运输中')).toBe(true)
  const rows = await screenRows(app)
  if (rows) {
    expect(rows.length).toBe(30)
    expect(rows.some(row => row.includes('▐林▌ 林晓'))).toBe(true)
    expect(rows.some(row => row.includes('┃ 林 ┃'))).toBe(false)
    const head = rows.findIndex(row => row.includes('订单号'))
    expect(cellOf(rows[head], '订单号')).toBeGreaterThan(40)
    // M order columns: number 13, date 6.
    const first = rows.findIndex(row => row.includes('RT2611220417'))
    expect(cellOf(rows[first], '11-22') - cellOf(rows[first], 'RT2611220417')).toBe(13)
  }
})

test('80×24: one scrolling column, the S footer', async () => {
  const app = await memberProfile(80, 24)
  const texts = app.texts()
  expect(texts).toContain('动画')
  expect(texts).toContain('鼠标')
  expect(texts).toContain('退出')
  expect(texts).toContain('帮助')
  expect(texts).not.toContain('动画开关')
  const rows = await screenRows(app)
  if (rows) {
    expect(rows.length).toBe(24)
    const pill = rows.find(row => row.includes('▐林▌ 林晓'))
    expect(pill).toContain('● 已记住当前公钥 · 至 12-22')
    expect(rows.some(row => row.includes('订单 4 · 地址 3 · 优惠券 1') && row.includes('再积 720 豆升级 Lv.4'))).toBe(true)
    expect(rows.some(row => row.includes('界面动画 m  蒸汽、加载指示与进度条') && row.includes('[ ■ 开 ]'))).toBe(true)
  }
  // The taste meters and the orders further down (one scroll view: everything is laid out).
  expect(shows(app, '8/10')).toBe(true)
  expect(shows(app, '最近订单 ORDERS')).toBe(true)
})

// --- States --------------------------------------------------------------------------

test('a new member: no orders, no taste yet', async () => {
  const app = await start(160, 50, { transport: 'fixture:empty' })
  await loginProfile(app)
  await app.waitFor('text:还没有订单')
  expect(app.texts()).toContain('去挑豆子')
  expect(app.texts()).toContain('下单后生成你的口味画像')
  // This connection has no key (the fixture answers `profile-no-key.json`).
  expect(shows(app, '○ 本次连接没有公钥')).toBe(true)
  expect(shows(app, '按画像推荐')).toBe(false)
  // `1 去挑豆子`: the key goes to the catalog.
  await app.keyboard.press('1')
  await app.waitFor('text:商品列表 · 0 款 · 推荐')
})

test('a member without an SSH key', async () => {
  const app = await start(130, 40)
  await loginProfile(app)
  await waitForPart(app, '○ 本次连接没有公钥')
  expect(shows(app, '● 已记住当前公钥')).toBe(false)
})

test('a key that is remembered shows its expiry', async () => {
  const app = await start(130, 40, { key: true })
  await loginProfile(app)
  await waitForPart(app, '● 已记住当前公钥 · 至 12-22')
})

test('m and o toggle the settings with their toasts', async () => {
  const app = await memberProfile(160, 50)
  expect(app.texts().filter(t => t === '■ 开').length).toBe(2)
  await app.keyboard.press('m')
  await app.waitFor('text:界面动画已关闭')
  expect(shows(app, '所有画面即时切换')).toBe(true)
  await app.waitUntil(() => app.texts().includes('□ 关'))
  await app.keyboard.press('m')
  await app.waitFor('text:界面动画已开启')
  await app.keyboard.press('o')
  await app.waitFor('text:鼠标追踪已关闭')
  expect(shows(app, '点击仍然可用')).toBe(true)
  await app.waitUntil(() => app.texts().filter(t => t === '□ 关').length === 1)
  await app.keyboard.press('o')
  await app.waitFor('text:鼠标追踪已开启')
  await app.waitUntil(() => app.texts().filter(t => t === '■ 开').length === 2)
})

test('q asks the shell for the logout confirmation; Esc keeps the profile', async () => {
  const app = await memberProfile(160, 50)
  await app.keyboard.press('q')
  await app.waitFor('text:退出登录 LOGOUT')
  expect(shows(app, '确认 CONFIRM')).toBe(true)
  await app.keyboard.press('escape')
  await app.waitFor('text:个人信息 ACCOUNT')
  // The button asks the same (the first of the two: the page's, then the status bar's).
  await app.press('label:q 退出登录 >> nth=0')
  await app.waitFor('text:退出登录 LOGOUT')
})

test('Tab walks the controls; ⏎ on a recommendation opens its product', async () => {
  const app = await memberProfile(160, 50)
  await waitForPart(app, '涅里 AA 双洗')
  // 动画开关 → 鼠标开关 → 退出登录 → the first recommendation.
  for (let i = 0; i < 4; i++)
    await app.keyboard.press('tab')
  await app.keyboard.press('enter')
  await app.waitFor('text:详情 DETAIL')
  expect(app.texts()).not.toContain('个人信息 ACCOUNT')
})

test('Tab to a setting and ⏎ toggles it', async () => {
  const app = await memberProfile(160, 50)
  await app.keyboard.press('tab')
  await app.keyboard.press('enter')
  await app.waitFor('text:界面动画已关闭')
})

test('a click on a recommendation opens its product', async () => {
  const app = await memberProfile(130, 40)
  await waitForPart(app, '保山 小粒 日晒')
  await app.press('label:保山 小粒 日晒')
  await app.waitFor('text:详情 DETAIL')
})

test('a guest sees the card, ⏎ goes to the login and comes back', async () => {
  const app = await start(160, 50)
  await enterAsGuest(app)
  await app.keyboard.press('4')
  await app.waitFor('text:你正在以访客身份浏览')
  expect(shows(app, '我的 PROFILE')).toBe(true)
  expect(app.texts()).toContain('登录后可以查看订单、管理地址本，并生成你的口味画像。')
  expect(app.texts()).not.toContain('个人信息 ACCOUNT')
  // The guest footer leads with ⏎ 去登录 and has no q.
  expect(app.texts()).toContain('去登录')
  expect(app.texts()).not.toContain('退出登录')
  const rows = await screenRows(app)
  if (rows) {
    const top = rows.findIndex(row => row.includes('我的 PROFILE') && row.includes('╭'))
    expect(cellOf(rows[top], '╭')).toBe(51)
    expect(rows[top + 10]).toContain('╰')
  }
  // Settings still toggle, for this session.
  await app.keyboard.press('o')
  await app.waitFor('text:鼠标追踪已关闭')
  await app.keyboard.press('enter')
  await loginReady(app)
  await logInAsMember(app)
  await app.waitFor('text:个人信息 ACCOUNT')
})

test('loading shows the spinner line until the orders arrive', async () => {
  // A manual clock: the fixtures answer after a timer, which runs only as the test says.
  const app = await open('.', {
    ...cells(160, 50),
    platform: 'linux',
    clock: {},
    storage: { 'coffee.splash': 'off', 'coffee.transport': 'fixture:member', 'coffee.fingerprint': KEY },
  })
  const clock = app.clock!
  for (let i = 0; i < 20 && !shows(app, '商品列表 · 8 款'); i++)
    await clock.runFor(50)
  await app.keyboard.press('4')
  expect(app.texts()).toContain('个人信息 ACCOUNT')
  expect(shows(app, '正在载入订单…')).toBe(true)
  expect(shows(app, '正在载入…')).toBe(true)
  expect(shows(app, 'RT2611220417')).toBe(false)
  await clock.runFor(50)
  expect(shows(app, 'RT2611220417')).toBe(true)
  expect(shows(app, '正在载入')).toBe(false)
})

test('a profile that fails to load: the account and the orders say so, ⏎ retries', async () => {
  // `fixture:member-error`: signing in works, every member call answers 500.
  const app = await start(160, 50, { transport: 'fixture:member-error' })
  await loginReady(app)
  await logInAsMember(app)
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  await app.keyboard.press('4')
  await waitForPart(app, '个人信息加载失败')
  await waitForPart(app, '订单加载失败')
  expect(shows(app, 'RT2611220417')).toBe(false)
  // The footer offers the retry, as every failed page does; the detail is the designed
  // copy, never a status or a code.
  expect(hint(app, '⏎ 重试')).toBe(true)
  expect(shows(app, '服务器内部错误')).toBe(true)
  expect(shows(app, 'HTTP')).toBe(false)
  await app.keyboard.press('enter')
  await waitForPart(app, '个人信息加载失败')
})

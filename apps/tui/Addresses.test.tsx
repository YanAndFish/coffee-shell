// 地址本 ADDRESS (docs/design/screens/05-address.md) at the four snapshot sizes: the member's
// grid and its keys, the new / edit dialog, the empty book and the guest card. `nsh test`
// runs these headless (texts and values only); `nsh test --target tui` also draws them, and
// the layout checks read the terminal's rows (`screenRows` is `null` headless).
//
// The fixture member (`fixture:member` with a key) resumes at once and has three addresses;
// every change answers its static fixture (a new address is always `周一 · 北京市 海淀区`,
// a delete always leaves cards 2 and 3, a new default is always card 3).
import { expect, open, test } from 'nsh/test'

const KEY = 'SHA256:fixtureKeyFixtureKeyFixtureKeyFixtureKey0'

interface Start { transport?: string, key?: boolean }

function start(cols: number, rows: number, { transport, key }: Start = {}) {
  const storage: Record<string, string> = {}
  if (transport)
    storage['coffee.transport'] = transport
  if (key)
    storage['coffee.fingerprint'] = KEY
  return open('.', {
    width: cols * 8,
    height: rows * 16,
    platform: 'linux',
    storage,
  })
}

type App = Awaited<ReturnType<typeof start>>

/** Code points that take two cells (their second cell is blank in the screenshot). */
function isWide(symbol: string) {
  const code = symbol.codePointAt(0) ?? 0
  return (code >= 0x2E80 && code <= 0xA4CF) || (code >= 0xAC00 && code <= 0xD7A3)
    || (code >= 0xF900 && code <= 0xFAFF) || (code >= 0xFE30 && code <= 0xFE4F)
    || (code >= 0xFF00 && code <= 0xFF60) || (code >= 0xFFE0 && code <= 0xFFE6)
}

/** The terminal's rows as text; `null` headless, where nothing is drawn. */
async function screenRows(app: App): Promise<string[] | null> {
  let bytes: Uint8Array
  try {
    bytes = await app.screenshot()
  }
  catch {
    return null
  }
  if (bytes.length === 0)
    return null
  const { rows } = JSON.parse(new TextDecoder().decode(bytes)) as { rows: { symbol: string }[][] }
  return rows.map((row) => {
    let line = ''
    for (let i = 0; i < row.length; i++) {
      line += row[i].symbol
      if (isWide(row[i].symbol))
        i++
    }
    return line
  })
}

function shows(app: App, part: string) {
  return app.texts().some(text => text.includes(part))
}

/** How many texts on screen are exactly `label` (a card button's; the footer says `默认`). */
function buttons(app: App, label: string) {
  return app.texts().filter(text => text === label).length
}

async function waitForPart(app: App, part: string) {
  await app.waitUntil(() => shows(app, part))
}

/** The member's address book: resumed by the key, then `3`. */
async function book(cols: number, rows: number) {
  const app = await start(cols, rows, { transport: 'fixture:member', key: true })
  await waitForPart(app, '欢迎回来，林晓')
  await app.keyboard.press('3')
  await waitForPart(app, '地址本 · 3 条')
  return app
}

/** The row on screen holding `part` (tui only). */
async function rowWith(app: App, part: string) {
  const rows = await screenRows(app)
  return rows?.find(row => row.includes(part)) ?? null
}

test('160×50: two columns of cards with their tags, actions and the add card', async () => {
  const app = await book(160, 50)
  expect(shows(app, '当前寄往：林晓 · 上海市 徐汇区')).toBe(true)
  expect(app.texts()).toContain('新增地址')
  expect(app.texts()).toContain('本单寄往')
  expect(app.texts()).toContain('默认')
  expect(app.texts()).toContain('公司')
  expect(shows(app, '138 **** 5521')).toBe(true)
  expect(shows(app, '139 **** 0817')).toBe(true)
  expect(app.texts()).toContain('文三路 128 号 5 幢 1 单元 301')
  expect(app.texts()).toContain('新增收货地址')
  // Every card has its four actions at XL; the default card's `space` is disabled.
  expect(buttons(app, '设为默认')).toBe(3)
  // The footer: the six hints of keymap.md.
  for (const hint of ['移动', '用于本单', '新增', '编辑', '删除'])
    expect(app.texts()).toContain(hint)

  const rows = await screenRows(app)
  if (rows) {
    const head = rows.find(row => row.includes('▶ 林晓'))!
    expect(head).toContain('┃  ▶ 林晓  138 **** 5521')
    expect(head).toContain('[本单寄往] ▐默认▌ [家]  ┃  │    林晓  138 **** 5521')
    expect(head.trimEnd().endsWith('[公司]  │')).toBe(true)
    // Two 77-column cards, two cells apart.
    const top = rows.find(row => row.includes('┏━'))!
    expect(top.trim().length).toBe(77 + 2 + 77)
    expect(rows.some(row => row.includes('╭╌╌'))).toBe(true)
  }
})

test('160×50: ←↑↓→ move on the grid; e edits the card with its full phone; Esc cancels', async () => {
  const app = await book(160, 50)
  // → card 2, e: its edit dialog, the phone in full (K10).
  await app.keyboard.press('right')
  await app.keyboard.press('e')
  await app.waitFor('text:编辑地址 EDIT')
  expect(app.texts()).toContain('编辑 INSERT')
  expect(app.value('label:收货人')).toBe('林晓')
  expect(app.value('label:手机号')).toBe('13800005521')
  expect(app.value('label:所在地区')).toBe('上海市 静安区')
  expect(app.value('label:详细地址')).toBe('江宁路 300 号 12F（前台代收）')
  // The page's keys do nothing while it is open.
  await app.keyboard.press('3')
  expect(app.texts()).toContain('编辑地址 EDIT')
  await app.keyboard.press('escape')
  await app.waitUntil(() => !shows(app, '编辑地址 EDIT'))
  await waitForPart(app, '地址本 · 3 条')

  // ↓ from card 2 is the add card (two columns): e does nothing there, ⏎ adds.
  await app.keyboard.press('down')
  await app.keyboard.press('e')
  expect(shows(app, '编辑地址 EDIT')).toBe(false)
  await app.keyboard.press('enter')
  await app.waitFor('text:新增地址 NEW')
  await app.keyboard.press('escape')
  await app.waitUntil(() => !shows(app, '新增地址 NEW'))

  // ← from the add card: card 3; ↑ card 1.
  await app.keyboard.press('left')
  const rows = await screenRows(app)
  if (rows)
    expect(rows.some(row => row.includes('▶ 陈默'))).toBe(true)
  await app.keyboard.press('up')
  await app.keyboard.press('e')
  await app.waitFor('text:编辑地址 EDIT')
  expect(app.value('label:所在地区')).toBe('上海市 徐汇区')
})

test('160×50: ⏎ ships this order to the card and goes to the cart', async () => {
  const app = await book(160, 50)
  await app.keyboard.press('right')
  await app.keyboard.press('enter')
  await waitForPart(app, '本单寄往这里')
  await app.waitUntil(() => !shows(app, '地址本 · 3 条'))
  expect(shows(app, '林晓 · 上海市')).toBe(true)
})

test('130×40: space sets the default, d deletes', async () => {
  const app = await book(130, 40)
  // space on the default card does nothing.
  await app.keyboard.press('space')
  expect(shows(app, '已设为默认地址')).toBe(false)
  await app.keyboard.press('right')
  await app.keyboard.press('space')
  await waitForPart(app, '已设为默认地址')
  expect(shows(app, '上海市 静安区')).toBe(true)

  await app.keyboard.press('left')
  await app.keyboard.press('d')
  await waitForPart(app, '已删除地址')
  await waitForPart(app, '地址本 · 2 条')
  expect(shows(app, '天平路 88 弄')).toBe(false)

  const rows = await screenRows(app)
  if (rows) {
    // L: two columns, the actions with the shorter `space 默认` (62 + 2 + 62 wide at most).
    const actions = rows.find(row => row.includes('用于本单'))!
    expect(actions).toContain('[ space 默认 ]')
    expect(actions.split('用于本单').length - 1).toBe(2)
  }
})

test('100×30: the new dialog validates, types, picks a tag and the default, then saves', async () => {
  const app = await book(100, 30)
  await app.keyboard.press('n')
  await app.waitFor('text:新增地址 NEW')
  // Empty: the server's first error, on its field.
  await app.keyboard.press('enter')
  await app.waitFor('text:✗ 请填写收货人')
  // The focus opened in 收货人, and stayed there: a key types into it and clears the error.
  await app.keyboard.press('z')
  await app.waitUntil(() => !shows(app, '✗ 请填写收货人'))
  expect(app.value('label:收货人')).toBe('z')
  await app.keyboard.press('enter')
  await app.waitFor('text:✗ 请填写所在地区')
  await app.type('label:所在地区', '北京市 海淀区')
  await app.type('label:详细地址', '学院路 37 号 2 号楼 415')
  // A bad phone: 手机号应为 11 位数字.
  await app.type('label:手机号', '123')
  await app.keyboard.press('enter')
  await app.waitFor('text:✗ 手机号应为 11 位数字')
  await app.clear('label:手机号')
  expect(app.value('label:手机号')).toBe('')

  // The error moved the focus to 手机号; three Tabs on: the tag picker (← → choose),
  // then 设为默认 (space works there only).
  await app.keyboard.press('tab')
  await app.keyboard.press('tab')
  await app.keyboard.press('tab')
  await app.keyboard.press('right')
  await app.keyboard.press('right')
  let rows = await screenRows(app)
  if (rows)
    expect(rows.some(row => row.includes('[ 家 ]  [ 公司 ]  ▐ 学校 ▌'))).toBe(true)
  await app.keyboard.press('tab')
  await app.keyboard.press('space')
  await app.waitFor('text:[✓]')
  rows = await screenRows(app)
  if (rows) {
    // Short (30 rows): the labels sit on the fields' top lines.
    expect(rows.some(row => row.includes('╭─ 所在地区 ─'))).toBe(true)
    expect(rows.some(row => row.includes('[ Esc 取消 ]  ▐ ⏎ 保存 ▌'))).toBe(true)
  }

  await app.keyboard.press('enter')
  await waitForPart(app, '已新增地址')
  await waitForPart(app, '地址本 · 4 条')
  expect(shows(app, '新增地址 NEW')).toBe(false)
  // The cursor is on the new card (compact: its actions show).
  rows = await screenRows(app)
  if (rows)
    expect(rows.some(row => row.includes('▶ 周一  —'))).toBe(true)
})

test('80×24: one column of compact cards; the stacked dialog fits; it scrolls to the cursor', async () => {
  const app = await book(80, 24)
  expect(shows(app, '地址本 · 3 条 · 当前寄往：')).toBe(true)
  // Only the cursor's card shows its actions at S.
  expect(buttons(app, '设为默认')).toBe(1)
  let rows = await screenRows(app)
  if (rows) {
    expect(rows.length).toBe(24)
    expect(rows.some(row => row.includes('上海市 徐汇区 · 天平路 88 弄 3 号 602 室'))).toBe(true)
    expect(rows.some(row => row.includes('+ 新增收货地址'))).toBe(true)
  }
  // ↓ moves by one card in one column.
  await app.keyboard.press('down')
  rows = await screenRows(app)
  if (rows)
    expect(rows.some(row => row.includes('▶ 林晓  138 **** 5521') && row.includes('[公司]'))).toBe(true)

  await app.keyboard.press('n')
  await app.waitFor('text:新增地址 NEW')
  rows = await screenRows(app)
  if (rows) {
    const top = rows.findIndex(row => row.includes('╔═ 新增地址 NEW'))
    const bottom = rows.findIndex(row => row.includes('╚'))
    expect(top).toBeGreaterThan(0)
    expect(bottom).toBeLessThan(23)
    expect(bottom - top + 1).toBe(21)
    // 收货人 (focused: heavy) above 手机号.
    const name = rows.findIndex(row => row.includes('┏━ 收货人 ━'))
    const phone = rows.findIndex(row => row.includes('─ 手机号 ─'))
    expect(phone).toBe(name + 3)
  }
  await app.type('label:收货人', '周一')
  await app.type('label:所在地区', '北京市 海淀区')
  await app.type('label:详细地址', '学院路 37 号')
  await app.keyboard.press('enter')
  await waitForPart(app, '地址本 · 4 条')
  // Five cells hardly fit (20 rows): wherever the cursor goes, its card shows whole (the
  // window's arithmetic is `grid::window`'s unit test).
  const whole = (rows: string[], part: string) => {
    const at = rows.findIndex(row => row.includes(part))
    return at > 0 && rows.slice(at).some(row => row.includes('┗') || row.includes('╰╌'))
  }
  rows = await screenRows(app)
  if (rows)
    expect(whole(rows, '▶ 周一')).toBe(true)
  for (let i = 0; i < 3; i++)
    await app.keyboard.press('up')
  rows = await screenRows(app)
  if (rows) {
    expect(whole(rows, '▶ 林晓  138 **** 5521')).toBe(true)
    expect(rows.some(row => row.includes('天平路'))).toBe(true)
  }
  for (let i = 0; i < 4; i++)
    await app.keyboard.press('down')
  rows = await screenRows(app)
  if (rows) {
    expect(rows.some(row => row.includes('▶') && row.includes('+ 新增收货地址'))).toBe(true)
    expect(rows[23]).toContain('帮助')
  }
})

test('an empty book: 0 条, 未指定, only the add card; ⏎ adds with 设为默认 checked', async () => {
  const app = await start(100, 30, { transport: 'fixture:empty' })
  await app.waitUntil(() => app.count('label:用户名') > 0)
  await app.type('label:用户名', '林晓')
  await app.type('label:密码', 'secret1')
  await app.keyboard.press('enter')
  await waitForPart(app, '欢迎回来，林晓')
  await app.keyboard.press('3')
  await waitForPart(app, '地址本 · 0 条')
  expect(shows(app, '当前寄往：未指定')).toBe(true)
  expect(shows(app, '新增收货地址')).toBe(true)
  expect(buttons(app, '设为默认')).toBe(0)
  await app.keyboard.press('enter')
  await app.waitFor('text:新增地址 NEW')
  expect(app.texts()).toContain('[✓]')
})

test('a guest sees the login card; ⏎ goes to the login page', async () => {
  const app = await start(160, 50)
  await app.waitUntil(() => app.count('label:用户名') > 0)
  await app.keyboard.press('g')
  await app.waitUntil(() => app.count('label:用户名') === 0)
  await app.keyboard.press('3')
  await app.waitFor('text:你正在以访客身份浏览')
  expect(app.texts()).toContain('地址本 ADDRESS')
  expect(app.texts()).toContain('登录后可以管理地址本，并为订单选择收货地址。')
  expect(app.texts()).toContain('去登录')
  expect(shows(app, '地址本 ·')).toBe(false)
  const card = await rowWith(app, '你正在以访客身份浏览')
  if (card)
    expect(card).toContain('│    你正在以访客身份浏览')
  await app.keyboard.press('enter')
  await app.waitUntil(() => app.count('label:用户名') > 0)
})

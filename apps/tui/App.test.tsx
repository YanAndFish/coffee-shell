// The Rust root tested from TypeScript. `nsh test` runs this file headless (the default
// platform there is a phone, whose app shortcuts never fire, so every test opens the app as
// a Linux desktop); `nsh test --target tui` runs the same steps on the terminal host's
// in-memory terminal, where the viewport is cells: `cells(120, 36)` asks for 120×36 of them
// at nsh's 8×16 points per cell.
//
// No host configures the session here, so the app answers from the committed contract
// fixtures. Storage picks the scenario (`coffee.transport`: `fixture:` a new visitor,
// `fixture:member` a remembered key, `fixture:empty`, `fixture:down`, `fixture:error`) and
// gives the connection an SSH key (`coffee.fingerprint`). The pages are placeholders: these
// tests prove the core (stores, router, commands, shell wiring), not the screens' looks.
import { expect, open, test } from 'nsh/test'

const KEY = 'SHA256:fixtureKeyFixtureKeyFixtureKeyFixtureKey0'

function cells(cols: number, rows: number) {
  return { width: cols * 8, height: rows * 16 }
}

interface Start { transport?: string, key?: boolean }

function start(cols: number, rows: number, { transport, key }: Start = {}) {
  const storage: Record<string, string> = {}
  if (transport)
    storage['coffee.transport'] = transport
  if (key)
    storage['coffee.fingerprint'] = KEY
  return open('.', {
    ...cells(cols, rows),
    platform: 'linux',
    storage,
  })
}

type App = Awaited<ReturnType<typeof start>>

/** Code point ranges that take two cells: CJK, Hangul, compatibility ideographs and full-width forms. */
const WIDE_RANGES: [number, number][] = [
  [0x2E80, 0xA4CF],
  [0xAC00, 0xD7A3],
  [0xF900, 0xFAFF],
  [0xFE30, 0xFE4F],
  [0xFF00, 0xFF60],
  [0xFFE0, 0xFFE6],
]

/** Whether `symbol` takes two cells (its second cell is blank in the screenshot). */
function isWide(symbol: string) {
  const code = symbol.codePointAt(0)
  return code !== undefined && WIDE_RANGES.some(([from, to]) => code >= from && code <= to)
}

interface Cell { symbol: string, fg: string, bg: string, modifiers: string[] }

/** The in-memory terminal's cells, row by row; `null` headless, where nothing is drawn. */
async function screenCells(app: App): Promise<Cell[][] | null> {
  let bytes: Uint8Array
  try {
    bytes = await app.screenshot()
  }
  catch {
    return null
  }
  if (bytes.length === 0)
    return null
  return (JSON.parse(new TextDecoder().decode(bytes)) as { rows: Cell[][] }).rows
}

/** The terminal's rows as text (wide characters' second cells dropped); `null` headless. */
async function screenRows(app: App): Promise<string[] | null> {
  const rows = await screenCells(app)
  return rows && rows.map((row) => {
    let line = ''
    for (let i = 0; i < row.length; i++) {
      line += row[i].symbol
      if (isWide(row[i].symbol))
        i++
    }
    return line
  })
}

/** Whether some text on screen contains `part`. */
function showsPart(app: App, part: string) {
  return app.texts().some(text => text.includes(part))
}

async function waitForPart(app: App, part: string) {
  await app.waitUntil(() => showsPart(app, part))
}

/** From the login page to the catalog as a guest (`g`). */
async function browseAsGuest(app: App) {
  await app.waitFor('text:登录 · SIGN IN')
  await app.keyboard.press('g')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
}

/** Logs in as the fixture member from the login page. */
async function logIn(app: App, password = 'secret1') {
  await app.type('label:用户名', '林晓')
  await app.type('label:密码', password)
  await app.keyboard.press('enter')
}

test('a new connection starts on the login page, with the demo notices', async () => {
  const app = await start(120, 36)
  await app.waitFor('text:登录 · SIGN IN')
  const texts = app.texts()
  expect(texts).toContain('◆ROAST/TTY')
  expect(texts).toContain('演示环境 · 不是真实商店')
  expect(texts).toContain('! 演示环境 · 请勿使用真实密码')
  expect(texts).toContain('登录 LOGIN')
  expect(texts).toContain('○ 未提供公钥')
  expect(texts).toContain('访客 GUEST')
  expect(texts).toContain('[ ] 记住我 30 天 · 当前连接没有 SSH 公钥，无法记住登录')
  // The login page takes none of the app pages' keys.
  await app.keyboard.press('2')
  expect(app.texts()).toContain('登录 · SIGN IN')
})

test('guest browse → add to cart → gated checkout → login → the merged cart', async () => {
  const app = await start(120, 36)
  await browseAsGuest(app)
  expect(app.texts()).toContain('以访客身份浏览')

  // Quick add: the toast and the header badge (the server's quote counts 2 units).
  await app.keyboard.press('a')
  await app.waitFor('text:已加入购物车')
  await app.waitFor('text:购物车 2')
  expect(showsPart(app, '耶加雪菲 孔加 · 250g · 整豆 × 1')).toBe(true)

  await app.keyboard.press('2')
  await app.waitFor('text:购物车 · 2 件')
  expect(app.texts()).toContain('收货地址：登录后选择收货地址')
  expect(showsPart(app, '合计 ¥208.00')).toBe(true)

  // Checkout as a guest: the login gate.
  await app.keyboard.press('c')
  await app.waitFor('text:登录 · SIGN IN')
  // The toast queues behind `已加入购物车`, which then gives way within 0.8 s.
  await app.waitFor('text:结算前请先登录')

  await logIn(app)
  // Back to the cart (K7), merged into the member's.
  await app.waitFor('text:购物车 · 3 件')
  await app.waitFor('text:欢迎回来，林晓')
  expect(showsPart(app, '已合并访客购物车 1 件')).toBe(true)
  expect(app.texts()).toContain('林晓 Lv.3')
  expect(app.texts()).toContain('购物车 3')
  // The page's keys work again after the login form's fields went away.
  expect(app.texts()).toContain('结算')

  // Checkout as a member: the receipt.
  await app.keyboard.press('c')
  await app.waitFor('text:订单回执 RECEIPT')
  expect(showsPart(app, '订单号 RT2611220417 · 预计 11 月 25 日 送达')).toBe(true)
  expect(app.texts()).toContain('! 演示订单 · 没有真实扣款，也不会发货')
  expect(app.texts()).toContain('继续选购')
  await app.keyboard.press('enter')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
})

test('letters typed in a field stay in the field; Esc leaves it', async () => {
  const app = await start(120, 36)
  await app.waitFor('text:登录 · SIGN IN')
  await app.type('label:用户名', 'gg')
  expect(app.value('label:用户名')).toBe('gg')
  expect(app.texts()).toContain('登录 · SIGN IN')
  // In the field: the field's hints.
  expect(app.texts()).toContain('离开输入')
  expect(app.texts()).not.toContain('访客浏览')
  await app.keyboard.press('escape')
  await app.waitFor('text:访客浏览')
  await app.keyboard.press('g')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
})

test('a wrong password shows the error and clears the password', async () => {
  const app = await start(120, 36)
  await app.waitFor('text:登录 · SIGN IN')
  await logIn(app, 'wrongpass')
  await app.waitFor('text:✗ 用户名或密码不正确')
  expect(app.value('label:密码')).toBe('')
  expect(app.texts()).toContain('访客 GUEST')
})

test('a remembered key resumes the session on connect', async () => {
  const app = await start(120, 36, { transport: 'fixture:member', key: true })
  await app.waitFor('text:欢迎回来，林晓')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  const texts = app.texts()
  expect(texts).toContain('林晓 Lv.3')
  expect(texts).toContain('● 公钥已识别')
  expect(texts).toContain('购物车 3')
  expect(showsPart(app, '已通过 SSH 公钥恢复登录')).toBe(true)
})

test('a key nobody remembered stays on the login page, 记住我 checked', async () => {
  const app = await start(120, 36, { key: true })
  await app.waitFor('text:登录 · SIGN IN')
  expect(app.texts()).toContain('[✓] 记住我 30 天 · 绑定当前 SSH 公钥，下次连接免密登录')
  expect(app.texts()).toContain('● 公钥已识别')
})

test('q asks before logging out; Esc cancels, q q confirms', async () => {
  const app = await start(120, 36, { transport: 'fixture:member', key: true })
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  await app.keyboard.press('q')
  await app.waitFor('text:退出登录 LOGOUT')
  expect(app.texts()).toContain('确认 CONFIRM')
  expect(app.texts()).toContain('同时解除当前公钥的免密登录。')
  expect(app.texts()).toContain('确认退出')
  // ?, 1–4 and the page's keys do nothing while it is open (K24).
  await app.keyboard.press('shift+/')
  await app.keyboard.press('2')
  expect(app.texts()).toContain('退出登录 LOGOUT')
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  expect(app.texts()).toContain('林晓 Lv.3')

  await app.keyboard.press('q')
  await app.waitFor('text:退出登录 LOGOUT')
  await app.keyboard.press('q')
  await app.waitFor('text:已退出登录')
  await app.waitFor('text:登录 · SIGN IN')
  expect(app.texts()).toContain('访客 GUEST')
})

test('a guest\'s q does nothing', async () => {
  const app = await start(120, 36)
  await browseAsGuest(app)
  await app.keyboard.press('q')
  expect(app.texts()).not.toContain('退出登录 LOGOUT')
})

test('tabs 1–4, the detail page and back', async () => {
  const app = await start(120, 36)
  await browseAsGuest(app)
  await app.keyboard.press('j')
  await app.keyboard.press('enter')
  await app.waitFor('text:详情 DETAIL · 慧兰 粉红波旁')
  expect(app.texts()).toContain('详情 DETAIL')
  expect(app.texts()).toContain('规格行')
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')

  await app.keyboard.press('2')
  await app.waitFor('text:购物车 · 0 件')
  expect(showsPart(app, '购物车是空的')).toBe(true)
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
  await browseAsGuest(app)
  await app.keyboard.press('2')
  await app.waitFor('text:购物车 · 0 件')
  await app.keyboard.press('/')
  await app.waitFor('text:离开搜索')
  await app.type('label:/ 搜索', '茉莉')
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
  expect(showsPart(app, '当前寄往：林晓')).toBe(true)
  await app.keyboard.press('n')
  await app.waitFor('text:新增地址 NEW')
  expect(app.texts()).toContain('编辑 INSERT')
  await app.keyboard.press('escape')
  await app.waitFor('text:地址本 · 3 条')
  await app.keyboard.press('4')
  await app.waitFor('text:个人信息 ACCOUNT')
  await waitForPart(app, 'RT2611220417')
  expect(showsPart(app, '熟豆会员 Lv.3 · 1,280 / 2,000 豆')).toBe(true)
})

test('? opens the help from the table, Esc closes it', async () => {
  const app = await start(120, 36)
  await browseAsGuest(app)
  await app.keyboard.press('shift+/')
  await app.waitFor('text:快捷键 KEYMAP')
  expect(app.texts()).toContain('帮助 HELP')
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
})

test('a backend that is down shows the error and ⏎ retries', async () => {
  const app = await start(100, 30, { transport: 'fixture:down' })
  await app.waitFor('text:登录 · SIGN IN')
  await app.keyboard.press('g')
  await app.waitFor('text:商品列表 · 加载失败')
  expect(app.texts()).toContain('✗ 商品加载失败')
  expect(app.texts()).toContain('网络错误：fixture:down · connection refused')
  // The footer offers the retry; moving the cursor is not offered.
  expect(app.texts()).not.toContain('移动')
  await app.keyboard.press('enter')
  await app.waitFor('text:商品列表 · 加载失败')
})

test('an empty catalog says so', async () => {
  const app = await start(100, 30, { transport: 'fixture:empty' })
  await app.waitFor('text:登录 · SIGN IN')
  await app.keyboard.press('g')
  await app.waitFor('text:商品列表 · 0 款 · 推荐')
  expect(app.texts()).toContain('∅ 没有匹配的豆子 · Esc 清除筛选')
})

test('the status bar\'s mode block is a filled pill: ink on amber, bold', async () => {
  const app = await start(100, 30)
  await browseAsGuest(app)
  const cells = await screenCells(app)
  if (!cells)
    return
  // nsh keeps an opaque accent fill as its exact colour and leaves the label in the
  // terminal's default text colour; coffee-host pins default text on that fill to `ink`.
  const bottom = cells[cells.length - 1]
  const block = bottom.slice(0, 13)
  expect(block.map(cell => cell.symbol).join('')).toBe(' 选 购  BROWSE ')
  for (const cell of block)
    expect(cell.bg).toBe('#f0b45a')
})

test('80×24: the S footer fits what the row holds; f opens the filter dialog', async () => {
  const app = await start(80, 24)
  await browseAsGuest(app)
  const texts = app.texts()
  // S hints (`↑↓ 移动` `⏎ 详情` `a 加入` `f 筛选`) as far as the row holds them beside the
  // demo notice (the system row takes the notice in the next wave).
  expect(texts).toContain('移动')
  expect(texts).toContain('帮助')
  expect(texts).not.toContain('烘焙')
  const rows = await screenRows(app)
  if (rows) {
    expect(rows.length).toBe(24)
    expect(rows[0]).toContain('◆ROAST/TTY')
    expect(rows[23]).toContain('演示环境 · 不是真实商店')
    expect(rows[23].trimEnd().endsWith('帮助')).toBe(true)
  }
  // `f` at S opens the filter dialog.
  await app.keyboard.press('f')
  await app.waitFor('text:筛选 FILTER')
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
})

test('60×20: a terminal opened too small shows only the notice with its size', async () => {
  const app = await start(60, 20)
  const texts = app.texts()
  expect(texts).toContain('终端太小')
  expect(texts).toContain('当前 60×20 · 至少需要 80×24')
  expect(texts).not.toContain('◆ROAST/TTY')
})

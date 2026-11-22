// The login / register screen (docs/design/screens/01-login.md) at the four snapshot sizes
// and in its states. Like App.test.tsx, every test opens the app as a Linux desktop of an
// explicit size, on the contract fixtures (storage `coffee.transport`, `coffee.fingerprint`),
// or on a stubbed backend where a test needs a slow or failing answer. `nsh test` runs it
// headless (texts and values); `nsh test --target tui` also checks the terminal's cells.
import type { App, Cell } from './testkit'
import { expect, open, test } from 'nsh/test'
import { cells, screenCells, shows, start } from './testkit'

const LIVE = 'http://login.test'

/** Each row as one string of one symbol per cell (a wide character's second cell is its own). */
function cellRows(rows: Cell[][]) {
  return rows.map(row => row.map(cell => cell.symbol || ' '))
}

/** The (row, column) in cells where `part` (ASCII or box drawing only) starts, or null. */
function find(rows: string[][], part: string): [number, number] | null {
  for (let y = 0; y < rows.length; y++) {
    const at = rows[y].join('').indexOf(part)
    if (at >= 0)
      return [y, at]
  }
  return null
}

async function ready(app: App, legend = '登录 · SIGN IN') {
  await app.waitFor(`text:${legend}`)
}

/** The focus arrived in a text field: the footer shows the field's keys. */
async function typing(app: App) {
  await app.waitFor('text:离开输入')
}

// --- The four sizes ---------------------------------------------------------------------

test('160×50: brand column with the block wordmark and cup, the full card at column 97', async () => {
  const app = await start(160, 50)
  await ready(app)
  const texts = app.texts()
  for (const part of [
    '一家开在终端里的精品咖啡烘焙店。',
    '每周二、五小批量烘焙，48 小时内发货。键盘飞快，鼠标也一样好用。',
    '✓ 8 款当季单品与意式拼配',
    '✓ 重量 · 研磨 · 包装 自由组合',
    '✓ 风味雷达，一眼看懂酸甜醇苦',
    '✓ 购物车与地址本云端同步',
    '欢迎回来',
    '登录以同步购物车、地址本与订单。',
    '用户名',
    '密码',
    '记住我 30 天',
    '当前连接没有 SSH 公钥，无法记住登录',
    '访客浏览',
    '注册新账号 →',
    'Tab 切换输入 · ⏎ 提交 · ? 查看全部快捷键',
    '! 演示环境 · 请勿使用真实密码',
  ])
    expect(texts).toContain(part)
  expect(shows(app, '登录 LOGIN')).toBe(true)
  expect(texts).toContain('[ ]')
  const cells = await screenCells(app)
  if (!cells)
    return
  const rows = cellRows(cells)
  const top = find(rows, '╭─ 登')
  expect(top?.[1]).toBe(97)
  const block = find(rows, '████   ███   ███   ████ █████')
  expect(block?.[1]).toBe(9)
  // The card's bottom line, 55 cells wide, from column 97.
  const bottom = rows.findIndex(row => row[97] === '╰')
  expect(bottom - top![0]).toBe(25)
  expect(rows[bottom].slice(97, 152).join('')).toBe(`╰${'─'.repeat(53)}╯`)
  // The cup under the steam, the selling points beside it.
  const cup = find(rows, '┌────────────┐')
  expect(cup?.[1]).toBe(11)
  expect(rows[cup![0] + 1].join('')).toContain('│▓▓▓▓▓▓▓▓▓▓▓▓│─╮')
  // The focused field's frame is heavy.
  await typing(app)
  const after = cellRows((await screenCells(app))!)
  expect(find(after, '┏━━━━')?.[1]).toBe(102)
})

test('130×40: the same structure, the wordmark from column 5', async () => {
  const app = await start(130, 40)
  await ready(app)
  expect(app.texts()).toContain('Tab 切换输入 · ⏎ 提交 · ? 查看全部快捷键')
  expect(app.texts()).toContain('✓ 8 款当季单品与意式拼配')
  const cells = await screenCells(app)
  if (!cells)
    return
  const rows = cellRows(cells)
  expect(find(rows, '████   ███   ███   ████ █████')?.[1]).toBe(5)
  expect(find(rows, '┌────────────┐')?.[1]).toBe(7)
  expect(find(rows, '╭─ 登')?.[1]).toBe(67)
})

test('100×30: one-row brand beside the compact card (labels on the frames)', async () => {
  const app = await start(100, 30)
  await ready(app)
  const texts = app.texts()
  expect(texts).toContain('一家开在终端里的精品咖啡烘焙店。')
  expect(texts).toContain('✓ 8 款当季单品与意式拼配')
  // The compact card: no Tab line, 记住我's note on its row.
  expect(texts).not.toContain('Tab 切换输入 · ⏎ 提交 · ? 查看全部快捷键')
  expect(texts).toContain(' · 当前连接没有 SSH 公钥')
  expect(texts).toContain('! 演示环境 · 请勿使用真实密码')
  const cells = await screenCells(app)
  if (!cells)
    return
  const rows = cellRows(cells)
  expect(rows.some(row => row.join('').includes('████'))).toBe(false)
  expect(rows.some(row => row.join('').includes('┌────────────┐'))).toBe(false)
  const top = find(rows, '╭─ 登')
  expect(top?.[1]).toBe(43)
  // 17 rows: the bottom line 16 rows under the top.
  expect(rows[top![0] + 16].slice(43, 98).join('')).toBe(`╰${'─'.repeat(53)}╯`)
  // The username label sits on its frame, two cells in.
  await typing(app)
  const after = cellRows((await screenCells(app))!)
  expect(find(after, '┏━ 用')?.[1]).toBe(46)
})

test('80×24: the brand row over the compact card, centred', async () => {
  const app = await start(80, 24)
  await ready(app)
  expect(app.texts()).toContain(' · 一家开在终端里的精品咖啡烘焙店。')
  expect(app.texts()).not.toContain('✓ 8 款当季单品与意式拼配')
  expect(app.texts()).toContain('! 演示环境 · 请勿使用真实密码')
  const cells = await screenCells(app)
  if (!cells)
    return
  const rows = cellRows(cells)
  expect(rows.length).toBe(24)
  const top = find(rows, '╭─ 登')
  expect(top?.[1]).toBe(12)
  expect(rows[top![0] + 16].slice(12, 67).join('')).toBe(`╰${'─'.repeat(53)}╯`)
  // The status bar keeps the S hints: Tab, ⏎ and the help.
  expect(rows[23].join('').replace(/ /g, '')).toContain('Tab切换⏎登录')
})

// --- Focus and keys ---------------------------------------------------------------------

test('the focus starts in 用户名: g types a g there (K1); Esc leaves, then g browses', async () => {
  const app = await start(160, 50)
  await ready(app)
  await typing(app)
  await app.keyboard.press('g')
  expect(app.value('label:用户名')).toBe('g')
  expect(shows(app, '登录 · SIGN IN')).toBe(true)
  // ? types too (the in-memory terminal sends it as `/`): no help.
  await app.keyboard.press('shift+/')
  expect(app.value('label:用户名')?.length).toBe(2)
  expect(app.texts()).not.toContain('快捷键 KEYMAP')
  await app.keyboard.press('escape')
  await app.waitUntil(() => !app.texts().includes('离开输入'))
  await app.keyboard.press('g')
  await app.waitFor('text:以访客身份浏览')
  expect(shows(app, '结算前需要登录')).toBe(true)
})

test('Tab cycles 用户名 → 密码 → 记住我 → 登录 → 访客浏览 → 注册新账号, round (with a key)', async () => {
  const app = await start(160, 50, { key: true })
  await ready(app)
  await typing(app)
  await app.keyboard.press('a')
  expect(app.value('label:用户名')).toBe('a')
  await app.keyboard.press('tab')
  await app.keyboard.press('b')
  expect(app.value('label:密码')).toBe('b')
  expect(app.value('label:用户名')).toBe('a')
  // 记住我: space toggles it (checked by default with a key).
  expect(app.texts()).toContain('[✓]')
  await app.keyboard.press('tab')
  await app.waitUntil(() => !app.texts().includes('离开输入'))
  await app.keyboard.press('space')
  await app.waitFor('text:[ ]')
  await app.keyboard.press('enter')
  await app.waitFor('text:[✓]')
  // Round: Shift+Tab back to the password field.
  await app.keyboard.press('shift+tab')
  await typing(app)
  await app.keyboard.press('c')
  expect(app.value('label:密码')).toBe('bc')
  // 登录 → 访客浏览: ⏎ there browses as a guest.
  await app.keyboard.press('tab')
  await app.keyboard.press('tab')
  await app.keyboard.press('tab')
  await app.keyboard.press('enter')
  await app.waitFor('text:以访客身份浏览')
})

test('Shift+Tab from 用户名 goes round to 注册新账号 →; ⏎ there switches to register', async () => {
  const app = await start(130, 40)
  await ready(app)
  await typing(app)
  await app.keyboard.press('shift+tab')
  await app.waitUntil(() => !app.texts().includes('离开输入'))
  await app.keyboard.press('enter')
  await ready(app, '注册 · SIGN UP')
  expect(shows(app, '注册 SIGN UP')).toBe(true)
})

test('without a key 记住我 is disabled with its reason, and Tab passes it', async () => {
  const app = await start(160, 50)
  await ready(app)
  expect(app.texts()).toContain('○ 未提供公钥')
  expect(app.texts()).toContain('[ ]')
  expect(app.texts()).toContain('当前连接没有 SSH 公钥，无法记住登录')
  await typing(app)
  await app.keyboard.press('tab')
  await app.keyboard.press('tab')
  // On 登录 now (记住我 is no stop): space presses it — local validation.
  await app.waitUntil(() => !app.texts().includes('离开输入'))
  await app.keyboard.press('space')
  await app.waitFor('text:✗ 请输入用户名')
  expect(app.texts()).toContain('[ ]')
})

test('with a key 记住我 is checked, with the key\'s note', async () => {
  const app = await start(100, 30, { key: true })
  await ready(app)
  expect(app.texts()).toContain('[✓]')
  expect(app.texts()).toContain(' · 绑定当前 SSH 公钥')
  expect(app.texts()).toContain('● 公钥已识别')
})

// --- Validation, server errors, success -------------------------------------------------

test('validation: an empty username, then a short password; an edit clears the line', async () => {
  const app = await start(130, 40)
  await ready(app)
  await typing(app)
  await app.keyboard.press('enter')
  await app.waitFor('text:✗ 请输入用户名')
  // The focus is (still) in 用户名.
  await typing(app)
  await app.keyboard.press('x')
  await app.waitUntil(() => !shows(app, '✗ 请输入用户名'))
  await app.type('label:密码', '123')
  await app.keyboard.press('enter')
  await app.waitFor('text:✗ 密码至少 6 位')
  expect(app.value('label:密码')).toBe('123')
})

test('a wrong password: the error line, the password cleared and focused', async () => {
  const app = await start(100, 30)
  await ready(app)
  await app.type('label:用户名', '林晓')
  await app.type('label:密码', 'wrongpass')
  await app.keyboard.press('enter')
  await app.waitFor('text:✗ 用户名或密码不正确')
  expect(app.value('label:密码')).toBe('')
  await typing(app)
  await app.keyboard.press('s')
  expect(app.value('label:密码')).toBe('s')
  expect(app.value('label:用户名')).toBe('林晓')
})

test('a successful login: the catalog and the welcome toast', async () => {
  const app = await start(80, 24)
  await ready(app)
  await app.type('label:用户名', '林晓')
  await app.type('label:密码', 'secret1')
  await app.keyboard.press('enter')
  await app.waitFor('text:欢迎回来，林晓')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  expect(app.texts()).not.toContain('登录 · SIGN IN')
})

test('register: the card switches in place, keeps the username, and signs up', async () => {
  const app = await start(160, 50)
  await ready(app)
  await app.type('label:用户名', 'bean_lover')
  await app.type('label:密码', 'secret1')
  await app.press('label:注册新账号')
  await ready(app, '注册 · SIGN UP')
  const texts = app.texts()
  for (const part of [
    '创建账号',
    '只需要用户名和密码，不收集邮箱或手机号。',
    '2–16 个字符',
    '至少 6 位',
    '注册并登录',
    '← 已有账号，去登录',
  ])
    expect(texts).toContain(part)
  expect(shows(app, '注册 SIGN UP')).toBe(true)
  expect(app.value('label:用户名')).toBe('bean_lover')
  expect(app.value('label:密码')).toBe('')
  await app.type('label:密码', 'secret1')
  await app.keyboard.press('enter')
  await app.waitFor('text:注册成功，欢迎 bean_lover')
  expect(shows(app, '已为你登录 · 购物车已同步')).toBe(true)
})

test('register: a taken username says so; Esc (no focus) goes back to login', async () => {
  const app = await start(100, 30)
  await ready(app)
  await app.press('label:注册新账号')
  await ready(app, '注册 · SIGN UP')
  expect(app.texts()).toContain('只需要用户名和密码。')
  await app.type('label:用户名', '林晓')
  await app.type('label:密码', 'secret1')
  await app.keyboard.press('enter')
  await app.waitFor('text:✗ 这个用户名已被注册')
  await typing(app)
  // A username the rule refuses.
  await app.keyboard.press('escape')
  await app.waitUntil(() => !app.texts().includes('离开输入'))
  await app.keyboard.press('escape')
  await ready(app)
  expect(shows(app, '登录 LOGIN')).toBe(true)
  expect(app.value('label:用户名')).toBe('林晓')
})

test('submitting: the progress block replaces the button, only ? works, then the catalog', async () => {
  // `fixture:slow` answers after 3 s on a manual clock: the request waits while the test looks.
  const app = await open('.', {
    ...cells(160, 50),
    platform: 'linux',
    clock: {},
    storage: { 'coffee.transport': 'fixture:slow' },
  })
  const clock = app.clock!
  for (let i = 0; i < 40 && !app.texts().includes('离开输入'); i++)
    await clock.runFor(50)
  await app.type('label:用户名', '林晓')
  await app.type('label:密码', 'secret1')
  await app.keyboard.press('enter')
  // The block shows once the submit takes longer than 150 ms.
  await clock.runFor(200)
  expect(app.texts()).toContain('握手 · 建立安全连接')
  expect(app.texts()).not.toContain('登录')
  // Everything but ? waits (K2): g neither browses nor types, Esc and Tab move nothing.
  for (const key of ['g', 'escape', 'tab', '2']) {
    await app.keyboard.press(key)
    expect(app.value('label:用户名')).toBe('林晓')
    expect(shows(app, '以访客身份浏览')).toBe(false)
    expect(app.texts()).toContain('握手 · 建立安全连接')
  }
  await app.keyboard.press('shift+/')
  expect(app.texts()).toContain('快捷键 KEYMAP')
  await app.keyboard.press('escape')
  expect(app.texts()).not.toContain('快捷键 KEYMAP')
  // The answers land (the login, then the cart and the book, 3 s each): on the catalog.
  for (let i = 0; i < 120 && !shows(app, '欢迎回来，林晓'); i++)
    await clock.runFor(250)
  expect(shows(app, '欢迎回来，林晓')).toBe(true)
})

test('a network error ends the submit: the input stays, ⏎ retries', async () => {
  const app = await start(160, 50, {
    transport: LIVE,
    network: {
      [`POST ${LIVE}/*`]: { error: 'connection refused' },
    },
  })
  await ready(app)
  await app.type('label:用户名', '林晓')
  await app.type('label:密码', 'secret1')
  await app.keyboard.press('enter')
  await app.waitFor('text:✗ 连接服务器失败，按 ⏎ 重试')
  expect(app.value('label:用户名')).toBe('林晓')
  expect(app.value('label:密码')).toBe('secret1')
  expect(app.texts()).toContain('登录')
})

test('resume on connect: the card says so while a remembered key is tried', async () => {
  const app = await start(100, 30, {
    transport: LIVE,
    key: true,
    network: {
      [`POST ${LIVE}/*`]: {
        status: 404,
        body: {
          error: { code: 'not_remembered', message: '当前公钥没有记住的登录' },
        },
        delay: 1500,
      },
    },
  })
  await ready(app)
  await app.waitFor('text:正在通过 SSH 公钥恢复登录…')
  await app.waitUntil(() => !shows(app, '正在通过 SSH 公钥恢复登录…'), { timeout: 5000 })
  expect(app.texts()).toContain('登录以同步购物车、地址本与订单。')
})

test('guest entry: the 访客浏览 button, and the gated checkout coming back to the card', async () => {
  const app = await start(130, 40)
  await ready(app)
  await app.press('label:g 访客浏览')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  await app.keyboard.press('a')
  await app.waitFor('text:已加入购物车')
  await app.keyboard.press('2')
  await app.waitFor('text:购物车 · 2 件')
  await app.keyboard.press('c')
  await ready(app)
  await app.waitFor('text:结算前请先登录')
  await typing(app)
})

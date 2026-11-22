// The Rust root tested from TypeScript. `nsh test` runs this file headless (the default
// platform there is a phone, whose app shortcuts never fire, so every test opens the app as
// a Linux desktop); `nsh test --target tui` runs the same steps on the terminal host's
// in-memory terminal, where the viewport is cells: `cells(160, 50)` asks for 160×50 of them
// at nsh's 8×16 points per cell.
//
// No host configures the session here, so the catalog comes from the committed contract
// fixtures; the storage key `coffee.transport` picks another fixture set
// (`fixture:empty`, `fixture:down`).
import { expect, open, test } from 'nsh/test'

function cells(cols: number, rows: number) {
  return { width: cols * 8, height: rows * 16 }
}

function start(cols: number, rows: number, transport?: string) {
  return open('.', {
    ...cells(cols, rows),
    platform: 'linux',
    storage: transport ? { 'coffee.transport': transport } : {},
  })
}

/**
 * The terminal's rows as text (wide characters' second cells dropped), from the in-memory
 * terminal's cell screenshot; `null` headless, where nothing is drawn.
 */
async function screenRows(app: Awaited<ReturnType<typeof start>>): Promise<string[] | null> {
  let bytes: Uint8Array
  try {
    bytes = await app.screenshot()
  }
  catch {
    return null
  }
  if (bytes.length === 0)
    return null
  const screen = JSON.parse(new TextDecoder().decode(bytes)) as { rows: { symbol: string }[][] }
  // CJK and full-width forms take two cells; the second is blank.
  const wide = /[⺀-꓏가-힣豈-﫿︰-﹏＀-｠￠-￦]/
  return screen.rows.map((row) => {
    let line = ''
    for (let i = 0; i < row.length; i++) {
      line += row[i].symbol
      if (wide.test(row[i].symbol))
        i++
    }
    return line
  })
}

/** The product name right after the `▶` cursor. */
function cursorRow(texts: string[]) {
  return texts[texts.indexOf('▶') + 1]
}

const NAMES = ['耶加雪菲 孔加', '慧兰 粉红波旁', '涅里 AA 双洗', '波奎特 瑰夏', '保山 小粒 日晒', '安提瓜 火山', '曼特宁 G1', '「夜班」意式拼配']

function expectShell(texts: string[]) {
  expect(texts).toContain('◆ROAST/TTY')
  expect(texts).toContain('演示环境 · 不是真实商店')
  expect(texts).toContain('选购 BROWSE')
}

test('160×50: the whole catalog, every column', async () => {
  const app = await start(160, 50)
  await app.waitFor('text:商品列表 · 8 款')
  const texts = app.texts()
  expectShell(texts)
  for (const name of NAMES)
    expect(texts.some(t => t.startsWith(name))).toBe(true)
  expect(texts).toContain('耶加雪菲 孔加 [新产季]')
  expect(texts).toContain('Ethiopia · Yirgacheffe Konga G1')
  expect(texts).toContain('杯测')
  expect(texts).toContain('处理法')
  expect(texts).toContain('¥98')
  expect(texts).toContain('●○○○○')
  expect(cursorRow(texts)).toBe('耶加雪菲 孔加 [新产季]')
})

test('100×30: no score column, one-line rows', async () => {
  const app = await start(100, 30)
  await app.waitFor('text:商品列表 · 8 款')
  const texts = app.texts()
  expectShell(texts)
  expect(texts).toContain('「夜班」意式拼配 [招牌]')
  expect(texts).toContain('处理法')
  expect(texts).not.toContain('杯测')
  expect(texts).not.toContain('Ethiopia · Yirgacheffe Konga G1')
})

test('80×24: name, origin, roast and price, and the footer still fits', async () => {
  const app = await start(80, 24)
  await app.waitFor('text:商品列表 · 8 款')
  const texts = app.texts()
  expectShell(texts)
  expect(texts).toContain('耶加雪菲 孔加 [新产季]')
  expect(texts).toContain('产区')
  expect(texts).toContain('价格')
  expect(texts).not.toContain('处理法')
  expect(texts).toContain('移动')
  expect(texts).toContain('帮助')

  // On the terminal: nothing is cut at the right edge.
  const rows = await screenRows(app)
  if (rows) {
    expect(rows.length).toBe(24)
    expect(rows[0]).toContain('◆ROAST/TTY')
    expect(rows[0].trimEnd().endsWith('访客')).toBe(true)
    expect(rows[23]).toContain('演示环境 · 不是真实商店')
    expect(rows[23].trimEnd().endsWith('帮助')).toBe(true)
    const price = rows.find(row => row.includes('「夜班」意式拼配 [招牌]'))
    expect(price?.trimEnd().endsWith('¥88  │')).toBe(true)
  }
})

/**
 * Resizes the app, as a terminal resize does. The terminal target cannot resize an open
 * app (`setViewport` rejects: "set the terminal viewport with open(entry, { width, height })"),
 * so there this returns false and the resize part of a test runs headless only; the
 * terminal target still opens the app at each size.
 */
async function resize(app: Awaited<ReturnType<typeof start>>, cols: number, rows: number) {
  try {
    await app.setViewport(cols * 8, rows * 16)
    return true
  }
  catch (error) {
    if (String((error as Error).message).includes('set the terminal viewport'))
      return false
    throw error
  }
}

test('shrinking below 80×24 shows only the notice, and enlarging restores the page as it was', async () => {
  const app = await start(160, 50)
  await app.waitFor('text:商品列表 · 8 款')
  await app.keyboard.press('j')
  await app.keyboard.press('j')
  expect(cursorRow(app.texts())).toBe('涅里 AA 双洗')

  if (!await resize(app, 60, 20)) {
    console.log('resizing an open app is not supported on this target: covered by `nsh test`')
    return
  }
  const small = app.texts()
  expect(small).toContain('终端太小')
  expect(small).toContain('当前 60×20 · 至少需要 80×24')
  expect(small).not.toContain('◆ROAST/TTY')
  expect(small.some(t => t.startsWith('耶加雪菲 孔加'))).toBe(false)
  // Keys do nothing while it is too small.
  await app.keyboard.press('j')

  await resize(app, 160, 50)
  const back = app.texts()
  expect(back).not.toContain('终端太小')
  expect(cursorRow(back)).toBe('涅里 AA 双洗')
})

test('60×20: a terminal opened too small shows only the notice with its size', async () => {
  const app = await start(60, 20)
  const texts = app.texts()
  expect(texts).toContain('终端太小')
  expect(texts).toContain('当前 60×20 · 至少需要 80×24')
  expect(texts).not.toContain('◆ROAST/TTY')
  expect(texts.some(t => t.startsWith('耶加雪菲 孔加'))).toBe(false)

  const rows = await screenRows(app)
  if (rows) {
    const screen = rows.join('\n')
    expect(screen).toContain('终端太小')
    expect(screen).toContain('80×24')
    expect(screen).not.toContain('ROAST')
  }
})

test('↑↓ and j k move the cursor and stop at the ends', async () => {
  const app = await start(100, 30)
  await app.waitFor('text:商品列表 · 8 款')
  await app.keyboard.press('k')
  expect(cursorRow(app.texts())).toBe('耶加雪菲 孔加 [新产季]')
  await app.keyboard.press('down')
  await app.keyboard.press('j')
  expect(cursorRow(app.texts())).toBe('涅里 AA 双洗')
  await app.keyboard.press('up')
  expect(cursorRow(app.texts())).toBe('慧兰 粉红波旁 [店长推荐]')
  for (let i = 0; i < 10; i++)
    await app.keyboard.press('j')
  expect(cursorRow(app.texts())).toBe('「夜班」意式拼配 [招牌]')
})

test('⏎ opens the detail placeholder and Esc comes back to the same row', async () => {
  const app = await start(100, 30)
  await app.waitFor('text:商品列表 · 8 款')
  await app.keyboard.press('j')
  await app.keyboard.press('enter')
  await app.waitFor('text:商品详情：第二阶段实现')
  const detail = app.texts()
  expect(detail).toContain('详情 DETAIL')
  expect(detail).toContain('Colombia · Huila Pink Bourbon')
  expect(detail).toContain('返回')
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款')
  expect(cursorRow(app.texts())).toBe('慧兰 粉红波旁 [店长推荐]')
})

test('tabs 1–4 switch pages; the others are honest placeholders', async () => {
  const app = await start(100, 30)
  await app.waitFor('text:商品列表 · 8 款')
  await app.keyboard.press('2')
  await app.waitFor('text:购物车：第二阶段实现')
  expect(app.texts()).toContain('购物车 CART')
  await app.keyboard.press('3')
  await app.waitFor('text:地址本：第二阶段实现')
  await app.keyboard.press('4')
  await app.waitFor('text:个人信息与订单：第二阶段实现')
  expect(app.texts()).toContain('我的 PROFILE')
  await app.keyboard.press('1')
  await app.waitFor('text:商品列表 · 8 款')
})

test('a backend that is down shows the error and the retry key', async () => {
  const app = await start(80, 24, 'fixture:down')
  await app.waitFor('text:商品列表 · 加载失败')
  const texts = app.texts()
  expect(texts).toContain('✗ 加载商品失败 · 连不上商店后端')
  expect(texts).toContain('网络错误：fixture:down · connection refused')
  expect(texts).toContain('重试')
  // The footer offers the retry; moving the cursor is not offered.
  expect(texts).not.toContain('移动')
  await app.keyboard.press('t')
  await app.waitFor('text:商品列表 · 加载失败')
})

test('an empty catalog says so', async () => {
  const app = await start(100, 30, 'fixture:empty')
  await app.waitFor('text:店里暂时没有豆子')
  expect(app.shows('text:商品列表 · 0 款')).toBe(true)
})

test('? shows the help toast', async () => {
  const app = await start(100, 30)
  await app.waitFor('text:商品列表 · 8 款')
  await app.keyboard.press('shift+/')
  await app.waitFor('text:快捷键帮助')
})

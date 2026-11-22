// The product detail page (docs/design/screens/03-detail.md) at the four design sizes.
// `nsh test` runs these headless (texts, styles and behaviour); `nsh test --target tui` runs
// the same steps on the terminal host's in-memory terminal, where the row checks also run.
//
// Data: the contract fixtures (`coffee.transport` = `fixture:`): `pinkb` (慧兰 粉红波旁, the
// list's second row: 26 in stock, 1kg sold out) and `geisha` (波奎特 瑰夏, the fourth: 6 in
// stock, 500g and 1kg sold out). The loading and error states use a backend URL whose
// requests the test answers itself (`network`), so it decides when and how they answer.
import { expect, open, test } from 'nsh/test'

import pinkb from '../../packages/contract/fixtures/v1/catalog/product-pinkb.json' with { type: 'json' }
import products from '../../packages/contract/fixtures/v1/products.json' with { type: 'json' }

const SIZES: [number, number][] = [
  [160, 50],
  [130, 40],
  [100, 30],
  [80, 24],
]

function cells(cols: number, rows: number) {
  return { width: cols * 8, height: rows * 16 }
}

type Network = NonNullable<Parameters<typeof open>[1]>['network']

function start(cols: number, rows: number, transport = 'fixture:', network?: Network) {
  return open('.', {
    ...cells(cols, rows),
    platform: 'linux',
    storage: { 'coffee.transport': transport },
    ...(network
      ? { network }
      : {}),
  })
}

type App = Awaited<ReturnType<typeof start>>

const WIDE_RANGES: [number, number][] = [
  [0x2E80, 0xA4CF],
  [0xAC00, 0xD7A3],
  [0xF900, 0xFAFF],
  [0xFE30, 0xFE4F],
  [0xFF00, 0xFF60],
  [0xFFE0, 0xFFE6],
]

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

function rowsOf(rows: Cell[][]) {
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

/** The terminal's rows as text (wide characters' second cells dropped); `null` headless. */
async function screenRows(app: App): Promise<string[] | null> {
  const rows = await screenCells(app)
  return rows && rowsOf(rows)
}

/** The cells of the first row holding `text`, from where it starts. */
function cellsAt(rows: Cell[][], text: string): Cell[] | null {
  for (const row of rows) {
    const symbols = row.map(cell => cell.symbol)
    for (let x = 0; x < row.length; x++) {
      let joined = ''
      let k = x
      while (k < row.length && joined.length < text.length) {
        joined += symbols[k]
        k += isWide(symbols[k])
          ? 2
          : 1
      }
      if (joined === text)
        return row.slice(x, k)
    }
  }
  return null
}

function showsPart(app: App, part: string) {
  return app.texts().some(text => text.includes(part))
}

async function waitForPart(app: App, part: string) {
  await app.waitUntil(() => showsPart(app, part))
}

/** Whether `line` has the radar's side label `香气` followed by braille (the radar drawn). */
function hasRadar(line: string) {
  const at = line.indexOf('香气 ')
  const code = at < 0
    ? undefined
    : line.codePointAt(at + 3)
  return code !== undefined && code >= 0x2800 && code <= 0x28FF
}

/** Expects every part on screen, naming the one missing. */
function expectParts(app: App, parts: string[]) {
  const texts = app.texts()
  for (const part of parts) {
    expect([
      part,
      texts.some(text => text.includes(part)),
    ]).toEqual([part, true])
  }
}

/** Login page → `g` (guest) → the list → row `row` (0-based) → its detail page. */
async function openProduct(app: App, row: number, origin: string) {
  await app.waitFor('text:登录 · SIGN IN')
  await app.keyboard.press('g')
  await app.waitFor('text:以访客身份浏览')
  for (let i = 0; i < row; i++)
    await app.keyboard.press('j')
  await app.keyboard.press('enter')
  await waitForPart(app, `选购 / ${origin} / `)
  await app.waitFor('text:规格 SKU')
}

const openPinkb = (app: App) => openProduct(app, 1, '哥伦比亚')
const openGeisha = (app: App) => openProduct(app, 3, '巴拿马')

/** Whether group `name`'s label is the focused one (bold `fg`; the others `mute`, regular). */
function focusedGroup(app: App, name: string) {
  // `数量` is also a status-bar hint (`+ 数量`), which is never bold.
  const matches = app.count(`text:${name}`)
  return Array.from({ length: matches }, (_, n) =>
    app.spanStyle(`text:${name} >> nth=${n}`).fontWeight === 700).some(Boolean)
}

for (const [cols, rows] of SIZES) {
  test(`${cols}×${rows}: the default selection, every part of the page`, async () => {
    const app = await start(cols, rows)
    await openPinkb(app)
    expectParts(app, [
      '← Esc 返回',
      '选购 / 哥伦比亚 / ',
      '慧兰 粉红波旁',
      '店长推荐',
      'Colombia · Huila Pink Bourbon',
      '粉红波旁的甜感是它最大的特点。',
      // The SKU panel: 250g · 整豆 · 标准袋 × 1, the server's prices.
      '规格 SKU',
      '重量',
      '研磨',
      '包装',
      '数量',
      '← → 切换',
      '整豆',
      '冷萃',
      '标准袋',
      '礼盒装 +¥20',
      '现货 26 份 · 每周二、五烘焙',
      '¥118.00',
      '1 × ¥118',
      '已选 250g · 整豆 · 标准袋',
      'SKU RT-PNK-250G-WB-STD',
      '加入购物车',
      '立即购买',
    ])
    expect(focusedGroup(app, '重量')).toBe(true)
    expect(focusedGroup(app, '研磨')).toBe(false)
    expect(app.texts()).not.toContain('← → 增减')
    // The weight notes: in the chip at S and M, under it at L and XL.
    const notes = cols >= 130
      ? ['¥53', '¥118', '¥224', '缺货']
      : [
          '100g ¥53',
          '250g ¥118',
          '500g ¥224',
          '1kg 缺货',
        ]
    expectParts(app, notes)

    const screen = await screenCells(app)
    if (!screen)
      return
    const lines = rowsOf(screen)
    expect(lines.length).toBe(rows)
    const sku = lines.findIndex(line => line.includes('╭─ 规格 SKU'))
    expect(sku).toBeGreaterThan(0)
    // The chosen chips are solid pills; the sold-out weight is struck through.
    if (cols >= 130) {
      expect(lines.some(line => line.includes('[ 100g ] ▐ 250g ▌ [ 500g ] [ 1kg  ]'))).toBe(true)
      expect(lines.some(line => line.includes('› 重量') && line.includes('← → 切换'))).toBe(true)
      // Two columns: the information panel and the SKU panel share their top row.
      expect(lines[sku].indexOf('╭')).toBeLessThan(lines[sku].indexOf('╭─ 规格 SKU'))
      expect(lines.some(line => line.includes('── 豆子档案') && line.includes('── 风味指标'))).toBe(true)
      expect(lines.some(line => line.includes('产区 ┈') && line.includes('慧兰 · 皮塔利托'))).toBe(true)
      expect(lines.some(line => line.includes('烘焙度 ┈') && line.includes('中浅烘 ●●○○○'))).toBe(true)
      expect(lines.some(line => line.includes('杯测 ┈') && line.includes('88 分'))).toBe(true)
      expect(lines.some(line => line.includes('冲煮建议  1:15 · 91°C · 2′20″') && line.includes('风味  荔枝 · 蔓越莓 · 蜂蜜 · 可可'))).toBe(true)
      // XL: the radar beside the 20-cell meters; L: the radar, the 10-cell meters under it.
      expect(lines.some(line => line.includes('酸度 ▉▉▉▉▉▉▉▉▉▉▉▉▉▉░░░░░░  7/10'))).toBe(cols >= 160)
      expect(lines.some(line => line.includes('酸度 ▉▉▉▉▉▉▉░░░  7/10'))).toBe(cols < 160)
      expect(lines.some(hasRadar)).toBe(true)
      expect(lines.some(line => line.includes('▐     a 加入购物车     ▌') || line.includes('▐    a 加入购物车    ▌'))).toBe(true)
    }
    else {
      expect(lines.some(line => line.includes('› 重量  [ 100g ¥53 ] ▐ 250g ¥118 ▌ [ 500g ¥224 ] [ 1kg 缺货 ]') && line.includes('← → 切换'))).toBe(true)
      expect(lines.some(line => line.includes('研磨  ▐整豆▌ [手冲] [意式] [法压] [冷萃]'))).toBe(true)
      expect(lines.some(line => line.includes('包装  ▐ 标准袋 ▌ [ 礼盒装 +¥20 ]'))).toBe(true)
      expect(lines.some(line => line.includes('数量  [ − │ 1 │ + ]  现货 26 份 · 每周二、五烘焙'))).toBe(true)
      // The radar becomes meters below 32 rows (10 cells).
      expect(lines.some(hasRadar)).toBe(false)
      // The actions stay on screen: in the panel at S, pinned under the content at M.
      const actions = lines.findIndex(line => line.includes('▐ a 加入购物车 ▌ [ b 立即购买 ]'))
      expect(actions).toBeGreaterThan(sku)
      if (cols < 100) {
        // S buys first: the SKU panel right under the back and title rows.
        expect(lines[sku - 2]).toContain('[ ← Esc 返回 ]  选购 / 哥伦比亚 / 慧兰 粉红波旁')
        expect(lines[sku - 1]).toContain('慧兰 粉红波旁 ▐店长推荐▌  Colombia · Huila Pink Bourbon')
        expect(lines[actions]).toContain('SKU RT-PNK-250G-WB-STD')
        expect(lines.some(line => line.includes('¥118.00  1 × ¥118  已选 250g · 整豆 · 标准袋'))).toBe(true)
      }
      else {
        // M: the purchase row is the content's last row, above the status bar (and the
        // shell's toast row while a toast shows).
        expect(actions).toBeGreaterThanOrEqual(rows - 3)
        expect(lines[actions]).toContain('¥118.00  1 × ¥118')
      }
      // The scroll bar takes the rightmost content cell.
      expect(lines.some(line => /[┃│]$/.test(line.trimEnd()) && line.includes('规格 SKU'))).toBe(true)
    }
    const struck = cellsAt(screen, cols >= 130
      ? ' 1kg  '
      : ' 1kg 缺货 ')
    expect(struck).not.toBeNull()
    expect(struck![1].modifiers).toContain('crossed_out')
    const chosen = cellsAt(screen, cols >= 130
      ? '▐ 250g ▌'
      : '▐ 250g ¥118 ▌')
    expect(chosen).not.toBeNull()
    expect(chosen![2].modifiers).toContain('bold')
  })
}

test('↑↓ move between the groups without wrapping; the hint follows', async () => {
  const app = await start(160, 50)
  await openPinkb(app)
  await app.keyboard.press('k')
  expect(focusedGroup(app, '重量')).toBe(true)
  await app.keyboard.press('down')
  expect(focusedGroup(app, '研磨')).toBe(true)
  expect(focusedGroup(app, '重量')).toBe(false)
  await app.keyboard.press('j')
  expect(focusedGroup(app, '包装')).toBe(true)
  await app.keyboard.press('j')
  expect(focusedGroup(app, '数量')).toBe(true)
  expect(app.texts()).toContain('← → 增减')
  expect(app.texts()).not.toContain('← → 切换')
  await app.keyboard.press('j')
  expect(focusedGroup(app, '数量')).toBe(true)
  await app.keyboard.press('up')
  expect(focusedGroup(app, '包装')).toBe(true)
  const rows = await screenRows(app)
  if (rows) {
    expect(rows.some(line => line.includes('› 包装') && line.includes('← → 切换'))).toBe(true)
    expect(rows.some(line => line.includes('› 重量'))).toBe(false)
  }
})

test('weight, grind and pack change the SKU; the total is the server\'s table', async () => {
  const app = await start(160, 50)
  await openPinkb(app)
  const cell = (weight: string, pack: string) =>
    pinkb.prices.find(p => p.weight === weight && p.pack === pack)!
  // → 500g.
  await app.keyboard.press('l')
  expectParts(app, [`¥${cell('500g', 'STD').totals[0]}.00`, '1 × ¥224', 'SKU RT-PNK-500G-WB-STD'])
  // → stops at 500g: 1kg is sold out.
  await app.keyboard.press('right')
  expectParts(app, ['已选 500g · 整豆 · 标准袋', 'SKU RT-PNK-500G-WB-STD'])
  // Grind → 手冲.
  await app.keyboard.press('j')
  await app.keyboard.press('l')
  expectParts(app, ['已选 500g · 手冲 · 标准袋', 'SKU RT-PNK-500G-PO-STD'])
  // Pack → 礼盒装: the total and every weight's note include the gift box (the server's).
  await app.keyboard.press('j')
  await app.keyboard.press('l')
  const gift = cell('500g', 'GIFT')
  expectParts(app, [
    `¥${gift.totals[0]}.00`,
    `1 × ¥${gift.unitPrice}`,
    'SKU RT-PNK-500G-PO-GIFT',
    `¥${cell('100g', 'GIFT').unitPrice}`,
    `¥${cell('250g', 'GIFT').unitPrice}`,
  ])
  // + twice: totals[2], not computed here.
  await app.keyboard.press('shift+=')
  await app.keyboard.press('=')
  expectParts(app, [`¥${gift.totals[2]}.00`, `3 × ¥${gift.unitPrice}`])
  // Back to the standard bag: its own row of the table.
  await app.keyboard.press('h')
  expectParts(app, [`¥${cell('500g', 'STD').totals[2]}.00`, 'SKU RT-PNK-500G-PO-STD'])
  // A click on a chip chooses it and moves the focus there.
  await app.press('text: 100g ')
  expect(focusedGroup(app, '重量')).toBe(true)
  expectParts(app, [`¥${cell('100g', 'STD').totals[2]}.00`, '已选 100g · 手冲 · 标准袋'])
})

test('a sold-out weight is skipped by ← →, struck and refuses a click', async () => {
  const app = await start(130, 40)
  await openGeisha(app)
  expectParts(app, ['已选 250g · 整豆 · 标准袋', '仅剩 6 份 · 售完即止'])
  await app.keyboard.press('l')
  expectParts(app, ['已选 250g · 整豆 · 标准袋'])
  await app.keyboard.press('h')
  expectParts(app, ['已选 100g · 整豆 · 标准袋', 'SKU RT-GSH-100G-WB-STD'])
  await app.keyboard.press('h')
  expectParts(app, ['已选 100g · 整豆 · 标准袋'])
  await app.keyboard.press('l')
  await app.keyboard.press('l')
  expectParts(app, ['已选 250g · 整豆 · 标准袋'])
  await app.press('text: 500g ')
  expectParts(app, ['已选 250g · 整豆 · 标准袋'])
  expect(app.spanStyle('text: 500g ').lineThrough).toBe(true)
  expect(app.spanStyle('text: 1kg  ').lineThrough).toBe(true)
  expect(app.spanStyle('text: 100g ').lineThrough).toBe(false)
})

test('the quantity stays within 1 … min(9, stock)', async () => {
  const app = await start(160, 50)
  await openPinkb(app)
  await app.keyboard.press('-')
  expectParts(app, ['1 × ¥118'])
  for (let i = 0; i < 12; i++)
    await app.keyboard.press('shift+=')
  expectParts(app, ['9 × ¥118', '¥1062.00'])
  // ← → on the quantity group step it too.
  await app.keyboard.press('j')
  await app.keyboard.press('j')
  await app.keyboard.press('j')
  await app.keyboard.press('right')
  expectParts(app, ['9 × ¥118'])
  await app.keyboard.press('left')
  expectParts(app, ['8 × ¥118', '¥944.00'])
  for (let i = 0; i < 12; i++)
    await app.keyboard.press('shift+-')
  expectParts(app, ['1 × ¥118', '¥118.00'])
  // The stepper's + is pressable.
  await app.press('label:增加')
  expectParts(app, ['2 × ¥118'])
})

test('a low-stock product: 仅剩 6 份, the quantity stops at 6', async () => {
  const app = await start(100, 30)
  await openGeisha(app)
  expectParts(app, [
    '仅剩 6 份 · 售完即止',
    '波奎特 瑰夏',
    '限量',
    '500g 缺货',
    '1kg 缺货',
  ])
  for (let i = 0; i < 9; i++)
    await app.keyboard.press('shift+=')
  expectParts(app, ['6 × ¥'])
  expect(showsPart(app, '7 × ¥')).toBe(false)
  // Moving the focus to the quantity scrolls it into view (one scroll owner).
  await app.keyboard.press('j')
  await app.keyboard.press('j')
  await app.keyboard.press('j')
  expect(focusedGroup(app, '数量')).toBe(true)
  const rows = await screenRows(app)
  if (rows) {
    expect(rows.some(line => line.includes('› 数量  [ − │ 6 │ + ]  仅剩 6 份 · 售完即止'))).toBe(true)
    // The pinned purchase row stays at the content's foot (a toast may sit under it).
    const actions = rows.findIndex(line => line.includes('▐ a 加入购物车 ▌ [ b 立即购买 ]'))
    expect(actions).toBeGreaterThanOrEqual(rows.length - 3)
  }
})

test('a adds with a toast; b adds and goes to the cart without one', async () => {
  const app = await start(160, 50)
  await openPinkb(app)
  await app.keyboard.press('l')
  await app.keyboard.press('shift+=')
  await app.keyboard.press('a')
  await app.waitFor('text:已加入购物车')
  await waitForPart(app, '慧兰 粉红波旁 · 500g · 整豆 × 2')
  // Still on the detail page.
  expect(app.texts()).toContain('规格 SKU')

  const buyer = await start(80, 24)
  await openGeisha(buyer)
  await buyer.press('label:b 立即购买')
  await buyer.waitFor('text:购物车 CART')
  expect(buyer.texts()).not.toContain('已加入购物车')
  expect(buyer.texts()).not.toContain('规格 SKU')
})

test('Esc and Backspace go back to the list, its cursor kept', async () => {
  const app = await start(160, 50)
  await openGeisha(app)
  await app.keyboard.press('escape')
  await app.waitUntil(() => !app.texts().includes('规格 SKU'))
  expect(app.texts()).toContain('选购 BROWSE')
  // The cursor is still on 瑰夏: ⏎ opens it again.
  await app.keyboard.press('enter')
  await waitForPart(app, '选购 / 巴拿马 / ')
  await app.waitFor('text:规格 SKU')
  await app.keyboard.press('backspace')
  await app.waitUntil(() => !app.texts().includes('规格 SKU'))
  // The back button does the same.
  await app.keyboard.press('enter')
  await app.waitFor('text:规格 SKU')
  await app.press('label:← Esc 返回')
  await app.waitUntil(() => !app.texts().includes('规格 SKU'))
  expect(app.texts()).toContain('选购 BROWSE')
})

const API = 'http://127.0.0.1:3999'

test('loading, then a failure with ⏎ 重试, then the product', async () => {
  // A step settles only once the app is quiet, which it is not while a request waits: the
  // first detail request reads the screen itself, while it is pending, then fails.
  let pending: string[] = []
  let calls = 0
  const app: App = await start(160, 50, API, {
    [`GET ${API}/v1/products*`]: (request) => {
      if (!request.url.includes('/v1/products/'))
        return { body: products }
      calls += 1
      if (calls > 1)
        return { body: pinkb }
      return new Promise((resolve) => {
        setTimeout(() => {
          pending = app.texts()
          resolve({
            status: 500,
            body: {
              error: { code: 'internal_error', message: '服务器内部错误' },
            },
          })
        }, 300)
      })
    },
  })
  await app.waitFor('text:登录 · SIGN IN')
  await app.keyboard.press('g')
  await app.waitFor('text:以访客身份浏览')
  await app.keyboard.press('j')
  await app.keyboard.press('enter')
  // While it loaded: the spinner line, the SKU panel's legend alone, no actions.
  expect(pending).toContain('正在载入…')
  expect(pending).toContain('规格 SKU')
  expect(pending).not.toContain('加入购物车')
  expect(pending).not.toContain('加入')
  await app.waitFor('text:✗ 商品加载失败')
  expect(app.texts()).toContain('重试')
  expect(app.texts()).not.toContain('正在载入…')
  await app.keyboard.press('enter')
  await app.waitFor('text:已选 250g · 整豆 · 标准袋')
  expect(app.texts()).not.toContain('✗ 商品加载失败')
  // One request to open the product, one to retry: none twice.
  expect(calls).toBe(2)
  await app.keyboard.press('escape')
  await app.waitUntil(() => !app.texts().includes('规格 SKU'))
})

test('80×24: a failure fills the one column; Esc goes back', async () => {
  const app = await start(80, 24, API, {
    [`GET ${API}/v1/products*`]: request => request.url.includes('/v1/products/')
      ? { error: 'connection refused' }
      : { body: products },
  })
  await app.waitFor('text:登录 · SIGN IN')
  await app.keyboard.press('g')
  await app.waitFor('text:以访客身份浏览')
  await app.keyboard.press('enter')
  await app.waitFor('text:✗ 商品加载失败')
  expect(app.texts()).not.toContain('规格 SKU')
  await app.keyboard.press('escape')
  await app.waitUntil(() => !app.texts().includes('✗ 商品加载失败'))
})

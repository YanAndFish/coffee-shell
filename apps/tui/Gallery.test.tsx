// The component gallery (`src/ui/gallery.rs`, opened through `src/ui/gallery.entry.tsx`) at
// the four design sizes. `nsh test` runs these headless (texts and behaviour);
// `nsh test --target tui` runs the same steps on the terminal host's in-memory terminal,
// where the cell checks also run. Colours there are nsh's own (before `coffee-tui` pins the
// palette): carrier fills show as their carrier RGB, marked tones carry `italic` (see
// `src/ui/theme.rs`, "Carriers"); the pinned colours are checked in
// `crates/coffee-tui/tests/pin_render.rs`.
import { expect, open, test } from 'nsh/test'

const ENTRY = './src/ui/gallery.entry.tsx'

function gallery(cols: number, rows: number) {
  return open(ENTRY, { width: cols * 8, height: rows * 16, platform: 'linux' })
}

type Gallery = Awaited<ReturnType<typeof gallery>>

interface Cell { symbol: string, fg: string, bg: string, modifiers: string[] }

/** The in-memory terminal's cells, row by row; `null` headless, where nothing is drawn. */
async function screenCells(app: Gallery): Promise<Cell[][] | null> {
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

/** Code point ranges that take two cells (their second cell is blank in the screenshot). */
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

/** The screen's rows as text (wide characters' second cells dropped). */
function rowsOf(cells: Cell[][]) {
  return cells.map((row) => {
    let line = ''
    for (let i = 0; i < row.length; i++) {
      line += row[i].symbol
      if (isWide(row[i].symbol))
        i++
    }
    return line
  })
}

/** The cells of the first row holding `text`, from where it starts. */
function cellsAt(cells: Cell[][], text: string): Cell[] | null {
  for (const row of cells) {
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

const TITLES = [
  '字标 · 窗口 · 导航 WORDMARK',
  '面板 PANEL',
  '状态栏 · 键帽 STATUSBAR',
  '按钮 BUTTON',
  '输入框 INPUT',
  '开关 · 数量 TOGGLE',
  '选项 · 标签 CHIP',
  '数据行 DATAROW',
  '购物车行 · 订单行 ROWS',
  '地址卡 ADDRESS',
  '计量 · 雷达 · 引导线 METER',
  '通知 TOAST',
  '空 · 载入 · 错误 STATES',
  '对话框 DIALOG',
]

/** A text each page must show, whatever the size. */
const SIGNS = [
  '◆ROAST/TTY',
  '筛选 FILTER',
  '选购 BROWSE',
  '查看详情',
  '用户名',
  '记住我 30 天',
  '店长推荐',
  '名称',
  '慧兰 粉红波旁',
  '新增收货地址',
  '产区 ┈┈┈┈┈┈┈┈┈┈ 慧兰 · 皮塔利托',
  '已加入购物车',
  '没有匹配的豆子',
  '确定退出登录吗？',
]

/** How many braille dot cells (not the blank pattern) `text` holds. */
function brailleIn(text: string) {
  return [...text].filter((c) => {
    const code = c.codePointAt(0) ?? 0
    return code > 0x2800 && code <= 0x28FF
  }).length
}

function hasText(app: Gallery, text: string) {
  return app.texts().some(t => t.includes(text))
}

async function goTo(app: Gallery, page: number) {
  await app.keyboard.press('home')
  for (let i = 0; i < page; i++)
    await app.keyboard.press('right')
  await app.waitFor(`text:${page + 1}/14 · ${TITLES[page]}`)
}

for (const [cols, rows] of [
  [160, 50],
  [130, 40],
  [100, 30],
  [80, 24],
]) {
  test(`${cols}×${rows}: every page shows, fits and keeps the frame`, async () => {
    const app = await gallery(cols, rows)
    await app.waitFor('text:1/14 · 字标 · 窗口 · 导航 WORDMARK')
    for (let page = 0; page < TITLES.length; page++) {
      if (page > 0)
        await app.keyboard.press('right')
      await app.waitFor(`text:${page + 1}/14 · ${TITLES[page]}`)
      expect(hasText(app, SIGNS[page])).toBe(true)
      // The system row's demo notice and the mode block stay on every page.
      expect(app.texts()).toContain('! 演示环境 · 不是真实商店')
      const cells = await screenCells(app)
      if (!cells)
        continue
      const lines = rowsOf(cells)
      expect(lines.length).toBe(rows)
      expect(lines[0]).toContain('! 演示环境 · 不是真实商店')
      expect(lines[rows - 1]).toContain('组件 GALLERY')
      // Nothing is cut at the right edge: no row ends in an ellipsis.
      for (const line of lines)
        expect(line.trimEnd().endsWith('…') && !line.includes('搜索')).toBe(false)
    }
    // Wrapping: past the last page is the first.
    await app.keyboard.press('right')
    await app.waitFor('text:1/14 · 字标 · 窗口 · 导航 WORDMARK')
  }, 60000)
}

test('160×50: wide tabs, the braille radar, two address cards side by side', async () => {
  const app = await gallery(160, 50)
  await goTo(app, 10)
  // The radar's axis names frame its canvas; on a tall screen it is not the meter fallback.
  expect(hasText(app, '酸度')).toBe(true)
  expect(app.texts().some(t => t.startsWith('香气 ') && t.endsWith(' 甜感'))).toBe(true)
  const cells = await screenCells(app)
  if (cells) {
    const braille = brailleIn(rowsOf(cells).join(''))
    expect(braille).toBeGreaterThan(40)
  }
  await goTo(app, 9)
  if (cells) {
    const lines = rowsOf((await screenCells(app))!)
    expect(lines.some(line => line.includes('┏') && line.includes('╭'))).toBe(true)
  }
})

test('80×24: the radar falls back to meters and the cart row drops its columns', async () => {
  const app = await gallery(80, 24)
  await goTo(app, 10)
  expect(app.texts().some(t => t.includes('酸度') && t.includes('8/10'))).toBe(true)
  const cells = await screenCells(app)
  if (cells) {
    const braille = brailleIn(rowsOf(cells).join(''))
    // Only the spinner of the progress block.
    expect(braille).toBeLessThanOrEqual(1)
  }
  await goTo(app, 8)
  // S: the spec and price move to the second row.
  expect(hasText(app, '250g · 手冲 · 标准袋 · ¥118')).toBe(true)
})

test('the cursor slides, the check box ticks and the switch turns', async () => {
  const app = await gallery(100, 30)
  await goTo(app, 7)
  const cursorAt = () => {
    const texts = app.texts()
    return texts[texts.indexOf('▶') + 1]
  }
  expect(cursorAt()).toBe('耶加雪菲 孔加')
  await app.keyboard.press('j')
  expect(cursorAt()).toBe('慧兰 粉红波旁')
  await app.keyboard.press('k')
  await app.keyboard.press('k')
  expect(cursorAt()).toBe('曼特宁 G1')

  await goTo(app, 5)
  expect(app.texts()).toContain('[✓]')
  await app.keyboard.press('space')
  expect(app.texts().filter(t => t === '[✓]').length).toBe(0)
  expect(hasText(app, '□ 关')).toBe(true)
  await app.keyboard.press('m')
  expect(app.texts().filter(t => t.includes('■ 开')).length).toBe(2)
  await app.keyboard.press('shift+=')
  expect(app.texts()).toContain('2')
})

test('Tab puts the focus in an input: a heavy amber frame, and typing goes in it', async () => {
  const app = await gallery(100, 30)
  await goTo(app, 4)
  expect(hasText(app, '┏')).toBe(false)
  await app.keyboard.press('tab')
  expect(hasText(app, '┏')).toBe(true)
  await app.keyboard.press('x')
  expect(app.value('label:用户名')).toBe('林晓x')
  const cells = await screenCells(app)
  if (cells) {
    const corner = cellsAt(cells, '┏')
    expect(corner).not.toBeNull()
    expect(corner![0].fg).toBe('yellow')
  }
  // Esc leaves the field; the coupon upper-cases what is typed (its `t` is not the toast's)
  // and a wrong one shows its error line.
  await app.keyboard.press('escape')
  expect(hasText(app, '┏')).toBe(false)
  await app.keyboard.press('tab')
  await app.keyboard.press('tab')
  await app.keyboard.press('tab')
  await app.keyboard.press('t')
  await app.keyboard.press('t')
  await app.keyboard.press('y')
  expect(app.value('label:优惠码')).toBe('TTY')
  expect(app.texts()).toContain('✗ 优惠码无效或已过期')
  expect(hasText(app, '已加入购物车')).toBe(false)
})

test('the confirmation dialog: its keys, its focus ring and its mode', async () => {
  const app = await gallery(100, 30)
  await goTo(app, 13)
  await app.keyboard.press('c')
  await app.waitFor('text:确认 CONFIRM')
  expect(app.texts().filter(t => t === '确定退出登录吗？').length).toBe(2)
  // Tab and ← → turn the ring inside the dialog: the page stays.
  await app.keyboard.press('tab')
  await app.keyboard.press('right')
  await app.keyboard.press('left')
  expect(hasText(app, '14/14')).toBe(true)
  await app.keyboard.press('escape')
  await app.waitFor('text:确认 CONFIRM', { state: 'gone' })
  await app.keyboard.press('o')
  await app.waitFor('text:编辑地址 EDIT')
  expect(hasText(app, '编辑 INSERT')).toBe(true)
  await app.keyboard.press('enter')
  await app.waitFor('text:编辑地址 EDIT', { state: 'gone' })
})

test('t shows our own toast at the bottom right, Esc closes it', async () => {
  const app = await gallery(100, 30)
  await app.keyboard.press('t')
  await app.waitFor('text:已加入购物车')
  const cells = await screenCells(app)
  if (cells) {
    const lines = rowsOf(cells)
    const row = lines.findIndex(line => line.includes('已加入购物车'))
    // Its box ends two rows above the bottom: the status bar's row stays the bar's.
    expect(lines[row + 2]).toContain('╰')
    expect(row + 2).toBe(30 - 3)
    expect(lines[29]).toContain('组件 GALLERY')
  }
  await app.keyboard.press('t')
  await app.waitFor('text:以访客身份浏览')
  await app.keyboard.press('escape')
  await app.waitFor('text:以访客身份浏览', { state: 'gone' })
})

test('states are glyphs and attributes: bold, reverse fills, strike-through, underline', async () => {
  const app = await gallery(100, 30)
  await goTo(app, 3)
  const cells = await screenCells(app)
  if (!cells)
    return
  // The primary button: half blocks in amber around the amber fill, its label bold.
  const primary = cellsAt(cells, '▐ ⏎ 查看详情 ▌')!
  expect(primary[0].fg).toBe('yellow')
  expect(primary[2].bg).toBe('#f0b45a')
  expect(primary[2].modifiers).toContain('bold')
  // A focused button: `›` before it, its label bold and underlined.
  const focused = cellsAt(cells, '›[ e 编辑 ]')!
  expect(focused[4].modifiers).toContain('bold')
  expect(focused[4].modifiers).toContain('underlined')
  // A disabled one: struck through, and drawn in a marked tone (italic carrier).
  const disabled = cellsAt(cells, '[ space 设为默认 ]')!
  expect(disabled[2].modifiers).toContain('crossed_out')
  expect(disabled[2].modifiers).toContain('italic')

  // The cursor row: `▶` and the name bold (the band's colour is the host's: the terminal
  // test host draws fills its own way; `pin_render.rs` checks the pinned band).
  await goTo(app, 7)
  const rows = (await screenCells(app))!
  const cursor = cellsAt(rows, '▶  耶加雪菲')!
  expect(cursor[0].modifiers).toContain('bold')
  expect(cursor[0].fg).toBe('yellow')

  // A key cap's key is bold; marked tones (the dim lines) carry the italic carrier mark.
  const caps = cellsAt(rows, '↑↓  移动')!
  expect(caps[0].modifiers).toContain('bold')
  await goTo(app, 9)
  const cards = (await screenCells(app))!
  const dashed = cellsAt(cards, '╭╌╌')!
  expect(dashed[0].modifiers).toContain('italic')
})

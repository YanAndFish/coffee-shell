// 选购 BROWSE (`src/pages/catalog`, docs/design/screens/02-list.md) at the four design sizes,
// against the contract fixtures (storage `coffee.transport` picks the scenario). `nsh test`
// runs these headless (texts and behaviour); `nsh test --target tui` runs the same steps on
// the in-memory terminal, where the cell checks (layout, cursor, scrolling, widths) also run.
import type { App } from './testkit'
import { expect, open, test } from 'nsh/test'
import { cells, enterAsGuest, isWide, screenRows, shows, start } from './testkit'

/** The cells `text` takes on one line. */
function widthOf(text: string) {
  return [...text].reduce((sum, c) => sum + (isWide(c)
    ? 2
    : 1), 0)
}

/** Whether `text` holds a braille dot cell (the radar), not the blank pattern. */
function hasBraille(text: string) {
  return [...text].some((c) => {
    const code = c.codePointAt(0) ?? 0
    return code > 0x2800 && code <= 0x28FF
  })
}

/** The row on screen with the `▶` cursor (tui only). */
async function cursorRow(app: App) {
  const rows = await screenRows(app)
  return rows && rows.find(row => row.includes('▶'))
}

/** The preview's summary line of the bean under the cursor. */
async function previewSays(app: App, summary: string) {
  await app.waitFor(`text:${summary}`)
}

test('160×50: three panels, the cursor slides and the preview follows', async () => {
  const app = await start(160, 50)
  await enterAsGuest(app)
  const texts = app.texts()
  for (const part of [
    '筛选 FILTER',
    '预览 PREVIEW',
    '产区',
    '烘焙度',
    '排序',
    '全部',
    '埃塞俄比亚',
    '拼配',
    '名称',
    '处理法',
    '价格 /250g',
    '查看详情',
    '快速加入',
    '¥98',
    ' / 250g 起',
    '#茉莉',
    '酸度',
    '苦度',
  ]) {
    expect([
      part,
      shows(app, part),
    ]).toEqual([part, true])
  }
  await previewSays(app, '浅烘 · 水洗 · 杯测 87.5')
  expect(shows(app, '高海拔小农水洗批次')).toBe(true)

  const rows = await screenRows(app)
  if (rows) {
    // XL: filter 27, list, preview 42, two cells apart; three rows a bean.
    const top = rows.findIndex(row => row.includes('╭─ 筛选 FILTER'))
    expect(top).toBeGreaterThan(0)
    expect(rows[top]).toContain('╭─ 商品列表 · 8 款 · 推荐')
    expect(rows[top]).toContain('╭─ 预览 PREVIEW')
    const first = rows.findIndex(row => row.includes('▶'))
    expect(rows[first]).toContain('耶加雪菲 孔加 [新产季]')
    expect(rows[first + 1]).toContain('Ethiopia · Yirgacheffe Konga G1')
    expect(rows[first + 3]).toContain('慧兰 粉红波旁 [店长推荐]')
    // The braille radar, its axis labels around it.
    expect(rows.some(row => row.includes('香气') && row.includes('甜感') && hasBraille(row))).toBe(true)
    expect(rows.some(row => row.includes('▐全部▌'))).toBe(true)
  }

  await app.keyboard.press('j')
  await previewSays(app, '中浅烘 · 水洗 · 杯测 88')
  expect(texts).toContain('耶加雪菲 孔加')
  expect((await cursorRow(app)) ?? '慧兰 粉红波旁').toContain('慧兰 粉红波旁')
  await app.keyboard.press('down')
  await previewSays(app, '中浅烘 · 双重水洗 · 杯测 87')
  // No wrap at the ends.
  await app.keyboard.press('k')
  await app.keyboard.press('up')
  await app.keyboard.press('up')
  await previewSays(app, '浅烘 · 水洗 · 杯测 87.5')
  expect((await cursorRow(app)) ?? '耶加雪菲').toContain('耶加雪菲')
})

test('160×50: ⏎ opens the detail, a adds with its toast', async () => {
  const app = await start(160, 50)
  await enterAsGuest(app)
  await app.keyboard.press('a')
  await app.waitFor('text:已加入购物车')
  expect(shows(app, '耶加雪菲 孔加 · 250g · 整豆 × 1')).toBe(true)
  await app.keyboard.press('j')
  await app.keyboard.press('enter')
  await app.waitUntil(() => shows(app, '选购 / 哥伦比亚 / 慧兰 粉红波旁'))
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  // The cursor stayed on its bean.
  await previewSays(app, '中浅烘 · 水洗 · 杯测 88')
})

test('160×50: a click selects a row, a double click opens it; the preview buttons press', async () => {
  const app = await start(160, 50)
  await enterAsGuest(app)
  await app.press('label:波奎特 瑰夏，巴拿马，浅烘，¥268 每 250 克')
  await previewSays(app, '浅烘 · 水洗 · 杯测 90')
  await app.press('label:a 快速加入')
  await app.waitFor('text:已加入购物车')
  expect(shows(app, '波奎特 瑰夏 · 250g · 整豆 × 1')).toBe(true)
  await app.press('label:⏎ 查看详情')
  await app.waitUntil(() => shows(app, '选购 / 巴拿马 / 波奎特 瑰夏'))
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  // A double click on a row opens it (headless: two presses in a row are one double click;
  // the in-memory terminal reports each press as a single click).
  if (!(await screenRows(app))) {
    await app.press('label:慧兰 粉红波旁，哥伦比亚，中浅烘，¥118 每 250 克')
    await app.press('label:慧兰 粉红波旁，哥伦比亚，中浅烘，¥118 每 250 克')
    await app.waitUntil(() => shows(app, '选购 / 哥伦比亚 / 慧兰 粉红波旁'))
    await app.keyboard.press('escape')
    await app.waitFor('text:商品列表 · 8 款 · 推荐')
  }
  // The sidebar's radios and chips set the query as the keys do.
  await app.press('label:肯尼亚')
  await app.waitFor('text:商品列表 · 1 款 · 推荐')
  await previewSays(app, '中浅烘 · 双重水洗 · 杯测 87')
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  await app.press('label:杯测分')
  await app.waitFor('text:商品列表 · 8 款 · 杯测分')
  await previewSays(app, '浅烘 · 水洗 · 杯测 90')
})

test('130×40: f, r and s cycle; Esc clears the filters but keeps the order', async () => {
  const app = await start(130, 40)
  await enterAsGuest(app)
  const rows = await screenRows(app)
  if (rows) {
    const top = rows.findIndex(row => row.includes('╭─ 筛选 FILTER'))
    expect(rows[top]).toContain('╭─ 预览 PREVIEW')
    expect(rows.some(row => row.includes('▐全部▌ [浅] [中] [深]'))).toBe(true)
    expect(rows.some(row => hasBraille(row))).toBe(true)
  }
  // f: 全部 → 埃塞俄比亚 → 哥伦比亚.
  await app.keyboard.press('f')
  await app.waitFor('text:商品列表 · 1 款 · 推荐')
  await previewSays(app, '浅烘 · 水洗 · 杯测 87.5')
  await app.keyboard.press('f')
  await previewSays(app, '中浅烘 · 水洗 · 杯测 88')
  // r: 浅 within 哥伦比亚, then back to 全部 origins with Esc.
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  await app.keyboard.press('r')
  await app.waitFor('text:商品列表 · 4 款 · 推荐')
  await app.keyboard.press('r')
  await app.waitFor('text:商品列表 · 1 款 · 推荐')
  await previewSays(app, '中烘 · 日晒 · 杯测 85')
  await app.keyboard.press('r')
  await app.waitFor('text:商品列表 · 3 款 · 推荐')
  await app.keyboard.press('r')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  // s: 价格 ↑ (保山 ¥68 first) → 价格 ↓ (瑰夏 ¥268) → 杯测分 → 推荐.
  await app.keyboard.press('s')
  await app.waitFor('text:商品列表 · 8 款 · 价格 ↑')
  await previewSays(app, '中烘 · 日晒 · 杯测 85')
  await app.keyboard.press('s')
  await app.waitFor('text:商品列表 · 8 款 · 价格 ↓')
  await previewSays(app, '浅烘 · 水洗 · 杯测 90')
  await app.keyboard.press('s')
  await app.waitFor('text:商品列表 · 8 款 · 杯测分')
  // Esc keeps the order.
  await app.keyboard.press('f')
  await app.waitFor('text:商品列表 · 1 款 · 杯测分')
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 杯测分')
  await app.keyboard.press('s')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
})

test('130×40: / searches on the server as you type; none found; Esc leaves, Esc clears', async () => {
  const app = await start(130, 40)
  await enterAsGuest(app)
  await app.keyboard.press('/')
  await app.waitFor('text:离开搜索')
  // Letters go into the box, not to the list's keys.
  await app.type('label:搜索豆子、产区或风味…', '茉莉')
  await app.waitFor('text:商品列表 · 2 款 · 推荐')
  expect(app.value('label:搜索豆子、产区或风味…')).toBe('茉莉')
  await previewSays(app, '浅烘 · 水洗 · 杯测 87.5')
  // ⏎ leaves the box with the text kept; the list's keys work again.
  await app.keyboard.press('enter')
  await app.waitFor('text:移动')
  await app.keyboard.press('j')
  await previewSays(app, '浅烘 · 水洗 · 杯测 90')
  // Nothing matches.
  await app.keyboard.press('/')
  await app.waitFor('text:离开搜索')
  await app.type('label:搜索豆子、产区或风味…', 'zzz')
  await app.waitFor('text:商品列表 · 0 款 · 推荐')
  expect(app.texts()).toContain('没有匹配的豆子')
  expect(app.texts()).toContain('清除筛选')
  expect(app.texts()).not.toContain('浅烘 · 水洗 · 杯测 87.5')
  await app.keyboard.press('escape')
  await app.waitUntil(() => !app.texts().includes('离开搜索'))
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 推荐')
  // The box goes once it is empty and left.
  expect(app.count('label:搜索豆子、产区或风味…')).toBe(0)
})

test('100×30: list + preview, the filter chip row, one line a bean, meters', async () => {
  const app = await start(100, 30)
  await enterAsGuest(app)
  const texts = app.texts()
  expect(texts).toContain('产区：全部')
  expect(texts).toContain('烘焙：全部')
  expect(texts).toContain('排序：推荐')
  expect(texts).toContain('预览 PREVIEW')
  expect(texts).not.toContain('筛选 FILTER')
  expect(texts).not.toContain('处理法')
  // Short: the cursor row's second line (subtitle · process) only.
  expect(texts).toContain('Ethiopia · Yirgacheffe Konga G1 · 水洗')
  expect(texts).not.toContain('Colombia · Huila Pink Bourbon · 水洗')
  // The radar falls back to meters.
  expect(shows(app, '7/10')).toBe(true)
  const rows = await screenRows(app)
  if (rows) {
    expect(rows.some(row => hasBraille(row))).toBe(false)
    expect(rows.some(row => row.includes('▉'))).toBe(true)
  }
  await app.keyboard.press('j')
  await app.waitFor('text:Colombia · Huila Pink Bourbon · 水洗')
  expect(app.texts()).not.toContain('Ethiopia · Yirgacheffe Konga G1 · 水洗')
  // The chips cycle as their keys do.
  await app.keyboard.press('f')
  await app.waitFor('text:产区：埃塞俄比亚')
  await app.press('text:烘焙：全部')
  await app.waitFor('text:烘焙：浅')
  await app.keyboard.press('s')
  await app.waitFor('text:排序：价格 ↑')
  await app.keyboard.press('escape')
  await app.waitFor('text:商品列表 · 8 款 · 价格 ↑')
})

test('100×32: the list scrolls to keep the cursor in sight', async () => {
  const app = await start(100, 32)
  await enterAsGuest(app)
  for (let i = 0; i < 7; i++)
    await app.keyboard.press('j')
  await previewSays(app, '中深烘 · 日晒 + 水洗 · 杯测 85.5')
  const rows = await screenRows(app)
  if (rows) {
    const cursor = rows.find(row => row.includes('▶'))
    expect(cursor).toContain('「夜班」意式拼配')
    // Three rows a bean do not fit 32 rows: the first bean scrolled away.
    expect(rows.some(row => row.includes('耶加雪菲 孔加'))).toBe(false)
  }
  for (let i = 0; i < 7; i++)
    await app.keyboard.press('k')
  await previewSays(app, '浅烘 · 水洗 · 杯测 87.5')
  expect((await cursorRow(app)) ?? '耶加雪菲 孔加').toContain('耶加雪菲 孔加')
})

test('80×24: one column, the summary row, nothing cut off', async () => {
  const app = await start(80, 24)
  await enterAsGuest(app)
  const texts = app.texts()
  expect(texts).toContain('产区 全部 · 烘焙度 全部 · 排序 推荐')
  expect(texts).toContain('筛选')
  expect(texts).not.toContain('预览 PREVIEW')
  expect(texts).not.toContain('筛选 FILTER')
  const rows = await screenRows(app)
  if (rows) {
    expect(rows.length).toBe(24)
    for (const row of rows)
      expect(widthOf(row)).toBe(80)
    // Every bean, whole, with its price inside the panel's right line.
    const beans = rows.filter(row => row.includes('¥') && row.includes('●'))
    expect(beans.length).toBe(8)
    for (const row of beans) {
      expect(row.includes('…')).toBe(false)
      expect(row.trimEnd().endsWith('│')).toBe(true)
    }
    for (const name of [
      '耶加雪菲 孔加 [新产季]',
      '「夜班」意式拼配 [招牌]',
      '¥268',
      '¥118',
    ])
      expect(rows.some(row => row.includes(name))).toBe(true)
  }
  // r and s still cycle at S; the summary follows.
  await app.keyboard.press('r')
  await app.waitFor('text:产区 全部 · 烘焙度 浅 · 排序 推荐')
  await app.keyboard.press('s')
  await app.waitFor('text:产区 全部 · 烘焙度 浅 · 排序 价格 ↑')
  await app.keyboard.press('escape')
  await app.waitFor('text:产区 全部 · 烘焙度 全部 · 排序 价格 ↑')
})

test('80×24: f opens the filter dialog; choices apply at once; Esc closes', async () => {
  const app = await start(80, 24)
  await enterAsGuest(app)
  await app.keyboard.press('f')
  await app.waitFor('text:筛选 FILTER')
  for (const part of [
    '↑↓ 选择 · Tab 下一组',
    '⏎ / space 选中，立即生效',
    '关闭',
    '烘焙度',
    '杯测分',
    '埃塞俄比亚',
  ]) {
    expect([
      part,
      shows(app, part),
    ]).toEqual([part, true])
  }
  const rows = await screenRows(app)
  if (rows) {
    expect(rows.some(row => row.includes('╔═ 筛选 FILTER'))).toBe(true)
    expect(rows.some(row => row.includes('› ◉ 全部'))).toBe(true)
    // An overlay over the list, not a box under it: the list keeps its 20 rows (its
    // bottom line just over the status bar) and the dialog's top line is on the list's.
    expect(rows[rows.length - 2]).toMatch(/^╰─+╯$/)
    const top = rows.findIndex(row => row.includes('╔═ 筛选 FILTER'))
    expect(top).toBeGreaterThan(2)
    expect(top).toBeLessThan(8)
    expect(rows[top]).toMatch(/^│[^\n\r\u2028\u2029\u2557]*\u2557.*│$/)
    // The status bar's mode.
    expect(rows[rows.length - 1]).toContain('筛选 FILTER')
  }
  // ↓ ⏎ chooses 埃塞俄比亚 at once (the page under the dialog follows).
  await app.keyboard.press('down')
  await app.keyboard.press('enter')
  await app.waitUntil(() => shows(app, '◉ 埃塞俄比亚'))
  const chosen = await screenRows(app)
  if (chosen)
    expect(chosen.some(row => row.includes('› ◉ 埃塞俄比亚'))).toBe(true)
  // Tab to the roast group, ↓ space: 浅; s jumps to the sort group.
  await app.keyboard.press('tab')
  await app.keyboard.press('down')
  await app.keyboard.press('space')
  await app.keyboard.press('s')
  await app.keyboard.press('down')
  await app.keyboard.press('down')
  await app.keyboard.press('enter')
  await app.keyboard.press('escape')
  await app.waitFor('text:产区 埃塞俄比亚 · 烘焙度 浅 · 排序 价格 ↓')
  expect(app.texts()).not.toContain('↑↓ 选择 · Tab 下一组')
  // The page's keys again.
  await app.keyboard.press('escape')
  await app.waitFor('text:产区 全部 · 烘焙度 全部 · 排序 价格 ↓')
})

test('keys typed while the first list loads land on it: g ↓ ↓ ⏎ opens the third bean', async () => {
  // A manual clock: the fixture answers wait for a timer, which runs only as the test says.
  const app = await open('.', {
    ...cells(130, 40),
    platform: 'linux',
    clock: {},
    storage: { 'coffee.splash': 'off' },
  })
  const clock = app.clock!
  for (let i = 0; i < 40 && !app.texts().includes('离开输入'); i++)
    await clock.runFor(50)
  await app.keyboard.press('escape')
  await app.keyboard.press('g')
  expect(shows(app, '商品列表 · 载入中')).toBe(true)
  expect(shows(app, '正在载入商品…')).toBe(true)
  await app.keyboard.press('down')
  await app.keyboard.press('down')
  for (let i = 0; i < 20 && !shows(app, '商品列表 · 8 款'); i++)
    await clock.runFor(10)
  await app.keyboard.press('enter')
  for (let i = 0; i < 20 && !shows(app, '选购 / 肯尼亚 / 涅里 AA 双洗'); i++)
    await clock.runFor(10)
  expect(shows(app, '选购 / 肯尼亚 / 涅里 AA 双洗')).toBe(true)
})

test('a backend that is down: the error and ⏎ retry', async () => {
  const app = await start(160, 50, { transport: 'fixture:down' })
  await enterAsGuest(app, '商品列表 · 加载失败')
  expect(app.texts()).toContain('✗ 商品加载失败')
  expect(app.texts()).toContain('重试')
  expect(app.texts()).toContain('预览 PREVIEW')
  expect(app.texts()).not.toContain('查看详情')
  await app.keyboard.press('enter')
  await app.waitFor('text:商品列表 · 加载失败')
  await app.press('label:⏎ 重试 >> nth=0')
  await app.waitFor('text:✗ 商品加载失败')
})

test('a backend error at 80×24 and an empty catalog at 130×40', async () => {
  const failing = await start(80, 24, { transport: 'fixture:error' })
  await enterAsGuest(failing, '商品列表 · 加载失败')
  expect(failing.texts()).toContain('✗ 商品加载失败')
  const empty = await start(130, 40, { transport: 'fixture:empty' })
  await enterAsGuest(empty, '商品列表 · 0 款 · 推荐')
  expect(empty.texts()).toContain('∅')
  expect(empty.texts()).toContain('没有匹配的豆子')
  expect(empty.texts()).toContain('清除筛选')
  expect(empty.texts()).not.toContain('查看详情')
  // The counts are the whole catalog's.
  expect(shows(empty, '○ 埃塞俄比亚')).toBe(true)
})

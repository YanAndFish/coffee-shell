// Shared helpers of the screen tests (`*.test.tsx`, run by `nsh test` and `nsh test --target
// tui`): opening the app at a size on a fixture scenario, reading the in-memory terminal's
// cells, and the two ways in — as a guest from the login page, or logged in as the fixture
// member. Not a test file itself (no `.test.`), so neither nsh nor vitest runs it.
//
// The login page opens with the focus in 用户名 (keymap.md K1): `g` types there, so a guest
// first leaves the field with Esc (`enterAsGuest`).
import type { OpenOptions } from 'nsh/test'
import { open } from 'nsh/test'

/** The SSH key a connection gets with `key: true` (storage `coffee.fingerprint`). */
export const KEY = 'SHA256:fixtureKeyFixtureKeyFixtureKeyFixtureKey0'

/** A viewport of `cols`×`rows` cells at nsh's 8×16 points per cell. */
export function cells(cols: number, rows: number) {
  return { width: cols * 8, height: rows * 16 }
}

export interface Start {
  /** Storage `coffee.transport`: `fixture:<scenario>` or a live base URL. */
  transport?: string
  /** The connection has an SSH key ([`KEY`]). */
  key?: boolean
  /** Stubbed answers of a live transport. */
  network?: OpenOptions['network']
}

/**
 * Opens the app as a Linux desktop of `cols`×`rows` cells (the headless default is a phone,
 * whose shortcuts never fire).
 */
export function start(cols: number, rows: number, { transport, key, network }: Start = {}) {
  const storage: Record<string, string> = {}
  if (transport)
    storage['coffee.transport'] = transport
  if (key)
    storage['coffee.fingerprint'] = KEY
  return open('.', {
    ...cells(cols, rows),
    platform: 'linux',
    storage,
    ...(network
      ? { network }
      : {}),
  })
}

export type App = Awaited<ReturnType<typeof start>>

export interface Cell { symbol: string, fg: string, bg: string, modifiers: string[] }

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
export function isWide(symbol: string) {
  const code = symbol.codePointAt(0)
  return code !== undefined && WIDE_RANGES.some(([from, to]) => code >= from && code <= to)
}

/** The in-memory terminal's cells, row by row; `null` headless, where nothing is drawn. */
export async function screenCells(app: App): Promise<Cell[][] | null> {
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

/** Rows of cells as text (wide characters' second cells dropped). */
export function rowsOf(rows: Cell[][]) {
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
export async function screenRows(app: App): Promise<string[] | null> {
  const rows = await screenCells(app)
  return rows && rowsOf(rows)
}

/** The column of the first (`last`: the last) cell of `row` showing `symbol`; -1 when none. */
export function column(row: Cell[], symbol: string, last = false) {
  const symbols = row.map(cell => cell.symbol)
  return last
    ? symbols.lastIndexOf(symbol)
    : symbols.indexOf(symbol)
}

/** Whether some text on screen contains `part`. */
export function shows(app: App, part: string) {
  return app.texts().some(text => text.includes(part))
}

/** Waits until some text on screen contains `part`. */
export async function waitForPart(app: App, part: string) {
  await app.waitUntil(() => shows(app, part))
}

/** Whether the status bar offers the hint `label` (`↑↓ 移动`), a pressable of that name. */
export function hint(app: App, label: string) {
  return app.shows(`label:${label}`)
}

/** The login page is up with the focus in its first field (the status bar says so). */
export async function loginReady(app: App, legend = '登录 · SIGN IN') {
  await app.waitFor(`text:${legend}`)
  await app.waitFor('text:离开输入')
}

/**
 * From the login page to the list as a guest: Esc leaves 用户名, then `g`; waits for the
 * list's legend (none: does not wait).
 */
export async function enterAsGuest(app: App, legend: string | null = '商品列表 · 8 款 · 推荐') {
  await loginReady(app)
  await app.keyboard.press('escape')
  await app.waitUntil(() => !app.texts().includes('离开输入'))
  await app.keyboard.press('g')
  if (legend)
    await app.waitFor(`text:${legend}`)
}

/** Logs in as the fixture member (`林晓`) with the form on the login page, ⏎ submitting. */
export async function logInAsMember(app: App, password = 'secret1') {
  await app.type('label:用户名', '林晓')
  await app.type('label:密码', password)
  await app.keyboard.press('enter')
}

/**
 * Opens the app on a manual clock (`fixture:slow` answers after 3 s; timers run only as the
 * test says): a request stays under way while the test looks at the screen and presses keys.
 */
export async function startOnClock(cols: number, rows: number, { transport = 'fixture:slow', key }: Start = {}) {
  const storage: Record<string, string> = { 'coffee.transport': transport }
  if (key)
    storage['coffee.fingerprint'] = KEY
  const app = await open('.', {
    ...cells(cols, rows),
    platform: 'linux',
    clock: {},
    storage,
  })
  return { app, clock: app.clock! }
}

type Clock = NonNullable<App['clock']>

/** Runs the clock `step` ms at a time until `done()` holds (at most `limit` ms). */
export async function runUntil(clock: Clock, done: () => boolean, step = 100, limit = 20000) {
  for (let spent = 0; spent < limit && !done(); spent += step)
    await clock.runFor(step)
  return done()
}

/** On a manual clock: the login page with the focus in 用户名, then logged in as `林晓`. */
export async function logInOnClock(app: App, clock: Clock) {
  await runUntil(clock, () => app.texts().includes('离开输入'), 50)
  await logInAsMember(app)
  await runUntil(clock, () => shows(app, '商品列表 · 8 款'))
}

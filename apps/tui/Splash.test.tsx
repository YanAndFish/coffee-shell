// The splash (docs/design/screens/00-splash.md): what a new connection shows while it starts.
// Every test opens the app as a connection does (`splash: true`), on a manual clock where it
// needs to look at the splash before it leaves (timers and fixture answers run only as the
// test says). The other screen tests open with the splash off (testkit's `start`).
import { expect, test } from 'nsh/test'
import { hint, loginReady, runUntil, shows, startOnClock } from './testkit'

test('a guest sees the brand and the steps, then the login page', async () => {
  const { app, clock } = await startOnClock(160, 50, { transport: 'fixture:', splash: true })
  await runUntil(clock, () => shows(app, '终端已连接'), 50)
  for (const part of [
    '一家开在终端里的精品咖啡烘焙店。',
    '终端已连接 · 160×50',
    '请登录或以访客浏览',
    '! 演示环境 · 不是真实商店',
  ])
    expect(shows(app, part)).toBe(true)
  expect(hint(app, '⏎ 跳过')).toBe(true)
  // It never flashes: still up before its shortest time…
  await clock.runFor(500)
  expect(shows(app, '终端已连接')).toBe(true)
  expect(shows(app, '今日豆单 · 8 款')).toBe(true)
  // …and gone after it, on the login page.
  await runUntil(clock, () => shows(app, '登录 · SIGN IN'), 100)
  await loginReady(app)
  expect(shows(app, '终端已连接')).toBe(false)
})

test('⏎ skips it at once', async () => {
  const { app, clock } = await startOnClock(100, 30, { transport: 'fixture:', splash: true })
  await runUntil(clock, () => shows(app, '终端已连接'), 50)
  await app.keyboard.press('enter')
  await runUntil(clock, () => shows(app, '登录 · SIGN IN'), 50, 400)
  expect(shows(app, '登录 · SIGN IN')).toBe(true)
})

test('a remembered key waits for the resume and the list: the catalog opens loaded', async () => {
  const { app, clock } = await startOnClock(130, 40, { transport: 'fixture:member', key: true, splash: true })
  await runUntil(clock, () => shows(app, '终端已连接'), 50)
  await runUntil(clock, () => shows(app, '欢迎回来，林晓') && shows(app, '今日豆单 · 8 款'), 50)
  expect(shows(app, '登录 · SIGN IN')).toBe(false)
  await runUntil(clock, () => shows(app, '商品列表 · 8 款'), 100)
  expect(shows(app, '正在载入商品…')).toBe(false)
  expect(shows(app, '已通过 SSH 公钥恢复登录')).toBe(true)
})

test('a slow resume keeps the splash; skipping goes to the login page, which carries on', async () => {
  const { app, clock } = await startOnClock(100, 30, { transport: 'fixture:slow', key: true, splash: true })
  await runUntil(clock, () => shows(app, '正在恢复登录…'), 50)
  await clock.runFor(1500)
  expect(shows(app, '正在恢复登录…')).toBe(true)
  await app.keyboard.press('enter')
  await runUntil(clock, () => shows(app, '登录 · SIGN IN'), 50, 400)
  expect(shows(app, '登录 · SIGN IN')).toBe(true)
})

test('a backend that is down says so on each step, then the login page', async () => {
  const { app, clock } = await startOnClock(80, 24, { transport: 'fixture:down', key: true, splash: true })
  await runUntil(clock, () => shows(app, '未能恢复登录'), 50)
  expect(shows(app, '未能恢复登录 · 连接服务器失败，请稍后重试')).toBe(true)
  await runUntil(clock, () => shows(app, '豆单没有载入'), 50)
  await runUntil(clock, () => shows(app, '登录 · SIGN IN'), 100)
  expect(shows(app, '登录 · SIGN IN')).toBe(true)
})

test('80×24: the short form fits (no cup)', async () => {
  const { app, clock } = await startOnClock(80, 24, { transport: 'fixture:slow', key: true, splash: true })
  await runUntil(clock, () => shows(app, '终端已连接 · 80×24'), 50)
  expect(shows(app, '终端已连接 · 80×24')).toBe(true)
  expect(shows(app, '一家开在终端里的精品咖啡烘焙店。')).toBe(true)
  expect(shows(app, '└────────────┘')).toBe(false)
})

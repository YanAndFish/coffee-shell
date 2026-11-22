// The Rust root tested from TypeScript: `nsh test` runs this file in a headless runner it
// builds with the crate, and `open(".")` starts the root (`nsh.json` "root").
// `nsh test --target tui` runs the same steps in the terminal host's in-memory terminal.
import { expect, open, test } from 'nsh/test'

test('shows the wordmark and the demo notice', async () => {
  const app = await open('.')
  expect(app.texts()).toContain('◆ROAST/TTY')
  expect(app.texts()).toContain('演示环境 · 不是真实商店')
})

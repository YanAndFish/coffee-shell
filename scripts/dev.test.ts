import type { AddressInfo } from 'node:net'
import { spawnSync } from 'node:child_process'
import { once } from 'node:events'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import process from 'node:process'
import { expect, it } from 'vitest'

it('网关检查进程退出后写入的监听地址也能用于端口检查', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'coffee-dev-check-'))
  const listener = createServer()
  try {
    listener.listen(0, '127.0.0.1')
    await once(listener, 'listening')
    const { port } = listener.address() as AddressInfo
    const address = `127.0.0.1:${port}`
    mkdirSync(join(dir, 'debug'))
    writeFileSync(join(dir, 'cargo'), '#!/usr/bin/env node\nconsole.log(JSON.stringify({ target_directory: process.env.CARGO_TARGET_DIR }))\n', { mode: 0o755 })
    writeFileSync(join(dir, 'debug/coffee-tui'), '')
    // 后台进程继承两条输出管道，让检查进程先退出，再写入摘要和提示。
    writeFileSync(join(dir, 'debug/tty-gateway'), `#!/bin/sh
(
  sleep 0.2
  echo 'listen: ${address}'
  echo 'delayed warning' >&2
) &
`, { mode: 0o755 })

    const result = spawnSync(process.execPath, ['scripts/dev.mjs', '--skip-build'], {
      encoding: 'utf8',
      timeout: 5000,
      env: {
        ...process.env,
        CARGO_TARGET_DIR: dir,
        PATH: `${dir}:${process.env.PATH}`,
        HOST: '127.0.0.1',
        PORT: '0',
      },
    })
    expect(result.error).toBeUndefined()
    expect(result.status).toBe(1)
    expect(result.stderr).toContain(`${address} is already in use`)
    expect(result.stdout).toContain('delayed warning')
  }
  finally {
    listener.close()
    rmSync(dir, { recursive: true, force: true })
  }
})
